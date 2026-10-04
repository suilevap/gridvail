use bevy::prelude::*;

use crate::model::ViewCamera;
use crate::schedule::GamePhase;

use super::{camera_controls, operate_camera, CameraOperator};

/// Owns the `ViewCamera` and the operator that moves it. The camera moves
/// after animation, so it follows objects where they are shown this frame.
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewCamera>()
            .init_resource::<CameraOperator>()
            .add_systems(
                Update,
                (camera_controls, operate_camera)
                    .chain()
                    .in_set(GamePhase::Presentation),
            );
    }
}
