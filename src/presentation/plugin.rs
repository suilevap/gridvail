use bevy::prelude::*;

use crate::model::{CellMarks, FrameSize, MapGrid, RenderBuffers, StaticLight};
use crate::schedule::{GamePhase, MarkCells, StartupPhase};

use super::{compose_frame, render_light_layers, DynamicLight};

/// Light-map processing and composition of a renderer-neutral cell frame.
pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameSize>()
            .init_resource::<CellMarks>()
            .add_systems(Startup, setup_buffers.in_set(StartupPhase::Presentation))
            .add_systems(
                Update,
                clear_cell_marks
                    .before(MarkCells)
                    .in_set(GamePhase::Visibility),
            )
            .add_systems(Update, render_light_layers.in_set(GamePhase::Lighting))
            .add_systems(
                Update,
                (resize_frame, compose_frame)
                    .chain()
                    .in_set(GamePhase::Presentation),
            );
    }
}

/// Marks are listed afresh every frame.
fn clear_cell_marks(mut marks: ResMut<CellMarks>) {
    marks.0.clear();
}

/// The frame is `FrameSize` (change it to resize the frame), independent of
/// the map, and follows the player.
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

/// Rebuilds the frame when `FrameSize` changes, such as when a renderer fits
/// it to a larger window. The new frame is composed from scratch.
fn resize_frame(grid: Res<MapGrid>, size: Res<FrameSize>, buffers: Option<ResMut<RenderBuffers>>) {
    let Some(mut buffers) = buffers else {
        return;
    };
    let size = size.0.max(IVec2::ONE);
    if !buffers.follows_player || IVec2::new(buffers.width, buffers.height) == size {
        return;
    }
    *buffers = RenderBuffers::following(size, (grid.width * grid.height) as usize);
}
