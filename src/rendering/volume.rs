//! Walls with height in the text renderer (`WallVolume`), drawn by a shader.
//!
//! The view looks down at a slant from the bottom of the screen, an oblique
//! projection: the floor stays as drawn, and height goes straight up the
//! screen (`Leaning`: up and to the right) whichever way the map is turned.
//!
//! Every standing wall is drawn by one full-screen quad: for each pixel the
//! shader walks down from the tallest wall's top toward the floor, and at
//! each height looks at the map point under it (one pixel lower, along the
//! lean) for a wall that tall whose glyph covers that point. The first one
//! found is the nearest. So a wall is its glyph extruded, without seams at
//! any zoom or turn, and it turns with the map smoothly: glyphs are sampled
//! in map space. It writes the depth sprites are sorted by, so actors behind
//! a wall are hidden by it and actors in front of it are not.
//!
//! What it reads: a frame-sized texture of each cell's wall colour and
//! height, one of its glyph (a slot in the glyph atlas: the glyph as turned
//! by a portal it is seen through, not by the view), and the atlas, the
//! font's glyphs rasterized at their 8 px design size.

use bevy::{
    asset::{embedded_asset, RenderAssetUsages},
    platform::collections::HashMap,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};
use swash::{
    scale::{Render, ScaleContext, Source},
    zeno::Format,
    FontRef,
};

use crate::lighting::palette_color;
use crate::model::{RenderBuffers, ViewCamera, Wall, WallVolume};
use crate::schedule::{GamePhase, StartupPhase};

use super::text::{ObjectSprite, TurnedGlyphs, CELL_FONT_BYTES, CELL_SIZE};

/// How far a standing wall's top is above the floor, in cells.
const WALL_HEIGHT: f32 = 0.625;
/// The font's design size: one atlas texel per font pixel.
const GLYPH_PX: usize = 8;
/// Atlas slots per side; slot 0 is blank.
const ATLAS_SLOTS: usize = 16;
const SHADER: &str = "embedded://pav_ecs_game_bevy_port/rendering/wall_volume.wgsl";

/// Draws standing walls with the wall shader when `WallVolume` is not flat.
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
                update_walls
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
    /// x: wall height in px at zoom 1, y: 1 for solid walls, z: cell px.
    #[uniform(3)]
    params: Vec4,
    /// Per frame slot: rgb the wall's lit colour, a its height (0: none).
    #[texture(4)]
    cells: Handle<Image>,
    /// Per frame slot: r its glyph's atlas slot / 255.
    #[texture(5)]
    glyphs: Handle<Image>,
    #[texture(6)]
    atlas: Handle<Image>,
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

/// The glyphs rasterized into the atlas so far, by character.
#[derive(Resource)]
struct GlyphAtlas {
    slots: HashMap<char, u8>,
    handle: Handle<Image>,
}

#[derive(Resource)]
struct WallVolumeQuad {
    entity: Entity,
    material: Handle<WallVolumeMaterial>,
}

fn data_image(size: IVec2, format: TextureFormat, bytes_per_texel: usize) -> Image {
    let size = size.max(IVec2::ONE);
    Image::new(
        Extent3d {
            width: size.x as u32,
            height: size.y as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; size.x as usize * size.y as usize * bytes_per_texel],
        format,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

fn setup(
    mut commands: Commands,
    buffers: Res<RenderBuffers>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WallVolumeMaterial>>,
) {
    let size = IVec2::new(buffers.width, buffers.height);
    let atlas = images.add(data_image(
        IVec2::splat((GLYPH_PX * ATLAS_SLOTS) as i32),
        TextureFormat::R8Unorm,
        1,
    ));
    let material = materials.add(WallVolumeMaterial {
        camera: Vec4::ZERO,
        offset_lean: Vec4::ZERO,
        frame: Vec4::ZERO,
        params: Vec4::ZERO,
        cells: images.add(data_image(size, TextureFormat::Rgba8UnormSrgb, 4)),
        glyphs: images.add(data_image(size, TextureFormat::Rgba8Unorm, 4)),
        atlas: atlas.clone(),
    });
    // The 2D camera never moves; the quad covers any window.
    let entity = commands
        .spawn((
            Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
            MeshMaterial2d(material.clone()),
            Transform::from_scale(Vec3::new(16384.0, 16384.0, 1.0)),
            Visibility::Hidden,
        ))
        .id();
    commands.insert_resource(WallVolumeQuad { entity, material });
    commands.insert_resource(GlyphAtlas {
        slots: HashMap::default(),
        handle: atlas,
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
            data[(slot_y + y as usize) * width + slot_x + x as usize] =
                if coverage >= 128 { 255 } else { 0 };
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

/// Fills the shader's textures from the frame's standing walls.
#[allow(clippy::too_many_arguments)]
fn update_walls(
    volume: Res<WallVolume>,
    camera: Res<ViewCamera>,
    buffers: Res<RenderBuffers>,
    turned: Res<TurnedGlyphs>,
    quad: Res<WallVolumeQuad>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<WallVolumeMaterial>>,
    mut visibility: Query<&mut Visibility>,
    walls: Query<(), With<Wall>>,
) {
    let shown = *volume != WallVolume::Flat;
    if let Ok(mut visibility) = visibility.get_mut(quad.entity) {
        visibility.set_if_neq(if shown {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    if !shown {
        return;
    }
    let Some(material) = materials.get(&quad.material) else {
        return;
    };
    let (cells_image, glyphs_image) = (material.cells.clone(), material.glyphs.clone());
    let size = IVec2::new(buffers.width, buffers.height);
    let mut cells = vec![0u8; (size.x * size.y * 4) as usize];
    let mut glyphs = vec![0u8; (size.x * size.y * 4) as usize];
    for object in &buffers.objects {
        // Moving walls stay glyphs (see `place_object_sprites`).
        if walls.get(object.entity).is_err() || object.position != object.pos.as_vec2() {
            continue;
        }
        let Some(index) = buffers.idx(object.pos) else {
            continue;
        };
        // In map space: only a portal's turn, not the view's.
        let ch = turned.turned(object.cell.ch, object.quarters as i32);
        let next = atlas.slots.len() + 1;
        let slot = match atlas.slots.get(&ch) {
            Some(&slot) => slot,
            None if next < ATLAS_SLOTS * ATLAS_SLOTS => {
                if let Some(mut image) = images.get_mut(&atlas.handle) {
                    rasterize(ch, next, &mut image);
                }
                atlas.slots.insert(ch, next as u8);
                next as u8
            }
            None => continue,
        };
        let color = palette_color(object.cell.color).to_srgba();
        let texel = index * 4;
        cells[texel..texel + 4].copy_from_slice(&[
            (color.red * 255.0) as u8,
            (color.green * 255.0) as u8,
            (color.blue * 255.0) as u8,
            (object.height.clamp(0.0, 1.0) * 255.0).round().max(1.0) as u8,
        ]);
        glyphs[texel] = slot;
    }
    // Only what changed is uploaded again.
    for (handle, data) in [(&cells_image, cells), (&glyphs_image, glyphs)] {
        let current = images.get(handle);
        if current.is_some_and(|image| {
            image.size().as_ivec2() == size && image.data.as_ref() == Some(&data)
        }) {
            continue;
        }
        if let Some(mut image) = images.get_mut(handle) {
            if image.size().as_ivec2() != size {
                let format = image.texture_descriptor.format;
                *image = data_image(size, format, 4);
            }
            image.data = Some(data);
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
            WALL_HEIGHT * CELL_SIZE.y,
            if *volume == WallVolume::Solid {
                1.0
            } else {
                0.0
            },
            CELL_SIZE.y,
            0.0,
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
    if unchanged {
        return;
    }
    if let Some(mut material) = materials.get_mut(&quad.material) {
        [
            material.camera,
            material.offset_lean,
            material.frame,
            material.params,
        ] = uniforms;
    }
}
