use bevy::prelude::*;

/// Sets `entity` moving, outside walking: `velocity` is a direction times
/// a speed in cells. The body travels a cell at a time, losing its
/// `Friction` (1 without one) of speed per cell, until it stops or is
/// blocked. All of a frame's impulses move together, so bodies can swap
/// and follow one another. A `forced` one (a knock, a throw) that is
/// blocked is a `Collision`; an unforced one (stepping after a pushed body)
/// just stops.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Impulse {
    pub entity: Entity,
    pub velocity: IVec2,
    pub forced: bool,
}

/// A forced moving body was blocked: `mover` ran into `obstacle` (a wall,
/// a door, another body) with `speed` cells of motion left, and stopped.
/// A `Pushable` obstacle carries that motion on.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Collision {
    pub mover: Entity,
    pub obstacle: Entity,
    pub speed: i32,
}

/// Can be set moving by impulses (knocked, thrown).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Movable;

/// Takes on the motion of what runs into it: a body knocked into it stops,
/// and it moves on with the speed that was left.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Pushable;
