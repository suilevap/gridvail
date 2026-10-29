// Figures standing in the text renderer: see `figures.rs`.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::view
#import gridvail::view::{frame_uv, on_texture, to_map, to_map_offset}

const MAX_PARTS: i32 = 12;

// xy: the map point at the screen centre, z: rotation, w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// xy: the view's offset in cells, zw: the lean of height on screen.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> offset_lean: vec4<f32>;
// xy: the frame's origin, zw: its size, in cells.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> frame: vec4<f32>;
// x: cell px, y: atlas texels per glyph, z: the figure's top in cells,
// w: its draw order.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;
// xy: where the figure is, in map cells, z: how many parts it draws.
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> figure: vec4<f32>;
// rgb: the light it is in, w: 1 to take it from the light maps.
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<uniform> color: vec4<f32>;
// Three for each part: (offset in map cells, bottom, top), (cos and sin of
// its turn, atlas corner), (tint, scale).
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var<uniform> parts: array<vec4<f32>, 36>;
// The cell font's glyphs: ink as alpha.
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var light_previous: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var light_current: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var light_sampler: sampler;
// y: how far the light has faded from the previous map to the current.
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var<uniform> lighting: vec4<f32>;
// xy: the previous light map's frame origin, zw: the current one's.
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var<uniform> light_origins: vec4<f32>;

// The light shown at map point `map`, as the wall shader blends it.
fn light_at(map: vec2<f32>) -> vec3<f32> {
    let now_uv = frame_uv(map, light_origins.zw, frame.zw);
    let now = select(vec4<f32>(0.0), textureSampleLevel(light_current, light_sampler, now_uv, 0.0), on_texture(now_uv));
    if lighting.y >= 1.0 {
        return now.rgb;
    }
    let uv = frame_uv(map, light_origins.xy, frame.zw);
    let before = select(vec4<f32>(0.0), textureSampleLevel(light_previous, light_sampler, uv, 0.0), on_texture(uv));
    return mix(before.rgb, now.rgb, lighting.y);
}

struct Fragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

@fragment
fn fragment(in: VertexOutput) -> Fragment {
    let pixel = in.world_position.xy;
    let lean = offset_lean.zw;
    // Screen px a cell of height spans.
    let px = params.x * camera.w;
    let top = params.z * px;
    let texels = params.y;
    let count = min(i32(figure.z), MAX_PARTS);
    // The map point under the pixel at height h (px) is
    // `floor_point - rise * h`, as in the wall shader.
    let floor_point = to_map(pixel, camera, offset_lean.xy, params.x);
    let rise = to_map_offset(lean, camera, params.x);
    let steps = i32(ceil(top));
    var out: Fragment;
    for (var i = 0; i <= steps; i = i + 1) {
        let h_px = max(top - f32(i), 0.0);
        let h = h_px / px;
        let map = floor_point - rise * h_px;
        let local = map - figure.xy;
        // The first part found standing here, in the order listed.
        for (var k = 0; k < count; k = k + 1) {
            let a = parts[k * 3];
            if h < a.z || h > a.w {
                continue;
            }
            let b = parts[k * 3 + 1];
            let c = parts[k * 3 + 2];
            // Into the glyph's own cell: unturned, unscaled.
            let d = local - a.xy;
            let q = vec2<f32>(b.x * d.x + b.y * d.y, -b.y * d.x + b.x * d.y) / c.w;
            let texel = vec2<i32>(floor((q + 0.5) * texels));
            if any(texel < vec2<i32>(0)) || any(texel >= vec2<i32>(i32(texels))) {
                continue;
            }
            let ink = textureLoad(atlas, vec2<i32>(b.zw) + texel, 0).a;
            if ink < 0.5 {
                continue;
            }
            var light = color.rgb;
            if color.w > 0.5 {
                light = light_at(map);
            }
            // Within a pixel of its top, the part's top, in its own
            // light; its side darker toward its bottom.
            let span = max(a.w - a.z, 1e-4);
            let at_top = h > a.w - 1.0 / px;
            let shade = select(0.4 + 0.3 * (h - a.z) / span, 1.0, at_top);
            out.color = vec4<f32>(light * c.rgb * shade, 1.0);
            let clip = view.clip_from_world * vec4<f32>(pixel, params.w, 1.0);
            out.depth = clip.z / clip.w;
            return out;
        }
    }
    discard;
    return out;
}
