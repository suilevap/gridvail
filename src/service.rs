//! Services: work the game asks for (a path, a plan, an answer for an AI)
//! that may take longer than a frame.
//!
//! A service answers with a [`Promise`] (flatbt-bevy's), a plain [`Future`]: the
//! asker keeps it and polls it on later frames, and does something cheap with
//! what it already knows until the answer lands. Dropping it cancels the work.
//! Anything that polls futures can wait on one; behavior trees use
//! flatbt's [`flatbt_bevy::prelude::await_future`].
//!
//! How a service runs its work is the service's own business, its
//! [`Runner`] (also flatbt-bevy's): the asker does not change whether the
//! answer comes at once or frames later. [`Services`] is the one handle an agent keeps to reach every
//! service.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bevy::prelude::*;
pub use flatbt_bevy::{Promise, Runner};

use crate::navigation::{share_paths, PathService};
use crate::schedule::GamePhase;

/// An answer held back for some polls after it lands: deterministic slow
/// work, for tests and tuning (see [`PathService::delayed`]).
pub struct Delayed<F> {
    future: F,
    polls: u32,
}

impl<F> Delayed<F> {
    pub fn new(future: F, polls: u32) -> Self {
        Self { future, polls }
    }
}

impl<F: Future + Unpin> Future for Delayed<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        if self.polls > 0 {
            self.polls -= 1;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        Pin::new(&mut self.future).poll(cx)
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
    use bevy::tasks::{block_on, poll_once};

    #[test]
    fn delayed_answers_after_its_polls() {
        let mut promise = Delayed::new(Runner::Inline.run(|| 7), 2);
        assert_eq!(block_on(poll_once(&mut promise)), None);
        assert_eq!(block_on(poll_once(&mut promise)), None);
        assert_eq!(block_on(poll_once(&mut promise)), Some(7));
    }
}
