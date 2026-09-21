use bevy::prelude::*;

use crate::schedule::GamePhase;

use super::{compute_fov, player_visibility, FovShared};

/// Cached field-of-view computation and player visibility projection.
pub struct VisionPlugin;

impl Plugin for VisionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FovShared>()
            .add_systems(Update, compute_fov.in_set(GamePhase::FieldOfView))
            .add_systems(Update, player_visibility.in_set(GamePhase::Visibility));
    }
}
