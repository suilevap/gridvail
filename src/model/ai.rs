use bevy::prelude::*;

use super::WalkStatus;

/// Picks random reachable floor cells to walk to, one after another.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Wander;

/// Cells an enemy can see the player across, given a clear line.
pub const ENEMY_SIGHT_RADIUS: i32 = 8;

/// How many keys on the map an agent knows of, nearest first.
pub const KNOWN_KEYS: usize = 4;

/// How far an idle enemy strolls from where it stands.
pub const STROLL_RADIUS: i32 = 5;

/// What an enemy's behavior tree reads: its view of the world this frame.
///
/// Refreshed by `ai::perceive` before the tree ticks. The tree only reads it;
/// what the enemy decides leaves through `EnemyAct`, and walking is
/// locomotion's, which reports back here through `walk`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
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
    /// Locomotion's report on the current walk.
    pub walk: Option<Walk>,
    /// Where this agent has been told to go, if anywhere.
    pub order: Option<IVec2>,
    /// Carries a key.
    pub has_key: bool,
    /// Keys lying on the map, nearest first (as the crow flies, so a near
    /// one may still be out of reach).
    pub keys: [Option<IVec2>; KNOWN_KEYS],
}

/// Where an agent is walking, and how it is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Walk {
    pub goal: IVec2,
    pub status: WalkStatus,
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

    /// Where `dest` is now.
    pub fn resolve(&self, dest: Dest) -> Option<IVec2> {
        match dest {
            Dest::Order => self.order,
            Dest::Cell(cell) => Some(cell),
        }
    }

    /// How walking to `goal` is going, if that is the current walk.
    pub fn walk_to(&self, goal: IVec2) -> Option<WalkStatus> {
        self.walk
            .filter(|walk| walk.goal == goal)
            .map(|walk| walk.status)
    }

    /// Whether the walk to `goal` has ended without getting there.
    pub fn failed_to_reach(&self, goal: IVec2) -> bool {
        matches!(
            self.walk_to(goal),
            Some(WalkStatus::Unreachable | WalkStatus::Blocked)
        )
    }
}

/// A destination an act asks locomotion to walk to. `Order` is resolved when
/// the act is carried out, so a moving order needs no new decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dest {
    Order,
    Cell(IVec2),
}

/// Where an agent has been told to go. Whoever gives orders writes it; the
/// agent's tree decides how to get there.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Order {
    pub target: Option<IVec2>,
}

/// An enemy that follows its `Order`, fetching a key when a door is in the
/// way.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Hunter;

/// What an enemy is doing this turn, and why.
///
/// Present only while the enemy's tree is running. `ai::carry_out` turns it
/// into a single step or a `Destination` for locomotion; `ai::show_mood`
/// into the enemy's glyph.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnemyAct {
    /// Just noticed the player: freezes for a beat.
    Alert,
    /// Walking to the visible player's cell.
    Hunt(IVec2),
    /// Bumping into the adjacent player.
    Attack(IVec2),
    /// Nothing to do this turn.
    #[default]
    Hold,
    /// Walking to where the player was last seen.
    Search(IVec2),
    /// Idle: strolling to a nearby cell.
    Patrol(IVec2),
    /// Idle: standing still for a while.
    Rest,
    /// Walking to a destination.
    GoTo(Dest),
}

/// How an act moves the enemy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Movement {
    /// One step this turn (`ZERO` waits), ordered directly.
    Step(IVec2),
    /// A walk locomotion plans and carries out.
    WalkTo(Dest),
}

impl EnemyAct {
    pub fn movement(self) -> Movement {
        match self {
            Self::Attack(step) => Movement::Step(step),
            Self::Alert | Self::Hold | Self::Rest => Movement::Step(IVec2::ZERO),
            Self::Hunt(cell) | Self::Search(cell) | Self::Patrol(cell) => {
                Movement::WalkTo(Dest::Cell(cell))
            }
            Self::GoTo(dest) => Movement::WalkTo(dest),
        }
    }
}
