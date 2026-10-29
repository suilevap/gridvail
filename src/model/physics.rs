use bevy::prelude::*;

/// Knocks `entity` moving: its `Speed` becomes `velocity` (a direction
/// times cells), and it is `Knocked` until it stops. The simulation's
/// movement passes move it like any moving body, slowed by its `Friction`.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Knock {
    pub entity: Entity,
    pub velocity: IVec2,
}

/// Moving because it was knocked, not by choice: running into something
/// then is a `Collision`. Removed once it stops.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Knocked;

/// A knocked body was blocked: `mover` ran into `obstacle` (a wall, a door,
/// another body) and stopped.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Collision {
    pub mover: Entity,
    pub obstacle: Entity,
}

/// `follower` steps into the cell `into` once whatever stands there has
/// left it (on a later movement pass, as walking into a cell just left is
/// blocked in the same pass); if it is not left, it stays.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Follow {
    pub follower: Entity,
    pub into: IVec2,
}

/// `a` and `b` trade places at once (walking cannot swap two bodies).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swap {
    pub a: Entity,
    pub b: Entity,
}

/// Can be knocked and swapped about.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Movable;
