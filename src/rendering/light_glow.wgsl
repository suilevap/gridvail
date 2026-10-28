// Bloom and haze over the text renderer: see `light.rs`. Added to what is
// already drawn. Both are baked per cell into textures, so this reads each
// once a pixel at the floor point the pixel shows, filtered linearly
// between cells by the GPU. With fog, the haze is stirred by two layers of
// tiling noise drifting over the map.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import gridvail::view::{frame_uv, on_texture, to_map}

// xy: the map point at the screen centre, z: rotation, w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// xy: the view's offset in cells, z: cell px.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> offset: vec4<f32>;
// xy: the textures' frame origin, zw: their size in cells.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> frame: vec4<f32>;
// Bloom, and haze: the light in the air above each frame cell.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var glow: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var glow_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var air: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var air_sampler: sampler;
// Tiling noise, repeated.
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var noise: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var noise_sampler: sampler;
// x: 1 for fog, 0 for the haze as it is; y: seconds (wrapping hourly).
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var<uniform> fog: vec4<f32>;

// The fog's two layers: how often the noise repeats per map cell, and how
// far it drifts per second. Each drift over an hour is a whole number of
// repeats, so the hourly wrap of the time does not jump.
const NEAR_SCALE: f32 = 0.06;
const NEAR_DRIFT: vec2<f32> = vec2<f32>(0.02, 0.01);
const FAR_SCALE: f32 = 0.11;
const FAR_DRIFT: vec2<f32> = vec2<f32>(-0.0125, 0.0175);

// How thick the fog is at map point `map`: about 1 on average, from clear
// to more than twice as thick.
fn fog_density(map: vec2<f32>) -> f32 {
    let near_uv = map * NEAR_SCALE + NEAR_DRIFT * fog.y;
    let far_uv = map * FAR_SCALE + FAR_DRIFT * fog.y;
    let near = textureSampleLevel(noise, noise_sampler, near_uv, 0.0).r;
    let far = textureSampleLevel(noise, noise_sampler, far_uv, 0.0).r;
    return smoothstep(0.2, 0.8, near * 0.65 + far * 0.35) * 2.2;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let map = to_map(in.world_position.xy, camera, offset.xy, offset.z);
    let uv = frame_uv(map, frame.xy, frame.zw);
    let bloom = textureSampleLevel(glow, glow_sampler, uv, 0.0).rgb;
    var haze = textureSampleLevel(air, air_sampler, uv, 0.0).rgb;
    if fog.x > 0.5 {
        haze *= fog_density(map);
    }
    return vec4<f32>(select(vec3<f32>(0.0), bloom + haze, on_texture(uv)), 1.0);
}
