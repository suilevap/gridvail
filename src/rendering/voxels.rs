//! A 3D world built of glyphs, under the perspective camera of the 3D wall
//! backend. Every glyph is solid: the font's 8 by 8 pixels pushed out into
//! little blocks, so it catches light on its edges and casts a shadow.
//! Walls are built of them (planks, and a `#` rim along the top), the
//! ground is strewn with small ones, plants grow in clumps, actors and
//! items stand up facing the camera, and every light source is a light.
//! It only reads the game, as the other renderers do; the text renderer
//! keeps the HUD, and its map is hidden.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::{
    asset::RenderAssetUsages,
    camera::{ClearColorConfig, Hdr},
    ecs::entity::EntityHashMap,
    image::{CompressedImageFormats, ImageSampler, ImageType},
    input::mouse::MouseWheel,
    light::GlobalAmbientLight,
    mesh::Indices,
    pbr::{DistanceFog, FogFalloff},
    platform::collections::{HashMap, HashSet},
    post_process::bloom::Bloom,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};

use crate::camera::CameraOperator;
use crate::lighting::palette_color;
use crate::model::{
    AnimatedPos, Glyph, LightKind, LightSource, MapGrid, Player, Pos, ViewCamera, Vis,
    VisibilityMap, Wall,
};
use crate::schedule::{GamePhase, StartupPhase};

use super::text::{grid_to_world_f, MapCell, ObjectSprite, CELL_SIZE};
use super::walls_3d::{camera_transform, CameraRig, WallCamera, CAMERA_FOV, WHEEL_ZOOM};

/// The game font's glyphs, 8 by 8 pixels each, 16 to a row, in the order
/// of `unscii-8-atlas.txt` (`tools/glyph_atlas.py` makes both).
const ATLAS: &[u8] = include_bytes!("../../assets/fonts/unscii-8-atlas.png");
const ATLAS_CHARS: &str = include_str!("../../assets/fonts/unscii-8-atlas.txt");

/// A glyph's side in world units: two glyphs to a cell.
const GLYPH: f32 = CELL_SIZE.x / 2.0;
/// How far a glyph's pixels stand out, in pixels.
const DEPTH: f32 = 1.5;

/// A wall's rows of glyphs, bottom first, each picked from its string: two
/// of planks, then the rim.
const WALL_ROWS: [&str; 3] = ["||I!|", "|=|+H", "#"];
/// How thick the dark core a wall's glyphs stand on is.
const WALL_CORE: f32 = 4.0;
/// How wide a lone wall's pillar is.
const PILLAR: f32 = 10.0;

/// Small glyphs lying on the ground.
const DEBRIS: &str = ".,'-`_:;";
/// Plants, growing in clumps, more of them along walls: a decoration of
/// this renderer, not in the game.
const PLANTS: &str = "YYY*\"v";
const PLANT_CHANCE: f32 = 0.22;
const PLANT_CHANCE_BY_WALL: f32 = 0.6;
/// Small trees of stacked `^`, out in the open.
const TREE_CHANCE: f32 = 0.04;
/// How far plants turn from facing the camera, at most, either way.
const PLANT_TURN: f32 = 1.2;
/// How far plants lean back from the camera, so they read from above.
const PLANT_LEAN: f32 = 0.45;

const GROUND: Color = Color::srgb(0.11, 0.1, 0.09);
const DUST: Color = Color::srgb(0.5, 0.43, 0.35);
const PLANK: Color = Color::srgb(0.46, 0.4, 0.34);
const RIM: Color = Color::srgb(0.72, 0.7, 0.8);
const CORE: Color = Color::srgb(0.07, 0.065, 0.06);
const GREENS: [Color; 4] = [
    Color::srgb(0.25, 0.6, 0.2),
    Color::srgb(0.42, 0.7, 0.2),
    Color::srgb(0.62, 0.72, 0.25),
    Color::srgb(0.18, 0.48, 0.3),
];

/// The night: a dim teal everywhere light does not reach.
const AMBIENT: Color = Color::srgb(0.35, 0.6, 0.65);
const AMBIENT_BRIGHTNESS: f32 = 220.0;
/// How much farther back than the 3D wall backend the camera stands, to
/// see a wide piece of the world.
const CAMERA_DISTANCE: f32 = 1.15;
const FOG: Color = Color::srgb(0.02, 0.045, 0.05);

pub struct VoxelRendererPlugin;

impl Plugin for VoxelRendererPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight {
            color: AMBIENT,
            brightness: AMBIENT_BRIGHTNESS,
            ..default()
        })
        .add_systems(
            Startup,
            setup
                .in_set(StartupPhase::Renderer)
                .after(super::text::setup),
        )
        .add_systems(
            Update,
            (move_camera, spawn_voxels, sync_voxels, hide_text_map)
                .chain()
                .in_set(GamePhase::Output)
                .after(super::text::place_object_sprites),
        );
    }
}

#[derive(Resource)]
struct Voxels {
    /// The font's glyphs, by character.
    bitmaps: HashMap<char, [u8; 8]>,
    /// Each glyph made solid, a unit tall.
    glyphs: HashMap<char, Handle<Mesh>>,
    /// A unit cube.
    cube: Handle<Mesh>,
    /// A cell of ground.
    tile: Handle<Mesh>,
    materials: HashMap<([u8; 4], u32), Handle<StandardMaterial>>,
    /// What stands for each game entity: its glyphs and blocks.
    shown: EntityHashMap<Vec<Entity>>,
    /// Each light source's light.
    lamps: EntityHashMap<Entity>,
}

/// One mesh of a thing, where it sits relative to the thing.
struct Piece {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    offset: Transform,
    /// Turned toward the camera, give or take this angle, if set.
    facing: Option<f32>,
}

impl Voxels {
    fn glyph(&mut self, glyph: char, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        if let Some(mesh) = self.glyphs.get(&glyph) {
            return mesh.clone();
        }
        let rows = self
            .bitmaps
            .get(&glyph)
            .or_else(|| self.bitmaps.get(&'?'))
            .copied()
            .unwrap_or_default();
        let mesh = meshes.add(glyph_mesh(rows));
        self.glyphs.insert(glyph, mesh.clone());
        mesh
    }

    /// A rough material of a colour, glowing `glow` times its colour.
    fn material(
        &mut self,
        color: Color,
        glow: f32,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        let key = (color.to_srgba().to_u8_array(), glow.to_bits());
        self.materials
            .entry(key)
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: color,
                    emissive: color.to_linear() * glow,
                    perceptual_roughness: 0.85,
                    ..default()
                })
            })
            .clone()
    }

    fn piece(
        &mut self,
        glyph: char,
        color: Color,
        offset: Transform,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Piece {
        Piece {
            mesh: self.glyph(glyph, meshes),
            material: self.material(color, 0.0, materials),
            offset,
            facing: None,
        }
    }

    /// A dark block of `size`, its bottom middle at `at`, turned `yaw`.
    fn block(
        &mut self,
        at: Vec3,
        size: Vec3,
        yaw: f32,
        materials: &mut Assets<StandardMaterial>,
    ) -> Piece {
        Piece {
            mesh: self.cube.clone(),
            material: self.material(CORE, 0.0, materials),
            offset: Transform::from_translation(at + Vec3::Z * size.z / 2.0)
                .with_rotation(Quat::from_rotation_z(yaw))
                .with_scale(size),
            facing: None,
        }
    }

    /// A wall at `cell`, joined to the walls toward `links` (world
    /// directions): a dark core with glyph planks on both faces and a rim
    /// of `#` along the top, or a pillar if it stands alone.
    fn wall(
        &mut self,
        cell: IVec2,
        links: &[Vec2],
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Vec<Piece> {
        let height = GLYPH * WALL_ROWS.len() as f32;
        let depth = DEPTH / 8.0 * GLYPH;
        let mut pieces = Vec::new();
        let mut salt = 0;
        let row_glyph = |row: usize, salt: &mut i32| {
            *salt += 1;
            let glyphs: Vec<char> = WALL_ROWS[row].chars().collect();
            let glyph = glyphs[(random(cell, *salt) * glyphs.len() as f32) as usize];
            let color = if row + 1 == WALL_ROWS.len() {
                RIM
            } else {
                PLANK
            };
            (glyph, shade(color, random(cell, *salt + 50)))
        };
        if links.is_empty() {
            pieces.push(self.block(
                Vec3::ZERO,
                Vec3::new(PILLAR, PILLAR, height - 1.0),
                0.0,
                materials,
            ));
            for face in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                for row in 0..WALL_ROWS.len() {
                    let (glyph, color) = row_glyph(row, &mut salt);
                    let at = (face * (PILLAR / 2.0 + depth / 2.0)).extend(row as f32 * GLYPH);
                    pieces.push(self.piece(
                        glyph,
                        color,
                        standing(at, face, GLYPH),
                        meshes,
                        materials,
                    ));
                }
            }
            return pieces;
        }
        pieces.push(self.block(
            Vec3::ZERO,
            Vec3::new(WALL_CORE + 1.0, WALL_CORE + 1.0, height),
            0.0,
            materials,
        ));
        for &along in links {
            let middle = along * GLYPH / 2.0;
            pieces.push(self.block(
                middle.extend(0.0),
                Vec3::new(GLYPH, WALL_CORE, height - 1.0),
                along.y.atan2(along.x),
                materials,
            ));
            let across = Vec2::new(along.y, -along.x);
            for face in [across, -across] {
                for row in 0..WALL_ROWS.len() {
                    let (glyph, color) = row_glyph(row, &mut salt);
                    let at = (middle + face * (WALL_CORE / 2.0 + depth / 2.0))
                        .extend(row as f32 * GLYPH);
                    pieces.push(self.piece(
                        glyph,
                        color,
                        standing(at, face, GLYPH),
                        meshes,
                        materials,
                    ));
                }
            }
        }
        pieces
    }
}

/// A glyph's pixels as blocks, `DEPTH` pixels deep: a unit wide (-0.5 to
/// 0.5 along X) and tall (0 to 1 along Y), facing +Z. Faces between
/// neighbouring pixels are left out.
fn glyph_mesh(rows: [u8; 8]) -> Mesh {
    let on = |x: i32, y: i32| {
        (0..8).contains(&x) && (0..8).contains(&y) && rows[y as usize] & (0x80 >> x) != 0
    };
    let half = DEPTH / 16.0;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face = |normal: [f32; 3], corners: [[f32; 3]; 4]| {
        let base = positions.len() as u32;
        positions.extend(corners);
        normals.extend([normal; 4]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    for y in 0..8 {
        for x in 0..8 {
            if !on(x, y) {
                continue;
            }
            let (x0, x1) = (x as f32 / 8.0 - 0.5, (x + 1) as f32 / 8.0 - 0.5);
            // Row 0 is the top.
            let (y0, y1) = ((7 - y) as f32 / 8.0, (8 - y) as f32 / 8.0);
            let (z0, z1) = (-half, half);
            face(
                [0.0, 0.0, 1.0],
                [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            );
            face(
                [0.0, 0.0, -1.0],
                [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            );
            if !on(x - 1, y) {
                face(
                    [-1.0, 0.0, 0.0],
                    [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
                );
            }
            if !on(x + 1, y) {
                face(
                    [1.0, 0.0, 0.0],
                    [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
                );
            }
            if !on(x, y - 1) {
                face(
                    [0.0, 1.0, 0.0],
                    [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
                );
            }
            if !on(x, y + 1) {
                face(
                    [0.0, -1.0, 0.0],
                    [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
                );
            }
        }
    }
    let uvs = vec![[0.0, 0.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// A glyph standing `size` tall on `at`, its face toward `face` (a world
/// direction across the ground).
fn standing(at: Vec3, face: Vec2, size: f32) -> Transform {
    // Stood up, the mesh faces -Y; then it turns about Z to `face`.
    let yaw = face.x.atan2(-face.y);
    Transform::from_translation(at)
        .with_rotation(Quat::from_rotation_z(yaw) * Quat::from_rotation_x(FRAC_PI_2))
        .with_scale(Vec3::splat(size))
}

/// A glyph `size` long lying on the ground at `at`, turned `yaw`.
fn lying(at: Vec3, yaw: f32, size: f32) -> Transform {
    let rotation = Quat::from_rotation_z(yaw);
    // Its middle, not its bottom edge, on `at`.
    let middle = rotation * Vec3::Y * size / 2.0;
    Transform::from_translation(at - middle + Vec3::Z * DEPTH / 16.0 * size)
        .with_rotation(rotation)
        .with_scale(Vec3::splat(size))
}

/// A colour a little darker or lighter, in four steps, by `chance` (0..1).
fn shade(color: Color, chance: f32) -> Color {
    let factor = 0.7 + (chance * 4.0).floor() * 0.15;
    let color = color.to_srgba();
    Color::srgb(
        color.red * factor,
        color.green * factor,
        color.blue * factor,
    )
}

/// Something of the map's decoration at a cell: ground, debris, plants.
#[derive(Component)]
struct Decor(IVec2);

/// Turns toward the camera across the ground, give or take this angle.
#[derive(Component)]
struct Facing(f32);

/// A piece of what stands for a game entity.
#[derive(Component)]
struct Shown {
    source: Entity,
    /// Where it sits relative to the entity's cell, for walls; actors and
    /// items stand up facing the camera instead.
    offset: Transform,
    wall: bool,
}

/// A light source's light.
#[derive(Component)]
struct Lamp {
    source: Entity,
    intensity: f32,
    flickers: bool,
}

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    grid: Res<MapGrid>,
    view: Res<ViewCamera>,
    player: Single<&Pos, With<Player>>,
    mut cameras_2d: Query<(Entity, &mut Camera), With<Camera2d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // The text renderer's camera keeps drawing the HUD, over the world.
    // Both cameras are HDR (for the world's bloom), so they draw into the
    // same picture.
    for (entity, mut camera) in &mut cameras_2d {
        camera.order = 1;
        camera.clear_color = ClearColorConfig::None;
        commands.entity(entity).insert(Hdr);
    }
    let rig = rig(
        &ViewCamera {
            position: player.0.as_vec2(),
            ..*view
        },
        &grid,
    );
    commands.spawn((
        WallCamera,
        Camera3d::default(),
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(FOG),
            ..default()
        },
        Hdr,
        Bloom {
            intensity: 0.25,
            ..Bloom::NATURAL
        },
        Projection::Perspective(PerspectiveProjection {
            fov: CAMERA_FOV,
            near: 1.0,
            far: 2_000.0,
            ..default()
        }),
        DistanceFog {
            color: FOG,
            falloff: FogFalloff::Linear {
                start: 380.0,
                end: 950.0,
            },
            ..default()
        },
        camera_transform(&rig),
    ));

    let mut voxels = Voxels {
        bitmaps: font_glyphs(),
        glyphs: HashMap::default(),
        cube: meshes.add(Cuboid::from_length(1.0)),
        tile: meshes.add(Plane3d::new(Vec3::Z, CELL_SIZE / 2.0)),
        materials: HashMap::default(),
        shown: EntityHashMap::default(),
        lamps: EntityHashMap::default(),
    };
    for y in 0..grid.height {
        for x in 0..grid.width {
            let cell = IVec2::new(x, y);
            let middle = grid_to_world_f(cell.as_vec2(), grid.width, grid.height);
            let mut decor = vec![Piece {
                mesh: voxels.tile.clone(),
                material: voxels.material(shade(GROUND, random(cell, 0)), 0.0, &mut materials),
                offset: Transform::default(),
                facing: None,
            }];
            if !grid.blocks_vision(cell) {
                decorate(
                    cell,
                    &grid,
                    &mut voxels,
                    &mut decor,
                    &mut meshes,
                    &mut materials,
                );
            }
            for piece in decor {
                let mut spawned = commands.spawn((
                    Decor(cell),
                    Mesh3d(piece.mesh),
                    MeshMaterial3d(piece.material),
                    piece
                        .offset
                        .with_translation(piece.offset.translation + middle),
                    Visibility::Hidden,
                ));
                if let Some(turn) = piece.facing {
                    spawned.insert(Facing(turn));
                }
            }
        }
    }
    commands.insert_resource(voxels);
}

/// Debris and plants for an open cell, the same every time.
fn decorate(
    cell: IVec2,
    grid: &MapGrid,
    voxels: &mut Voxels,
    decor: &mut Vec<Piece>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let spot = |salt: i32| {
        Vec3::new(random(cell, salt) - 0.5, random(cell, salt + 1) - 0.5, 0.0) * CELL_SIZE.x * 0.85
    };
    let pick = |glyphs: &str, chance: f32| {
        let glyphs: Vec<char> = glyphs.chars().collect();
        glyphs[(chance * glyphs.len() as f32) as usize]
    };
    let debris = (random(cell, 10) * 4.0) as i32 - 1;
    for n in 0..debris.max(0) {
        let salt = 20 + n * 5;
        let glyph = pick(DEBRIS, random(cell, salt + 2));
        let color = shade(DUST, random(cell, salt + 3));
        let size = 4.0 + random(cell, salt + 4) * 2.0;
        let at = lying(spot(salt), random(cell, salt + 4) * TAU, size);
        decor.push(voxels.piece(glyph, color, at, meshes, materials));
    }
    let by_wall = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y]
        .iter()
        .any(|&step| grid.blocks_vision(cell + step));
    let chance = if by_wall {
        PLANT_CHANCE_BY_WALL
    } else {
        PLANT_CHANCE
    };
    if random(cell, 40) < chance {
        let count = 1 + (random(cell, 41) * 4.0) as i32;
        for n in 0..count {
            let salt = 50 + n * 7;
            let glyph = pick(PLANTS, random(cell, salt));
            let color = shade(
                GREENS[(random(cell, salt + 1) * GREENS.len() as f32) as usize],
                random(cell, salt + 2),
            );
            let size = 6.0 + random(cell, salt + 3) * 5.0;
            let at = standing(spot(salt + 5), Vec2::NEG_Y, size);
            let mut plant = voxels.piece(glyph, color, at, meshes, materials);
            plant.facing = Some((random(cell, salt + 4) - 0.5) * PLANT_TURN);
            decor.push(plant);
        }
    } else if !by_wall && random(cell, 42) < TREE_CHANCE {
        // Stacked `^`, twice, crossed, so it has a shape from every side.
        let color = shade(GREENS[3], random(cell, 43));
        for (n, (size, lift)) in [(15.0, 0.0), (12.0, 6.0), (9.0, 11.0)]
            .into_iter()
            .enumerate()
        {
            for turn in [0.0, FRAC_PI_2] {
                let face = Vec2::from_angle(random(cell, 44) * TAU + turn);
                let at = standing(Vec3::Z * lift, face, size);
                decor.push(voxels.piece('^', shade(color, n as f32 * 0.3), at, meshes, materials));
            }
        }
    }
}

fn rig(view: &ViewCamera, grid: &MapGrid) -> CameraRig {
    let mut rig = CameraRig::of(view, grid);
    rig.distance *= CAMERA_DISTANCE;
    rig
}

/// Follows the view camera, as the 3D wall backend's camera does, from
/// farther back; the mouse wheel zooms.
fn move_camera(
    mut wheel: MessageReader<MouseWheel>,
    grid: Res<MapGrid>,
    view: Res<ViewCamera>,
    mut operator: ResMut<CameraOperator>,
    mut camera: Single<&mut Transform, With<WallCamera>>,
) {
    let wheel_delta: f32 = wheel.read().map(|event| event.y).sum();
    if wheel_delta != 0.0 {
        let zoom = operator.target_zoom() * (1.0 + wheel_delta * WHEEL_ZOOM);
        operator.zoom_to(zoom);
    }
    let desired = camera_transform(&rig(&view, &grid));
    if **camera != desired {
        **camera = desired;
    }
}

/// Reads the bundled font atlas into 8-row bitmaps by character.
fn font_glyphs() -> HashMap<char, [u8; 8]> {
    let atlas = Image::from_buffer(
        ATLAS,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        RenderAssetUsages::MAIN_WORLD,
    )
    .expect("the glyph atlas is a PNG");
    let width = atlas.width() as usize;
    let data = atlas.data.as_deref().unwrap_or_default();
    let on = |x: usize, y: usize| data.get((y * width + x) * 4 + 3).is_some_and(|&a| a > 127);
    ATLAS_CHARS
        .trim_end()
        .chars()
        .enumerate()
        .map(|(index, glyph)| {
            let (column, row) = (index % 16, index / 16);
            let mut rows = [0u8; 8];
            for (y, bits) in rows.iter_mut().enumerate() {
                for x in 0..8 {
                    if on(column * 8 + x, row * 8 + y) {
                        *bits |= 0x80 >> x;
                    }
                }
            }
            (glyph, rows)
        })
        .collect()
}

/// A number in 0..1 for a cell and a salt, the same every time.
fn random(cell: IVec2, salt: i32) -> f32 {
    let mut h = (cell.x as u32).wrapping_mul(0x9E37_79B1)
        ^ (cell.y as u32).wrapping_mul(0x85EB_CA77)
        ^ (salt as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65_536.0
}

/// Builds the new game entities in the world: walls of glyphs, other
/// glyphs standing, and lights as lights.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn spawn_voxels(
    mut commands: Commands,
    mut voxels: ResMut<Voxels>,
    glyphs: Query<(Entity, &Glyph, &Pos, Has<Wall>, Option<&LightSource>)>,
    walls: Query<&Pos, With<Wall>>,
    lights: Query<(Entity, &LightSource)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut wall_cells: Option<HashSet<IVec2>> = None;
    for (source, glyph, pos, wall, light) in &glyphs {
        if voxels.shown.contains_key(&source) {
            continue;
        }
        let pieces = if wall {
            let cells = wall_cells.get_or_insert_with(|| walls.iter().map(|pos| pos.0).collect());
            // Grid rows run down the screen, world Y up.
            let links: Vec<Vec2> = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y]
                .into_iter()
                .filter(|&step| cells.contains(&(pos.0 + step)))
                .map(|step| Vec2::new(step.x as f32, -step.y as f32))
                .collect();
            voxels.wall(pos.0, &links, &mut meshes, &mut materials)
        } else {
            // The player carries a light, but is not one.
            let (color, glow) = match light.filter(|light| light.kind != LightKind::None) {
                Some(light) => (lamp_color(light.kind as u8), 6.0),
                None => (palette_color(glyph.color), 0.15),
            };
            vec![Piece {
                mesh: voxels.glyph(glyph.ch, &mut meshes),
                material: voxels.material(color, glow, &mut materials),
                offset: Transform::default(),
                facing: None,
            }]
        };
        let shown = pieces
            .into_iter()
            .map(|piece| {
                commands
                    .spawn((
                        Shown {
                            source,
                            offset: piece.offset,
                            wall,
                        },
                        Mesh3d(piece.mesh),
                        MeshMaterial3d(piece.material),
                        Transform::default(),
                        Visibility::Hidden,
                    ))
                    .id()
            })
            .collect();
        voxels.shown.insert(source, shown);
    }
    for (source, light) in &lights {
        if !voxels.lamps.contains_key(&source) {
            let lamp = commands.spawn(lamp(source, light)).id();
            voxels.lamps.insert(source, lamp);
        }
    }
}

/// A light kind's colour, by its `LightKind` number.
fn lamp_color(kind: u8) -> Color {
    match kind {
        1 => Color::srgb(1.0, 0.55, 0.2),
        2 => Color::srgb(0.35, 0.75, 1.0),
        4 => Color::srgb(0.45, 1.0, 0.35),
        _ => Color::srgb(0.75, 0.85, 1.0),
    }
}

fn lamp(source: Entity, light: &LightSource) -> impl Bundle {
    let strength = match light.kind {
        LightKind::Fire => 1.4,
        LightKind::Electricity | LightKind::Acid => 0.8,
        LightKind::None => 0.5,
    };
    let intensity = light.value as f32 * strength * 6.0e6;
    (
        Lamp {
            source,
            intensity,
            flickers: light.kind == LightKind::Fire,
        },
        PointLight {
            color: lamp_color(light.kind as u8),
            intensity,
            range: (light.radius as f32 + 1.0) * CELL_SIZE.x * 1.5,
            radius: 2.0,
            shadow_maps_enabled: light.kind != LightKind::None,
            ..default()
        },
        Transform::default(),
    )
}

/// Moves every piece and light to where its entity is shown, shows what
/// the player knows of (moving things only while seen), and drops what
/// stood for entities that are gone.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn sync_voxels(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<MapGrid>,
    camera: Single<Ref<Transform>, With<WallCamera>>,
    mut voxels: ResMut<Voxels>,
    sources: Query<(&Pos, Option<&AnimatedPos>)>,
    viewer: Single<&VisibilityMap, With<Player>>,
    mut shown: Query<(&Shown, &mut Transform, &mut Visibility), Without<WallCamera>>,
    mut decor: Query<(&Decor, &mut Visibility), Without<Shown>>,
    mut facing: Query<(&Facing, &mut Transform), (Without<Shown>, Without<WallCamera>)>,
    mut lights: Query<
        (&Lamp, &mut PointLight, &mut Transform),
        (Without<Shown>, Without<Facing>, Without<WallCamera>),
    >,
) {
    let vis = |cell: IVec2| {
        grid.idx(cell)
            .and_then(|i| viewer.data.get(i))
            .copied()
            .unwrap_or(Vis::empty())
    };
    let shows = |show: bool| {
        if show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        }
    };
    for (decor, mut visibility) in &mut decor {
        visibility.set_if_neq(shows(vis(decor.0).contains(Vis::KNOWN)));
    }
    let place = |source: Entity| {
        sources.get(source).ok().map(|(pos, shown)| {
            let shown = shown.copied().unwrap_or(AnimatedPos::at(pos.0));
            (pos.0, shown)
        })
    };
    // Across the ground toward the camera.
    let back = (camera.rotation * Vec3::Z)
        .truncate()
        .normalize_or(Vec2::NEG_Y);
    if camera.is_changed() {
        for (facing, mut transform) in &mut facing {
            let face = Vec2::from_angle(facing.0).rotate(back);
            transform.rotation =
                standing(Vec3::ZERO, face, 1.0).rotation * Quat::from_rotation_x(-PLANT_LEAN);
        }
    }
    for (piece, mut transform, mut visibility) in &mut shown {
        let Some((cell, at)) = place(piece.source) else {
            continue;
        };
        let ground = grid_to_world_f(at.position, grid.width, grid.height);
        if piece.wall {
            visibility.set_if_neq(shows(vis(cell).contains(Vis::KNOWN)));
            // Rising and sinking walls come up out of the floor and go down
            // into it.
            let sunk = (1.0 - at.height) * GLYPH * WALL_ROWS.len() as f32;
            *transform = piece
                .offset
                .with_translation(piece.offset.translation + ground - Vec3::Z * sunk);
        } else {
            visibility.set_if_neq(shows(vis(cell).contains(Vis::VISIBLE)));
            // Facing the camera square on, so it reads as it does in text.
            let lift = Vec3::Z * at.lift * CELL_SIZE.y;
            *transform = Transform::from_translation(ground + lift)
                .with_rotation(camera.rotation)
                .with_scale(Vec3::splat(CELL_SIZE.y * 0.8));
        }
    }
    let now = time.elapsed_secs();
    for (lamp, mut light, mut transform) in &mut lights {
        if let Some((_, at)) = place(lamp.source) {
            transform.translation =
                grid_to_world_f(at.position, grid.width, grid.height) + Vec3::Z * CELL_SIZE.y * 1.2;
        }
        if lamp.flickers {
            let seed = lamp.source.index_u32() as f32;
            let flicker = (now * 7.0 + seed).sin() * 0.5 + (now * 13.3 + seed * 2.0).sin() * 0.5;
            light.intensity = lamp.intensity * (0.9 + 0.1 * flicker);
        }
    }
    let Voxels { shown, lamps, .. } = &mut *voxels;
    shown.retain(|source, shown| {
        let alive = sources.contains(*source);
        if !alive {
            for &entity in shown.iter() {
                commands.entity(entity).despawn();
            }
        }
        alive
    });
    lamps.retain(|source, lamp| {
        let alive = sources.contains(*source);
        if !alive {
            commands.entity(*lamp).despawn();
        }
        alive
    });
}

/// The world is drawn in 3D; the text renderer only keeps the HUD.
#[allow(clippy::type_complexity)]
fn hide_text_map(mut cells: Query<&mut Visibility, Or<(With<MapCell>, With<ObjectSprite>)>>) {
    for mut visibility in &mut cells {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_atlas_holds_the_fonts_glyphs() {
        let glyphs = font_glyphs();
        assert_eq!(glyphs.len(), ATLAS_CHARS.trim_end().chars().count());
        assert_eq!(glyphs[&'.'], [0, 0, 0, 0, 0, 0x18, 0x18, 0]);
        assert_eq!(glyphs[&'▒'][0], 0b1010_1010);
        let used = WALL_ROWS.concat() + DEBRIS + PLANTS + "^@";
        for glyph in used.chars() {
            assert!(glyphs[&glyph].iter().any(|&row| row != 0), "{glyph}");
        }
    }

    #[test]
    fn a_solid_glyph_has_faces_only_where_its_pixels_end() {
        // One pixel: a cube, six faces of two triangles.
        let one = glyph_mesh([0x80, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(one.indices().unwrap().len(), 6 * 6);
        // Two side by side share no face: ten faces.
        let two = glyph_mesh([0xC0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(two.indices().unwrap().len(), 10 * 6);
    }

    #[test]
    fn a_standing_glyph_faces_where_it_is_told() {
        for face in [Vec2::X, Vec2::NEG_Y, Vec2::new(0.6, 0.8)] {
            let transform = standing(Vec3::ZERO, face, 1.0);
            let normal = transform.rotation * Vec3::Z;
            assert!(normal.truncate().distance(face) < 1e-5, "{face}");
            assert!((transform.rotation * Vec3::Y).distance(Vec3::Z) < 1e-5);
        }
    }
}
