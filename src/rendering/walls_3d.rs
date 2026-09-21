//! Optional perspective 3D wall backend layered under the text renderer.
//!
//! The 16 autotile masks own one combined mesh each and palette entries own
//! one material each. Wall entities share those assets for GPU instancing.

use bevy::{
    asset::RenderAssetUsages,
    camera::ClearColorConfig,
    image::ImageSampler,
    input::mouse::MouseWheel,
    mesh::VertexAttributeValues,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    shader::ShaderRef,
};

use crate::lighting::{light_to_palette, palette_color};
use crate::model::{Glyph, MapGrid, Player, Pos, RenderBuffers, Vis, VisibilityMap, Wall};
use crate::presentation::DynamicLight;
use crate::schedule::{GamePhase, StartupPhase};
use crate::simulation::Rules;

use super::text::{grid_to_world, MapCell, CELL_SIZE};

const WALL_STROKE: f32 = 3.5;
const WALL_HEIGHT: f32 = 16.0;
const FLOOR_DEPTH: f32 = 1.0;
const FLOOR_BRIGHTNESS: f32 = 0.35;
const BILLBOARD_CENTER_HEIGHT: f32 = 12.0;
const CAMERA_PITCH: f32 = 55.0_f32.to_radians();
const CAMERA_FOV: f32 = 50.0_f32.to_radians();
const MIN_CAMERA_DISTANCE: f32 = 150.0;
const MAX_CAMERA_DISTANCE: f32 = 650.0;
const PALETTE_SIZE: usize = 16;
const DUNGEON_SHADER: &str = "shaders/dungeon_material.wgsl";

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct DungeonMaterial {
    #[uniform(0)]
    color: LinearRgba,
    /// x/y: broad/fine variation strength, z/w: broad/fine frequency.
    #[uniform(1)]
    variation: Vec4,
    #[texture(2)]
    #[sampler(3)]
    light_map: Option<Handle<Image>>,
    /// xy: map dimensions, zw: cell dimensions; zero selects a solid surface.
    #[uniform(4)]
    map: Vec4,
    alpha_mode: AlphaMode,
}

impl Material for DungeonMaterial {
    fn fragment_shader() -> ShaderRef {
        DUNGEON_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
}

/// Adds perspective wall meshes beneath the text renderer.
pub struct ExtrudedWallRendererPlugin;

impl Plugin for ExtrudedWallRendererPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<DungeonMaterial>::default())
            .add_systems(
                Startup,
                setup
                    .in_set(StartupPhase::Renderer)
                    .after(super::text::setup),
            )
            .add_systems(
                Update,
                (move_camera, sync_walls, sync_floors, project_text_cells)
                    .chain()
                    .in_set(GamePhase::Output),
            );
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
struct WallMaterials(Vec<Handle<DungeonMaterial>>);

#[derive(Resource)]
struct FloorTexture {
    handle: Handle<Image>,
    pixels: Vec<u8>,
}

#[derive(Resource)]
struct CameraRig {
    focus: Vec3,
    yaw: f32,
    distance: f32,
}

#[derive(Component)]
struct ExtrudedWall {
    cell_index: usize,
    symbol: char,
}

#[derive(Component)]
struct WallCamera;

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    grid: Res<MapGrid>,
    rules: Res<Rules>,
    walls: Query<(&Pos, &Glyph), With<Wall>>,
    player: Single<&Pos, With<Player>>,
    mut cameras_2d: Query<(&mut Camera, &mut Projection), With<Camera2d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<DungeonMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    for (mut camera, mut projection) in &mut cameras_2d {
        camera.order = 1;
        camera.clear_color = ClearColorConfig::None;
        *projection = Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::WindowSize,
            ..OrthographicProjection::default_2d()
        });
    }

    let rig = CameraRig {
        focus: grid_to_world(player.0, grid.width, grid.height),
        yaw: -45.0_f32.to_radians(),
        distance: 280.0,
    };
    commands.spawn((
        WallCamera,
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: CAMERA_FOV,
            near: 1.0,
            far: 2_000.0,
            ..default()
        }),
        camera_transform(&rig),
    ));
    commands.insert_resource(rig);

    let wall_meshes: Vec<_> = (0..16).map(|mask| meshes.add(wall_mesh(mask))).collect();
    let wall_materials: Vec<_> = (0..PALETTE_SIZE)
        .map(|index| {
            materials.add(DungeonMaterial {
                // The composed frame already contains the CPU light result.
                color: palette_color(index as u8).to_linear(),
                variation: Vec4::new(0.18, 0.08, 0.026, 0.16),
                light_map: None,
                map: Vec4::ZERO,
                alpha_mode: AlphaMode::Opaque,
            })
        })
        .collect();
    let pixels = vec![0; (grid.width * grid.height) as usize * 4];
    let mut light_image = Image::new_fill(
        Extent3d {
            width: grid.width as u32,
            height: grid.height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    light_image.sampler = ImageSampler::linear();
    let light_map = images.add(light_image);
    let floor_material = materials.add(DungeonMaterial {
        color: LinearRgba::WHITE,
        variation: Vec4::new(0.18, 0.07, 0.020, 0.11),
        light_map: Some(light_map.clone()),
        map: Vec4::new(
            grid.width as f32,
            grid.height as f32,
            CELL_SIZE.x,
            CELL_SIZE.y,
        ),
        alpha_mode: AlphaMode::Blend,
    });
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(
            grid.width as f32 * CELL_SIZE.x,
            grid.height as f32 * CELL_SIZE.y,
            FLOOR_DEPTH,
        ))),
        MeshMaterial3d(floor_material),
        Transform::from_xyz(0.0, 0.0, -(FLOOR_DEPTH * 0.5 + 0.05)),
    ));

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
    commands.insert_resource(FloorTexture {
        handle: light_map,
        pixels,
    });
}

fn move_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    grid: Res<MapGrid>,
    player: Single<&Pos, With<Player>>,
    mut rig: ResMut<CameraRig>,
    mut camera: Single<&mut Transform, With<WallCamera>>,
) {
    let dt = time.delta_secs();
    let orbit = (keys.pressed(KeyCode::KeyE) as i8 - keys.pressed(KeyCode::KeyQ) as i8) as f32;
    rig.yaw += orbit * dt * 1.5;
    let wheel_delta: f32 = wheel.read().map(|event| event.y).sum();
    rig.distance =
        (rig.distance - wheel_delta * 18.0).clamp(MIN_CAMERA_DISTANCE, MAX_CAMERA_DISTANCE);

    let target = grid_to_world(player.0, grid.width, grid.height);
    let follow = 1.0 - (-8.0 * dt).exp();
    rig.focus = rig.focus.lerp(target, follow);
    if rig.focus.distance_squared(target) < 0.0001 {
        rig.focus = target;
    }
    let desired = camera_transform(&rig);
    if **camera != desired {
        **camera = desired;
    }
}

fn camera_transform(rig: &CameraRig) -> Transform {
    let horizontal = rig.distance * CAMERA_PITCH.cos();
    let offset = Vec3::new(
        rig.yaw.cos() * horizontal,
        rig.yaw.sin() * horizontal,
        rig.distance * CAMERA_PITCH.sin(),
    );
    Transform::from_translation(rig.focus + offset).looking_at(rig.focus, Vec3::Z)
}

fn sync_walls(
    buffers: Res<RenderBuffers>,
    materials: Res<WallMaterials>,
    mut walls: Query<(
        &ExtrudedWall,
        &mut MeshMaterial3d<DungeonMaterial>,
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

fn sync_floors(
    visibility_map: Single<&VisibilityMap, With<Player>>,
    light: Res<DynamicLight>,
    mut floor: ResMut<FloorTexture>,
    mut images: ResMut<Assets<Image>>,
) {
    let (pixels, remainder) = floor.pixels.as_chunks_mut::<4>();
    debug_assert!(remainder.is_empty());
    for (index, pixel) in pixels.iter_mut().enumerate() {
        let known = visibility_map
            .data
            .get(index)
            .is_some_and(|cell| cell.contains(Vis::KNOWN));
        let palette = light
            .data
            .get(index)
            .map(light_to_palette)
            .unwrap_or_default()
            .min((PALETTE_SIZE - 1) as u8);
        let color = floor_color(palette).to_srgba();
        pixel[0] = (color.red * 255.0).round() as u8;
        pixel[1] = (color.green * 255.0).round() as u8;
        pixel[2] = (color.blue * 255.0).round() as u8;
        pixel[3] = if known { 255 } else { 0 };
    }

    let needs_upload = images
        .get(&floor.handle)
        .and_then(|image| image.data.as_deref())
        != Some(floor.pixels.as_slice());
    if needs_upload {
        let FloorTexture { handle, pixels } = &*floor;
        if let Some(mut image) = images.get_mut(handle) {
            if let Some(data) = image.data.as_mut() {
                data.copy_from_slice(pixels);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn project_text_cells(
    grid: Res<MapGrid>,
    camera_3d: Single<(&Camera, &Transform), With<WallCamera>>,
    camera_2d: Single<(&Camera, &Transform), (With<Camera2d>, Without<WallCamera>)>,
    mut cells: Query<
        (&MapCell, &Text2d, &mut Transform, &mut Visibility),
        (Without<WallCamera>, Without<Camera2d>),
    >,
) {
    let (perspective, perspective_transform) = *camera_3d;
    let (overlay, overlay_transform) = *camera_2d;
    let perspective_global = GlobalTransform::from(*perspective_transform);
    let overlay_global = GlobalTransform::from(*overlay_transform);

    for (cell, text, mut transform, mut visibility) in &mut cells {
        let is_billboard = text.0.contains('\n');
        let height = if is_billboard {
            BILLBOARD_CENTER_HEIGHT
        } else {
            1.0
        };
        let ground = grid_to_world(cell.0, grid.width, grid.height) + Vec3::Z * height;
        let Ok(viewport) = perspective.world_to_viewport(&perspective_global, ground) else {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
            continue;
        };
        let Ok(projected) = overlay.viewport_to_world_2d(&overlay_global, viewport) else {
            continue;
        };

        let mut scale = perspective
            .world_to_viewport(&perspective_global, ground + Vec3::Y * CELL_SIZE.y)
            .map(|next| (next.distance(viewport) / CELL_SIZE.y).clamp(0.45, 2.0))
            .unwrap_or(1.0);
        if is_billboard {
            scale *= 0.65;
        } else if text.0.chars().count() > 1 {
            scale *= 0.75;
        }
        transform.translation.x = projected.x;
        transform.translation.y = projected.y;
        transform.scale = Vec3::splat(scale);
        if *visibility != Visibility::Visible {
            *visibility = Visibility::Visible;
        }
    }
}

fn wall_mesh(mask: usize) -> Mesh {
    let center = Vec3::new(0.0, 0.0, WALL_HEIGHT * 0.5);
    let mut mesh =
        Mesh::from(Cuboid::new(WALL_STROKE, WALL_STROKE, WALL_HEIGHT)).translated_by(center);

    for (bit, size, translation) in wall_arms() {
        if mask & bit != 0 {
            mesh.merge(&Mesh::from(Cuboid::from_size(size)).translated_by(translation))
                .expect("cuboid wall meshes have compatible attributes");
        }
    }
    add_face_shading(&mut mesh);
    mesh
}

fn wall_arms() -> [(usize, Vec3, Vec3); 4] {
    let horizontal_length = CELL_SIZE.x * 0.5 + WALL_STROKE * 0.5;
    let vertical_length = CELL_SIZE.y * 0.5 + WALL_STROKE * 0.5;
    [
        (
            1 << 0,
            Vec3::new(horizontal_length, WALL_STROKE, WALL_HEIGHT),
            Vec3::new(CELL_SIZE.x * 0.25, 0.0, WALL_HEIGHT * 0.5),
        ),
        // Map Y grows downward while world Y grows upward.
        (
            1 << 1,
            Vec3::new(WALL_STROKE, vertical_length, WALL_HEIGHT),
            Vec3::new(0.0, -CELL_SIZE.y * 0.25, WALL_HEIGHT * 0.5),
        ),
        (
            1 << 2,
            Vec3::new(horizontal_length, WALL_STROKE, WALL_HEIGHT),
            Vec3::new(-CELL_SIZE.x * 0.25, 0.0, WALL_HEIGHT * 0.5),
        ),
        (
            1 << 3,
            Vec3::new(WALL_STROKE, vertical_length, WALL_HEIGHT),
            Vec3::new(0.0, CELL_SIZE.y * 0.25, WALL_HEIGHT * 0.5),
        ),
    ]
}

/// Bake face shading into vertex colors. The palette material remains the
/// authoritative CPU-light color; this only reveals the mesh height.
fn add_face_shading(mesh: &mut Mesh) {
    let colors = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(normals)) => normals
            .iter()
            .map(|normal| {
                let brightness = if normal[2] > 0.5 { 1.0 } else { 0.55 };
                [brightness, brightness, brightness, 1.0]
            })
            .collect::<Vec<_>>(),
        _ => return,
    };
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
}

fn floor_color(index: u8) -> Color {
    let color = palette_color(index).to_linear();
    Color::linear_rgba(
        color.red * FLOOR_BRIGHTNESS,
        color.green * FLOOR_BRIGHTNESS,
        color.blue * FLOOR_BRIGHTNESS,
        color.alpha,
    )
}

pub(super) fn billboard_pattern(symbol: char) -> Option<&'static str> {
    match symbol {
        // A trailing hair space optically centers these asymmetric glyphs
        // without moving the body or changing the billboard anchor.
        '@' => Some("@ \n/|\\\n/ \\"),
        'x' => Some("x \n/|\\\n/ \\"),
        '>' => Some("> \n/|\\\n/ \\"),
        '<' => Some("< \n/|\\\n/ \\"),
        '^' => Some("^ \n/|\\\n/ \\"),
        'V' => Some("V \n/|\\\n/ \\"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{billboard_pattern, wall_arms};
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

    #[test]
    fn map_y_arms_are_flipped_into_world_y() {
        let arms = wall_arms();
        assert_eq!(arms[1].0, 1 << 1);
        assert!(arms[1].2.y < 0.0, "+map Y must extend toward -world Y");
        assert_eq!(arms[3].0, 1 << 3);
        assert!(arms[3].2.y > 0.0, "-map Y must extend toward +world Y");
    }

    #[test]
    fn player_billboard_is_the_requested_human_shape() {
        assert_eq!(billboard_pattern('@'), Some("@ \n/|\\\n/ \\"));
        assert!(billboard_pattern('?').is_none());
    }
}
