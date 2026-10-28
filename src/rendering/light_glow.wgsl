// Bloom and haze over the text renderer: see `light.rs`. Added to what is
// already drawn.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import gridvail::view::{blend, blend_corner, on_frame, to_map, to_map_offset}

// xy: the map point at the screen centre, z: rotation, w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// xy: the view's offset in cells, zw: which way height goes on screen.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> offset_lean: vec4<f32>;
// xy: the frame's origin, zw: its size, in cells.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> frame: vec4<f32>;
// x: cell px, y: how high the haze reaches, in px at zoom 1, z: bloom
// strength, w: haze strength.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;
// The light each frame cell shows; alpha 1 where it is seen now.
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var light_map: texture_2d<f32>;
// The brightest light, spread over a few cells.
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var glow_map: texture_2d<f32>;

// Points along the view ray the haze is gathered from.
const HAZE_STEPS: i32 = 8;

fn light_texel(cell: vec2<i32>) -> vec4<f32> {
    if !on_frame(cell, frame) {
        return vec4<f32>(0.0);
    }
    return textureLoad(light_map, cell, 0);
}

fn glow_texel(cell: vec2<i32>) -> vec4<f32> {
    if !on_frame(cell, frame) {
        return vec4<f32>(0.0);
    }
    return textureLoad(glow_map, cell, 0);
}

// The light and glow maps blended between cells at map point `map`.
fn light_at(map: vec2<f32>) -> vec4<f32> {
    let c = blend_corner(map, frame);
    let texels = array<vec4<f32>, 4>(
        light_texel(c),
        light_texel(c + vec2<i32>(1, 0)),
        light_texel(c + vec2<i32>(0, 1)),
        light_texel(c + vec2<i32>(1, 1)),
    );
    return blend(map, frame, texels);
}

fn glow_at(map: vec2<f32>) -> vec4<f32> {
    let c = blend_corner(map, frame);
    let texels = array<vec4<f32>, 4>(
        glow_texel(c),
        glow_texel(c + vec2<i32>(1, 0)),
        glow_texel(c + vec2<i32>(0, 1)),
        glow_texel(c + vec2<i32>(1, 1)),
    );
    return blend(map, frame, texels);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = in.world_position.xy;
    let floor_point = to_map(pixel, camera, offset_lean.xy, params.x);
    // The bright light around the floor point blooms.
    var color = glow_at(floor_point).rgb * params.z;
    // The view ray leaves the floor point toward the viewer, rising: the air
    // it passes through glows with the light there. Light seen now only,
    // and bright light far more than dim (its square), so lamps hang in a
    // haze while dim cells stay clear.
    let rise = to_map_offset(offset_lean.zw, camera, params.x);
    let height = params.y * camera.w;
    var haze = vec3<f32>(0.0);
    for (var i = 0; i < HAZE_STEPS; i = i + 1) {
        let h = (f32(i) + 0.5) / f32(HAZE_STEPS) * height;
        let point = floor_point - rise * h;
        let light = light_at(point);
        haze += light.rgb * light.rgb * light.a;
    }
    color += haze / f32(HAZE_STEPS) * params.w;
    return vec4<f32>(color, 1.0);
}
