use bevy::prelude::*;

/// Picks random reachable floor cells to walk to, one after another.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Wander;
