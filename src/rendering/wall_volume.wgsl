// Standing objects for the text renderer: see `volume.rs`.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::view

// xy: the map point at the screen centre, z: rotation, w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// xy: the view's offset in cells, zw: the lean of height on screen.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> offset_lean: vec4<f32>;
// xy: the frame's origin, zw: its size, in cells.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> frame: vec4<f32>;
// x: the tallest stature allowed, in px at zoom 1 (the wall map's alpha is
// height over it), y: cell px, z: wall map texels per cell, w: the tallest
// object in the frame, in px at zoom 1.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;
// Standing objects from above: lit colour times alpha, height as alpha.
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var wall_map: texture_2d<f32>;
// x: 1 when light fades with height above the floor.
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<uniform> shading: vec4<f32>;

// Light from the floor: an object's own light is full at the floor and
// fades by this factor per cell of height, so a taller wall's top is darker
// than a lower one's under the same light.
const FLOOR_LIGHT_FALLOFF: f32 = 0.9;
// Sides are this much darker than a top at the same height, so the glyph's
// outline still shows on top.
const SIDE_SHADE: f32 = 0.8;

struct Fragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

// A 2D world offset (y up) as a map offset (`ViewCamera::to_map` without
// the translation).
fn to_map_offset(screen: vec2<f32>) -> vec2<f32> {
    let v = vec2<f32>(screen.x, -screen.y) / (params.y * camera.w);
    let c = cos(camera.z);
    let s = sin(camera.z);
    return vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y);
}

// The map point shown at a 2D world point.
fn to_map(screen: vec2<f32>) -> vec2<f32> {
    let v = -offset_lean.xy;
    let c = cos(camera.z);
    let s = sin(camera.z);
    return to_map_offset(screen) + vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y) + camera.xy;
}

// Where a map point is shown (`view_translation`).
fn to_screen(map: vec2<f32>) -> vec2<f32> {
    let d = map - camera.xy;
    let c = cos(camera.z);
    let s = sin(camera.z);
    let turned = vec2<f32>(c * d.x + s * d.y, -s * d.x + c * d.y);
    let v = (turned + offset_lean.xy) * camera.w * params.y;
    return vec2<f32>(v.x, -v.y);
}

@fragment
fn fragment(in: VertexOutput) -> Fragment {
    let pixel = in.world_position.xy;
    let lean = offset_lean.zw;
    let full = params.x * camera.w;
    let top = params.w * camera.w;
    let map_size = vec2<i32>(frame.zw * params.z);
    // The pixel's ray is a straight line in map space: the map point under
    // the pixel at height h is `floor_point - toward_viewer * h`.
    let floor_point = to_map(pixel);
    let rise = to_map_offset(lean);
    // From the tallest top down to the floor, a screen pixel at a time: the
    // first object found is the nearest.
    let steps = i32(ceil(top));
    var out: Fragment;
    for (var i = 0; i <= steps; i = i + 1) {
        let h = max(top - f32(i), 0.0);
        let map = floor_point - rise * h;
        let texel = vec2<i32>(floor((map - frame.xy + 0.5) * params.z));
        if any(texel < vec2<i32>(0)) || any(texel >= map_size) {
            continue;
        }
        let wall = textureLoad(wall_map, texel, 0);
        let height = wall.a * full;
        if wall.a < 0.004 || h > height {
            continue;
        }
        let own = wall.rgb / wall.a;
        // Within a pixel of its height: the object's top, in its own light.
        let at_top = h > height - 1.0;
        var color: vec3<f32>;
        if shading.x > 0.5 {
            let cells_up = h / (params.y * camera.w);
            let fade = exp(-cells_up * FLOOR_LIGHT_FALLOFF);
            color = own * fade * select(SIDE_SHADE, 1.0, at_top);
        } else if at_top {
            color = own;
        } else {
            color = own * (0.3 + 0.4 * h / height);
        }
        // The draw order sprites get (`depth_order`), by the cell it is in.
        let cell = floor(map + 0.5);
        let z = 1.5 - dot(to_screen(cell), lean) / 16384.0;
        let clip = view.clip_from_world * vec4<f32>(pixel, z, 1.0);
        out.color = vec4<f32>(color, 1.0);
        out.depth = clip.z / clip.w;
        return out;
    }
    discard;
    return out;
}
