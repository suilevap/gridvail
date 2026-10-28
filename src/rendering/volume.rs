//! Walls with height in the text renderer (`WallVolume`), drawn by a shader.
//!
//! The view looks down at a slant from the bottom of the screen, an oblique
//! projection: the floor stays as drawn, and height goes straight up the
//! screen (`Leaning`: up and to the right) whichever way the map is turned.
//!
//! Two passes on the GPU. First a camera of its own draws every wall into
//! the wall map, a texture of the frame at one texel per font pixel (8 per
//! cell): each wall is a sprite of its glyph at its animated position (so
//! sliding walls slide), coloured with its lit colour, with its height (how
//! far it has risen) as alpha. Then one full-screen quad draws the walls
//! standing: for each pixel the shader walks down from the tallest top
//! toward the floor, and at each height looks at the map point under it
//! (one pixel lower, along the lean) for wall ink that tall. The first found
//! is the nearest. So a wall is its glyph extruded, without seams at any
//! zoom or turn, and it turns with the map smoothly: the map is sampled in
//! map space. It writes the depth sprites are sorted by, so actors behind a
//! wall are hidden by it and actors in front of it are not.

use bevy::{
    asset::{embedded_asset, RenderAssetUsages},
    camera::{visibility::RenderLayers, ClearColorConfig, RenderTarget},
    platform::collections::HashMap,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{
        AsBindGroup, Extent3d, TextureDimension, TextureFormat, TextureUsages,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};
use swash::{
    scale::{Render, ScaleContext, Source},
    zeno::Format,
    FontRef,
};

use crate::model::{GlyphStatures, RenderBuffers, Stature, ViewCamera, WallVolume};
use crate::schedule::{GamePhase, StartupPhase};

use super::text::{ObjectSprite, TurnedGlyphs, CELL_FONT_BYTES, CELL_SIZE};

/// The tallest an object may stand, in cells: the wall map's alpha is its
/// stature (times how far it has risen) over this.
const MAX_STATURE: f32 = 4.0;
/// The font's design size: one atlas and wall map texel per font pixel.
const GLYPH_PX: usize = 8;
/// Atlas slots per side.
const ATLAS_SLOTS: usize = 16;
/// The render layer only the wall map camera sees.
const WALL_MAP_LAYER: usize = 1;
const SHADER: &str = "embedded://pav_ecs_game_bevy_port/rendering/wall_volume.wgsl";

/// Draws walls standing with the wall shader when `WallVolume` is not flat.
pub struct WallVolumePlugin;

impl Plugin for WallVolumePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "wall_volume.wgsl");
        app.add_plugins(Material2dPlugin::<WallVolumeMaterial>::default())
            .add_systems(
                Startup,
                setup
                    .in_set(StartupPhase::Renderer)
                    .after(super::text::setup),
            )
            .add_systems(
                Update,
                draw_wall_map
                    .in_set(GamePhase::Output)
                    .after(super::text::place_object_sprites),
            );
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct WallVolumeMaterial {
    /// xy: the map point at the screen centre, z: rotation, w: zoom.
    #[uniform(0)]
    camera: Vec4,
    /// xy: the view's offset in cells, zw: the lean of height on screen.
    #[uniform(1)]
    offset_lean: Vec4,
    /// xy: the frame's origin, zw: its size, in cells.
    #[uniform(2)]
    frame: Vec4,
    /// x: wall height in px at zoom 1, y: cell px, z: wall map texels per
    /// cell.
    #[uniform(3)]
    params: Vec4,
    /// The walls from above: lit colour times height, height as alpha.
    #[texture(4)]
    wall_map: Handle<Image>,
}

impl Material2d for WallVolumeMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    // Masked materials write depth, which the text sprites test against.
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Mask(0.5)
    }
}

#[derive(Resource)]
struct WallVolumeQuad {
    entity: Entity,
    material: Handle<WallVolumeMaterial>,
    /// The camera drawing the wall map, and the map's size in cells.
    map_camera: Entity,
    map_size: IVec2,
    /// Sprites drawing walls into the wall map, reused frame to frame.
    sprites: Vec<Entity>,
}

/// The cell font's glyphs, white with their ink as alpha, rasterized as
/// they are needed: the glyph of each slot by character.
#[derive(Resource)]
struct GlyphAtlas {
    slots: HashMap<char, usize>,
    handle: Handle<Image>,
}

fn wall_map_image(size: IVec2) -> Image {
    let size = size.max(IVec2::ONE) * GLYPH_PX as i32;
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x as u32,
            height: size.y as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    image
}

fn setup(
    mut commands: Commands,
    buffers: Res<RenderBuffers>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WallVolumeMaterial>>,
) {
    let map_size = IVec2::new(buffers.width, buffers.height);
    let wall_map = images.add(wall_map_image(map_size));
    let map_camera = commands
        .spawn((
            Camera2d,
            Camera {
                // Before the main camera, which reads the map.
                order: -1,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(wall_map.clone().into()),
            RenderLayers::layer(WALL_MAP_LAYER),
        ))
        .id();
    let material = materials.add(WallVolumeMaterial {
        camera: Vec4::ZERO,
        offset_lean: Vec4::ZERO,
        frame: Vec4::ZERO,
        params: Vec4::ZERO,
        wall_map,
    });
    // The main 2D camera never moves; the quad covers any window.
    let entity = commands
        .spawn((
            Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
            MeshMaterial2d(material.clone()),
            Transform::from_scale(Vec3::new(16384.0, 16384.0, 1.0)),
            Visibility::Hidden,
        ))
        .id();
    commands.insert_resource(WallVolumeQuad {
        entity,
        material,
        map_camera,
        map_size,
        sprites: Vec::new(),
    });
    let atlas_side = (GLYPH_PX * ATLAS_SLOTS) as u32;
    commands.insert_resource(GlyphAtlas {
        slots: HashMap::default(),
        handle: images.add(Image::new_fill(
            Extent3d {
                width: atlas_side,
                height: atlas_side,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[255, 255, 255, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        )),
    });
}

/// Rasterizes `ch` into atlas slot `slot`, without antialiasing.
fn rasterize(ch: char, slot: usize, atlas: &mut Image) {
    let Some(font) = FontRef::from_index(CELL_FONT_BYTES, 0) else {
        return;
    };
    let mut context = ScaleContext::new();
    let mut scaler = context
        .builder(font)
        .size(GLYPH_PX as f32)
        .hint(false)
        .build();
    let ascent = font.metrics(&[]).scale(GLYPH_PX as f32).ascent.round() as i32;
    let Some(image) = Render::new(&[Source::Outline])
        .format(Format::Alpha)
        .render(&mut scaler, font.charmap().map(ch))
    else {
        return;
    };
    let placement = image.placement;
    let Some(data) = atlas.data.as_mut() else {
        return;
    };
    let width = GLYPH_PX * ATLAS_SLOTS;
    let (slot_x, slot_y) = (slot % ATLAS_SLOTS * GLYPH_PX, slot / ATLAS_SLOTS * GLYPH_PX);
    for row in 0..placement.height as i32 {
        for col in 0..placement.width as i32 {
            let x = placement.left + col;
            let y = ascent - placement.top + row;
            if !(0..GLYPH_PX as i32).contains(&x) || !(0..GLYPH_PX as i32).contains(&y) {
                continue;
            }
            let coverage = image.data[(row as u32 * placement.width + col as u32) as usize];
            let texel = (slot_y + y as usize) * width + slot_x + x as usize;
            data[texel * 4 + 3] = if coverage >= 128 { 255 } else { 0 };
        }
    }
}

/// Which way height goes on screen, per unit of height.
fn lean(volume: WallVolume) -> Vec2 {
    match volume {
        WallVolume::Leaning => Vec2::new(0.5, 1.0),
        _ => Vec2::Y,
    }
}

/// Draw order for a sprite seen at a slant: further along the lean is
/// further away, nearer is drawn over what is behind it, then deeper glyphs
/// on top within a cell. It stays between the ground (0) and the HUD. The
/// shader gives walls the same order.
pub(super) fn depth_order(sprite: &ObjectSprite, camera: &ViewCamera, volume: WallVolume) -> f32 {
    let base = super::text::object_translation(sprite, camera);
    1.5 - base.truncate().dot(lean(volume)) / 16384.0 + sprite.depth as f32 * 1e-5
}

/// Places a sprite for each wall in the frame for the wall map camera, and
/// passes the view to the shader.
#[allow(clippy::too_many_arguments)]
fn draw_wall_map(
    mut commands: Commands,
    volume: Res<WallVolume>,
    camera: Res<ViewCamera>,
    buffers: Res<RenderBuffers>,
    turned: Res<TurnedGlyphs>,
    mut quad: ResMut<WallVolumeQuad>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<WallVolumeMaterial>>,
    statures: Res<GlyphStatures>,
    own_statures: Query<&Stature>,
    mut sprites: Query<(&mut Sprite, &mut Transform, &mut Visibility)>,
    mut quads: Query<&mut Visibility, (With<Mesh2d>, Without<Sprite>)>,
    mut cameras: Query<(&mut RenderTarget, &mut Transform), Without<Sprite>>,
) {
    let shown = *volume != WallVolume::Flat;
    let size = IVec2::new(buffers.width, buffers.height);
    let quad = &mut *quad;
    if let Ok(mut visibility) = quads.get_mut(quad.entity) {
        visibility.set_if_neq(if shown {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    // The map camera looks at the frame, one world unit per texel: cell
    // (x, y) is the square from (x, -y) to (x + 1, -(y + 1)) times 8, from
    // the frame's origin.
    let texels = GLYPH_PX as f32;
    if let Ok((mut target, mut transform)) = cameras.get_mut(quad.map_camera) {
        if quad.map_size != size {
            quad.map_size = size;
            let wall_map = images.add(wall_map_image(size));
            *target = RenderTarget::Image(wall_map.clone().into());
            if let Some(mut material) = materials.get_mut(&quad.material) {
                material.wall_map = wall_map;
            }
        }
        let centre = Vec3::new(size.x as f32, -size.y as f32, 0.0) * texels / 2.0;
        if transform.translation != centre {
            transform.translation = centre;
        }
    }

    let mut used = 0;
    let mut tallest: f32 = 0.0;
    if shown {
        for object in &buffers.objects {
            let stature = statures
                .of(object.cell.ch, own_statures.get(object.entity).ok())
                .min(MAX_STATURE);
            if stature <= 0.0 {
                continue;
            }
            tallest = tallest.max(stature);
            // In map space: only a portal's turn, not the view's.
            let ch = turned.turned(object.cell.ch, object.quarters as i32);
            let next = atlas.slots.len();
            let slot = match atlas.slots.get(&ch) {
                Some(&slot) => slot,
                None if next < ATLAS_SLOTS * ATLAS_SLOTS => {
                    if let Some(mut image) = images.get_mut(&atlas.handle) {
                        rasterize(ch, next, &mut image);
                    }
                    atlas.slots.insert(ch, next);
                    next
                }
                None => continue,
            };
            let corner = Vec2::new(
                (slot % ATLAS_SLOTS * GLYPH_PX) as f32,
                (slot / ATLAS_SLOTS * GLYPH_PX) as f32,
            );
            let lit = Color::from(object.cell.color).to_linear();
            let risen = stature * object.height.clamp(0.0, 1.0);
            let color = Color::LinearRgba(lit.with_alpha(risen / MAX_STATURE));
            let at = (object.position - buffers.origin.as_vec2() + 0.5) * texels;
            let translation = Vec3::new(at.x, -at.y, 0.0);
            let sprite = Sprite {
                image: atlas.handle.clone(),
                rect: Some(Rect::from_corners(corner, corner + texels)),
                custom_size: Some(Vec2::splat(texels)),
                color,
                ..default()
            };
            match quad.sprites.get(used) {
                Some(&entity) => {
                    if let Ok((mut drawn, mut transform, mut visibility)) = sprites.get_mut(entity)
                    {
                        if drawn.rect != sprite.rect || drawn.color != sprite.color {
                            *drawn = sprite;
                        }
                        if transform.translation != translation {
                            transform.translation = translation;
                        }
                        visibility.set_if_neq(Visibility::Visible);
                    }
                }
                None => quad.sprites.push(
                    commands
                        .spawn((
                            sprite,
                            Transform::from_translation(translation),
                            RenderLayers::layer(WALL_MAP_LAYER),
                        ))
                        .id(),
                ),
            }
            used += 1;
        }
    }
    for &entity in &quad.sprites[used.min(quad.sprites.len())..] {
        if let Ok((.., mut visibility)) = sprites.get_mut(entity) {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }

    let lean = lean(*volume);
    let uniforms = [
        camera.position.extend(camera.rotation).extend(camera.zoom),
        Vec4::new(camera.offset.x, camera.offset.y, lean.x, lean.y),
        Vec4::new(
            buffers.origin.x as f32,
            buffers.origin.y as f32,
            size.x as f32,
            size.y as f32,
        ),
        Vec4::new(
            MAX_STATURE * CELL_SIZE.y,
            CELL_SIZE.y,
            texels,
            tallest * CELL_SIZE.y,
        ),
    ];
    let unchanged = materials.get(&quad.material).is_some_and(|material| {
        [
            material.camera,
            material.offset_lean,
            material.frame,
            material.params,
        ] == uniforms
    });
    if !unchanged {
        if let Some(mut material) = materials.get_mut(&quad.material) {
            [
                material.camera,
                material.offset_lean,
                material.frame,
                material.params,
            ] = uniforms;
        }
    }
}
