use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
pub struct Player(pub usize);

#[derive(Component, Clone, Copy, Debug)]
pub struct Enemy;

#[derive(Component, Clone, Copy, Debug)]
pub struct Wall;

#[derive(Component, Clone, Copy, Debug)]
pub struct Lamp;

#[derive(Component, Clone, Copy, Debug)]
pub struct AcidPool;

#[derive(Component, Clone, Copy, Debug)]
pub struct ElectroField;

/// Participates in the occupancy grid. Bound decorations do not.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Collider;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Active;

/// Action budget. `count` is assigned from `recharge`, never added.
#[derive(Component, Clone, Copy, Debug)]
pub struct Tokens {
    pub count: i32,
    pub recharge: i32,
}

impl Tokens {
    pub const fn new(recharge: i32) -> Self {
        Self { count: 0, recharge }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveCommand {
    pub target: IVec2,
    pub relative: bool,
    pub active: bool,
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DestroyRequested;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DirectionBasedOnSpeed;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct DirectionTile {
    pub rule: String,
}

#[derive(Component, Clone, Debug, Default)]
pub struct Tile {
    pub mask: u8,
    pub rule: String,
}
