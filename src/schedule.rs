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
    Animation,
    Presentation,
    Output,
    Finalize,
}

/// Systems that turn the player's input (keys, touch, agents) into this
/// frame's `MoveCommand`, so whatever reads those commands before they are
/// carried out (combat picking targets) runs after all of them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerCommands;
