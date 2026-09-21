//! Optional 3D wall backend layered under the text renderer.
//!
//! Each of the 16 autotile masks owns one combined mesh and every palette
//! entry owns one material. Wall entities only reference those shared assets,
//! which lets Bevy batch equal mesh/material pairs for GPU instancing.

use bevy::{camera::ClearColorConfig, mesh::VertexAttributeValues, prelude::*};

use crate::lighting::palette_color;
use crate::model::{Glyph, MapGrid, Pos, RenderBuffers, Wall};
use crate::schedule::{GamePhase, StartupPhase};
use crate::simulation::Rules;

use super::text::{grid_to_world, CELL_SIZE};

const WALL_STROKE: f32 = 3.5;
const WALL_HEIGHT: f32 = 16.0;
const VIEW_TILT_RADIANS: f32 = 25.0_f32.to_radians();
const VIEW_YAW_RADIANS: f32 = 18.0_f32.to_radians();
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

    let camera_distance = 500.0;
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
        Transform::from_xyz(
            camera_distance * VIEW_YAW_RADIANS.sin() * VIEW_TILT_RADIANS.cos(),
            -camera_distance * VIEW_TILT_RADIANS.sin(),
            camera_distance * VIEW_YAW_RADIANS.cos() * VIEW_TILT_RADIANS.cos(),
        )
        .looking_at(Vec3::ZERO, Vec3::Y),
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
        let screen_position = grid_to_world(position.0, grid.width, grid.height);
        // Invert the camera's ground-plane projection and anchor the top
        // surface at the original cell center. This keeps wall symbols aligned
        // with Text2d while height projects diagonally down-left.
        let height_offset = Vec2::new(
            -VIEW_YAW_RADIANS.sin(),
            VIEW_TILT_RADIANS.sin() * VIEW_YAW_RADIANS.cos(),
        ) * WALL_HEIGHT;
        let ground = screen_to_ground(screen_position.truncate() - height_offset);
        let translation = Vec3::new(ground.x, ground.y, 0.0);
        commands.spawn((
            ExtrudedWall {
                cell_index,
                symbol: glyph.ch,
            },
            Mesh3d(wall_meshes[mask].clone()),
            MeshMaterial3d(wall_materials[0].clone()),
            Transform::from_translation(translation),
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
    let center = Vec3::new(0.0, 0.0, WALL_HEIGHT * 0.5);
    let mut mesh =
        Mesh::from(Cuboid::new(WALL_STROKE, WALL_STROKE, WALL_HEIGHT)).translated_by(center);
    let horizontal_length = CELL_SIZE.x * 0.5 + WALL_STROKE * 0.5;
    let vertical_length = CELL_SIZE.y * 0.5 + WALL_STROKE * 0.5;

    let arms = [
        (
            1 << 0,
            Vec3::new(horizontal_length, WALL_STROKE, WALL_HEIGHT),
            Vec3::new(CELL_SIZE.x * 0.25, 0.0, WALL_HEIGHT * 0.5),
        ),
        (
            1 << 1,
            Vec3::new(WALL_STROKE, vertical_length, WALL_HEIGHT),
            Vec3::new(0.0, CELL_SIZE.y * 0.25, WALL_HEIGHT * 0.5),
        ),
        (
            1 << 2,
            Vec3::new(horizontal_length, WALL_STROKE, WALL_HEIGHT),
            Vec3::new(-CELL_SIZE.x * 0.25, 0.0, WALL_HEIGHT * 0.5),
        ),
        (
            1 << 3,
            Vec3::new(WALL_STROKE, vertical_length, WALL_HEIGHT),
            Vec3::new(0.0, -CELL_SIZE.y * 0.25, WALL_HEIGHT * 0.5),
        ),
    ];
    for (bit, size, translation) in arms {
        if mask & bit != 0 {
            mesh.merge(&Mesh::from(Cuboid::from_size(size)).translated_by(translation))
                .expect("cuboid wall meshes have compatible attributes");
        }
    }
    add_face_shading(&mut mesh);
    map_screen_shape_to_ground(&mut mesh);
    mesh
}

fn screen_to_ground(screen: Vec2) -> Vec2 {
    let x = screen.x / VIEW_YAW_RADIANS.cos();
    let y =
        (screen.y - VIEW_TILT_RADIANS.sin() * VIEW_YAW_RADIANS.sin() * x) / VIEW_TILT_RADIANS.cos();
    Vec2::new(x, y)
}

fn map_screen_shape_to_ground(mesh: &mut Mesh) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    for position in positions {
        let ground = screen_to_ground(Vec2::new(position[0], position[1]));
        position[0] = ground.x;
        position[1] = ground.y;
    }
}

/// Bake face shading into vertex colors. The material still supplies the
/// renderer-neutral CPU-light palette; this only reveals the mesh depth.
fn add_face_shading(mesh: &mut Mesh) {
    let colors = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(normals)) => normals
            .iter()
            .map(|normal| {
                let brightness = if normal[2] > 0.5 {
                    1.0
                } else if normal[1] < -0.5 {
                    0.42
                } else if normal[0] > 0.5 {
                    0.58
                } else {
                    0.65
                };
                [brightness, brightness, brightness, 1.0]
            })
            .collect::<Vec<_>>(),
        _ => return,
    };
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
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
