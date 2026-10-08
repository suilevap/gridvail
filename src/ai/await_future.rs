//! Waiting on a [`Future`] from a behavior tree.
//!
//! [`await_future`] starts work that answers with a future (a service's
//! [`Promise`](crate::service::Promise), a Bevy task, anything), keeps the
//! future in the node's own state while the tree runs, and writes the answer
//! into a `scope!` output once it lands. Its input comes from the scope too:
//!
//! ```ignore
//! scope! {
//!     let target: IVec2;
//!     let path: Path;
//!     sequence {
//!         pick_target.with(out target);
//!         action(await_future(plan_path, thinking)).with(target, out path);
//!         action(Follow).with(path);
//!     }
//! }
//! ```
//!
//! The tree does not care how the work runs: an answer ready at once passes
//! straight on in the tick that asked; otherwise the node reports the waiting
//! act until it lands. Leaving the branch drops the future, which cancels the
//! work. Nothing here is about any particular service.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use flatbt_bevy::prelude::*;

/// Starts work for the input with `start` (none to start: the node fails),
/// reports `waiting` until its future lands, then writes the answer to the
/// output and succeeds; an answer of `None` fails.
pub fn await_future<Start, Waiting>(start: Start, waiting: Waiting) -> AwaitFuture<Start, Waiting> {
    AwaitFuture { start, waiting }
}

pub struct AwaitFuture<Start, Waiting> {
    start: Start,
    waiting: Waiting,
}

/// The future, and its answer once polled out of it.
pub struct Awaiting<F: Future> {
    future: F,
    answer: Option<F::Output>,
}

impl<F: Future + Unpin> Awaiting<F> {
    /// Polls once, without blocking: a noop waker, since the tree polls again
    /// on its next tick anyway.
    fn poll(&mut self) {
        if self.answer.is_some() {
            return;
        }
        let mut cx = Context::from_waker(Waker::noop());
        if let Poll::Ready(answer) = Pin::new(&mut self.future).poll(&mut cx) {
            self.answer = Some(answer);
        }
    }
}

impl<'a, Context_, Act, Input, T, F, Start, Waiting>
    BtAction<Context_, Act, (&'a Input, &'a mut Option<T>)> for AwaitFuture<Start, Waiting>
where
    Input: ?Sized,
    F: Future<Output = Option<T>> + Unpin + Send + 'static,
    T: Send + 'static,
    Start: Fn(&mut Context_, &Input) -> Option<F>,
    Waiting: Fn(&Context_) -> Act,
{
    type State = Awaiting<F>;

    fn start(
        &self,
        ctx: &mut Context_,
        (input, _): (&'a Input, &'a mut Option<T>),
    ) -> Option<Awaiting<F>> {
        let mut awaiting = Awaiting {
            future: (self.start)(ctx, input)?,
            answer: None,
        };
        // An answer ready at once passes on in this same tick.
        awaiting.poll();
        Some(awaiting)
    }

    fn is_in_progress(
        &self,
        awaiting: &Awaiting<F>,
        _: &Context_,
        _: (&'a Input, &'a mut Option<T>),
    ) -> bool {
        awaiting.answer.is_none()
    }

    fn tick(
        &self,
        awaiting: &mut Awaiting<F>,
        ctx: &mut Context_,
        _: (&'a Input, &'a mut Option<T>),
    ) -> Act {
        awaiting.poll();
        (self.waiting)(ctx)
    }

    fn complete(
        &self,
        awaiting: &mut Awaiting<F>,
        _: &mut Context_,
        (_, out): (&'a Input, &'a mut Option<T>),
    ) -> bool {
        match awaiting.answer.take().flatten() {
            Some(value) => {
                *out = Some(value);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use flatbt_bevy::prelude::*;

    use super::await_future;
    use crate::service::Runner;

    #[derive(Default)]
    struct Asker {
        runner: Runner,
        got: Vec<u32>,
    }

    /// Doubles the scope's input, as work run by the asker's runner; zero has
    /// no answer.
    fn tree(input: u32) -> impl BtNode<Asker, &'static str> {
        scope! {
            let value: u32 = move |_: &mut Asker| input;
            let doubled: u32;
            sequence {
                action(await_future(
                    |asker: &mut Asker, value: &u32| {
                        let value = *value;
                        Some(asker.runner.run(move || (value > 0).then_some(value * 2)))
                    },
                    |_: &Asker| "waiting",
                ))
                .with(value, out doubled);
                leaf_with(|asker: &mut Asker, doubled: &u32| {
                    asker.got.push(*doubled);
                    NodeResult::Running("using")
                })
                .with(doubled);
            }
        }
    }

    #[test]
    fn an_answer_ready_at_once_passes_on_in_the_same_tick() {
        let tree = tree(7);
        let mut state = BtState::new(&tree);
        let mut asker = Asker::default();
        let result = update(&tree, &mut state, &mut asker, EntryMode::Evaluate);
        assert_eq!(result, NodeResult::Running("using"));
        assert_eq!(asker.got, [14]);
    }

    #[test]
    fn a_slow_answer_waits_then_passes_on() {
        let tree = tree(7);
        let mut state = BtState::new(&tree);
        let mut asker = Asker {
            runner: Runner::Deferred(2),
            ..Asker::default()
        };
        for _ in 0..2 {
            let result = update(&tree, &mut state, &mut asker, EntryMode::Resume);
            assert_eq!(result, NodeResult::Running("waiting"));
        }
        let result = update(&tree, &mut state, &mut asker, EntryMode::Resume);
        assert_eq!(result, NodeResult::Running("using"));
        assert_eq!(asker.got, [14]);
    }

    #[test]
    fn no_answer_fails() {
        let tree = tree(0);
        let mut state = BtState::new(&tree);
        let mut asker = Asker::default();
        let result = update(&tree, &mut state, &mut asker, EntryMode::Evaluate);
        assert_eq!(result, NodeResult::Failure);
        assert!(asker.got.is_empty());
    }
}
