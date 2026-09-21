use bevy::prelude::*;

use crate::model::StaticLight;
use crate::schedule::GamePhase;

use super::{compose_frame, flush_cells, render_light_layers, update_hud};

/// Light-map rendering, frame composition, and Bevy text output.
pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StaticLight>()
            .add_systems(Update, render_light_layers.in_set(GamePhase::Lighting))
            .add_systems(
                Update,
                (compose_frame, flush_cells, update_hud)
                    .chain()
                    .in_set(GamePhase::Presentation),
            );
    }
}
