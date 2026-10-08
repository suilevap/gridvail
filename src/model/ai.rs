use bevy::prelude::*;

use super::TraversalPrefs;
use crate::navigation::Path;
use crate::service::Services;

/// Picks random reachable floor cells to walk to, one after another.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Wander;

/// How far an enemy sees: the radius of its `VisualSensor`.
pub const ENEMY_SIGHT_RADIUS: i32 = 8;

/// How much of a cell an enemy must see to notice the player there, on the
/// scale of its field of view (`FovResult`): partly hidden cells count less.
/// The same as the player's own threshold for now.
pub const ENEMY_SIGHT_THRESHOLD: f32 = super::VISIBILITY_THRESHOLD;

/// Frames a thinking enemy keeps its turn open for its path, before it waits
/// out the turn and goes on thinking on its next one. Short, so a slow plan
/// never holds the player's next turn back for long.
pub const THINK_FRAMES: u8 = 6;

/// How far an idle enemy strolls from where it stands.
pub const STROLL_RADIUS: i32 = 5;

/// What an enemy's behavior tree reads: its view of the world this frame.
///
/// Refreshed by `ai::perceive` before the tree ticks. The tree reads it, and
/// asks services (paths) through it; what the enemy decides leaves through
/// `EnemyAct`.
#[derive(Component, Clone, Debug, Default)]
pub struct EnemyMind {
    pub pos: IVec2,
    /// A token is available and the turn accepts commands.
    pub has_turn: bool,
    /// The player's cell, while in sight.
    pub player: Option<IVec2>,
    /// Where the player was last seen; cleared on arrival.
    pub last_seen: Option<IVec2>,
    /// Random state for the tree, reseeded from the shared RNG each turn.
    pub seed: u32,
    /// A floor cell nearby, picked afresh each turn, for idle walks.
    pub stroll: Option<IVec2>,
    /// What walking costs this enemy: closed doors, cells others stand on.
    pub prefs: TraversalPrefs,
    /// The services to ask (paths, for now): one shared handle, handed over
    /// once.
    pub services: Option<Services>,
    /// Frames this turn spent thinking (see [`THINK_FRAMES`]).
    pub think_frames: u8,
}

impl EnemyMind {
    /// Aware of the player: seen now, or seen and not yet searched for.
    pub fn aware(&self) -> bool {
        self.last_seen.is_some()
    }

    pub fn sees_player(&self) -> bool {
        self.player.is_some()
    }

    pub fn next_to_player(&self) -> bool {
        self.player.is_some_and(|player| self.adjacent(player))
    }

    pub fn adjacent(&self, cell: IVec2) -> bool {
        (cell - self.pos).abs().element_sum() == 1
    }

    /// Xorshift over `seed`, so the tree draws without touching the world.
    pub fn next_random(&mut self) -> u32 {
        let mut x = self.seed.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        x
    }
}

/// What an enemy is doing this turn, and why.
///
/// Present only while the enemy's tree is running. `ai::carry_out` turns it
/// into a step; `ai::show_mood` into the enemy's glyph.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub enum EnemyAct {
    /// Just noticed the player: freezes for a beat.
    Alert,
    /// Bumping into the adjacent player.
    Attack(IVec2),
    /// Nothing to do this turn.
    #[default]
    Hold,
    /// Idle: standing still for a while.
    Rest,
    /// Waiting for a path to be planned, to walk it in this mood. Keeps the
    /// enemy's turn open (up to the turn's end) rather than spending it.
    Think(Mood),
    /// Walking a path, a step per turn.
    Move(Mood, Path),
}

/// Why an enemy walks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    /// To the visible player.
    Hunt,
    /// To where the player was last seen.
    Search,
    /// Idle: to a nearby cell.
    Patrol,
}

impl EnemyAct {
    /// The step this act takes from `pos` this turn (`ZERO` waits), or none
    /// yet: a thinking enemy keeps its turn.
    pub fn step(&self, pos: IVec2) -> Option<IVec2> {
        match self {
            Self::Attack(step) => Some(*step),
            Self::Alert | Self::Hold | Self::Rest => Some(IVec2::ZERO),
            Self::Think(_) => None,
            Self::Move(_, path) => {
                Some(path.next_after(pos).map_or(IVec2::ZERO, |next| next - pos))
            }
        }
    }
}
