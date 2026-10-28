// Bloom and haze over the text renderer: see `light.rs`. Added to what is
// already drawn. Both are baked per cell into textures, so this reads each
// once a pixel at the floor point the pixel shows, filtered linearly
// between cells by the GPU. With fog, the haze is stirred by two layers of
// tiling noise drifting over the map, keeps to the open floor between
// walls, and parts around things moving through it.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals
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
// x: 1 for fog, 0 for the haze as it is; y: how many of `trail` are live.
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var<uniform> fog: vec4<f32>;
// The wakes of things moving through the fog, live ones first: xy the map
// point, z its width in cells, w its strength.
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var<uniform> trail: array<vec4<f32>, 32>;

// How much a wake clears and swirls the fog.
const WAKE_CLEARS: f32 = 0.85;
const WAKE_SWIRL: f32 = 0.08;
// Past this many widths a wake does not reach.
const WAKE_REACH: f32 = 3.0;

// The fog's two layers: how often the noise repeats per map cell, and how
// far it drifts per second. Each drift over an hour is a whole number of
// repeats, so the hourly wrap of the time (`globals.time`) does not jump.
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
    let live = i32(fog.y);
    for (var i = 0; i < live; i = i + 1) {
        let wake = trail[i];
        let d = map - wake.xy;
        let spread = dot(d, d) / (wake.z * wake.z);
        if spread > WAKE_REACH * WAKE_REACH {
            continue;
        }
        let weight = wake.w * exp(-spread);
        cleared += weight;
        swirl += vec2<f32>(-d.y, d.x) * weight;
    }
    let time = globals.time;
    let stirred = map + swirl * WAKE_SWIRL / NEAR_SCALE;
    let near_uv = stirred * NEAR_SCALE + NEAR_DRIFT * time;
    let far_uv = stirred * FAR_SCALE + FAR_DRIFT * time;
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
