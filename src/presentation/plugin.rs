use bevy::prelude::*;

use crate::model::{FrameSize, MapGrid, RenderBuffers, StaticLight};
use crate::schedule::{GamePhase, StartupPhase};

use super::{compose_frame, render_light_layers, DynamicLight};

/// Light-map processing and composition of a renderer-neutral cell frame.
pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_buffers.in_set(StartupPhase::Presentation))
            .add_systems(Update, render_light_layers.in_set(GamePhase::Lighting))
            .add_systems(Update, compose_frame.in_set(GamePhase::Presentation));
    }
}

/// The frame is `FrameSize` (insert one to change it), independent of the
/// map, and follows the player.
fn setup_buffers(mut commands: Commands, grid: Res<MapGrid>, size: Option<Res<FrameSize>>) {
    let mut static_light = StaticLight::new();
    static_light.resize(grid.width, grid.height);
    commands.insert_resource(static_light);
    commands.insert_resource(DynamicLight::sized((grid.width * grid.height) as usize));
    let size = size.map_or_else(FrameSize::default, |size| *size);
    commands.insert_resource(RenderBuffers::following(
        size.0,
        (grid.width * grid.height) as usize,
    ));
}
