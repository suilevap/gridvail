//! Optional 3D wall backend layered under the text renderer.
//!
//! Each of the 16 autotile masks owns one combined mesh and every palette
//! entry owns one material. Wall entities only reference those shared assets,
//! which lets Bevy batch equal mesh/material pairs for GPU instancing.

use bevy::{camera::ClearColorConfig, prelude::*};

use crate::lighting::palette_color;
use crate::model::{Glyph, MapGrid, Pos, RenderBuffers, Wall};
use crate::schedule::{GamePhase, StartupPhase};
use crate::simulation::Rules;

use super::text::{grid_to_world, CELL_SIZE};

const WALL_STROKE: f32 = 3.5;
const WALL_DEPTH: f32 = 12.0;
const PALETTE_SIZE: usize = 16;

/// Installs extruded wall meshes beneath the normal text renderer.
///
/// The text renderer remains responsible for the HUD and every non-wall cell.
pub struct ExtrudedWallRendererPlugin;

impl Plugin for ExtrudedWallRendererPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            setup
                .in_set(StartupPhase::Renderer)
                .after(super::text::setup),
        )
        .add_systems(Update, sync_walls.in_set(GamePhase::Output));
    }
}

#[derive(Resource)]
pub(super) struct ExtrudedWallCells {
    symbols: Vec<char>,
}

impl ExtrudedWallCells {
    pub(super) fn symbol_at(&self, index: usize) -> Option<char> {
        self.symbols
            .get(index)
            .copied()
            .filter(|symbol| *symbol != '\0')
    }
}

#[derive(Resource)]
struct WallMaterials(Vec<Handle<StandardMaterial>>);

#[derive(Component)]
struct ExtrudedWall {
    cell_index: usize,
    symbol: char,
}

fn setup(
    mut commands: Commands,
    grid: Res<MapGrid>,
    rules: Res<Rules>,
    walls: Query<(&Pos, &Glyph), With<Wall>>,
    mut cameras_2d: Query<&mut Camera, With<Camera2d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for mut camera in &mut cameras_2d {
        camera.order = 1;
        camera.clear_color = ClearColorConfig::None;
    }

    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::AutoMin {
                min_width: (grid.width + 2) as f32 * CELL_SIZE.x,
                min_height: (grid.height + 4) as f32 * CELL_SIZE.y,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 0.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let wall_meshes: Vec<_> = (0..16).map(|mask| meshes.add(wall_mesh(mask))).collect();
    let wall_materials: Vec<_> = (0..PALETTE_SIZE)
        .map(|index| {
            materials.add(StandardMaterial {
                // The renderer-neutral frame already contains the original
                // CPU light result. Unlit PBR materials preserve that palette
                // exactly and avoid evaluating a second lighting model.
                base_color: palette_color(index as u8),
                unlit: true,
                ..default()
            })
        })
        .collect();

    let mut wall_cells = vec!['\0'; (grid.width * grid.height) as usize];
    for (position, glyph) in &walls {
        let Some(mask) = rules
            .wall
            .symbols
            .iter()
            .position(|symbol| *symbol == glyph.ch)
        else {
            continue;
        };
        let Some(cell_index) = grid.idx(position.0) else {
            continue;
        };
        wall_cells[cell_index] = glyph.ch;
        commands.spawn((
            ExtrudedWall {
                cell_index,
                symbol: glyph.ch,
            },
            Mesh3d(wall_meshes[mask].clone()),
            MeshMaterial3d(wall_materials[0].clone()),
            Transform::from_translation(grid_to_world(position.0, grid.width, grid.height)),
            Visibility::Hidden,
        ));
    }

    commands.insert_resource(ExtrudedWallCells {
        symbols: wall_cells,
    });
    commands.insert_resource(WallMaterials(wall_materials));
}

fn sync_walls(
    buffers: Res<RenderBuffers>,
    materials: Res<WallMaterials>,
    mut walls: Query<(
        &ExtrudedWall,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
    )>,
) {
    for (wall, mut material, mut visibility) in &mut walls {
        let cell = buffers.current[wall.cell_index];
        if cell == buffers.previous[wall.cell_index] {
            continue;
        }
        let visible = cell.ch == wall.symbol;
        let desired_visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != desired_visibility {
            *visibility = desired_visibility;
        }
        if visible {
            let desired = &materials.0[cell.color.min((PALETTE_SIZE - 1) as u8) as usize];
            if material.0 != *desired {
                material.0 = desired.clone();
            }
        }
    }
}

fn wall_mesh(mask: usize) -> Mesh {
    let mut mesh = Mesh::from(Cuboid::new(WALL_STROKE, WALL_STROKE, WALL_DEPTH));
    let horizontal_length = CELL_SIZE.x * 0.5 + WALL_STROKE * 0.5;
    let vertical_length = CELL_SIZE.y * 0.5 + WALL_STROKE * 0.5;

    let arms = [
        (
            1 << 0,
            Vec3::new(horizontal_length, WALL_STROKE, WALL_DEPTH),
            Vec3::new(CELL_SIZE.x * 0.25, 0.0, 0.0),
        ),
        (
            1 << 1,
            Vec3::new(WALL_STROKE, vertical_length, WALL_DEPTH),
            Vec3::new(0.0, CELL_SIZE.y * 0.25, 0.0),
        ),
        (
            1 << 2,
            Vec3::new(horizontal_length, WALL_STROKE, WALL_DEPTH),
            Vec3::new(-CELL_SIZE.x * 0.25, 0.0, 0.0),
        ),
        (
            1 << 3,
            Vec3::new(WALL_STROKE, vertical_length, WALL_DEPTH),
            Vec3::new(0.0, -CELL_SIZE.y * 0.25, 0.0),
        ),
    ];
    for (bit, size, translation) in arms {
        if mask & bit != 0 {
            mesh.merge(&Mesh::from(Cuboid::from_size(size)).translated_by(translation))
                .expect("cuboid wall meshes have compatible attributes");
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use crate::content::tile_rules::TileRule;

    #[test]
    fn every_wall_symbol_selects_its_mask_mesh() {
        let rule = TileRule::parse(include_str!("../../assets/rules/wall_rule.txt"));
        for (mask, symbol) in rule.symbols.iter().enumerate() {
            assert_eq!(
                rule.symbols
                    .iter()
                    .position(|candidate| candidate == symbol),
                Some(mask),
                "wall glyphs must uniquely identify their connectivity"
            );
        }
    }
}
