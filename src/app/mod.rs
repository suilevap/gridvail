//! Concrete Gridvail game assembly.
//!
//! `GamePlugin` composes game content and domain logic. Rendering is installed
//! separately by the executable, so another renderer can consume the same
//! `RenderBuffers` without changing the game.

mod map;

pub use map::MapPlugin;

use bevy::prelude::*;

use crate::ai::AiPlugin;
use crate::presentation::PresentationPlugin;
use crate::schedule::{GamePhase, StartupPhase};
use crate::simulation::SimulationPlugin;
use crate::vision::VisionPlugin;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Startup,
            (
                StartupPhase::Content,
                StartupPhase::Derive,
                StartupPhase::Presentation,
                StartupPhase::Renderer,
            )
                .chain(),
        )
        .configure_sets(
            Update,
            (
                GamePhase::Simulation,
                GamePhase::FieldOfView,
                GamePhase::Lighting,
                GamePhase::Visibility,
                GamePhase::Presentation,
                GamePhase::Output,
                GamePhase::Finalize,
            )
                .chain(),
        )
        .add_plugins((
            MapPlugin,
            SimulationPlugin::new(42),
            AiPlugin,
            VisionPlugin,
            PresentationPlugin,
        ));
    }
}
