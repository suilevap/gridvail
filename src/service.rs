//! Service jobs: work the game asks for (a path, a plan, an answer for an
//! AI) that may take longer than a frame.
//!
//! A service owns its work as a [`Job`]: everything it needs, by value or
//! behind an `Arc`, so it can run on another thread. Starting one returns a
//! [`Ticket`], which the service keeps with the agent and polls on later
//! frames. Until it lands, whoever asked does something cheap with what it
//! already knows, and the game goes on rendering and taking turns. Dropping a
//! ticket cancels its job, so an actor that despawns or changes its mind
//! leaves nothing running.
//!
//! [`ServiceMode`] decides where jobs run. `Inline` is the default because it
//! is deterministic: the same seed plays the same way, which the scenario
//! tests rely on. The game binary opts into `Background`.

use bevy::prelude::*;
use bevy::tasks::{block_on, poll_once, AsyncComputeTaskPool, Task};

/// Where service jobs run.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ServiceMode {
    /// In the frame that asked: the answer is there at once.
    #[default]
    Inline,
    /// On Bevy's async compute pool: the answer lands in whichever frame the
    /// job finishes, and nothing waits for it.
    Background,
    /// In the frame that asked, but handed over only after this many more
    /// polls. Deterministic slow thinking, for tests and tuning.
    Deferred(u32),
}

/// One unit of service work, owned so it can run anywhere.
pub trait Job: Send + 'static {
    type Output: Send + 'static;

    fn run(self) -> Self::Output;
}

/// A job in flight.
pub struct Ticket<T> {
    state: TicketState<T>,
}

enum TicketState<T> {
    Ready { value: T, polls_left: u32 },
    Running(Task<T>),
}

impl<T: Send + 'static> Ticket<T> {
    pub fn start<J: Job<Output = T>>(job: J, mode: ServiceMode) -> Self {
        let state = match mode {
            ServiceMode::Inline => TicketState::Ready {
                value: job.run(),
                polls_left: 0,
            },
            ServiceMode::Deferred(polls) => TicketState::Ready {
                value: job.run(),
                polls_left: polls,
            },
            ServiceMode::Background => {
                TicketState::Running(AsyncComputeTaskPool::get().spawn(async move { job.run() }))
            }
        };
        Self { state }
    }

    /// The answer, once it is ready. A ticket that answered is spent.
    pub fn poll(&mut self) -> Option<T>
    where
        T: Default,
    {
        match &mut self.state {
            TicketState::Ready { polls_left, value } => {
                if *polls_left > 0 {
                    *polls_left -= 1;
                    None
                } else {
                    Some(std::mem::take(value))
                }
            }
            TicketState::Running(task) => {
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

    /// Waits for the test to let it finish.
    struct Gated(mpsc::Receiver<u32>);

    impl Job for Gated {
        type Output = u32;

        fn run(self) -> u32 {
            self.0.recv().unwrap_or(0)
        }
    }

    struct Now(u32);

    impl Job for Now {
        type Output = u32;

        fn run(self) -> u32 {
            self.0
        }
    }

    #[test]
    fn inline_answers_at_once() {
        assert_eq!(Ticket::start(Now(7), ServiceMode::Inline).poll(), Some(7));
    }

    #[test]
    fn deferred_answers_after_its_polls() {
        let mut ticket = Ticket::start(Now(7), ServiceMode::Deferred(2));
        assert_eq!(ticket.poll(), None);
        assert_eq!(ticket.poll(), None);
        assert_eq!(ticket.poll(), Some(7));
    }

    /// A background job that takes its time leaves the caller free to go on
    /// polling, frame after frame, and answers once it is done.
    #[test]
    fn background_never_blocks_the_caller() {
        AsyncComputeTaskPool::get_or_init(TaskPool::default);
        let (release, gate) = mpsc::channel();
        let mut ticket = Ticket::start(Gated(gate), ServiceMode::Background);
        for _ in 0..20 {
            let polled = Instant::now();
            assert_eq!(ticket.poll(), None);
            assert!(polled.elapsed() < Duration::from_millis(50));
            std::thread::sleep(Duration::from_millis(1));
        }
        release.send(7).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(answer) = ticket.poll() {
                assert_eq!(answer, 7);
                break;
            }
            assert!(Instant::now() < deadline, "the job never finished");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
