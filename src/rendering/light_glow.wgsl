// Bloom and haze over the text renderer: see `light.rs`. Added to what is
// already drawn. Both are baked per cell into the glow texture, so this
// reads it once a pixel at the floor point the pixel shows, filtered
// linearly between cells by the GPU.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import gridvail::view::{frame_uv, on_texture, to_map}

// xy: the map point at the screen centre, z: rotation, w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// xy: the view's offset in cells, z: cell px.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> offset: vec4<f32>;
// xy: the glow texture's frame origin, zw: its size in cells.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> frame: vec4<f32>;
// The light in the air above each frame cell.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var glow: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var glow_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let map = to_map(in.world_position.xy, camera, offset.xy, offset.z);
    let uv = frame_uv(map, frame.xy, frame.zw);
    let air = textureSampleLevel(glow, glow_sampler, uv, 0.0).rgb;
    return vec4<f32>(select(vec3<f32>(0.0), air, on_texture(uv)), 1.0);
}
