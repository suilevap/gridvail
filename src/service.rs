//! Tasks: work the game asks for (a path, a plan, an answer for an AI) that
//! may take longer than a frame.
//!
//! [`Task::spawn`] takes the work as a closure owning everything it needs (by
//! value or behind an `Arc`), so it can run on another thread, and returns a
//! [`Task`] the asker keeps and polls on later frames, like a promise. Until it
//! lands the asker does something cheap with what it already knows, and the
//! game goes on rendering and taking turns. Dropping a task cancels it, so an
//! asker that despawns or changes its mind leaves nothing running.
//!
//! [`ServiceMode`] decides where tasks run, without the asker noticing:
//! `Inline` is the default because it is deterministic (the same seed plays
//! the same way, which the scenario tests rely on); the game binary runs them
//! in the `Background`.

use bevy::prelude::*;
use bevy::tasks::{block_on, poll_once, AsyncComputeTaskPool};

/// Where tasks run.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ServiceMode {
    /// At once, in the frame that asked: the answer is there on the first poll.
    #[default]
    Inline,
    /// On Bevy's async compute pool: the answer lands in whichever frame the
    /// work finishes, and nothing waits for it.
    Background,
    /// In the frame that asked, but handed over only after this many more
    /// polls. Deterministic slow thinking, for tests and tuning.
    Deferred(u32),
}

/// Work in flight, or its answer.
pub struct Task<T> {
    state: TaskState<T>,
}

enum TaskState<T> {
    Ready { value: Option<T>, polls_left: u32 },
    Running(bevy::tasks::Task<T>),
}

impl<T: Send + 'static> Task<T> {
    /// Runs `work` as `mode` says.
    pub fn spawn(mode: ServiceMode, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let state = match mode {
            ServiceMode::Inline => TaskState::Ready {
                value: Some(work()),
                polls_left: 0,
            },
            ServiceMode::Deferred(polls) => TaskState::Ready {
                value: Some(work()),
                polls_left: polls,
            },
            ServiceMode::Background => {
                TaskState::Running(AsyncComputeTaskPool::get().spawn(async move { work() }))
            }
        };
        Self { state }
    }

    /// A task already answered, for an answer known without any work.
    pub fn ready(value: T) -> Self {
        Self {
            state: TaskState::Ready {
                value: Some(value),
                polls_left: 0,
            },
        }
    }

    /// The answer, once it is ready. A task that answered is spent: later
    /// polls return `None`.
    pub fn poll(&mut self) -> Option<T> {
        match &mut self.state {
            TaskState::Ready { polls_left, value } => {
                if *polls_left > 0 {
                    *polls_left -= 1;
                    None
                } else {
                    value.take()
                }
            }
            TaskState::Running(task) => {
                if task.is_finished() {
                    block_on(poll_once(task))
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::TaskPool;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn inline_answers_at_once() {
        assert_eq!(Task::spawn(ServiceMode::Inline, || 7).poll(), Some(7));
    }

    #[test]
    fn an_answer_is_handed_over_once() {
        let mut task = Task::ready(7);
        assert_eq!(task.poll(), Some(7));
        assert_eq!(task.poll(), None);
    }

    #[test]
    fn deferred_answers_after_its_polls() {
        let mut task = Task::spawn(ServiceMode::Deferred(2), || 7);
        assert_eq!(task.poll(), None);
        assert_eq!(task.poll(), None);
        assert_eq!(task.poll(), Some(7));
    }

    /// Work in the background that takes its time leaves the caller free to
    /// go on polling, frame after frame, and answers once it is done.
    #[test]
    fn background_never_blocks_the_caller() {
        AsyncComputeTaskPool::get_or_init(TaskPool::default);
        let (release, gate) = mpsc::channel::<u32>();
        let mut task = Task::spawn(ServiceMode::Background, move || gate.recv().unwrap_or(0));
        for _ in 0..20 {
            let polled = Instant::now();
            assert_eq!(task.poll(), None);
            assert!(polled.elapsed() < Duration::from_millis(50));
            std::thread::sleep(Duration::from_millis(1));
        }
        release.send(7).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(answer) = task.poll() {
                assert_eq!(answer, 7);
                break;
            }
            assert!(Instant::now() < deadline, "the task never finished");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
