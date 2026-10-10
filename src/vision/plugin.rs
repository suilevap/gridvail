use bevy::prelude::*;

use crate::schedule::GamePhase;

use super::{compute_fov, compute_player_view, player_visibility, FovShared, PortalFovShared};

/// Cached field-of-view computation and player visibility projection.
pub struct VisionPlugin;

impl Plugin for VisionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FovShared>()
            .init_resource::<PortalFovShared>()
            .add_systems(
                Update,
                (compute_fov, compute_player_view).in_set(GamePhase::FieldOfView),
            )
            .add_systems(Update, player_visibility.in_set(GamePhase::Visibility));
    }
}
