use bevy::prelude::*;

/// A rail in the floor: room planners prefer walls to glide along rails
/// over bare floor. It lies flat, drawn with the wall shapes joined to the
/// rails next to it (`Tile::rail`), dimmed.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rail;

/// A floor cell that presses when an actor steps onto it and springs back
/// when it leaves. Each press switches every [`Room`] to its next layout.
/// It is a stand-in for trying rooms out until a trigger/action system,
/// where switching a room is one kind of action, replaces it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PressurePlate {
    pub pressed: bool,
}

impl PressurePlate {
    pub const GLYPH: char = '◘';
}

/// A rectangle of the map whose walls switch between layouts. It is not
/// placed on the map itself: its walls are ordinary `Wall` entities, spawned
/// and despawned as the layout changes. Rooms know nothing of what asks
/// them to switch: anything may call [`Room::advance`] or set `target`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Room {
    pub origin: IVec2,
    pub size: IVec2,
    /// The wall cells of each layout, in map coordinates.
    pub layouts: Vec<Vec<IVec2>>,
    /// The layout on the map.
    pub current: usize,
    /// The layout asked for. It replaces `current` once nothing stands
    /// where its new walls go.
    pub target: usize,
    /// The name of the strategy for how its walls get from one layout to
    /// the next (`RoomPlanners`); `None` uses the default one.
    pub planner: Option<String>,
}

impl Room {
    pub fn new(origin: IVec2, size: IVec2, layouts: Vec<Vec<IVec2>>) -> Self {
        Self {
            origin,
            size,
            layouts,
            current: 0,
            target: 0,
            planner: None,
        }
    }

    /// Uses the planner named `planner` (`RoomPlanners`) for its switches.
    pub fn with_planner(mut self, planner: Option<String>) -> Self {
        self.planner = planner;
        self
    }

    /// Asks for the layout after the one asked for last.
    pub fn advance(&mut self) {
        self.target = (self.target + 1) % self.layouts.len().max(1);
    }
}
