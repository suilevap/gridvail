use bevy::prelude::*;

/// Cells an enemy can see the player across, given a clear line.
pub const ENEMY_SIGHT_RADIUS: i32 = 8;

/// The four steps an enemy can take, in the order `EnemyMind::open` stores them.
pub const STEPS: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

/// What an enemy's behavior tree reads: its view of the world this frame.
///
/// Refreshed by `ai::perceive` before the tree ticks. The tree only reads it;
/// what the enemy decides leaves through `EnemyAct`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EnemyMind {
    pub pos: IVec2,
    /// A token is available and the turn accepts commands.
    pub has_turn: bool,
    /// The player's cell, while in sight.
    pub player: Option<IVec2>,
    /// Where the player was last seen; cleared on arrival.
    pub last_seen: Option<IVec2>,
    /// Neighbours free to step into (empty or the player), indexed like `STEPS`.
    pub open: [bool; 4],
    /// Random state for the tree, reseeded from the shared RNG each turn.
    pub seed: u32,
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
        self.player
            .is_some_and(|player| (player - self.pos).abs().element_sum() == 1)
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

    /// One open step that closes the distance to `target`, preferring the
    /// longer axis, or `None` when neither closing step is open.
    pub fn step_toward(&self, target: IVec2) -> Option<IVec2> {
        let delta = target - self.pos;
        let along_x = IVec2::new(delta.x.signum(), 0);
        let along_y = IVec2::new(0, delta.y.signum());
        let (first, second) = if delta.x.abs() >= delta.y.abs() {
            (along_x, along_y)
        } else {
            (along_y, along_x)
        };
        [first, second]
            .into_iter()
            .find(|&step| step != IVec2::ZERO && self.is_open(step))
    }

    pub fn is_open(&self, step: IVec2) -> bool {
        STEPS
            .iter()
            .position(|&s| s == step)
            .is_some_and(|i| self.open[i])
    }
}

/// What an enemy is doing this turn, and why.
///
/// Present only while the enemy's tree is running. `ai::carry_out` turns it
/// into a `MoveCommand`; `ai::show_mood` into the enemy's glyph.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyAct {
    /// Just noticed the player: freezes for a beat.
    Alert,
    /// Closing on the visible player.
    Hunt(IVec2),
    /// Bumping into the adjacent player.
    Attack(IVec2),
    /// Player in sight but no step closes in.
    Hold,
    /// Heading to where the player was last seen.
    Search(IVec2),
    /// Idle: walking a straight stretch.
    Patrol(IVec2),
    /// Idle: standing still for a while.
    Rest,
}

impl EnemyAct {
    pub fn step(self) -> IVec2 {
        match self {
            Self::Hunt(step) | Self::Attack(step) | Self::Search(step) | Self::Patrol(step) => step,
            Self::Alert | Self::Hold | Self::Rest => IVec2::ZERO,
        }
    }
}
