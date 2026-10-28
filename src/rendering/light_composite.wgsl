// Adds the glow, drawn at a fraction of the window's resolution
// (`light.rs`), to what is drawn, scaled up with linear filtering.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var glow: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var glow_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(glow, glow_sampler, in.uv, 0.0).rgb, 1.0);
}
