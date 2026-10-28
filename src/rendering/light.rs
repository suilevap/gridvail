//! Light as textures for the text renderer's shaders, and the glow it gives
//! (`LightEffects`).
//!
//! The light every frame cell shows (`RenderBuffers::light`) goes into a
//! texture of a texel per cell, filtered linearly, so one read blends it
//! between cell centres and light changes smoothly across a cell instead of
//! a cell at a time: the wall shader lights standing walls from it pixel by
//! pixel.
//!
//! Light changes a turn at a time, so walls would flicker at every step.
//! Instead there are two light textures, the light before the latest change
//! and the latest (`LightTextures::previous` and `current`), and the wall
//! shader fades from one to the other on the GPU (`LightTextures::blend`,
//! over `LIGHT_FADE`). When the light changes, the new light is written over
//! the older one, which becomes current. Each keeps the frame origin it was
//! built for, so light stays on its map cells as the frame scrolls.
//!
//! With `Glow`, the light in the air above each cell is baked on the CPU
//! into a glow texture (`LightTextures::glow`): the brightest light seen
//! now spread over a few cells (bloom), and the light along the view ray
//! (haze), which rises a fixed way in map space whatever the zoom. A
//! full-screen quad draws that, with one filtered read a pixel; it does not
//! fade. It is smooth, so it is drawn at a fraction of the window's
//! resolution (`GLOW_DOWNSCALE`), by a camera of its own into a texture,
//! and a second quad over everything adds that texture to what is drawn,
//! scaled up. With `Fog`, the haze (baked
//! apart from the bloom) thickens and thins with two layers of tiling noise
//! (`fog_noise`) drifting over the map, so lit air moves like fog.
//!
//! The CPU only works, and the GPU only receives textures, when the light
//! changes or (for the haze) the view turns.

use bevy::{
    asset::{embedded_asset, RenderAssetUsages},
    camera::{visibility::RenderLayers, ClearColorConfig, RenderTarget},
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    mesh::MeshVertexBufferLayoutRef,
    platform::collections::HashMap,
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

use crate::model::{
    AnimatedPos, LightEffects, MapGrid, RenderBuffers, Speed, ViewCamera, WallVolume,
};
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
/// The glow is drawn this many window pixels to a texel (each way), then
/// scaled up: 1/16 of the pixels at 4.
const GLOW_DOWNSCALE: f32 = 4.0;
/// The render layer only the glow camera sees.
const GLOW_LAYER: usize = 2;
const COMPOSITE_SHADER: &str = "embedded://pav_ecs_game_bevy_port/rendering/light_composite.wgsl";
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
        embedded_asset!(app, "light_composite.wgsl");
        app.init_resource::<LightEffects>()
            .add_plugins(Material2dPlugin::<GlowMaterial>::default())
            .add_plugins(Material2dPlugin::<CompositeMaterial>::default())
            .init_resource::<FogTrail>()
            .add_systems(Startup, setup.in_set(StartupPhase::Renderer))
            .add_systems(
                Update,
                (
                    update_light_textures,
                    stir_fog,
                    draw_glow,
                    fit_glow_to_window,
                )
                    .chain()
                    .in_set(GamePhase::Output)
                    .after(super::text::place_object_sprites),
            );
    }
}

/// One frame's light as a texture, a texel per frame cell from `origin`,
/// with the texels it was made from.
#[derive(Clone, Debug)]
pub(super) struct LightFrame {
    /// The light each frame cell shows, sRGB; alpha 1 where it is seen now
    /// (remembered cells are lit too, dimmed, but light no air).
    pub light: Handle<Image>,
    /// The frame's origin (`RenderBuffers::origin`) it was built for.
    pub origin: IVec2,
    /// `light`'s texels.
    texels: Vec<u8>,
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
    /// The light in the air above each cell of the current light, baked per
    /// cell: the brightest light spread over a few cells (bloom), and the
    /// light along the view ray (haze, in `air`, apart so fog can stir it).
    /// They do not fade: each is drawn with one filtered read a pixel.
    pub glow: Handle<Image>,
    pub air: Handle<Image>,
    /// The haze direction `glow` was baked for (`haze_direction`).
    haze: Vec2,
    size: IVec2,
    /// Seconds since `current` came; until the first light, none.
    since_change: Option<f32>,
    /// The frame's light this frame, to compare with `current`'s.
    texels: Vec<u8>,
    /// Which frame cells are walls (anything that blocks sight, and off the
    /// map), read with the light: fog keeps to the open floor between them.
    walls: Vec<bool>,
    baker: AirBaker,
}

/// Bakes the light in the air (`LightTextures::glow`) from light texels, with
/// room to work in.
struct AirBaker {
    /// Bloom before and between the two passes of its blur.
    bright: Vec<Vec3>,
    spread: Vec<Vec3>,
    /// What lights the air: light seen now, squared (bright light far more
    /// than dim).
    lit: Vec<Vec3>,
    /// The baked texels: bloom, and haze.
    bloom: Vec<u8>,
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
    /// xy: the view's offset in cells, z: cell px.
    #[uniform(1)]
    offset: Vec4,
    /// xy: the glow texture's frame origin, zw: its size in cells.
    #[uniform(2)]
    frame: Vec4,
    /// Filtered linearly: the GPU blends them between cells.
    #[texture(3)]
    #[sampler(4)]
    glow: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    air: Handle<Image>,
    /// Tiling noise the fog drifts by, repeated over the map.
    #[texture(7)]
    #[sampler(8)]
    noise: Handle<Image>,
    /// x: 1 for fog (0: the haze as it is), y: how many of `trail` are
    /// live.
    #[uniform(9)]
    fog: Vec4,
    /// The wakes of things moving through the fog (`FogTrail::wakes`).
    #[uniform(10)]
    trail: [Vec4; TRAIL_POINTS],
}

/// Points along the paths of things moving through fog, which stir it.
const TRAIL_POINTS: usize = 32;
/// How far (in cells) a mover goes between its trail points.
const TRAIL_SPACING: f32 = 0.35;
/// How long a mover's wake lasts (seconds for it to fade to 1/e; it is gone
/// after four), how wide it starts and how fast it spreads (cells, cells a
/// second).
const WAKE_LIFE: f32 = 1.2;
const WAKE_WIDTH: f32 = 0.7;
const WAKE_SPREAD: f32 = 0.6;

/// Where things moving through fog (anything with a `Speed`) have just
/// been, and the wakes they leave there: the fog shader clears and swirls
/// the fog around each, less as it ages, so movers leave a wake that closes
/// behind them.
#[derive(Resource)]
struct FogTrail {
    /// A ring of points: xy the map point, z the time it was left there
    /// (`Time::elapsed_secs_wrapped`), w 1 (0: no point yet).
    points: [Vec4; TRAIL_POINTS],
    next: usize,
    /// Where each mover left its latest point.
    last: HashMap<Entity, Vec2>,
    /// The wakes still stirring the fog, first: xy the map point, z its
    /// width in cells, w its strength (1 when new); the rest are zero.
    /// Worked out once a frame here, so the shader loops over the live ones
    /// only, none once the fog has settled.
    wakes: [Vec4; TRAIL_POINTS],
    live: usize,
}

impl Default for FogTrail {
    fn default() -> Self {
        Self {
            points: [Vec4::ZERO; TRAIL_POINTS],
            next: 0,
            last: HashMap::default(),
            wakes: [Vec4::ZERO; TRAIL_POINTS],
            live: 0,
        }
    }
}

impl FogTrail {
    /// Notes that a mover is at `at` at `time`: a new point once it has
    /// gone `TRAIL_SPACING` from its last.
    fn track(&mut self, mover: Entity, at: Vec2, time: f32) {
        let last = self.last.entry(mover).or_insert(at);
        if last.distance(at) < TRAIL_SPACING {
            return;
        }
        *last = at;
        self.points[self.next] = Vec4::new(at.x, at.y, time, 1.0);
        self.next = (self.next + 1) % TRAIL_POINTS;
    }

    /// The wakes as they are at `now`.
    fn age(&mut self, now: f32) {
        self.wakes = [Vec4::ZERO; TRAIL_POINTS];
        self.live = 0;
        for point in self.points {
            let age = now - point.z;
            if point.w == 0.0 || !(0.0..WAKE_LIFE * 4.0).contains(&age) {
                continue;
            }
            self.wakes[self.live] = Vec4::new(
                point.x,
                point.y,
                WAKE_WIDTH + age * WAKE_SPREAD,
                (-age / WAKE_LIFE).exp(),
            );
            self.live += 1;
        }
    }
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
        additive(descriptor);
        Ok(())
    }
}

/// The glow drawn small (`GlowQuad::target`), added to what is drawn.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct CompositeMaterial {
    #[texture(0)]
    #[sampler(1)]
    glow: Handle<Image>,
}

impl Material2d for CompositeMaterial {
    fn fragment_shader() -> ShaderRef {
        COMPOSITE_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        additive(descriptor);
        Ok(())
    }
}

/// Makes a pipeline add its colour to what is drawn.
fn additive(descriptor: &mut RenderPipelineDescriptor) {
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
}

/// The glow quad, the camera drawing it small into `target`, and the quad
/// adding `target` over everything.
#[derive(Resource)]
struct GlowQuad {
    entity: Entity,
    material: Handle<GlowMaterial>,
    camera: Entity,
    composite: Entity,
    composite_material: Handle<CompositeMaterial>,
    /// The texture's size in texels; it covers that times `GLOW_DOWNSCALE`
    /// window pixels.
    target: UVec2,
}

/// A texture the glow camera draws into, `size` texels, filtered linearly.
fn glow_target(size: UVec2) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x.max(1),
            height: size.y.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    image.sampler = ImageSampler::linear();
    image
}

/// A texture of `size` texels, a texel per frame cell.
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
        light: images.add(filtered(light_image(size, TextureFormat::Rgba8UnormSrgb))),
        origin: IVec2::ZERO,
        texels: vec![0; (size.x * size.y).max(1) as usize * 4],
    }
}

/// Texels a side of the fog's noise texture.
const NOISE_SIZE: usize = 128;

/// Tiling noise for fog, grey from 0 to 1: smooth value noise in three
/// octaves (4, 8 and 16 lattice cells a side), each wrapping at the edge,
/// so the texture repeats without seams. Filtered linearly and repeated.
fn fog_noise() -> Image {
    // A random value from 0 to 1 for lattice point (x, y) of an octave.
    let lattice = |x: usize, y: usize, octave: u32| {
        let mut h = (x as u32).wrapping_mul(0x27d4_eb2d)
            ^ (y as u32).wrapping_mul(0x1656_67b1)
            ^ octave.wrapping_mul(0x9e37_79b9);
        h = (h ^ (h >> 15)).wrapping_mul(0x85eb_ca6b);
        h = (h ^ (h >> 13)).wrapping_mul(0xc2b2_ae35);
        (h ^ (h >> 16)) as f32 / u32::MAX as f32
    };
    let octaves = [(4, 0.55), (8, 0.3), (16, 0.15)];
    let mut values = vec![0.0f32; NOISE_SIZE * NOISE_SIZE];
    for (y, row) in values.chunks_mut(NOISE_SIZE).enumerate() {
        for (x, value) in row.iter_mut().enumerate() {
            for (octave, &(cells, weight)) in octaves.iter().enumerate() {
                let at = Vec2::new(x as f32, y as f32) * cells as f32 / NOISE_SIZE as f32;
                let corner = at.floor();
                let t = at - corner;
                let t = t * t * (Vec2::splat(3.0) - 2.0 * t);
                let (x0, y0) = (corner.x as usize, corner.y as usize);
                let point = |dx: usize, dy: usize| {
                    lattice((x0 + dx) % cells, (y0 + dy) % cells, octave as u32)
                };
                let top = point(0, 0) + (point(1, 0) - point(0, 0)) * t.x;
                let bottom = point(0, 1) + (point(1, 1) - point(0, 1)) * t.x;
                *value += (top + (bottom - top) * t.y) * weight;
            }
        }
    }
    // Stretched to the full range.
    let (low, high) = values.iter().fold((f32::MAX, f32::MIN), |(low, high), &v| {
        (low.min(v), high.max(v))
    });
    let texels = values
        .iter()
        .flat_map(|&v| {
            let grey = ((v - low) / (high - low) * 255.0).round() as u8;
            [grey, grey, grey, 255]
        })
        .collect();
    let mut image = Image::new(
        Extent3d {
            width: NOISE_SIZE as u32,
            height: NOISE_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        texels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

/// `image`, filtered linearly: one read blends it between cells.
fn filtered(mut image: Image) -> Image {
    image.sampler = ImageSampler::linear();
    image
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

/// How far around a cell (in cells, a square) fog looks for open floor.
const OPEN_RADIUS: i32 = 2;

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
            bloom: Vec::new(),
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
        self.bloom = vec![0; texels * 4];
        self.air = vec![0; texels * 4];
    }

    /// Bakes the light in the air above each texel of `texels` (a `size`
    /// grid): into `bloom` the light seen now that is bright enough to
    /// bloom, blurred, and into `air` the haze, the light the air holds
    /// along `haze` from each cell, averaged, with as alpha how open the
    /// floor around the cell is (the share of cells within `OPEN_RADIUS`
    /// that are not `walls`), which fog keeps to.
    fn bake(&mut self, texels: &[u8], walls: &[bool], size: IVec2, haze: Vec2) {
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
                let bloom = self.bright[texel] * BLOOM_STRENGTH;
                let air = gathered / HAZE_STEPS as f32 * HAZE_STRENGTH;
                let mut open = 0;
                for dy in -OPEN_RADIUS..=OPEN_RADIUS {
                    for dx in -OPEN_RADIUS..=OPEN_RADIUS {
                        let (nx, ny) = (x + dx, y + dy);
                        let inside = nx >= 0 && ny >= 0 && nx < size.x && ny < size.y;
                        // Past the frame's edge counts as open.
                        if !inside || !walls[(ny * size.x + nx) as usize] {
                            open += 1;
                        }
                    }
                }
                let around = (2 * OPEN_RADIUS + 1).pow(2);
                let open = (open * 255 / around) as u8;
                for (out, light, alpha) in
                    [(&mut self.bloom, bloom, 255), (&mut self.air, air, open)]
                {
                    out[texel * 4..texel * 4 + 4].copy_from_slice(&[
                        self.srgb[srgb_step(light.x)],
                        self.srgb[srgb_step(light.y)],
                        self.srgb[srgb_step(light.z)],
                        alpha,
                    ]);
                }
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
            glow: Handle::default(),
            air: Handle::default(),
            haze: Vec2::NAN,
            size: IVec2::ZERO,
            since_change: None,
            texels: Vec::new(),
            walls: Vec::new(),
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
        // sRGB keeps the glow's faint edge from coming in steps.
        self.glow = images.add(filtered(light_image(size, TextureFormat::Rgba8UnormSrgb)));
        self.air = images.add(filtered(light_image(size, TextureFormat::Rgba8UnormSrgb)));
        self.haze = Vec2::NAN;
        self.since_change = None;
        self.blend = 1.0;
        self.texels = vec![0; texels * 4];
        self.walls = vec![false; texels];
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

    /// Fills `walls` from the map, for the frame at `origin`.
    fn build_walls(&mut self, grid: &MapGrid, origin: IVec2) {
        for y in 0..self.size.y {
            for x in 0..self.size.x {
                let p = origin + IVec2::new(x, y);
                self.walls[(y * self.size.x + x) as usize] =
                    !grid.is_valid(p) || grid.blocks_vision(p);
            }
        }
    }

    /// Takes the frame's light, built into the texels, as the current
    /// light, built for `origin`: written over the older pair, which
    /// becomes current, and faded to from the one shown so far. The first
    /// light, or with `at_once`, is shown at once. With `haze` its air is
    /// baked too, the haze going that way, and shown at once.
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
            self.bake_glow(haze, images);
        }
        if at_once {
            self.previous.origin = origin;
            self.previous.texels.copy_from_slice(&self.texels);
            upload(images, &self.previous.light, &self.previous.texels);
        }
        self.since_change = Some(if at_once { LIGHT_FADE } else { 0.0 });
        self.blend = if at_once { 1.0 } else { 0.0 };
    }

    /// Bakes the air above the current light, the haze going `haze`.
    fn bake_glow(&mut self, haze: Vec2, images: &mut Assets<Image>) {
        self.baker
            .bake(&self.current.texels, &self.walls, self.size, haze);
        self.haze = haze;
        upload(images, &self.glow, &self.baker.bloom);
        upload(images, &self.air, &self.baker.air);
    }

    /// Bakes the air again if it was baked for another haze direction (the
    /// view turned).
    fn turn_haze(&mut self, haze: Vec2, images: &mut Assets<Image>) {
        if self.haze.is_nan() || (self.haze - haze).length() >= HAZE_TURN {
            self.bake_glow(haze, images);
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
    mut composites: ResMut<Assets<CompositeMaterial>>,
) {
    let textures = LightTextures::new(IVec2::new(buffers.width, buffers.height), &mut images);
    let material = materials.add(GlowMaterial {
        camera: Vec4::ZERO,
        offset: Vec4::ZERO,
        frame: Vec4::ZERO,
        glow: textures.glow.clone(),
        air: textures.air.clone(),
        noise: images.add(fog_noise()),
        fog: Vec4::ZERO,
        trail: [Vec4::ZERO; TRAIL_POINTS],
    });
    // The main 2D camera never moves, nor does the glow camera; the quad
    // covers any window. Only the glow camera sees it.
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let entity = commands
        .spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(material.clone()),
            Transform::from_scale(Vec3::new(16384.0, 16384.0, 1.0)),
            Visibility::Hidden,
            RenderLayers::layer(GLOW_LAYER),
        ))
        .id();
    let target = images.add(glow_target(UVec2::ONE));
    let camera = commands
        .spawn((
            Camera2d,
            Camera {
                // Before the main camera, which adds what it draws.
                order: -2,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                is_active: false,
                ..default()
            },
            // A texel is `GLOW_DOWNSCALE` window pixels each way.
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: bevy::camera::ScalingMode::WindowSize,
                scale: GLOW_DOWNSCALE,
                ..OrthographicProjection::default_2d()
            }),
            RenderTarget::Image(target.clone().into()),
            RenderLayers::layer(GLOW_LAYER),
            // The glow is smooth: no edges to smooth.
            Msaa::Off,
        ))
        .id();
    let composite_material = composites.add(CompositeMaterial { glow: target });
    let composite = commands
        .spawn((
            Mesh2d(quad),
            MeshMaterial2d(composite_material.clone()),
            Transform::from_xyz(0.0, 0.0, OVERLAY_Z),
            Visibility::Hidden,
        ))
        .id();
    commands.insert_resource(GlowQuad {
        entity,
        material,
        camera,
        composite,
        composite_material,
        target: UVec2::ONE,
    });
    commands.insert_resource(textures);
}

/// Sizes the glow's texture to the window (a texel per `GLOW_DOWNSCALE`
/// pixels each way) and the quad adding it to cover it, and runs the glow
/// camera only while light glows.
fn fit_glow_to_window(
    effects: Res<LightEffects>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut quad: ResMut<GlowQuad>,
    mut images: ResMut<Assets<Image>>,
    mut composites: ResMut<Assets<CompositeMaterial>>,
    mut cameras: Query<(&mut Camera, &mut RenderTarget)>,
    mut composite: Query<(&mut Transform, &mut Visibility)>,
) {
    let shown = effects.glows();
    if let Ok((mut camera, _)) = cameras.get_mut(quad.camera) {
        if camera.is_active != shown {
            camera.is_active = shown;
        }
    }
    if let Ok((_, mut visibility)) = composite.get_mut(quad.composite) {
        visibility.set_if_neq(if shown {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let size = (window.size() / GLOW_DOWNSCALE)
        .ceil()
        .as_uvec2()
        .max(UVec2::ONE);
    if !shown || size == quad.target {
        return;
    }
    quad.target = size;
    let image = images.add(glow_target(size));
    if let Ok((_, mut target)) = cameras.get_mut(quad.camera) {
        *target = RenderTarget::Image(image.clone().into());
    }
    if let Some(mut material) = composites.get_mut(&quad.composite_material) {
        material.glow = image;
    }
    // The texture covers its texels times the downscale, a little past the
    // window's edge when the window does not divide evenly.
    if let Ok((mut transform, _)) = composite.get_mut(quad.composite) {
        let covers = size.as_vec2() * GLOW_DOWNSCALE;
        transform.scale = covers.extend(1.0);
    }
}

/// Sends the frame's light to the GPU when it changes (with `Glow`, baking
/// the air above it too, again when the view turns), and moves the fade to
/// it on.
pub(super) fn update_light_textures(
    time: Res<Time>,
    effects: Res<LightEffects>,
    camera: Res<ViewCamera>,
    volume: Option<Res<WallVolume>>,
    (buffers, grid): (Res<RenderBuffers>, Res<MapGrid>),
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
    let haze = effects
        .glows()
        .then(|| haze_direction(camera.rotation, super::volume::lean(volume)));
    // Light shown while the effects were off is stale: show the light now.
    // Walls that move change what light falls where, so they are read again
    // with the light.
    if textures.build_light(&buffers) || effects.is_changed() {
        textures.build_walls(&grid, buffers.origin);
        textures.change(buffers.origin, haze, effects.is_changed(), &mut images);
    } else if let Some(haze) = haze {
        textures.turn_haze(haze, &mut images);
    }
}

/// With `Fog`, leaves a trail point behind each mover as it goes.
fn stir_fog(
    time: Res<Time>,
    effects: Res<LightEffects>,
    movers: Query<(Entity, &AnimatedPos), With<Speed>>,
    mut trail: ResMut<FogTrail>,
) {
    if *effects != LightEffects::Fog {
        return;
    }
    let now = time.elapsed_secs_wrapped();
    for (mover, shown) in &movers {
        trail.track(mover, shown.position, now);
    }
    trail.age(now);
}

/// Shows the glow quad with `Glow` or `Fog` and passes it the view, the
/// light and, for fog, the time it drifts by and the movers' trail.
#[allow(clippy::too_many_arguments)]
fn draw_glow(
    effects: Res<LightEffects>,
    camera: Res<ViewCamera>,
    buffers: Res<RenderBuffers>,
    (textures, trail): (Res<LightTextures>, Res<FogTrail>),
    quad: Res<GlowQuad>,
    mut materials: ResMut<Assets<GlowMaterial>>,
    mut visibility: Query<&mut Visibility>,
) {
    let shown = effects.glows();
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
    // The shader drifts the fog by the time itself (`globals.time`), so
    // nothing here changes while the view, the light and the fog's wakes
    // stay still.
    let fog = if *effects == LightEffects::Fog {
        Vec4::new(1.0, trail.live as f32, 0.0, 0.0)
    } else {
        Vec4::ZERO
    };
    let uniforms = [
        camera.position.extend(camera.rotation).extend(camera.zoom),
        camera.offset.extend(CELL_SIZE.y).extend(0.0),
        textures
            .current
            .origin
            .as_vec2()
            .extend(buffers.width as f32)
            .extend(buffers.height as f32),
        fog,
    ];
    let unchanged = materials.get(&quad.material).is_some_and(|material| {
        [
            material.camera,
            material.offset,
            material.frame,
            material.fog,
        ] == uniforms
            && material.glow == textures.glow
            && material.air == textures.air
            && material.trail == trail.wakes
    });
    if !unchanged {
        if let Some(mut material) = materials.get_mut(&quad.material) {
            [
                material.camera,
                material.offset,
                material.frame,
                material.fog,
            ] = uniforms;
            material.glow = textures.glow.clone();
            material.air = textures.air.clone();
            material.trail = trail.wakes;
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
            .bake(&textures.texels, &[false; 64], size, Vec2::new(100.0, 0.0));
        let glow = |x, y| texel(&textures.baker.bloom, x, y)[0];
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
        let walls = textures.walls.clone();
        textures.baker.bake(&textures.texels, &walls, size, haze);
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

    #[test]
    fn fog_noise_fills_its_range_and_tiles_without_seams() {
        let image = fog_noise();
        let data = image.data.unwrap();
        let at = |x: usize, y: usize| data[(y * NOISE_SIZE + x) * 4] as i32;
        let values = (0..NOISE_SIZE * NOISE_SIZE).map(|i| data[i * 4]);
        assert_eq!(values.clone().min(), Some(0));
        assert_eq!(values.max(), Some(255));
        // Across the edge it changes no more than between neighbours inside.
        let mut inside = 0;
        let mut across = 0;
        for i in 0..NOISE_SIZE {
            for j in 0..NOISE_SIZE - 1 {
                inside = inside.max((at(j, i) - at(j + 1, i)).abs());
                inside = inside.max((at(i, j) - at(i, j + 1)).abs());
            }
            across = across.max((at(NOISE_SIZE - 1, i) - at(0, i)).abs());
            across = across.max((at(i, NOISE_SIZE - 1) - at(i, 0)).abs());
        }
        assert!(across <= inside, "{across} > {inside}");
    }

    #[test]
    fn fog_keeps_to_open_floor() {
        let mut images = Assets::default();
        let mut textures = LightTextures::new(IVec2::new(8, 8), &mut images);
        let buffers = RenderBuffers::new(8, 8);
        textures.build_light(&buffers);
        // A wall down the middle, two cells thick.
        let mut walls = [false; 64];
        for y in 0..8 {
            walls[y * 8 + 3] = true;
            walls[y * 8 + 4] = true;
        }
        let size = textures.size;
        textures
            .baker
            .bake(&textures.texels, &walls, size, Vec2::ZERO);
        let open = |x: usize| textures.baker.air[(4 * 8 + x) * 4 + 3];
        assert!(open(5) < open(6) && open(6) < open(7), "thinner near walls");
        assert_eq!(open(7), 255, "open past the wall's reach");
    }

    #[test]
    fn movers_leave_a_trail_point_every_little_way() {
        let mut trail = FogTrail::default();
        let mover = Entity::from_raw_u32(1).unwrap();
        trail.track(mover, Vec2::ZERO, 0.0);
        assert_eq!(trail.points[0].w, 0.0, "standing still leaves none");
        trail.track(mover, Vec2::new(0.2, 0.0), 0.1);
        assert_eq!(trail.points[0].w, 0.0, "not yet far enough");
        trail.track(mover, Vec2::new(0.4, 0.0), 0.2);
        assert_eq!(trail.points[0], Vec4::new(0.4, 0.0, 0.2, 1.0));
        // The ring reuses its oldest points.
        for step in 1..=TRAIL_POINTS {
            trail.track(mover, Vec2::new(0.4 + step as f32, 0.0), 0.2);
        }
        assert_eq!(trail.points[0].x, 0.4 + TRAIL_POINTS as f32);
    }

    #[test]
    fn only_wakes_still_stirring_go_to_the_shader() {
        let mut trail = FogTrail::default();
        let mover = Entity::from_raw_u32(1).unwrap();
        trail.track(mover, Vec2::ZERO, 0.0);
        trail.track(mover, Vec2::new(1.0, 0.0), 0.0);
        trail.track(mover, Vec2::new(2.0, 0.0), 3.0);
        trail.age(3.0);
        assert_eq!(trail.live, 2);
        // New: full strength, its first width; older: weaker and wider.
        assert_eq!(trail.wakes[1], Vec4::new(2.0, 0.0, WAKE_WIDTH, 1.0));
        assert!(trail.wakes[0].w < 0.1 && trail.wakes[0].z > WAKE_WIDTH);
        // Once settled, none: the shader loops over nothing.
        trail.age(3.0 + WAKE_LIFE * 4.0);
        assert_eq!(trail.live, 0);
        assert_eq!(trail.wakes, [Vec4::ZERO; TRAIL_POINTS]);
    }

    /// The frame slot of cell (x, y) of an 8x8 frame at the origin.
    fn buffers_index(x: i32, y: i32) -> usize {
        RenderBuffers::new(8, 8).idx(IVec2::new(x, y)).unwrap()
    }
}
