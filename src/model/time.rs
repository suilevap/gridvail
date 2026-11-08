use bevy::prelude::*;

/// Turns an actor owes: each of its turns pays one off instead of being
/// played (its token is spent before anyone decides anything), until none
/// are left and `DebtPaid` says so. Anything can add to it with `Delay`:
/// winding up a weapon, a stun, a stagger. Whatever decides for the actor
/// never needs to know: an actor in debt has no turn.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Debt {
    pub turns: u8,
}

/// Adds `turns` to what `entity` owes.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delay {
    pub entity: Entity,
    pub turns: u8,
}

/// An actor paid off all it owed this turn.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebtPaid(pub Entity);

/// A message held on an entity until its debt is paid, then sent: an
/// action with a cast time is a `Delay` plus the action `Scheduled`. Any
/// message type can be scheduled once `time::release_scheduled::<M>` runs
/// for it.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Scheduled<M: Message + Clone>(pub M);

/// `message`, sent `secs` seconds from now: something shown that waits for
/// its moment on screen (a target's shake at the blow's contact, blood when
/// the blow lands). It is to seconds what `Scheduled` is to turns. Whatever
/// reads `M` never knows it waited. Any message type can wait, once the app
/// has `add_later_message::<M>()` (`cues::ShowLater`).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Later<M: Message> {
    pub secs: f32,
    pub message: M,
}

impl<M: Message> Later<M> {
    /// `message`, `secs` seconds from now.
    pub const fn after(secs: f32, message: M) -> Self {
        Self { secs, message }
    }

    /// `message`, as soon as what waits is sent.
    pub const fn now(message: M) -> Self {
        Self::after(0.0, message)
    }
}

/// The `Later` messages of type `M` still waiting, each with its seconds
/// left. A new map drops them: what was to be shown went with the old one.
#[derive(Resource, Debug)]
pub struct Waiting<M> {
    queue: Vec<(f32, M)>,
}

impl<M> Default for Waiting<M> {
    fn default() -> Self {
        Self { queue: Vec::new() }
    }
}

impl<M> Waiting<M> {
    /// Each waiting message, with its seconds left.
    pub fn iter(&self) -> impl Iterator<Item = (f32, &M)> {
        self.queue.iter().map(|(secs, message)| (*secs, message))
    }

    /// Takes one more message to wait `secs`.
    pub fn push(&mut self, secs: f32, message: M) {
        self.queue.push((secs, message));
    }

    /// `dt` seconds pass.
    pub fn age(&mut self, dt: f32) {
        for (secs, _) in &mut self.queue {
            *secs -= dt;
        }
    }

    /// Gives each message whose time has come to `send`, in the order
    /// they were asked for, and keeps the rest.
    pub fn release(&mut self, mut send: impl FnMut(M))
    where
        M: Clone,
    {
        self.queue.retain(|(secs, message)| {
            let due = *secs <= 0.0;
            if due {
                send(message.clone());
            }
            !due
        });
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}
