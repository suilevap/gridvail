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
/// carried out (picking targets) runs after all of them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerCommands;

/// The stages a turn's actions go through within `GamePhase::Simulation`,
/// in order, before the turn's moves. Each system names the stage it
/// belongs to, not the systems around it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionStage {
    /// Actions begun on an earlier turn go on (and may end), before anyone
    /// decides anything this turn.
    Ongoing,
    /// Actors decide what to do (after `PlayerCommands`).
    Decide,
    /// Decisions turn into effects: hits, knockbacks, weapons readied.
    Effects,
    /// Knocked things move.
    Movement,
    /// What landed turns into damage.
    Hits,
    /// Damage is taken.
    Damage,
}

/// Systems that list cells to mark for the player (`CellMarks`), after the
/// marks of the frame before are cleared.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MarkCells;
