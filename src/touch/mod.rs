//! Touch controls, with no on-screen buttons: one finger swipes to walk,
//! two fingers pinch to zoom and twist to turn the view. Independent of
//! keyboard input; both drive the same player commands and camera.

#![allow(clippy::type_complexity)]

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::GamePhase;
use crate::simulation::{move_commands, player_input};

mod pinch;
mod swipe;

pub use pinch::*;
pub use swipe::*;

#[cfg(test)]
mod tests;

pub struct TouchControlPlugin;

impl Plugin for TouchControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Swipe>()
            .init_resource::<Pinch>()
            .init_resource::<ControlScheme>()
            .add_systems(
                PreUpdate,
                (read_swipes, read_pinch)
                    .after(bevy::input::InputSystems)
                    .run_if(resource_exists::<Touches>),
            )
            .add_systems(
                Update,
                swipe_commands
                    .after(player_input)
                    .before(move_commands)
                    .in_set(GamePhase::Simulation),
            );
    }
}
