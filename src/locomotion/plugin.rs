use bevy::prelude::*;

use super::follow_paths;
use crate::schedule::TurnPhase;

/// Walks actors with a [`Destination`](crate::model::Destination) and a
/// [`PathFollow`](crate::model::PathFollow) there, reporting the outcome.
pub struct LocomotionPlugin;

impl Plugin for LocomotionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::model::TurnMode>()
            .add_systems(Update, follow_paths.in_set(TurnPhase::Follow));
    }
}
