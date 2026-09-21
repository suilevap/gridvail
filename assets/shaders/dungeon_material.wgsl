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

// Reconstruct the square CPU grid as a smooth field around jittered Voronoi
// sites. The simulation remains cell-based, but its visual influence no longer
// forms axis-aligned rectangles.
fn sample_jittered_light(world: vec2<f32>) -> vec4<f32> {
    var grid = vec2<f32>(
        world.x / map.z + map.x * 0.5,
        map.y * 0.5 - world.y / map.w,
    );
    let warp_point = grid * 0.17;
    let warp = vec2<f32>(
        value_noise(warp_point + vec2<f32>(13.7, 2.1)),
        value_noise(warp_point + vec2<f32>(4.3, 29.8)),
    ) - 0.5;
    grid += warp * 1.35;
    let center = vec2<i32>(floor(grid));
    let dimensions = vec2<i32>(i32(map.x), i32(map.y));
    var weighted_color = vec4<f32>(0.0);
    var total_weight = 0.0;

    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let cell = center + vec2<i32>(x, y);
            let seed = vec2<f32>(f32(cell.x), f32(cell.y));
            let jitter = (vec2<f32>(hash(seed), hash(seed + 53.19)) - 0.5) * 0.72;
            let site = seed + vec2<f32>(0.5) + jitter;
            let offset = grid - site;
            let weight = exp2(-dot(offset, offset) * 1.85);
            let texel = textureLoad(
                light_map,
                clamp(cell, vec2<i32>(0), dimensions - vec2<i32>(1)),
                0,
            );
            weighted_color += texel * weight;
            total_weight += weight;
        }
    }
    return weighted_color / max(total_weight, 0.0001);
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
        surface_color = sample_jittered_light(world);
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
