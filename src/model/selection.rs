use bevy::prelude::*;

/// Picks things by moving into them (the player).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Selector;

/// Can be picked: moving into it picks it instead of bumping it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Selectable;

/// What a selector has picked, and the direction it was picked in (from
/// the selector to it). Picking costs no turn; it stays picked across turns
/// while the target stands where it was picked, until cancelled or acted
/// on.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selected {
    pub target: Entity,
    pub dir: IVec2,
}

/// The second input on a picked target: `actor` acts on `target`, picked
/// in direction `picked`, with direction `dir`. It is the actor's action
/// for the turn. What it does depends on whoever answers it (maneuvers,
/// for now): the target, the actor's weapon, anything.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextAction {
    pub actor: Entity,
    pub target: Entity,
    pub picked: IVec2,
    pub dir: IVec2,
}

/// The player asked to drop whatever they were composing (a pick).
#[derive(Message, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CancelIntent;
