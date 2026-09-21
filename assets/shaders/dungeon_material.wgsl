#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> base_color: vec4<f32>;

// x/y: broad/fine strength, z/w: broad/fine world-space frequency.
@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var<uniform> variation: vec4<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(2)
var light_map: texture_2d<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(3)
var light_map_sampler: sampler;

// xy: map dimensions, zw: cell dimensions; zero dimensions select solid mode.
@group(#{MATERIAL_BIND_GROUP}) @binding(4)
var<uniform> map: vec4<f32>;

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

fn voronoi(point: vec2<f32>) -> f32 {
    let cell = floor(point);
    let fraction = fract(point);
    var nearest = 8.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let neighbor = vec2<f32>(f32(x), f32(y));
            let seed = cell + neighbor;
            let feature = neighbor + vec2<f32>(hash(seed), hash(seed + 41.37));
            nearest = min(nearest, distance(feature, fraction));
        }
    }
    return nearest;
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

    var surface_color = vec4<f32>(1.0);
    var coverage = 1.0;
    if map.x > 0.5 {
        let uv = vec2<f32>(
            world.x / (map.x * map.z) + 0.5,
            0.5 - world.y / (map.y * map.w),
        );
        surface_color = textureSample(light_map, light_map_sampler, clamp(uv, vec2(0.0), vec2(1.0)));
        let organic = (value_noise(world * 0.075) - 0.5) * 0.42
            + (voronoi(world * 0.045) - 0.5) * 0.28;
        coverage = smoothstep(0.16, 0.84, surface_color.a + organic);
        if coverage < 0.015 {
            discard;
        }
    }
    return vec4<f32>(
        base_color.rgb * surface_color.rgb * vertex_color.rgb * mottling,
        base_color.a * coverage,
    );
}
