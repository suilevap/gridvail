use bevy::prelude::*;

/// Busy with an action that takes more than one turn: each of the actor's
/// turns goes to it (its token is spent before anyone decides anything)
/// until `turns` run out, then `ChannelDone` says it is complete. Whatever
/// began the action reacts to that; nothing else needs to know the actor
/// is busy.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channeling {
    pub turns: u8,
}

/// An actor finished channeling this turn.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelDone(pub Entity);
