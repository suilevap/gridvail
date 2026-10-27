//! Concrete Gridvail game assembly.
//!
//! `GamePlugin` composes game content and domain logic. Rendering is installed
//! separately by the executable, so another renderer can consume the same
//! `RenderBuffers` without changing the game.

mod intro;
mod map;

pub use intro::{intro_arrival, Intro, IntroLayout, IntroPlugin};
pub use map::{bundled_index, bundled_map, CurrentMap, LoadMap, MapPlugin, MapText, BUNDLED_MAPS};

use bevy::prelude::*;

use crate::ai::AiPlugin;
use crate::camera::CameraPlugin;
use crate::locomotion::LocomotionPlugin;
use crate::navigation::NavigationPlugin;
use crate::presentation::PresentationPlugin;
use crate::schedule::{GamePhase, StartupPhase};
use crate::service::ServicesPlugin;
use crate::simulation::SimulationPlugin;
use crate::transition::TransitionPlugin;
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
                GamePhase::Navigation,
                GamePhase::FieldOfView,
                GamePhase::Lighting,
                GamePhase::Visibility,
                GamePhase::Animation,
                GamePhase::Presentation,
                GamePhase::Output,
                GamePhase::Finalize,
            )
                .chain(),
        )
        .add_plugins((
            MapPlugin,
            IntroPlugin,
            TransitionPlugin,
            SimulationPlugin::new(42),
            NavigationPlugin,
            ServicesPlugin,
            AiPlugin,
            LocomotionPlugin,
            VisionPlugin,
            PresentationPlugin,
            CameraPlugin,
        ));
    }
}
