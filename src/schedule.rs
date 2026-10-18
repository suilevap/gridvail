//! Shared scheduling contract between the domain plugins and the concrete app.

use bevy::prelude::*;

/// Startup passes that require deferred entity/resource creation between them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StartupPhase {
    /// Load the selected content and spawn its initial entities.
    Content,
    /// Derive data such as wall tiles from the spawned world.
    Derive,
    /// Allocate map-sized buffers used to compose a renderer-neutral frame.
    Presentation,
    /// Create entities and resources owned by the selected renderer.
    Renderer,
}

/// Coarse update phases owned by the domain plugins.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamePhase {
    Simulation,
    /// Refresh pathfinding data from the simulated map.
    Navigation,
    FieldOfView,
    Lighting,
    Visibility,
    /// Turn whole-cell moves into continuous motion for display.
    Planning,
    Animation,
    Presentation,
    Output,
    Finalize,
}

/// Neutral ordering contract for systems composing an action. Lower-level
/// plugins do not need to import an AI or application's private functions.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TurnPhase {
    Input,
    Start,
    Decide,
    Follow,
    Attacks,
    Spend,
    Move,
    Damage,
    Cleanup,
}
