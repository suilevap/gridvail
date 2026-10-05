use bevy::prelude::*;

use super::follow_paths;
use crate::schedule::GamePhase;
use crate::simulation::move_commands;

/// Walks actors with a [`Destination`](crate::model::Destination) and a
/// [`PathFollow`](crate::model::PathFollow) there.
pub struct LocomotionPlugin;

impl Plugin for LocomotionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            follow_paths
                .in_set(GamePhase::Simulation)
                .before(move_commands),
        );
    }
}
