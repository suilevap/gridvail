//! Reusable attack, defence and damage data. No enemy archetypes or UI text.

use super::ActionTaken;
use bevy::prelude::*;
pub const MAX_ATTACK_CELLS: usize = 8;

#[derive(Component, Clone, Copy, Debug)]
pub struct Health {
    pub current: i32,
    pub maximum: i32,
}

impl Health {
    pub const fn new(maximum: i32) -> Self {
        Self {
            current: maximum,
            maximum,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Team(pub u8);

/// An attack's locked world-space footprint. Shared by AI, damage and display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttackArea {
    pub origin: IVec2,
    pub direction: IVec2,
    pub cells: [Option<IVec2>; MAX_ATTACK_CELLS],
    pub damage: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttackPhase {
    #[default]
    Idle,
    Windup(u8),
    Ready,
    Recovering(u8),
}

#[derive(Component, Clone, Copy, Debug, Default)]
#[require(AttackPoints, AttackExecuted, ActionTaken)]
pub struct AttackCycle {
    pub phase: AttackPhase,
    pub area: AttackArea,
    pub spent: AttackArea,
    /// Interruption has already advanced this cycle for this action.
    pub advanced_at: u64,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct AttackTiming {
    pub windup: u8,
    pub recovery: u8,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct AttackPower(pub i32);
#[derive(Component, Clone, Copy, Debug)]
#[require(AttackCycle)]
pub struct LineAttack {
    pub reach: u8,
}
#[derive(Component, Clone, Copy, Debug, Default)]
#[require(AttackCycle)]
pub struct NeighbourAttack;
#[derive(Component, Clone, Copy, Debug)]
pub struct Lunge {
    pub cells: i32,
}
/// Turn towards a newly locked attack; without it, movement owns facing.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct AimAtAttack;

/// A producer fills this; hit collection consumes it. No damage is applied here.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct AttackPoints {
    pub area: AttackArea,
    pub active: bool,
}
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct AttackExecuted {
    pub fired: bool,
    pub direction: IVec2,
}
#[derive(Component, Clone, Copy, Debug)]
#[require(AttackPoints, AttackExecuted, ActionTaken)]
pub struct BumpAttack {
    pub damage: i32,
}

/// Windows describe attack phases, never character types. Without a cycle,
/// Always and OutsideRecovery apply (e.g. a permanently guarded object).
#[derive(Clone, Copy, Debug)]
pub enum AttackWindow {
    Always,
    Preparing,
    Recovering,
    OutsideRecovery,
}
impl AttackWindow {
    pub fn includes(self, cycle: Option<&AttackCycle>) -> bool {
        let phase = cycle.map(|cycle| cycle.phase).unwrap_or_default();
        match self {
            Self::Always => true,
            Self::Preparing => matches!(phase, AttackPhase::Windup(_) | AttackPhase::Ready),
            Self::Recovering => matches!(phase, AttackPhase::Recovering(_)),
            Self::OutsideRecovery => !matches!(phase, AttackPhase::Recovering(_)),
        }
    }
}
#[derive(Component, Clone, Copy, Debug)]
pub struct DirectionalGuard {
    pub window: AttackWindow,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Armour {
    pub reduction: i32,
    pub window: AttackWindow,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Vulnerability {
    pub bonus: i32,
    pub window: AttackWindow,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct InterruptResistance {
    pub window: AttackWindow,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct InterruptOnHit {
    pub recovery: u8,
}
/// Defensive state is separate from health. A deliberate wait refreshes it;
/// a hit consumes one charge. No component insertion on ordinary turns.
#[derive(Component, Clone, Copy, Debug)]
pub struct Brace {
    pub reduction: i32,
    pub charges: u8,
}
