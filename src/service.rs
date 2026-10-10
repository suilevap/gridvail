//! Services: work the game asks for (a path, a plan, an answer for an AI)
//! that may take longer than a frame.
//!
//! A service answers with a [`Promise`], which is a plain [`Future`]: the
//! asker keeps it and polls it on later frames, and does something cheap with
//! what it already knows until the answer lands. Dropping it cancels the work.
//! Anything that polls futures can wait on one; behavior trees use
//! flatbt's [`flatbt_bevy::prelude::await_future`].
//!
//! How a service runs its work is the service's own business, its
//! [`Runner`]: the asker does not change whether the answer comes at once or
//! frames later. [`Services`] is the one handle an agent keeps to reach every
//! service.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;

use crate::navigation::{share_paths, PathService};
use crate::schedule::GamePhase;

/// How a service runs its work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Runner {
    /// At once, in the frame that asked: the answer is there on the first poll.
    /// Deterministic: the same seed plays the same way.
    #[default]
    Inline,
    /// On Bevy's async compute pool: the answer lands in whichever frame the
    /// work finishes, and nothing waits for it.
    Background,
    /// At once, but handed over only after this many more polls.
    /// Deterministic slow thinking, for tests and tuning.
    Deferred(u32),
}

impl Runner {
    /// Runs `work`, promising its answer.
    pub fn run<T: Send + 'static>(self, work: impl FnOnce() -> T + Send + 'static) -> Promise<T> {
        Promise(match self {
            Runner::Inline => PromiseState::Ready {
                value: Some(work()),
                polls_left: 0,
            },
            Runner::Deferred(polls) => PromiseState::Ready {
                value: Some(work()),
                polls_left: polls,
            },
            Runner::Background => {
                PromiseState::Running(AsyncComputeTaskPool::get().spawn(async move { work() }))
            }
        })
    }
}

/// An answer a service promised: a [`Future`] that is ready at once, after a
/// few polls, or when background work finishes, as its [`Runner`] decided.
pub struct Promise<T>(PromiseState<T>);

enum PromiseState<T> {
    Ready { value: Option<T>, polls_left: u32 },
    Running(bevy::tasks::Task<T>),
}

impl<T> Promise<T> {
    /// An answer known without any work.
    pub fn ready(value: T) -> Self {
        Self(PromiseState::Ready {
            value: Some(value),
            polls_left: 0,
        })
    }
}

impl<T: Unpin> Future for Promise<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        match &mut self.get_mut().0 {
            PromiseState::Ready { value, polls_left } => {
                if *polls_left > 0 {
                    *polls_left -= 1;
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                match value.take() {
                    Some(value) => Poll::Ready(value),
                    None => Poll::Pending,
                }
            }
            PromiseState::Running(task) => Pin::new(task).poll(cx),
        }
    }
}

/// Every service an agent may ask, behind one shared handle: an agent keeps a
/// clone (one reference count) rather than a copy of any service.
#[derive(Resource, Clone, Default)]
pub struct Services(Arc<ServiceSet>);

/// The services themselves.
#[derive(Default)]
pub struct ServiceSet {
    pub paths: PathService,
}

impl Services {
    /// The given services, for configuring how they run (tests run them
    /// inline, for instance).
    pub fn new(services: ServiceSet) -> Self {
        Self(Arc::new(services))
    }

    /// Every service running inline: deterministic, for tests and tools.
    pub fn inline() -> Self {
        Self::new(ServiceSet {
            paths: PathService::with_runner(Runner::Inline),
        })
    }

    pub fn paths(&self) -> &PathService {
        &self.0.paths
    }
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Services").finish_non_exhaustive()
    }
}

/// Provides [`Services`] (insert your own first to configure them) and keeps
/// what they know of the world current.
pub struct ServicesPlugin;

impl Plugin for ServicesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Services>()
            .add_systems(Update, share_paths.in_set(GamePhase::Simulation));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::{block_on, poll_once, TaskPool};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn poll<T: Unpin>(promise: &mut Promise<T>) -> Option<T> {
        block_on(poll_once(promise))
    }

    #[test]
    fn inline_answers_at_once() {
        assert_eq!(poll(&mut Runner::Inline.run(|| 7)), Some(7));
    }

    #[test]
    fn deferred_answers_after_its_polls() {
        let mut promise = Runner::Deferred(2).run(|| 7);
        assert_eq!(poll(&mut promise), None);
        assert_eq!(poll(&mut promise), None);
        assert_eq!(poll(&mut promise), Some(7));
    }

    /// Work in the background that takes its time leaves the caller free to
    /// go on polling, frame after frame, and answers once it is done.
    #[test]
    fn background_never_blocks_the_caller() {
        AsyncComputeTaskPool::get_or_init(TaskPool::default);
        let (release, gate) = mpsc::channel::<u32>();
        let mut promise = Runner::Background.run(move || gate.recv().unwrap_or(0));
        for _ in 0..20 {
            let polled = Instant::now();
            assert_eq!(poll(&mut promise), None);
            assert!(polled.elapsed() < Duration::from_millis(50));
            std::thread::sleep(Duration::from_millis(1));
        }
        release.send(7).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(answer) = poll(&mut promise) {
                assert_eq!(answer, 7);
                break;
            }
            assert!(Instant::now() < deadline, "the work never finished");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
