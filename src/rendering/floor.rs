//! The floor drawn by a shader instead of the text renderer's dots.
//!
//! One quad covers the window behind the text. Its fragment shader turns
//! each pixel back into the map position it shows (the inverse of where
//! the text renderer puts cells, so it turns and zooms with the view) and
//! draws the floor there as tiles in the chosen `FloorStyle`: glyphs from
//! `assets/floor_tiles.txt`, laid out by its styles, each lit as a whole.
//! Floor seen directly is lit from the frame's `FloorLight` lamps, a ray to
//! each glyph through the cells that block light.

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
    window::PrimaryWindow,
};

use crate::content::floor_tiles::{parse_floor_tiles, FloorTiles, Pick, MAX_STYLE_GLYPHS};
use crate::model::{FloorLight, FloorStyle, LightKind, ViewCamera};
use crate::schedule::{GamePhase, StartupPhase};

use super::extruded::ExtrudedWalls;
use super::text::CELL_SIZE;

const FLOOR_SHADER: &str = "shaders/floor.wgsl";

/// The bundled floor tiles: glyphs and the styles that lay them out.
pub const FLOOR_TILES: &str = include_str!("../../assets/floor_tiles.txt");

/// The floor tiles, parsed.
#[derive(Resource, Debug)]
pub(super) struct FloorTileSet(pub FloorTiles);

/// A floor tile style, as the shader reads it.
#[derive(ShaderType, Clone, Debug)]
struct Tiles {
    /// x: slots along a cell's side; y: jitter, in glyph pixels; z: the
    /// share left empty; w: how many slots every other row moves along.
    arrange: Vec4,
    /// x, y: the block's width and height in slots; z: how many glyphs;
    /// w: how it picks them: 0 at random, 1 as a block, 2 by brightness.
    pick: Vec4,
    /// x: how bright lit glyphs are.
    gain: Vec4,
    /// The glyphs it uses, four to an element, as indices into the glyph
    /// atlas.
    glyphs: [Vec4; MAX_STYLE_GLYPHS / 4],
}

/// Behind every glyph.
const FLOOR_DEPTH: f32 = -1.0;

/// The most lamps the shader casts; the rest only light their cells.
const MAX_LAMPS: usize = 32;

/// The lamps, as the shader reads them.
#[derive(ShaderType, Clone, Debug)]
struct Lamps {
    /// x: how many are used.
    count: Vec4,
    /// x, y: where it is, in cells from the window's top-left corner;
    /// z: radius; w: value.
    at: [Vec4; MAX_LAMPS],
    /// x: its kind, 0 to 3 (none, fire, electricity, acid).
    kind: [Vec4; MAX_LAMPS],
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct FloorMaterial {
    /// x, y: the map point at the screen centre; z: the view's rotation;
    /// w: its zoom.
    #[uniform(0)]
    camera: Vec4,
    /// x, y: where that point shows, in view cells (the camera's offset);
    /// z, w: the map cell at the window's top left.
    #[uniform(1)]
    window: Vec4,
    /// x, y: the window's size in cells; z: seconds, for animation.
    #[uniform(2)]
    style: Vec4,
    /// `FloorLight::light`, blended between cells.
    #[texture(3)]
    #[sampler(4)]
    light: Handle<Image>,
    #[uniform(5)]
    lamps: Lamps,
    /// `FloorLight::info`, read a cell at a time.
    #[texture(6)]
    info: Handle<Image>,
    /// x, y: where the player is, in cells from the window's top-left
    /// corner.
    #[uniform(7)]
    viewer: Vec4,
    #[uniform(8)]
    tiles: Tiles,
    /// The glyphs side by side, 8 by 8 pixels each, read a pixel at a time.
    #[texture(9)]
    glyphs: Handle<Image>,
}

impl Material2d for FloorMaterial {
    fn fragment_shader() -> ShaderRef {
        FLOOR_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Component)]
struct FloorQuad;

/// The floor shader's quad and its textures, for the text renderer. It
/// needs a GPU, so headless apps leave it out.
pub struct FloorShaderPlugin;

impl Plugin for FloorShaderPlugin {
    fn build(&self, app: &mut App) {
        let tiles = parse_floor_tiles(FLOOR_TILES).expect("the bundled floor tiles parse");
        app.insert_resource(FloorTileSet(tiles))
            .add_plugins(Material2dPlugin::<FloorMaterial>::default())
            .add_systems(Startup, setup.in_set(StartupPhase::Renderer))
            .add_systems(Update, draw_floor.in_set(GamePhase::Output));
    }
}

fn setup(
    tiles: Res<FloorTileSet>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FloorMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.spawn((
        FloorQuad,
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(materials.add(FloorMaterial {
            camera: Vec4::ZERO,
            window: Vec4::ZERO,
            style: Vec4::ZERO,
            light: images.add(cell_image(UVec2::ONE, TextureFormat::Rgba8UnormSrgb)),
            lamps: Lamps {
                count: Vec4::ZERO,
                at: [Vec4::ZERO; MAX_LAMPS],
                kind: [Vec4::ZERO; MAX_LAMPS],
            },
            info: images.add(cell_image(UVec2::ONE, TextureFormat::Rgba8Unorm)),
            viewer: Vec4::ZERO,
            tiles: Tiles {
                arrange: Vec4::ZERO,
                pick: Vec4::ZERO,
                gain: Vec4::ZERO,
                glyphs: [Vec4::ZERO; MAX_STYLE_GLYPHS / 4],
            },
            glyphs: images.add(glyph_atlas(&tiles.0)),
        })),
        Transform::from_xyz(0.0, 0.0, FLOOR_DEPTH),
        Visibility::Hidden,
    ));
}

/// Every glyph side by side, 8 by 8 pixels each: on is white.
fn glyph_atlas(tiles: &FloorTiles) -> Image {
    let width = 8 * tiles.glyphs.len().max(1);
    let mut pixels = vec![0; width * 8 * 4];
    for (glyph, rows) in tiles.glyphs.iter().enumerate() {
        for (y, row) in rows.iter().enumerate() {
            for x in 0..8 {
                if row & (0x80 >> x) != 0 {
                    let at = (y * width + glyph * 8 + x) * 4;
                    pixels[at..at + 4].fill(255);
                }
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: width as u32,
            height: 8,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

/// A texture a texel per cell of a window `size` cells large.
fn cell_image(size: UVec2, format: TextureFormat) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        format,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

/// Copies `data` into the texture `handle` names, making a new one if the
/// window changed size; uploads only what changed.
fn upload(
    images: &mut Assets<Image>,
    handle: &mut Handle<Image>,
    size: UVec2,
    format: TextureFormat,
    data: &[u8],
) {
    if images
        .get(handle.id())
        .is_none_or(|image| image.size() != size)
    {
        *handle = images.add(cell_image(size, format));
    }
    let stale = images
        .get(handle.id())
        .and_then(|image| image.data.as_deref())
        != Some(data);
    if stale {
        if let Some(mut image) = images.get_mut(handle.id()) {
            if let Some(texels) = image.data.as_mut() {
                texels.copy_from_slice(data);
            }
        }
    }
}

/// Shows the quad over the whole window for a shader style, with the
/// frame's floor light and the view, and hides it otherwise.
#[allow(clippy::too_many_arguments)]
fn draw_floor(
    style: Option<Res<FloorStyle>>,
    tile_set: Res<FloorTileSet>,
    floor: Option<Res<FloorLight>>,
    camera: Res<ViewCamera>,
    time: Res<Time>,
    extruded_walls: Option<Res<ExtrudedWalls>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut quad: Single<
        (
            &MeshMaterial2d<FloorMaterial>,
            &mut Transform,
            &mut Visibility,
        ),
        With<FloorQuad>,
    >,
    mut materials: ResMut<Assets<FloorMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let style = style
        .and_then(|style| style.0)
        .and_then(|style| tile_set.0.styles.get(style));
    let (handle, transform, visibility) = &mut *quad;
    let shown = style.is_some()
        && extruded_walls.is_none()
        && floor.as_ref().is_some_and(|floor| !floor.light.is_empty());
    visibility.set_if_neq(if shown {
        Visibility::Visible
    } else {
        Visibility::Hidden
    });
    let (Some(floor), Some(style), true) = (floor, style, shown) else {
        return;
    };
    if let Some(window) = window {
        transform.scale = Vec3::new(window.width(), window.height(), 1.0);
    }
    let Some(mut material) = materials.get_mut(&handle.0) else {
        return;
    };
    debug_assert_eq!(
        CELL_SIZE,
        Vec2::splat(16.0),
        "the shader assumes 16 px cells"
    );
    material.camera = Vec4::new(
        camera.position.x,
        camera.position.y,
        camera.rotation,
        camera.zoom,
    );
    material.window = Vec4::new(
        camera.offset.x,
        camera.offset.y,
        floor.origin.x as f32,
        floor.origin.y as f32,
    );
    material.style = Vec4::new(
        floor.cells.x as f32,
        floor.cells.y as f32,
        time.elapsed_secs_wrapped(),
        0.0,
    );
    let (block, pick) = match style.pick {
        Pick::Random => (UVec2::ONE, 0.0),
        Pick::Block(width, height) => (UVec2::new(width, height), 1.0),
        Pick::Ramp => (UVec2::ONE, 2.0),
    };
    let tiles = &mut material.tiles;
    tiles.arrange = Vec4::new(
        style.slots as f32,
        style.jitter as f32,
        style.empty,
        style.shift,
    );
    tiles.pick = Vec4::new(
        block.x as f32,
        block.y as f32,
        style.glyphs.len() as f32,
        pick,
    );
    tiles.gain = Vec4::new(style.gain, 0.0, 0.0, 0.0);
    tiles.glyphs = [Vec4::ZERO; MAX_STYLE_GLYPHS / 4];
    for (slot, &glyph) in style.glyphs.iter().enumerate() {
        tiles.glyphs[slot / 4][slot % 4] = glyph as f32;
    }
    // Cell (x, y) spans x - 0.5 to x + 0.5; the window's corner is at its
    // top-left cell's corner.
    let corner = floor.origin.as_vec2() - 0.5;
    let viewer = floor.viewer - corner;
    material.viewer = Vec4::new(viewer.x, viewer.y, 0.0, 0.0);
    let lamps = &mut material.lamps;
    let used = floor.lamps.len().min(MAX_LAMPS);
    lamps.count = Vec4::new(used as f32, 0.0, 0.0, 0.0);
    for (slot, lamp) in floor.lamps.iter().take(MAX_LAMPS).enumerate() {
        let at = lamp.position - corner;
        lamps.at[slot] = Vec4::new(at.x, at.y, lamp.radius, lamp.value);
        let kind = match lamp.kind {
            LightKind::None => 0.0,
            LightKind::Fire => 1.0,
            LightKind::Electricity => 2.0,
            LightKind::Acid => 3.0,
        };
        lamps.kind[slot] = Vec4::new(kind, 0.0, 0.0, 0.0);
    }

    let size = floor.cells.as_uvec2();
    let FloorMaterial { light, info, .. } = &mut *material;
    upload(
        &mut images,
        light,
        size,
        TextureFormat::Rgba8UnormSrgb,
        &floor.light,
    );
    upload(
        &mut images,
        info,
        size,
        TextureFormat::Rgba8Unorm,
        &floor.info,
    );
}
