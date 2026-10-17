//! Tactical action state, independent of input and rendering.

use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
pub struct Health {
    pub current: i32,
    pub maximum: i32,
    pub guarded: bool,
}

impl Health {
    pub const fn new(maximum: i32) -> Self {
        Self {
            current: maximum,
            maximum,
            guarded: false,
        }
    }

    /// Bracing absorbs one point of the next hit, then expires.
    pub fn hit(&mut self, damage: i32) -> i32 {
        let damage = (damage - i32::from(self.guarded)).max(0);
        self.guarded = false;
        self.current = (self.current - damage).max(0);
        damage
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BattleRole {
    #[default]
    Skirmisher,
    Spearman,
    ShieldBearer,
    Brute,
}

impl BattleRole {
    pub const fn health(self) -> i32 {
        match self {
            Self::Skirmisher | Self::Spearman => 3,
            Self::ShieldBearer => 4,
            Self::Brute => 5,
        }
    }

    pub const fn reach(self) -> i32 {
        match self {
            Self::Spearman => 2,
            _ => 1,
        }
    }

    pub const fn damage(self) -> i32 {
        match self {
            Self::Spearman => 2,
            Self::Brute => 3,
            _ => 1,
        }
    }

    pub const fn glyph(self) -> char {
        match self {
            Self::Skirmisher => 'e',
            Self::Spearman => 'S',
            Self::ShieldBearer => 'G',
            Self::Brute => 'B',
        }
    }

    pub const fn recovery_actions(self) -> u8 {
        match self {
            Self::Skirmisher => 1,
            Self::Brute => 3,
            _ => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BattleIntent {
    #[default]
    Idle,
    Move(IVec2),
    Strike {
        origin: IVec2,
        direction: IVec2,
        reach: i32,
        damage: i32,
    },
    WindUp {
        origin: IVec2,
        damage: i32,
    },
    Slam {
        origin: IVec2,
        damage: i32,
    },
    Recover,
}

impl BattleIntent {
    /// A lane or the eight neighbours of a slam; no per-turn allocation.
    pub fn cells(self) -> [Option<IVec2>; 8] {
        let mut cells = [None; 8];
        match self {
            Self::Strike {
                origin,
                direction,
                reach,
                ..
            } => {
                cells[0] = (reach > 0).then_some(origin + direction);
                cells[1] = (reach > 1).then_some(origin + direction * 2);
            }
            Self::WindUp { origin, .. } | Self::Slam { origin, .. } => {
                let mut index = 0;
                for y in -1..=1 {
                    for x in -1..=1 {
                        if x != 0 || y != 0 {
                            cells[index] = Some(origin + IVec2::new(x, y));
                            index += 1;
                        }
                    }
                }
            }
            _ => {}
        }
        cells
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Fighter {
    pub role: BattleRole,
    pub intent: BattleIntent,
    pub needs_plan: bool,
    /// Future recovery actions, in addition to the currently advertised one.
    pub recovery_left: u8,
    /// The spent attack stays visible while recovering, including on a miss.
    pub last_attack: BattleIntent,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct BattleStart(pub IVec2);

/// Present when the tactical rules are installed. Legacy simulations can use
/// GamePlugin alone; the playable executable also installs BattlePlugin.
#[derive(Resource, Debug, Default)]
pub struct BattleState {
    pub round: u64,
    pub committed: bool,
    pub defeated: bool,
    pub won: bool,
    pub restart_requested: bool,
    pub feedback: &'static str,
}

/// Shared by keyboard and stationary touch holds. Each hold commits once.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct WaitHold {
    pub elapsed: f32,
    pub latched: bool,
    pub touch_held: bool,
}

impl WaitHold {
    pub const SECONDS: f32 = 0.6;

    pub fn progress(&self) -> f32 {
        (self.elapsed / Self::SECONDS).clamp(0.0, 1.0)
    }

    pub fn update(&mut self, held: bool, ready: bool, seconds: f32) -> bool {
        if !held {
            self.elapsed = 0.0;
            self.latched = false;
            return false;
        }
        if self.latched {
            return false;
        }
        if !ready {
            self.elapsed = 0.0;
            return false;
        }
        self.elapsed = (self.elapsed + seconds).min(Self::SECONDS);
        if self.elapsed >= Self::SECONDS {
            self.latched = true;
            true
        } else {
            false
        }
    }
}
