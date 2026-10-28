// Bloom and haze over the text renderer: see `light.rs`. Added to what is
// already drawn. Both are baked per cell into the glow textures, so this
// only looks up the light in the air above the floor point each pixel
// shows, blended between cells.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import gridvail::view::{blend, blend_corner, on_frame, to_map}

// xy: the map point at the screen centre, z: rotation, w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// xy: the view's offset in cells.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> offset: vec4<f32>;
// xy: the frame's size in cells, z: how far the light has faded from the
// previous textures to the current ones, w: cell px.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> frame: vec4<f32>;
// xy: the previous textures' frame origin, zw: the current ones'.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> origins: vec4<f32>;
// The light in the air above each frame cell: before the light's latest
// change, and the latest.
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var glow_previous: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var glow_current: texture_2d<f32>;

// One function a texture: WebGL2 does not take textures as arguments.
fn previous_texel(cell: vec2<i32>) -> vec4<f32> {
    if !on_frame(cell, vec4<f32>(origins.xy, frame.xy)) {
        return vec4<f32>(0.0);
    }
    return textureLoad(glow_previous, cell, 0);
}

fn current_texel(cell: vec2<i32>) -> vec4<f32> {
    if !on_frame(cell, vec4<f32>(origins.zw, frame.xy)) {
        return vec4<f32>(0.0);
    }
    return textureLoad(glow_current, cell, 0);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let map = to_map(in.world_position.xy, camera, offset.xy, frame.w);
    let at = vec4<f32>(origins.zw, frame.xy);
    var c = blend_corner(map, at);
    var glow = blend(map, at, array<vec4<f32>, 4>(
        current_texel(c),
        current_texel(c + vec2<i32>(1, 0)),
        current_texel(c + vec2<i32>(0, 1)),
        current_texel(c + vec2<i32>(1, 1)),
    ));
    // Fading from the previous light, which is skipped once faded.
    if frame.z < 1.0 {
        let before_at = vec4<f32>(origins.xy, frame.xy);
        c = blend_corner(map, before_at);
        let before = blend(map, before_at, array<vec4<f32>, 4>(
            previous_texel(c),
            previous_texel(c + vec2<i32>(1, 0)),
            previous_texel(c + vec2<i32>(0, 1)),
            previous_texel(c + vec2<i32>(1, 1)),
        ));
        glow = mix(before, glow, frame.z);
    }
    return vec4<f32>(glow.rgb, 1.0);
}
