use bevy::prelude::*;

/// Locomotion's progress along the path to an actor's
/// [`Destination`](super::Destination).
///
/// `steps` holds the cells from where the path was planned to the goal;
/// reserve it up front ([`PathFollow::with_capacity`]) so planning never
/// grows it.
#[derive(Component, Clone, Debug, Default)]
pub struct PathFollow {
    pub steps: Vec<IVec2>,
    /// Index in `steps` of the next cell to enter.
    pub next: usize,
    /// The goal and door cost the path was planned for; a different
    /// destination means planning again.
    pub planned_for: Option<(IVec2, Option<u32>)>,
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

    /// Forgets the current path.
    pub fn clear(&mut self) {
        self.steps.clear();
        self.next = 0;
        self.planned_for = None;
        self.ordered_from = None;
        self.blocked = 0;
    }
}
