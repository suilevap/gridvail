#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> base_color: vec4<f32>;

// x/y: broad/fine strength, z/w: broad/fine world-space frequency.
@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var<uniform> variation: vec4<f32>;

fn hash(point: vec2<f32>) -> f32 {
    return fract(sin(dot(point, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn value_noise(point: vec2<f32>) -> f32 {
    let cell = floor(point);
    let fraction = fract(point);
    let blend = fraction * fraction * (3.0 - 2.0 * fraction);
    let a = hash(cell);
    let b = hash(cell + vec2<f32>(1.0, 0.0));
    let c = hash(cell + vec2<f32>(0.0, 1.0));
    let d = hash(cell + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, blend.x), mix(c, d, blend.x), blend.y);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world = in.world_position.xy;
    let broad = value_noise(world * variation.z) - 0.5;
    let fine = value_noise(world * variation.w + vec2<f32>(19.7, 7.3)) - 0.5;
    let mottling = max(0.55, 1.0 + broad * variation.x + fine * variation.y);

    var vertex_color = vec4<f32>(1.0);
#ifdef VERTEX_COLORS
    vertex_color = in.color;
#endif
    return vec4<f32>(base_color.rgb * vertex_color.rgb * mottling, base_color.a);
}
