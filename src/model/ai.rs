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
    /// Where this agent has been told to go, if anywhere.
    pub order: Option<IVec2>,
    /// The navigation service's answer to the last `GoTo` this agent asked.
    pub route: Option<RouteAnswer>,
    /// Carries a key.
    pub has_key: bool,
    /// The closest key lying on the map.
    pub nearest_key: Option<IVec2>,
    /// Neighbours holding a closed door, indexed like `STEPS`.
    pub closed_doors: [bool; 4],
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

    pub fn adjacent(&self, cell: IVec2) -> bool {
        (cell - self.pos).abs().element_sum() == 1
    }

    /// A closed door at `cell`, as far as this agent can tell: only
    /// neighbouring doors are known.
    pub fn closed_door_at(&self, cell: IVec2) -> bool {
        STEPS
            .iter()
            .position(|&step| self.pos + step == cell)
            .is_some_and(|i| self.closed_doors[i])
    }

    /// Where `dest` is now.
    pub fn resolve(&self, dest: Dest) -> Option<IVec2> {
        match dest {
            Dest::Order => self.order,
            Dest::Cell(cell) => Some(cell),
        }
    }

    /// The navigation service's last answer about `dest`, if it was asked.
    pub fn route_to(&self, dest: Dest) -> Option<RouteStatus> {
        self.route
            .filter(|answer| answer.dest == dest)
            .map(|answer| answer.status)
    }

    /// The closed door the route to `dest` runs into, if any.
    pub fn door_toward(&self, dest: Dest) -> Option<IVec2> {
        match self.route_to(dest) {
            Some(RouteStatus::Blocked(door)) => Some(door),
            _ => None,
        }
    }
}

/// A destination for the navigation service. `Order` is resolved when the
/// service runs, so a moving order needs no new request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dest {
    Order,
    Cell(IVec2),
}

/// What the navigation service found for a `GoTo`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouteStatus {
    /// On the way; this turn's step leads there.
    #[default]
    Moving,
    /// Standing on the destination.
    Arrived,
    /// The best route runs through this closed door.
    Blocked(IVec2),
    /// The next cell is taken by another actor.
    Waiting,
    /// No route at all.
    Unreachable,
    /// A plan is on its way and there is nothing to follow meanwhile.
    Pending,
}

/// The service's answer, tagged with what was asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteAnswer {
    pub dest: Dest,
    pub status: RouteStatus,
}

/// Navigation state the service keeps per agent: the last request, its
/// answer, and the step it chose this turn.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Route {
    pub answer: Option<RouteAnswer>,
    pub step: IVec2,
}

/// Where an agent has been told to go. Whoever gives orders writes it; the
/// agent's tree decides how to get there.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Order {
    pub target: Option<IVec2>,
}

/// An enemy that follows its `Order`, opening doors on the way.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Hunter;

/// What an enemy is doing this turn, and why.
///
/// Present only while the enemy's tree is running. `ai::carry_out` turns it
/// into a `MoveCommand`; `ai::show_mood` into the enemy's glyph.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnemyAct {
    /// Just noticed the player: freezes for a beat.
    Alert,
    /// Closing on the visible player.
    Hunt(IVec2),
    /// Bumping into the adjacent player.
    Attack(IVec2),
    /// Nothing to do this turn: blocked, or waiting on a request.
    #[default]
    Hold,
    /// Heading to where the player was last seen.
    Search(IVec2),
    /// Idle: walking a straight stretch.
    Patrol(IVec2),
    /// Idle: standing still for a while.
    Rest,
    /// A request to the navigation service, which picks the step.
    GoTo(Dest),
    /// Walking into an adjacent door, which opens it with a key.
    Open(IVec2),
}

impl EnemyAct {
    /// This turn's step, or `None` when the navigation service picks it.
    pub fn step(self) -> Option<IVec2> {
        match self {
            Self::Hunt(step)
            | Self::Attack(step)
            | Self::Search(step)
            | Self::Patrol(step)
            | Self::Open(step) => Some(step),
            Self::Alert | Self::Hold | Self::Rest => Some(IVec2::ZERO),
            Self::GoTo(_) => None,
        }
    }
}
