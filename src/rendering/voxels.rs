//! A 3D world of glyph voxels, under the perspective camera of the 3D wall
//! backend: walls are stacks of cubes with a glyph on every face, the floor
//! a glyph tile a cell, actors and items glyphs standing up facing the
//! camera, and every light source a light that lights and shadows them.
//! It only reads the game, as the other renderers do; the text renderer
//! keeps the HUD, and its map is hidden.

use bevy::{
    asset::RenderAssetUsages,
    camera::ClearColorConfig,
    ecs::entity::EntityHashMap,
    image::{CompressedImageFormats, ImageSampler, ImageType},
    light::GlobalAmbientLight,
    pbr::{DistanceFog, FogFalloff},
    platform::collections::HashMap,
    prelude::*,
};

use crate::lighting::palette_color;
use crate::model::{
    AnimatedPos, Glyph, LightKind, LightSource, MapGrid, Player, Pos, ViewCamera, Vis,
    VisibilityMap, Wall,
};
use crate::schedule::{GamePhase, StartupPhase};

use super::text::{grid_to_world_f, MapCell, ObjectSprite, CELL_SIZE};
use super::walls_3d::{camera_transform, CameraRig, WallCamera, CAMERA_FOV, WHEEL_ZOOM};
use crate::camera::CameraOperator;
use bevy::input::mouse::MouseWheel;

/// The game font's glyphs, 8 by 8 pixels each, 16 to a row, in the order
/// of `unscii-8-atlas.txt` (`tools/glyph_atlas.py` makes both).
const ATLAS: &[u8] = include_bytes!("../../assets/fonts/unscii-8-atlas.png");
const ATLAS_CHARS: &str = include_str!("../../assets/fonts/unscii-8-atlas.txt");

/// How many voxels high walls stand.
const WALL_VOXELS: usize = 2;
/// The glyph on every face of a wall voxel.
const WALL_GLYPH: char = '#';
/// The marks on the floor, one picked at random (the same at the same
/// place) for a cell, or none.
const FLOOR_GLYPHS: [char; 6] = ['.', ',', '`', '\'', '-', ':'];
const BARE_FLOOR: f32 = 0.45;
/// Plants standing on the floor here and there: a decoration of this
/// renderer, not in the game.
const PLANTS: [char; 4] = ['Y', '*', '^', '"'];
const PLANT_CHANCE: f32 = 0.16;
const PLANT_SIZE: f32 = 0.6;

/// The night: a dim teal everywhere light does not reach.
const AMBIENT: Color = Color::srgb(0.35, 0.6, 0.65);
const AMBIENT_BRIGHTNESS: f32 = 60.0;
/// How much farther back than the 3D wall backend the camera stands, to
/// see a wide piece of the world.
const CAMERA_DISTANCE: f32 = 2.0;
const FOG: Color = Color::srgb(0.02, 0.04, 0.05);

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

/// How a glyph is drawn on a voxel's face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Face {
    /// A wall's face: the glyph on stone.
    Stone,
    /// The ground: the glyph on earth.
    Ground,
    /// Standing up, its glyph only, in a colour (a palette index) that
    /// glows a little.
    Standing(u8),
    /// A light source's glyph, glowing in its light's colour.
    Glowing(u8),
    /// A plant: its glyph only, green.
    Plant,
}

#[derive(Resource)]
struct Voxels {
    cube: Handle<Mesh>,
    tile: Handle<Mesh>,
    upright: Handle<Mesh>,
    /// The font's glyphs, by character.
    glyphs: HashMap<char, [u8; 8]>,
    materials: HashMap<(char, Face), Handle<StandardMaterial>>,
    /// What stands for each game entity: its voxels or its glyph.
    shown: EntityHashMap<Vec<Entity>>,
    /// Each light source's light.
    lamps: EntityHashMap<Entity>,
}

impl Voxels {
    fn material(
        &mut self,
        glyph: char,
        face: Face,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        if let Some(material) = self.materials.get(&(glyph, face)) {
            return material.clone();
        }
        let rows = self
            .glyphs
            .get(&glyph)
            .or_else(|| self.glyphs.get(&'?'))
            .copied()
            .unwrap_or_default();
        let (on, off): ([u8; 4], [u8; 4]) = match face {
            Face::Stone => ([235, 225, 205, 255], [70, 66, 62, 255]),
            Face::Ground => ([150, 135, 115, 255], [34, 32, 30, 255]),
            Face::Standing(_) | Face::Glowing(_) | Face::Plant => ([255; 4], [0; 4]),
        };
        let mut pixels = Vec::with_capacity(8 * 8 * 4);
        for row in rows {
            for x in 0..8 {
                pixels.extend_from_slice(if row & (0x80 >> x) != 0 { &on } else { &off });
            }
        }
        let mut image = Image::new(
            bevy::render::render_resource::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            pixels,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::nearest();
        let texture = images.add(image);
        let material = match face {
            Face::Stone | Face::Ground => StandardMaterial {
                base_color_texture: Some(texture),
                perceptual_roughness: 0.95,
                ..default()
            },
            Face::Standing(_) | Face::Glowing(_) | Face::Plant => {
                let (color, glow) = match face {
                    Face::Plant => (Color::srgb(0.35, 0.75, 0.3).to_linear(), 0.04),
                    Face::Glowing(kind) => (lamp_color(kind).to_linear(), 8.0),
                    Face::Standing(color) => (palette_color(color).to_linear(), 0.35),
                    _ => unreachable!(),
                };
                StandardMaterial {
                    base_color: color.into(),
                    base_color_texture: Some(texture),
                    emissive: color * glow,
                    alpha_mode: AlphaMode::Mask(0.5),
                    cull_mode: None,
                    double_sided: true,
                    perceptual_roughness: 0.9,
                    ..default()
                }
            }
        };
        let handle = materials.add(material);
        self.materials.insert((glyph, face), handle.clone());
        handle
    }
}

/// A floor tile, at its cell.
#[derive(Component)]
struct FloorTile(IVec2);

/// A plant, standing on its cell.
#[derive(Component)]
struct Plant(IVec2);

/// A voxel of a wall, `layer` voxels up, or an actor's or item's glyph.
#[derive(Component)]
struct Shown {
    source: Entity,
    layer: usize,
    standing: bool,
}

/// A light source's light.
#[derive(Component)]
struct Lamp(Entity);

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    grid: Res<MapGrid>,
    view: Res<ViewCamera>,
    player: Single<&Pos, With<Player>>,
    mut cameras_2d: Query<&mut Camera, With<Camera2d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // The text renderer's camera keeps drawing the HUD, over the world.
    for mut camera in &mut cameras_2d {
        camera.order = 1;
        camera.clear_color = ClearColorConfig::None;
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
        Projection::Perspective(PerspectiveProjection {
            fov: CAMERA_FOV,
            near: 1.0,
            far: 2_000.0,
            ..default()
        }),
        DistanceFog {
            color: FOG,
            falloff: FogFalloff::Linear {
                start: 520.0,
                end: 1_200.0,
            },
            ..default()
        },
        camera_transform(&rig),
    ));

    let mut voxels = Voxels {
        cube: meshes.add(Cuboid::from_length(CELL_SIZE.x)),
        tile: meshes.add(Plane3d::new(Vec3::Z, CELL_SIZE / 2.0)),
        upright: meshes.add(Rectangle::from_size(CELL_SIZE)),
        glyphs: font_glyphs(),
        materials: HashMap::default(),
        shown: EntityHashMap::default(),
        lamps: EntityHashMap::default(),
    };
    for y in 0..grid.height {
        for x in 0..grid.width {
            let cell = IVec2::new(x, y);
            let chance = hash(cell);
            let glyph = if chance < BARE_FLOOR {
                ' '
            } else {
                FLOOR_GLYPHS[(hash(cell + 17) * FLOOR_GLYPHS.len() as f32) as usize]
            };
            let material = voxels.material(glyph, Face::Ground, &mut images, &mut materials);
            if hash(cell + 41) < PLANT_CHANCE && !grid.blocks_vision(cell) {
                let glyph = PLANTS[(hash(cell + 7) * PLANTS.len() as f32) as usize];
                let material = voxels.material(glyph, Face::Plant, &mut images, &mut materials);
                // A little off the cell's middle, so plants do not line up.
                let nudge = Vec2::new(hash(cell + 3) - 0.5, hash(cell + 5) - 0.5) * 0.5;
                commands.spawn((
                    Plant(cell),
                    Mesh3d(voxels.upright.clone()),
                    MeshMaterial3d(material),
                    Transform::from_translation(
                        grid_to_world_f(cell.as_vec2() + nudge, grid.width, grid.height)
                            + Vec3::Z * CELL_SIZE.y * PLANT_SIZE * 0.5,
                    )
                    .with_scale(Vec3::splat(PLANT_SIZE)),
                    Visibility::Hidden,
                ));
            }
            commands.spawn((
                FloorTile(cell),
                Mesh3d(voxels.tile.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(grid_to_world_f(
                    cell.as_vec2(),
                    grid.width,
                    grid.height,
                )),
                Visibility::Hidden,
            ));
        }
    }
    commands.insert_resource(voxels);
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

/// A number in 0..1 for a cell, the same every time.
fn hash(cell: IVec2) -> f32 {
    let mut h =
        (cell.x as u32).wrapping_mul(0x9E37_79B1) ^ (cell.y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65_536.0
}

/// Stands the new game entities up in the world: walls as voxel stacks,
/// other glyphs upright, and lights as lights.
#[allow(clippy::too_many_arguments)]
fn spawn_voxels(
    mut commands: Commands,
    mut voxels: ResMut<Voxels>,
    glyphs: Query<(Entity, &Glyph, Has<Wall>, Option<&LightSource>)>,
    lights: Query<(Entity, &LightSource)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (source, glyph, wall, light) in &glyphs {
        if voxels.shown.contains_key(&source) {
            continue;
        }
        let mut shown = Vec::new();
        if wall {
            let material = voxels.material(WALL_GLYPH, Face::Stone, &mut images, &mut materials);
            for layer in 0..WALL_VOXELS {
                shown.push(
                    commands
                        .spawn((
                            Shown {
                                source,
                                layer,
                                standing: false,
                            },
                            Mesh3d(voxels.cube.clone()),
                            MeshMaterial3d(material.clone()),
                            Transform::default(),
                            Visibility::Hidden,
                        ))
                        .id(),
                );
            }
        } else {
            // The player carries a light, but is not one.
            let face = match light.filter(|light| light.kind != LightKind::None) {
                Some(light) => Face::Glowing(light.kind as u8),
                None => Face::Standing(glyph.color),
            };
            let material = voxels.material(glyph.ch, face, &mut images, &mut materials);
            shown.push(
                commands
                    .spawn((
                        Shown {
                            source,
                            layer: 0,
                            standing: true,
                        },
                        Mesh3d(voxels.upright.clone()),
                        MeshMaterial3d(material),
                        Transform::default(),
                        Visibility::Hidden,
                    ))
                    .id(),
            );
        }
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
    (
        Lamp(source),
        PointLight {
            color: lamp_color(light.kind as u8),
            intensity: light.value as f32 * strength * 6.0e6,
            range: (light.radius as f32 + 1.0) * CELL_SIZE.x * 1.5,
            radius: 2.0,
            shadow_maps_enabled: light.kind != LightKind::None,
            ..default()
        },
        Transform::default(),
    )
}

/// Moves every voxel, glyph and light to where its entity is shown, shows
/// what the player knows of (moving things only while seen), and drops
/// what stood for entities that are gone.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn sync_voxels(
    mut commands: Commands,
    grid: Res<MapGrid>,
    camera: Single<&Transform, With<WallCamera>>,
    mut voxels: ResMut<Voxels>,
    sources: Query<(&Pos, Option<&AnimatedPos>, Has<Wall>)>,
    viewer: Single<&VisibilityMap, With<Player>>,
    mut shown: Query<(&Shown, &mut Transform, &mut Visibility), Without<WallCamera>>,
    mut tiles: Query<
        (&FloorTile, &mut Visibility),
        (Without<Shown>, Without<Plant>, Without<WallCamera>),
    >,
    mut lights: Query<(&Lamp, &mut Transform), (Without<Shown>, Without<WallCamera>)>,
    mut plants: Query<
        (&Plant, &mut Transform, &mut Visibility),
        (
            Without<Shown>,
            Without<Lamp>,
            Without<FloorTile>,
            Without<WallCamera>,
        ),
    >,
) {
    let vis = |cell: IVec2| {
        grid.idx(cell)
            .and_then(|i| viewer.data.get(i))
            .copied()
            .unwrap_or(Vis::empty())
    };
    for (tile, mut visibility) in &mut tiles {
        visibility.set_if_neq(if vis(tile.0).contains(Vis::KNOWN) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    // Upright glyphs turn to face the camera across the ground.
    let facing = (camera.translation.xy()).extend(0.0);
    let upright = |at: Vec3| {
        let toward = (facing - at).truncate();
        Quat::from_rotation_z(toward.y.atan2(toward.x) + std::f32::consts::FRAC_PI_2)
            * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)
    };
    for (plant, mut transform, mut visibility) in &mut plants {
        visibility.set_if_neq(if vis(plant.0).contains(Vis::KNOWN) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        transform.rotation = upright(transform.translation);
    }
    let place = |source: Entity| {
        sources.get(source).ok().map(|(pos, shown, wall)| {
            let shown = shown.copied().unwrap_or(AnimatedPos::at(pos.0));
            (pos.0, shown, wall)
        })
    };
    for (voxel, mut transform, mut visibility) in &mut shown {
        let Some((cell, at, wall)) = place(voxel.source) else {
            continue;
        };
        let seen = vis(cell);
        let show = if wall {
            seen.contains(Vis::KNOWN)
        } else {
            seen.contains(Vis::VISIBLE)
        };
        visibility.set_if_neq(if show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        let ground = grid_to_world_f(at.position, grid.width, grid.height);
        if voxel.standing {
            let lift = at.lift * CELL_SIZE.y;
            transform.translation = ground + Vec3::Z * (CELL_SIZE.y * 0.5 + lift);
            transform.rotation = upright(ground);
        } else {
            // Rising and sinking walls come up out of the floor and go down
            // into it.
            let sunk = (1.0 - at.height) * CELL_SIZE.y * WALL_VOXELS as f32;
            transform.translation =
                ground + Vec3::Z * (CELL_SIZE.y * (voxel.layer as f32 + 0.5) - sunk);
        }
    }
    for (lamp, mut transform) in &mut lights {
        if let Some((_, at, _)) = place(lamp.0) {
            transform.translation =
                grid_to_world_f(at.position, grid.width, grid.height) + Vec3::Z * CELL_SIZE.y * 1.2;
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
        for glyph in [WALL_GLYPH, '@']
            .into_iter()
            .chain(FLOOR_GLYPHS)
            .chain(PLANTS)
        {
            assert!(glyphs[&glyph].iter().any(|&row| row != 0), "{glyph}");
        }
    }
}
