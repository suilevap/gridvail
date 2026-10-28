use bevy::prelude::*;

/// Moves `entity` to the cell `to` at once, outside walking: knocked,
/// pulled, swapped. All the displacements of a frame are resolved together,
/// so two entities can swap and a chain can follow one another. A `forced`
/// one that is blocked is an `Impact`; an unforced one (following into a
/// cell) just stays.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Displace {
    pub entity: Entity,
    pub to: IVec2,
    pub forced: bool,
}

/// A forced displacement was blocked: `mover` was driven into `obstacle`
/// (a wall, a door, another actor) and stayed where it was.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Impact {
    pub mover: Entity,
    pub obstacle: Entity,
}
