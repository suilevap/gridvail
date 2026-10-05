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

/// Picks random reachable cells to walk to, one after another.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Wander;

/// Walks to `goal` along a planned path, one step per action.
///
/// Set `goal` (and clear `steps`) to send the actor somewhere; the path is
/// planned on its next action. `steps` holds the cells from where it was
/// planned to the goal; reserve it up front so planning never grows it.
#[derive(Component, Clone, Debug, Default)]
pub struct PathFollow {
    pub goal: Option<IVec2>,
    pub steps: Vec<IVec2>,
    /// Index in `steps` of the next cell to enter.
    pub next: usize,
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

    /// Sends the actor to `goal`, replanning on its next action.
    pub fn go_to(&mut self, goal: IVec2) {
        self.goal = Some(goal);
        self.clear_path();
    }

    /// Stops walking.
    pub fn stop(&mut self) {
        self.goal = None;
        self.clear_path();
    }

    fn clear_path(&mut self) {
        self.steps.clear();
        self.next = 0;
        self.ordered_from = None;
        self.blocked = 0;
    }
}
