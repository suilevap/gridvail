//! Shared scheduling contract between the domain plugins and the concrete app.

use bevy::prelude::*;

/// Startup passes that require deferred entity/resource creation between them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StartupPhase {
    /// Load the selected content and spawn its initial entities.
    Content,
    /// Derive data such as wall tiles from the spawned world.
    Derive,
}

/// Coarse update phases owned by the domain plugins.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamePhase {
    Simulation,
    FieldOfView,
    Lighting,
    Visibility,
    Presentation,
    Finalize,
}
