use bevy::prelude::*;

/// Where an actor should walk: locomotion's input. Anything may write it
/// (the AI's wandering, a chase, a player's click); [`PathFollow::status`]
/// reports how it went.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Destination {
    goal: Option<IVec2>,
    /// Counts requests, so asking for the same goal again (after being
    /// blocked, say) is a new request that locomotion plans afresh.
    request: u32,
}

impl Destination {
    /// Asks to walk to `goal`.
    pub fn go_to(&mut self, goal: IVec2) {
        self.goal = Some(goal);
        self.request = self.request.wrapping_add(1);
    }

    /// Asks to stop walking.
    pub fn stop(&mut self) {
        self.goal = None;
        self.request = self.request.wrapping_add(1);
    }

    pub fn goal(&self) -> Option<IVec2> {
        self.goal
    }

    /// Changes with every [`Destination::go_to`] or [`Destination::stop`].
    pub fn request(&self) -> u32 {
        self.request
    }
}

/// How an actor values ways through the map when its paths are planned.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraversalPrefs {
    /// Extra cost, in steps, of a path through a closed door (which spends
    /// a key); `None` keeps paths away from closed doors.
    pub door_cost: Option<u32>,
    /// Extra cost, in steps, of a path through a cell another actor stands
    /// on when it is planned, so walkers route around each other; `None`
    /// ignores actors, who will likely have moved by then.
    pub crowd_cost: Option<u32>,
}

/// How walking to the current [`Destination`] is going.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WalkStatus {
    /// No destination.
    #[default]
    Idle,
    /// On the way.
    Walking,
    /// At the goal.
    Arrived,
    /// No path to the goal (with the current [`TraversalPrefs`]).
    Unreachable,
    /// Gave up after too many steps that did not move the actor (another
    /// actor in the way).
    Blocked,
    /// A plan is on its way (see `locomotion::Planning`); the actor keeps to
    /// its old path meanwhile, or waits.
    Planning,
}

impl WalkStatus {
    /// Whether walking has ended, one way or another, until the next
    /// request.
    pub fn is_done(self) -> bool {
        matches!(self, Self::Arrived | Self::Unreachable | Self::Blocked)
    }
}

/// Locomotion's progress toward an actor's [`Destination`]: the path, the
/// position on it, and the outcome so far.
///
/// `steps` holds the cells from where the path was planned to the goal;
/// reserve it up front ([`PathFollow::with_capacity`]) so planning never
/// grows it.
#[derive(Component, Clone, Debug, Default)]
pub struct PathFollow {
    pub status: WalkStatus,
    pub steps: Vec<IVec2>,
    /// Index in `steps` of the next cell to enter.
    pub next: usize,
    /// The request and door cost the path was planned for; a different one
    /// means planning again.
    pub planned_for: Option<(u32, Option<u32>)>,
    /// Where the last step was ordered from, to notice blocked steps.
    pub ordered_from: Option<IVec2>,
    /// Consecutive steps that did not move the actor.
    pub blocked: u8,
}

impl PathFollow {
    /// Room for paths up to `cells` long, so following never allocates.
    pub fn with_capacity(cells: usize) -> Self {
        Self {
            steps: Vec::with_capacity(cells),
            ..Self::default()
        }
    }

    /// Forgets the current path, ending with `status`.
    pub fn finish(&mut self, status: WalkStatus) {
        self.status = status;
        self.steps.clear();
        self.next = 0;
        self.ordered_from = None;
        self.blocked = 0;
    }
}
