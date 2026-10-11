use bevy::prelude::*;

/// A floor cell that presses when an actor steps onto it and springs back
/// when it leaves. Each press switches every [`Room`] to its next layout.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PressurePlate {
    pub pressed: bool,
}

impl PressurePlate {
    pub const GLYPH: char = '◘';
}

/// A rectangle of the map whose walls switch between layouts. It is not
/// placed on the map itself: its walls are ordinary `Wall` entities, spawned
/// and despawned as the layout changes.
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
}

impl Room {
    pub fn new(origin: IVec2, size: IVec2, layouts: Vec<Vec<IVec2>>) -> Self {
        Self {
            origin,
            size,
            layouts,
            current: 0,
            target: 0,
        }
    }

    /// Asks for the layout after the one asked for last.
    pub fn advance(&mut self) {
        self.target = (self.target + 1) % self.layouts.len().max(1);
    }
}
