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
