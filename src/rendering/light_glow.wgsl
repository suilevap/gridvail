// Bloom and haze over the text renderer: see `light.rs`. Added to what is
// already drawn. Both are baked per cell into textures, so this reads each
// once a pixel at the floor point the pixel shows, filtered linearly
// between cells by the GPU. With fog, the haze is stirred by two layers of
// tiling noise drifting over the map, keeps to the open floor between
// walls, and parts around things moving through it.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import gridvail::view::{frame_uv, on_texture, to_map}

// xy: the map point at the screen centre, z: rotation, w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// xy: the view's offset in cells, z: cell px.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> offset: vec4<f32>;
// xy: the textures' frame origin, zw: their size in cells.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> frame: vec4<f32>;
// Bloom, and haze: the light in the air above each frame cell (the haze's
// alpha: how open the floor around the cell is).
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var glow: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var glow_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var air: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var air_sampler: sampler;
// Tiling noise, repeated.
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var noise: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var noise_sampler: sampler;
// x: 1 for fog, 0 for the haze as it is; y: seconds (wrapping hourly).
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var<uniform> fog: vec4<f32>;
// Where things moving through the fog have just been: xy the map point, z
// the time they were there, w 1 (0: none).
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var<uniform> trail: array<vec4<f32>, 32>;

// How long a mover's wake lasts (seconds for it to fade to 1/e), how wide
// it starts and how fast it spreads (cells, cells a second), and how much
// it clears and swirls the fog.
const WAKE_LIFE: f32 = 1.2;
const WAKE_WIDTH: f32 = 0.7;
const WAKE_SPREAD: f32 = 0.6;
const WAKE_CLEARS: f32 = 0.85;
const WAKE_SWIRL: f32 = 0.08;

// The fog's two layers: how often the noise repeats per map cell, and how
// far it drifts per second. Each drift over an hour is a whole number of
// repeats, so the hourly wrap of the time does not jump.
const NEAR_SCALE: f32 = 0.06;
const NEAR_DRIFT: vec2<f32> = vec2<f32>(0.02, 0.01);
const FAR_SCALE: f32 = 0.11;
const FAR_DRIFT: vec2<f32> = vec2<f32>(-0.0125, 0.0175);

// How thick the fog is at map point `map`, where the floor around is
// `open` (0 walled in, 1 open): about 1 on average in the open, from clear
// to more than twice as thick, thinning against walls and in movers' wakes,
// which also swirl it.
fn fog_density(map: vec2<f32>, open: f32) -> f32 {
    var cleared = 0.0;
    var swirl = vec2<f32>(0.0);
    for (var i = 0; i < 32; i = i + 1) {
        let point = trail[i];
        let age = fog.y - point.z;
        if point.w == 0.0 || age < 0.0 || age > WAKE_LIFE * 4.0 {
            continue;
        }
        let width = WAKE_WIDTH + age * WAKE_SPREAD;
        let d = map - point.xy;
        let weight = exp(-age / WAKE_LIFE) * exp(-dot(d, d) / (width * width));
        cleared += weight;
        swirl += vec2<f32>(-d.y, d.x) * weight;
    }
    let stirred = map + swirl * WAKE_SWIRL / NEAR_SCALE;
    let near_uv = stirred * NEAR_SCALE + NEAR_DRIFT * fog.y;
    let far_uv = stirred * FAR_SCALE + FAR_DRIFT * fog.y;
    let near = textureSampleLevel(noise, noise_sampler, near_uv, 0.0).r;
    let far = textureSampleLevel(noise, noise_sampler, far_uv, 0.0).r;
    let density = smoothstep(0.2, 0.8, near * 0.65 + far * 0.35) * 2.2;
    let walls = smoothstep(0.3, 0.9, open);
    return density * walls * (1.0 - WAKE_CLEARS * clamp(cleared, 0.0, 1.0));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let map = to_map(in.world_position.xy, camera, offset.xy, offset.z);
    let uv = frame_uv(map, frame.xy, frame.zw);
    let bloom = textureSampleLevel(glow, glow_sampler, uv, 0.0).rgb;
    let air_here = textureSampleLevel(air, air_sampler, uv, 0.0);
    var haze = air_here.rgb;
    if fog.x > 0.5 {
        haze *= fog_density(map, air_here.a);
    }
    return vec4<f32>(select(vec3<f32>(0.0), bloom + haze, on_texture(uv)), 1.0);
}
