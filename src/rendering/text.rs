//! Bevy `Text2d` renderer for the renderer-neutral composed cell buffer.

use std::fmt::Write;

use bevy::prelude::*;

use crate::lighting::{palette_color, GRAY};
use crate::model::*;
use crate::schedule::{GamePhase, StartupPhase};

const CELL_SIZE: Vec2 = Vec2::new(12.0, 20.0);

#[derive(Component)]
struct HudText;

#[derive(Component, Clone, Copy)]
struct MapCell(IVec2);

/// Renders `RenderBuffers` through one Bevy text entity per map cell.
pub struct TextRendererPlugin;

impl Plugin for TextRendererPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup.in_set(StartupPhase::Renderer))
            .add_systems(
                Update,
                (flush_cells, update_hud).chain().in_set(GamePhase::Output),
            );
    }
}

fn setup(mut commands: Commands, grid: Res<MapGrid>, fonts: Option<ResMut<Assets<Font>>>) {
    let (width, height) = (grid.width, grid.height);
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::render::camera::ScalingMode::AutoMin {
                min_width: (width + 2) as f32 * CELL_SIZE.x,
                min_height: (height + 4) as f32 * CELL_SIZE.y,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));

    // The default Bevy font omits the box-drawing and marker glyphs. Headless
    // tests do not install font assets, so they use the default handle.
    let font = fonts
        .map(|mut fonts| {
            fonts.add(
                Font::try_from_bytes(
                    include_bytes!("../../assets/fonts/DejaVuSansMono.ttf").to_vec(),
                )
                .expect("bundled DejaVu font"),
            )
        })
        .unwrap_or_default();

    for y in 0..height {
        for x in 0..width {
            let position = IVec2::new(x, y);
            let mut cell_text = String::with_capacity(4);
            cell_text.push(' ');
            commands.spawn((
                MapCell(position),
                Text2d::new(cell_text),
                TextFont {
                    font: font.clone(),
                    font_size: 20.0,
                    ..default()
                },
                TextColor(palette_color(GRAY)),
                Transform::from_translation(grid_to_world(position, width, height)),
            ));
        }
    }

    commands.spawn((
        HudText,
        Text::new(String::with_capacity(192)),
        TextFont {
            font,
            font_size: 15.0,
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            right: Val::Px(8.0),
            ..default()
        },
    ));
}

fn flush_cells(
    buffers: Res<RenderBuffers>,
    mut cells: Query<(&MapCell, &mut Text2d, &mut TextColor)>,
) {
    for (cell, mut text, mut color) in cells.iter_mut() {
        let Some(index) = buffers.idx(cell.0) else {
            continue;
        };
        if buffers.current[index] != buffers.previous[index] {
            let cell = buffers.current[index];
            text.0.clear();
            text.0.push(if cell.ch == '\0' { ' ' } else { cell.ch });
            color.0 = palette_color(cell.color);
        }
    }
}

fn update_hud(
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    players: Query<(&Pos, &Tokens), With<Player>>,
    enemies: Query<Entity, (With<Enemy>, Without<DestroyRequested>)>,
    collisions: Res<CollisionBuffer>,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let (position, tokens) = players
        .iter()
        .next()
        .map(|(pos, tokens)| (pos.0, tokens.count))
        .unwrap_or((IVec2::ZERO, 0));
    for mut text in hud.iter_mut() {
        text.0.clear();
        write!(
            text.0,
            "PavEcsGame Lite Bevy port | arrows/WASD\nTick {} | {} | map {}x{} | player ({},{}) | tokens {} | enemies {} | bumps {}",
            turn.tick,
            turn.phase_name(),
            grid.width,
            grid.height,
            position.x,
            position.y,
            tokens,
            enemies.iter().count(),
            collisions.0.len(),
        )
        .expect("writing to String cannot fail");
    }
}

fn grid_to_world(position: IVec2, width: i32, height: i32) -> Vec3 {
    Vec3::new(
        (position.x as f32 - width as f32 / 2.0 + 0.5) * CELL_SIZE.x,
        (height as f32 / 2.0 - position.y as f32 - 0.5) * CELL_SIZE.y,
        0.0,
    )
}
