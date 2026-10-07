//! Waiting on a [`Task`] from a behavior tree.
//!
//! [`await_task`] starts a task, keeps it in the node's own state while the
//! tree runs, and writes the answer into a `scope!` output once it lands:
//!
//! ```ignore
//! scope! {
//!     let path: Path;
//!     sequence {
//!         action(await_task(plan_to_player, thinking)).with(out path);
//!         action(Follow).with(path);
//!     }
//! }
//! ```
//!
//! The tree does not care where the task runs: inline it lands on the tick
//! that started it, and the node passes straight on; in the background the
//! node reports the waiting act until it lands. Leaving the branch drops the
//! task, which cancels it. Nothing here is about any particular service.

use flatbt_bevy::prelude::*;

use crate::service::Task;

/// Starts a task with `start` (none to start: the node fails), reports
/// `waiting` until it lands, then writes its answer to the output and
/// succeeds; an answer of `None` fails.
pub fn await_task<Start, Waiting>(start: Start, waiting: Waiting) -> AwaitTask<Start, Waiting> {
    AwaitTask { start, waiting }
}

pub struct AwaitTask<Start, Waiting> {
    start: Start,
    waiting: Waiting,
}

/// The task, and its answer once polled out of it.
pub struct Awaiting<T> {
    task: Task<Option<T>>,
    answer: Option<Option<T>>,
}

impl<T: Send + 'static> Awaiting<T> {
    fn poll(&mut self) {
        if self.answer.is_none() {
            self.answer = self.task.poll();
        }
    }
}

impl<'a, Context, Act, T, Start, Waiting> BtAction<Context, Act, &'a mut Option<T>>
    for AwaitTask<Start, Waiting>
where
    T: Send + 'static,
    Start: Fn(&mut Context) -> Option<Task<Option<T>>>,
    Waiting: Fn(&Context) -> Act,
{
    type State = Awaiting<T>;

    fn start(&self, ctx: &mut Context, _: &'a mut Option<T>) -> Option<Awaiting<T>> {
        let mut awaiting = Awaiting {
            task: (self.start)(ctx)?,
            answer: None,
        };
        // An inline task has answered already: pass on in this same tick.
        awaiting.poll();
        Some(awaiting)
    }

    fn is_in_progress(&self, awaiting: &Awaiting<T>, _: &Context, _: &'a mut Option<T>) -> bool {
        awaiting.answer.is_none()
    }

    fn tick(&self, awaiting: &mut Awaiting<T>, ctx: &mut Context, _: &'a mut Option<T>) -> Act {
        awaiting.poll();
        (self.waiting)(ctx)
    }

    fn complete(
        &self,
        awaiting: &mut Awaiting<T>,
        _: &mut Context,
        out: &'a mut Option<T>,
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

    use super::await_task;
    use crate::service::{ServiceMode, Task};

    #[derive(Default)]
    struct Asker {
        mode: ServiceMode,
        answer: Option<u32>,
        got: Vec<u32>,
    }

    fn tree() -> impl BtNode<Asker, &'static str> {
        scope! {
            let value: u32;
            sequence {
                action(await_task(
                    |asker: &mut Asker| {
                        let answer = asker.answer;
                        Some(Task::spawn(asker.mode, move || answer))
                    },
                    |_: &Asker| "waiting",
                ))
                .with(out value);
                leaf_with(|asker: &mut Asker, value: &u32| {
                    asker.got.push(*value);
                    NodeResult::Running("using")
                })
                .with(value);
            }
        }
    }

    #[test]
    fn an_inline_answer_passes_on_in_the_same_tick() {
        let tree = tree();
        let mut state = BtState::new(&tree);
        let mut asker = Asker {
            answer: Some(7),
            ..Asker::default()
        };
        let result = update(&tree, &mut state, &mut asker, EntryMode::Evaluate);
        assert_eq!(result, NodeResult::Running("using"));
        assert_eq!(asker.got, [7]);
    }

    #[test]
    fn a_slow_answer_waits_then_passes_on() {
        let tree = tree();
        let mut state = BtState::new(&tree);
        let mut asker = Asker {
            mode: ServiceMode::Deferred(2),
            answer: Some(7),
            ..Asker::default()
        };
        for _ in 0..2 {
            let result = update(&tree, &mut state, &mut asker, EntryMode::Resume);
            assert_eq!(result, NodeResult::Running("waiting"));
        }
        let result = update(&tree, &mut state, &mut asker, EntryMode::Resume);
        assert_eq!(result, NodeResult::Running("using"));
        assert_eq!(asker.got, [7]);
    }

    #[test]
    fn no_answer_fails() {
        let tree = tree();
        let mut state = BtState::new(&tree);
        let mut asker = Asker::default();
        let result = update(&tree, &mut state, &mut asker, EntryMode::Evaluate);
        assert_eq!(result, NodeResult::Failure);
        assert!(asker.got.is_empty());
    }
}
