//! The floor drawn by a shader instead of the text renderer's dots.
//!
//! One quad covers the window behind the text. Its fragment shader turns
//! each pixel back into the map position it shows (the inverse of where
//! the text renderer puts cells, so it turns and zooms with the view) and
//! draws the floor there in the chosen `FloorStyle` from the frame's
//! `FloorLight`, light finer than the grid.

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
    window::PrimaryWindow,
};

use crate::model::{FloorLight, FloorStyle, ViewCamera};
use crate::schedule::{GamePhase, StartupPhase};

use super::extruded::ExtrudedWalls;
use super::text::CELL_SIZE;

const FLOOR_SHADER: &str = "shaders/floor.wgsl";

/// Behind every glyph.
const FLOOR_DEPTH: f32 = -1.0;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct FloorMaterial {
    /// x, y: the map point at the screen centre; z: the view's rotation;
    /// w: its zoom.
    #[uniform(0)]
    camera: Vec4,
    /// x, y: where that point shows, in view cells (the camera's offset);
    /// z, w: the map cell at the top left of `light`.
    #[uniform(1)]
    window: Vec4,
    /// x, y: `light`'s size in cells; z: seconds, for animation; w: the
    /// style (`FloorStyle` as a number, 1 and up).
    #[uniform(2)]
    style: Vec4,
    /// `FloorLight`, a texture.
    #[texture(3)]
    #[sampler(4)]
    light: Handle<Image>,
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

/// The floor shader's quad and its light texture, for the text renderer.
/// It needs a GPU, so headless apps leave it out.
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
    let light = images.add(light_image(UVec2::ONE));
    commands.spawn((
        FloorQuad,
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(materials.add(FloorMaterial {
            camera: Vec4::ZERO,
            window: Vec4::ZERO,
            style: Vec4::ZERO,
            light,
        })),
        Transform::from_xyz(0.0, 0.0, FLOOR_DEPTH),
        Visibility::Hidden,
    ));
}

fn light_image(size: UVec2) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
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
        && floor.as_ref().is_some_and(|floor| !floor.data.is_empty());
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
    debug_assert_eq!(
        CELL_SIZE,
        Vec2::splat(16.0),
        "the shader assumes 16 px cells"
    );

    let size = floor.cells.as_uvec2() * FloorLight::TEXELS as u32;
    let fits = images
        .get(&material.light)
        .is_some_and(|image| image.size() == size);
    if !fits {
        material.light = images.add(light_image(size));
    }
    let stale = images
        .get(&material.light)
        .and_then(|image| image.data.as_deref())
        != Some(floor.data.as_slice());
    if stale {
        if let Some(mut image) = images.get_mut(&material.light) {
            if let Some(data) = image.data.as_mut() {
                data.copy_from_slice(&floor.data);
            }
        }
    }
}
