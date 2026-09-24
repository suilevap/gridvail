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
    FieldOfView,
    Lighting,
    Visibility,
    Presentation,
    Output,
    Finalize,
}

/// Steps inside `GamePhase::Simulation`, so deciders plug in between input and
/// the movement pipeline without editing it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimulationStep {
    /// Advance the turn, recharge tokens, and read player input.
    Control,
    /// Non-player actors decide their commands.
    Decide,
    /// Spend commands, move, resolve collisions, and derive tiles.
    Resolve,
}
