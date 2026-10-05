use bevy::prelude::*;

/// Picks random reachable floor cells to walk to, one after another.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Wander;

/// Where an actor's AI wants to go, and what passing a closed door is
/// worth to it. Locomotion walks there and clears `goal` once it arrives or
/// gives up (no path, or blocked too often).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Destination {
    pub goal: Option<IVec2>,
    /// Extra cost, in steps, of a path through a closed door (which spends
    /// a key); `None` keeps paths away from closed doors.
    pub door_cost: Option<u32>,
}

impl Destination {
    /// Sends the actor to `goal`.
    pub fn go_to(&mut self, goal: IVec2) {
        self.goal = Some(goal);
    }
}
