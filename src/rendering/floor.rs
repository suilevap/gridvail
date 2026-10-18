//! The floor drawn by a shader instead of the text renderer's dots.
//!
//! One quad covers the window behind the text. Its fragment shader turns
//! each pixel back into the map position it shows (the inverse of where
//! the text renderer puts cells, so it turns and zooms with the view) and
//! draws the floor there in the chosen `FloorStyle`. Floor seen directly is
//! lit a pixel at a time from the frame's `FloorLight` lamps, a ray to each
//! through the cells that block light, so shadows have exact edges.

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

use crate::model::{FloorLight, FloorStyle, LightKind, ViewCamera};
use crate::schedule::{GamePhase, StartupPhase};

use super::extruded::ExtrudedWalls;
use super::text::CELL_SIZE;

const FLOOR_SHADER: &str = "shaders/floor.wgsl";

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
    /// x, y: the window's size in cells; z: seconds, for animation; w: the
    /// style (`FloorStyle` as a number, 1 and up).
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
        app.add_plugins(Material2dPlugin::<FloorMaterial>::default())
            .add_systems(Startup, setup.in_set(StartupPhase::Renderer))
            .add_systems(Update, draw_floor.in_set(GamePhase::Output));
    }
}

fn setup(
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
        })),
        Transform::from_xyz(0.0, 0.0, FLOOR_DEPTH),
        Visibility::Hidden,
    ));
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
    let style = style.map_or(FloorStyle::Text, |style| *style);
    let (handle, transform, visibility) = &mut *quad;
    let shown = style != FloorStyle::Text
        && extruded_walls.is_none()
        && floor.as_ref().is_some_and(|floor| !floor.light.is_empty());
    visibility.set_if_neq(if shown {
        Visibility::Visible
    } else {
        Visibility::Hidden
    });
    let (Some(floor), true) = (floor, shown) else {
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
        style as u32 as f32,
    );
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
