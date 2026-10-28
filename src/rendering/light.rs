//! Light as textures for the text renderer's shaders, and the glow it gives
//! (`LightEffects`).
//!
//! The light every frame cell shows (`RenderBuffers::light`) goes into a
//! texture of a texel per cell, which the shaders blend between cell
//! centres (`view.wgsl`), so light changes smoothly across a cell instead of
//! a cell at a time: the wall shader lights standing walls from it pixel by
//! pixel. With `Glow`, the brightest of that light, spread over a few cells
//! on the CPU (a small blur, cheaper than one per pixel), makes a glow
//! texture, and a full-screen quad drawn over everything, added to it,
//! blooms it around the brightest points and gathers the light along each
//! pixel's view ray through the air, a haze that rises toward the viewer
//! like the walls do.
//!
//! Light changes a turn at a time, so it would jump at every step. Instead
//! there are two of each texture, the light before the latest change and
//! the latest (`LightTextures::previous` and `current`), and the shaders
//! fade from one to the other on the GPU (`LightTextures::blend`, over
//! `LIGHT_FADE`). When the light changes, the new light is written over the
//! older pair, which becomes the current one. Each pair keeps the frame
//! origin it was built for, so light stays on its map cells as the frame
//! scrolls. The CPU only works, and the GPU only receives textures, when
//! the light changes.

use bevy::{
    asset::{embedded_asset, RenderAssetUsages},
    mesh::MeshVertexBufferLayoutRef,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{
        AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
        RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension, TextureFormat,
        TextureUsages,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin},
};

use crate::model::{LightEffects, RenderBuffers, ViewCamera, WallVolume};
use crate::schedule::{GamePhase, StartupPhase};

use super::text::CELL_SIZE;

const SHADER: &str = "embedded://pav_ecs_game_bevy_port/rendering/light_glow.wgsl";
/// Light (linear luminance) under this does not bloom.
const BLOOM_THRESHOLD: f32 = 0.45;
/// How far bloom spreads, in cells either side, and how sharply.
const BLOOM_RADIUS: usize = 3;
const BLOOM_SIGMA: f32 = 1.4;
/// How strongly the glow and the haze add to what is drawn.
const BLOOM_STRENGTH: f32 = 0.7;
const HAZE_STRENGTH: f32 = 0.12;
/// How high above the floor the haze reaches, in cells.
const HAZE_HEIGHT: f32 = 2.0;
/// Over walls, sprites and the ground, under the HUD.
const OVERLAY_Z: f32 = 900.0;
/// Points along the view ray the haze is gathered from.
const HAZE_STEPS: usize = 8;
/// Steps of the linear to sRGB table: fine enough at the dark end.
const SRGB_STEPS: usize = 4096;
/// Seconds light takes to fade from one turn's to the next: a step's move
/// takes 0.12 s.
const LIGHT_FADE: f32 = 0.15;

/// Smooth light for the shaders, and bloom and haze with `Glow`.
pub struct LightEffectsPlugin;

impl Plugin for LightEffectsPlugin {
    fn build(&self, app: &mut App) {
        super::load_view_shader(app);
        embedded_asset!(app, "light_glow.wgsl");
        app.init_resource::<LightEffects>()
            .add_plugins(Material2dPlugin::<GlowMaterial>::default())
            .add_systems(Startup, setup.in_set(StartupPhase::Renderer))
            .add_systems(
                Update,
                (update_light_textures, draw_glow)
                    .chain()
                    .in_set(GamePhase::Output)
                    .after(super::text::place_object_sprites),
            );
    }
}

/// One frame's light as textures, a texel per frame cell from `origin`,
/// with the texels they were made from.
#[derive(Clone, Debug)]
pub(super) struct LightFrame {
    /// The light each frame cell shows, sRGB; alpha 1 where it is seen now
    /// (remembered cells are lit too, dimmed, but light no air).
    pub light: Handle<Image>,
    /// The light in the air above each cell: the brightest light spread
    /// over a few cells (bloom) and the light along the view ray (haze),
    /// baked per cell, so drawing it is one lookup a pixel.
    pub glow: Handle<Image>,
    /// The frame's origin (`RenderBuffers::origin`) it was built for.
    pub origin: IVec2,
    /// `light`'s texels.
    texels: Vec<u8>,
    /// The haze direction `glow` was baked for (`haze_direction`).
    haze: Vec2,
}

/// The light before the latest change and the latest, which the shaders
/// fade between.
#[derive(Resource)]
pub(super) struct LightTextures {
    pub previous: LightFrame,
    pub current: LightFrame,
    /// How far the shaders have faded from `previous` to `current`, from 0
    /// to 1.
    pub blend: f32,
    size: IVec2,
    /// Seconds since `current` came; until the first light, none.
    since_change: Option<f32>,
    /// The frame's light this frame, to compare with `current`'s.
    texels: Vec<u8>,
    baker: AirBaker,
}

/// Bakes the light in the air (`LightFrame::glow`) from light texels, with
/// room to work in.
struct AirBaker {
    /// Bloom before and between the two passes of its blur.
    bright: Vec<Vec3>,
    spread: Vec<Vec3>,
    /// What lights the air: light seen now, squared (bright light far more
    /// than dim).
    lit: Vec<Vec3>,
    /// The baked texels.
    air: Vec<u8>,
    /// sRGB byte to linear, and linear (in `SRGB_STEPS` steps) to sRGB.
    linear: [f32; 256],
    srgb: Vec<u8>,
    weights: [f32; BLOOM_RADIUS + 1],
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct GlowMaterial {
    /// xy: the map point at the screen centre, z: rotation, w: zoom.
    #[uniform(0)]
    camera: Vec4,
    /// xy: the view's offset in cells.
    #[uniform(1)]
    offset: Vec4,
    /// xy: the frame's size in cells, z: how far the light has faded from
    /// the previous textures to the current ones, w: cell px.
    #[uniform(2)]
    frame: Vec4,
    /// xy: the previous textures' frame origin, zw: the current ones'.
    #[uniform(3)]
    origins: Vec4,
    #[texture(4)]
    glow_previous: Handle<Image>,
    #[texture(5)]
    glow_current: Handle<Image>,
}

impl Material2d for GlowMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    /// Light adds to what is drawn: blending it would darken.
    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let additive = BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::Zero,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
        };
        if let Some(fragment) = descriptor.fragment.as_mut() {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(additive);
            }
        }
        Ok(())
    }
}

#[derive(Resource)]
struct GlowQuad {
    entity: Entity,
    material: Handle<GlowMaterial>,
}

/// A texture of `size` texels, which the shaders blend between.
fn light_image(size: IVec2, format: TextureFormat) -> Image {
    let size = size.max(IVec2::ONE);
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x as u32,
            height: size.y as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        format,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    image
}

/// A frame's pair of textures, dark.
fn light_frame(size: IVec2, images: &mut Assets<Image>) -> LightFrame {
    LightFrame {
        light: images.add(light_image(size, TextureFormat::Rgba8UnormSrgb)),
        // sRGB keeps the faint edge of the glow from coming in steps.
        glow: images.add(light_image(size, TextureFormat::Rgba8UnormSrgb)),
        origin: IVec2::ZERO,
        texels: vec![0; (size.x * size.y).max(1) as usize * 4],
        haze: Vec2::NAN,
    }
}

/// Eases a fade's start and end.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Which way, and how far, the haze above a map point reaches in map
/// space: the view ray rises toward the viewer `HAZE_HEIGHT` cells along
/// the lean of height on screen (`lean`), turned with the view
/// (`rotation`). It does not depend on zoom, so only turning the view or
/// changing how walls lean changes it.
pub(super) fn haze_direction(rotation: f32, lean: Vec2) -> Vec2 {
    let (s, c) = rotation.sin_cos();
    let v = Vec2::new(lean.x, -lean.y);
    Vec2::new(c * v.x - s * v.y, s * v.x + c * v.y) * HAZE_HEIGHT
}

/// Haze baked this much apart (in cells) is the same.
const HAZE_TURN: f32 = 0.01;

impl AirBaker {
    fn new() -> Self {
        let mut linear = [0.0; 256];
        for (byte, value) in linear.iter_mut().enumerate() {
            *value = Color::srgb_u8(byte as u8, 0, 0).to_linear().red;
        }
        let srgb = (0..SRGB_STEPS)
            .map(|step| {
                let value = step as f32 / (SRGB_STEPS - 1) as f32;
                let srgb = Color::linear_rgb(value, 0.0, 0.0).to_srgba().red;
                (srgb * 255.0).round() as u8
            })
            .collect();
        let mut weights = [0.0; BLOOM_RADIUS + 1];
        for (d, weight) in weights.iter_mut().enumerate() {
            *weight = (-((d * d) as f32) / (2.0 * BLOOM_SIGMA * BLOOM_SIGMA)).exp();
        }
        let total = weights[0] + 2.0 * weights[1..].iter().sum::<f32>();
        weights.iter_mut().for_each(|weight| *weight /= total);
        Self {
            bright: Vec::new(),
            spread: Vec::new(),
            lit: Vec::new(),
            air: Vec::new(),
            linear,
            srgb,
            weights,
        }
    }

    fn resize(&mut self, texels: usize) {
        self.bright = vec![Vec3::ZERO; texels];
        self.spread = vec![Vec3::ZERO; texels];
        self.lit = vec![Vec3::ZERO; texels];
        self.air = vec![0; texels * 4];
    }

    /// Bakes the light in the air above each texel of `texels` (a `size`
    /// grid) into `air`: the light seen now that is bright enough to bloom,
    /// blurred, and the haze, the light the air holds along `haze` from
    /// each cell, averaged.
    fn bake(&mut self, texels: &[u8], size: IVec2, haze: Vec2) {
        let (width, height) = (size.x as usize, size.y as usize);
        for (texel, (bright, lit)) in self.bright.iter_mut().zip(&mut self.lit).enumerate() {
            let rgba = &texels[texel * 4..texel * 4 + 4];
            if rgba[3] == 0 {
                *bright = Vec3::ZERO;
                *lit = Vec3::ZERO;
                continue;
            }
            let color = Vec3::new(
                self.linear[rgba[0] as usize],
                self.linear[rgba[1] as usize],
                self.linear[rgba[2] as usize],
            );
            let luminance = color.dot(Vec3::new(0.2126, 0.7152, 0.0722));
            let over = ((luminance - BLOOM_THRESHOLD) / (1.0 - BLOOM_THRESHOLD)).max(0.0);
            *bright = color * over * over;
            *lit = color * color;
        }
        // Across, then down.
        blur(
            &self.bright,
            &mut self.spread,
            width,
            height,
            1,
            &self.weights,
        );
        blur(
            &self.spread,
            &mut self.bright,
            width,
            height,
            width,
            &self.weights,
        );
        // The haze above a cell gathers the light from it toward the viewer,
        // blended between cells as the shaders do.
        let lit_at = |at: Vec2| {
            let corner = at.floor();
            let t = at - corner;
            let texel = |x: i32, y: i32| {
                let inside = x >= 0 && y >= 0 && x < size.x && y < size.y;
                if inside {
                    self.lit[(y * size.x + x) as usize]
                } else {
                    Vec3::ZERO
                }
            };
            let (x, y) = (corner.x as i32, corner.y as i32);
            let top = texel(x, y).lerp(texel(x + 1, y), t.x);
            let bottom = texel(x, y + 1).lerp(texel(x + 1, y + 1), t.x);
            top.lerp(bottom, t.y)
        };
        for y in 0..size.y {
            for x in 0..size.x {
                let texel = (y * size.x + x) as usize;
                let cell = Vec2::new(x as f32, y as f32);
                let mut gathered = Vec3::ZERO;
                for step in 0..HAZE_STEPS {
                    let along = (step as f32 + 0.5) / HAZE_STEPS as f32;
                    gathered += lit_at(cell - haze * along);
                }
                let air = self.bright[texel] * BLOOM_STRENGTH
                    + gathered / HAZE_STEPS as f32 * HAZE_STRENGTH;
                self.air[texel * 4..texel * 4 + 4].copy_from_slice(&[
                    self.srgb[srgb_step(air.x)],
                    self.srgb[srgb_step(air.y)],
                    self.srgb[srgb_step(air.z)],
                    255,
                ]);
            }
        }
    }
}

impl LightTextures {
    fn new(size: IVec2, images: &mut Assets<Image>) -> Self {
        let mut textures = Self {
            previous: light_frame(IVec2::ONE, images),
            current: light_frame(IVec2::ONE, images),
            blend: 1.0,
            size: IVec2::ZERO,
            since_change: None,
            texels: Vec::new(),
            baker: AirBaker::new(),
        };
        textures.resize(size, images);
        textures
    }

    fn resize(&mut self, size: IVec2, images: &mut Assets<Image>) {
        let size = size.max(IVec2::ONE);
        let texels = (size.x * size.y) as usize;
        self.size = size;
        self.previous = light_frame(size, images);
        self.current = light_frame(size, images);
        self.since_change = None;
        self.blend = 1.0;
        self.texels = vec![0; texels * 4];
        self.baker.resize(texels);
    }

    /// Fills the texels from the frame; whether they differ from the
    /// current light's.
    fn build_light(&mut self, buffers: &RenderBuffers) -> bool {
        for y in 0..self.size.y {
            for x in 0..self.size.x {
                let texel = (y * self.size.x + x) as usize;
                let Some(index) = buffers.idx(buffers.origin + IVec2::new(x, y)) else {
                    continue;
                };
                let light = buffers.light[index];
                let seen = if buffers.seen[index].is_some() {
                    255
                } else {
                    0
                };
                self.texels[texel * 4..texel * 4 + 4]
                    .copy_from_slice(&[light.r, light.g, light.b, seen]);
            }
        }
        self.texels != self.current.texels
    }

    /// Takes the frame's light, built into the texels, as the current
    /// light, built for `origin`: written over the older pair, which
    /// becomes current, and faded to from the one shown so far. The first
    /// light, or with `at_once`, is shown at once. With `haze` its air is
    /// baked too, the haze going that way.
    fn change(
        &mut self,
        origin: IVec2,
        haze: Option<Vec2>,
        at_once: bool,
        images: &mut Assets<Image>,
    ) {
        let at_once = at_once || self.since_change.is_none();
        std::mem::swap(&mut self.previous, &mut self.current);
        self.current.origin = origin;
        self.current.texels.copy_from_slice(&self.texels);
        upload(images, &self.current.light, &self.current.texels);
        if let Some(haze) = haze {
            self.baker.bake(&self.current.texels, self.size, haze);
            self.current.haze = haze;
            upload(images, &self.current.glow, &self.baker.air);
        }
        if at_once {
            self.previous.origin = origin;
            self.previous.texels.copy_from_slice(&self.texels);
            upload(images, &self.previous.light, &self.previous.texels);
            if let Some(haze) = haze {
                self.previous.haze = haze;
                upload(images, &self.previous.glow, &self.baker.air);
            }
        }
        self.since_change = Some(if at_once { LIGHT_FADE } else { 0.0 });
        self.blend = if at_once { 1.0 } else { 0.0 };
    }

    /// Bakes the air of each pair again where it was baked for another
    /// haze direction (the view turned).
    fn turn_haze(&mut self, haze: Vec2, images: &mut Assets<Image>) {
        if (self.current.haze - haze).length() >= HAZE_TURN || self.current.haze.is_nan() {
            self.baker.bake(&self.current.texels, self.size, haze);
            self.current.haze = haze;
            upload(images, &self.current.glow, &self.baker.air);
        }
        if (self.previous.haze - haze).length() >= HAZE_TURN || self.previous.haze.is_nan() {
            self.baker.bake(&self.previous.texels, self.size, haze);
            self.previous.haze = haze;
            upload(images, &self.previous.glow, &self.baker.air);
        }
    }

    /// Moves the fade on by `seconds`.
    fn advance(&mut self, seconds: f32) {
        if let Some(since) = self.since_change.as_mut() {
            *since += seconds;
            self.blend = smoothstep(*since / LIGHT_FADE);
        }
    }

    /// The shaders' `origins` uniform: xy the previous textures' frame
    /// origin, zw the current ones'.
    pub fn origins(&self) -> Vec4 {
        self.previous
            .origin
            .as_vec2()
            .extend(self.current.origin.x as f32)
            .extend(self.current.origin.y as f32)
    }
}

/// The step of the linear to sRGB table for linear light `value`.
fn srgb_step(value: f32) -> usize {
    (value.clamp(0.0, 1.0) * (SRGB_STEPS - 1) as f32).round() as usize
}

/// One pass of a separable blur over a `width` x `height` grid, along
/// `stride` (1 across, `width` down); off the grid is dark.
fn blur(
    from: &[Vec3],
    to: &mut [Vec3],
    width: usize,
    height: usize,
    stride: usize,
    weights: &[f32],
) {
    let along = if stride == 1 { width } else { height };
    for (texel, out) in to.iter_mut().enumerate() {
        let at = if stride == 1 {
            texel % width
        } else {
            texel / width
        };
        let mut sum = from[texel] * weights[0];
        for (d, &weight) in weights.iter().enumerate().skip(1) {
            if at >= d {
                sum += from[texel - d * stride] * weight;
            }
            if at + d < along {
                sum += from[texel + d * stride] * weight;
            }
        }
        *out = sum;
    }
}

/// Puts `texels` into `image`'s data, which marks it to be sent again.
fn upload(images: &mut Assets<Image>, image: &Handle<Image>, texels: &[u8]) {
    if let Some(mut image) = images.get_mut(image) {
        match image.data.as_mut() {
            Some(data) if data.len() == texels.len() => data.copy_from_slice(texels),
            _ => image.data = Some(texels.to_vec()),
        }
    }
}

fn setup(
    mut commands: Commands,
    buffers: Res<RenderBuffers>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GlowMaterial>>,
) {
    let textures = LightTextures::new(IVec2::new(buffers.width, buffers.height), &mut images);
    let material = materials.add(GlowMaterial {
        camera: Vec4::ZERO,
        offset: Vec4::ZERO,
        frame: Vec4::ZERO,
        origins: Vec4::ZERO,
        glow_previous: textures.previous.glow.clone(),
        glow_current: textures.current.glow.clone(),
    });
    // The main 2D camera never moves; the quad covers any window.
    let entity = commands
        .spawn((
            Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(0.0, 0.0, OVERLAY_Z).with_scale(Vec3::new(16384.0, 16384.0, 1.0)),
            Visibility::Hidden,
        ))
        .id();
    commands.insert_resource(GlowQuad { entity, material });
    commands.insert_resource(textures);
}

/// Sends the frame's light to the GPU when it changes (with `Glow`, baking
/// the air above it too, again when the view turns), and moves the fade to
/// it on.
pub(super) fn update_light_textures(
    time: Res<Time>,
    effects: Res<LightEffects>,
    camera: Res<ViewCamera>,
    volume: Option<Res<WallVolume>>,
    buffers: Res<RenderBuffers>,
    mut textures: ResMut<LightTextures>,
    mut images: ResMut<Assets<Image>>,
) {
    if *effects == LightEffects::Flat {
        return;
    }
    let textures = &mut *textures;
    let size = IVec2::new(buffers.width, buffers.height);
    if textures.size != size {
        textures.resize(size, &mut images);
    }
    textures.advance(time.delta_secs());
    let volume = volume.map_or(WallVolume::Flat, |volume| *volume);
    let haze = (*effects == LightEffects::Glow)
        .then(|| haze_direction(camera.rotation, super::volume::lean(volume)));
    // Light shown while the effects were off is stale: show the light now.
    if textures.build_light(&buffers) || effects.is_changed() {
        textures.change(buffers.origin, haze, effects.is_changed(), &mut images);
    } else if let Some(haze) = haze {
        textures.turn_haze(haze, &mut images);
    }
}

/// Shows the glow quad with `Glow` and passes it the view and the light.
fn draw_glow(
    effects: Res<LightEffects>,
    camera: Res<ViewCamera>,
    buffers: Res<RenderBuffers>,
    textures: Res<LightTextures>,
    quad: Res<GlowQuad>,
    mut materials: ResMut<Assets<GlowMaterial>>,
    mut visibility: Query<&mut Visibility>,
) {
    let shown = *effects == LightEffects::Glow;
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
    let uniforms = [
        camera.position.extend(camera.rotation).extend(camera.zoom),
        camera.offset.extend(0.0).extend(0.0),
        Vec4::new(
            buffers.width as f32,
            buffers.height as f32,
            textures.blend,
            CELL_SIZE.y,
        ),
        textures.origins(),
    ];
    let unchanged = materials.get(&quad.material).is_some_and(|material| {
        [
            material.camera,
            material.offset,
            material.frame,
            material.origins,
        ] == uniforms
            && material.glow_current == textures.current.glow
            && material.glow_previous == textures.previous.glow
    });
    if !unchanged {
        if let Some(mut material) = materials.get_mut(&quad.material) {
            [
                material.camera,
                material.offset,
                material.frame,
                material.origins,
            ] = uniforms;
            material.glow_previous = textures.previous.glow.clone();
            material.glow_current = textures.current.glow.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::portal::CellTransform;
    use crate::model::{Rgb, SeenCell};

    #[test]
    fn the_blur_spreads_light_evenly_and_keeps_it() {
        let textures = LightTextures::new(IVec2::new(9, 9), &mut Assets::default());
        let mut from = vec![Vec3::ZERO; 81];
        from[40] = Vec3::ONE;
        let mut across = vec![Vec3::ZERO; 81];
        let mut both = vec![Vec3::ZERO; 81];
        blur(&from, &mut across, 9, 9, 1, &textures.baker.weights);
        blur(&across, &mut both, 9, 9, 9, &textures.baker.weights);
        let total: Vec3 = both.iter().sum();
        assert!((total - Vec3::ONE).abs().max_element() < 1e-5, "{total}");
        // Even in every direction, and less further out.
        assert_eq!(both[39], both[41]);
        assert_eq!(both[31], both[49]);
        assert_eq!(both[39], both[31]);
        assert!(both[40].x > both[41].x && both[41].x > both[42].x);
        assert_eq!(both[44], Vec3::ZERO, "past the radius");
    }

    #[test]
    fn only_bright_light_seen_now_glows() {
        let mut images = Assets::default();
        let mut textures = LightTextures::new(IVec2::new(8, 8), &mut images);
        let mut buffers = RenderBuffers::new(8, 8);
        let seen = Some(SeenCell {
            world: IVec2::ZERO,
            transform: CellTransform::IDENTITY,
            portal: false,
        });
        let at = |x: i32, y: i32| buffers_index(x, y);
        // A bright cell seen now, a bright one remembered, a dim one seen.
        buffers.light[at(1, 1)] = Rgb::new(255, 240, 170);
        buffers.seen[at(1, 1)] = seen;
        buffers.light[at(6, 1)] = Rgb::new(255, 240, 170);
        buffers.light[at(1, 6)] = Rgb::new(60, 60, 64);
        buffers.seen[at(1, 6)] = seen;
        assert!(textures.build_light(&buffers));
        textures.change(IVec2::ZERO, None, false, &mut images);
        assert!(!textures.build_light(&buffers), "unchanged");
        let texel = |texels: &[u8], x: usize, y: usize| {
            let i = (y * 8 + x) * 4;
            [texels[i], texels[i + 1], texels[i + 2], texels[i + 3]]
        };
        assert_eq!(texel(&textures.texels, 1, 1), [255, 240, 170, 255]);
        assert_eq!(texel(&textures.texels, 6, 1), [255, 240, 170, 0]);
        // Haze off the frame, to see the bloom alone.
        let size = textures.size;
        textures
            .baker
            .bake(&textures.texels, size, Vec2::new(100.0, 0.0));
        let glow = |x, y| texel(&textures.baker.air, x, y)[0];
        assert!(glow(1, 1) > glow(2, 1) && glow(2, 1) > glow(3, 1) && glow(3, 1) > 0);
        assert_eq!(glow(6, 1), 0, "remembered light does not glow");
        assert_eq!(glow(1, 6), 0, "dim light does not glow");
    }

    #[test]
    fn haze_rises_from_light_seen_now_toward_the_viewer() {
        let mut images = Assets::default();
        let mut textures = LightTextures::new(IVec2::new(8, 8), &mut images);
        let mut buffers = RenderBuffers::new(8, 8);
        let seen = Some(SeenCell {
            world: IVec2::ZERO,
            transform: CellTransform::IDENTITY,
            portal: false,
        });
        // Too dim to bloom, bright enough to haze; and the same remembered.
        buffers.light[buffers_index(3, 5)] = Rgb::new(200, 150, 30);
        buffers.seen[buffers_index(3, 5)] = seen;
        buffers.light[buffers_index(6, 5)] = Rgb::new(200, 150, 30);
        textures.build_light(&buffers);
        // Unturned and standing up the screen, the haze above a cell comes
        // from the cells below it on screen, toward the viewer.
        let haze = haze_direction(0.0, Vec2::Y);
        assert_eq!(haze, Vec2::new(0.0, -HAZE_HEIGHT));
        let size = textures.size;
        textures.baker.bake(&textures.texels, size, haze);
        let air = |x: usize, y: usize| textures.baker.air[(y * 8 + x) * 4];
        assert!(air(3, 4) > 0 && air(3, 3) > 0, "above it");
        assert_eq!(air(3, 6), 0, "not below it");
        assert_eq!(air(3, 2), 0, "only as high as the haze reaches");
        assert_eq!(air(6, 4), 0, "remembered light lights no air");
        // Turned a quarter, the haze leans the turned way.
        let turned = haze_direction(std::f32::consts::FRAC_PI_2, Vec2::Y);
        assert!((turned - Vec2::new(HAZE_HEIGHT, 0.0)).length() < 1e-5);
    }

    #[test]
    fn new_light_takes_the_older_textures_and_fades_in_over_about_a_step() {
        let mut images = Assets::default();
        let mut textures = LightTextures::new(IVec2::new(8, 8), &mut images);
        let mut buffers = RenderBuffers::new(8, 8);
        let data = |images: &Assets<Image>, image: &Handle<Image>| {
            images.get(image).unwrap().data.clone().unwrap()
        };
        // The first light is shown at once, in both pairs.
        buffers.light[buffers_index(2, 2)] = Rgb::new(200, 150, 30);
        textures.build_light(&buffers);
        textures.change(IVec2::ZERO, None, false, &mut images);
        assert_eq!(textures.blend, 1.0);
        let first = data(&images, &textures.current.light);
        assert_eq!(data(&images, &textures.previous.light), first);
        let older = textures.current.light.clone();
        // The next turn's light goes over the older pair, which becomes
        // current, and fades in from the light shown so far.
        buffers.light[buffers_index(2, 2)] = Rgb::BLACK;
        assert!(textures.build_light(&buffers));
        textures.change(IVec2::ZERO, None, false, &mut images);
        assert_eq!(textures.previous.light, older);
        assert_eq!(data(&images, &textures.previous.light), first);
        assert_ne!(data(&images, &textures.current.light), first);
        assert_eq!(textures.blend, 0.0);
        textures.advance(1.0 / 60.0);
        assert!(textures.blend > 0.0 && textures.blend < 0.1);
        textures.advance(0.12 - 1.0 / 60.0);
        assert!(textures.blend > 0.7, "mostly faded after a step's move");
        textures.advance(0.1);
        assert_eq!(textures.blend, 1.0);
    }

    #[test]
    fn each_pair_keeps_the_frame_origin_it_was_built_for() {
        let mut images = Assets::default();
        let mut textures = LightTextures::new(IVec2::new(8, 8), &mut images);
        let buffers = RenderBuffers::new(8, 8);
        textures.build_light(&buffers);
        textures.change(IVec2::new(3, 4), None, false, &mut images);
        textures.change(IVec2::new(4, 4), None, false, &mut images);
        assert_eq!(textures.origins(), Vec4::new(3.0, 4.0, 4.0, 4.0));
        // Shown at once (the effects were switched on): both from now.
        textures.change(IVec2::new(5, 4), None, true, &mut images);
        assert_eq!(textures.origins(), Vec4::new(5.0, 4.0, 5.0, 4.0));
        assert_eq!(textures.blend, 1.0);
    }

    /// The frame slot of cell (x, y) of an 8x8 frame at the origin.
    fn buffers_index(x: i32, y: i32) -> usize {
        RenderBuffers::new(8, 8).idx(IVec2::new(x, y)).unwrap()
    }
}
