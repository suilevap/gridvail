use bevy::prelude::*;

/// Hit points. Anything with health can be damaged; at zero it dies.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    pub hp: i32,
    pub max: i32,
}

impl Health {
    pub const fn full(max: i32) -> Self {
        Self { hp: max, max }
    }

    pub fn is_dead(&self) -> bool {
        self.hp <= 0
    }
}

/// Stays in the world when it dies (the player): whatever reacts to `Died`
/// decides what follows. Anything else that dies is destroyed.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct KeepOnDeath;

/// Damage to take: `amount` hit points off `target`, if it has health.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Damage {
    pub target: Entity,
    pub amount: i32,
}

/// Something's health ran out.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Died(pub Entity);
