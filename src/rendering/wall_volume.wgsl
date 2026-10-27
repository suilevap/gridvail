// Standing walls for the text renderer: see `volume.rs`.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::view

// xy: the map point at the screen centre, z: rotation, w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// xy: the view's offset in cells, zw: the lean of height on screen.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> offset_lean: vec4<f32>;
// xy: the frame's origin, zw: its size, in cells.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> frame: vec4<f32>;
// x: wall height in px at zoom 1, y: 1 for solid walls, z: cell px.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var cells: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var glyphs: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var atlas: texture_2d<f32>;

const GLYPH_PX: f32 = 8.0;
const ATLAS_SLOTS: i32 = 16;

struct Fragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

// The map point shown at a 2D world point (`ViewCamera::to_map`).
fn to_map(screen: vec2<f32>) -> vec2<f32> {
    let view_cells = vec2<f32>(screen.x, -screen.y) / params.z;
    let v = view_cells / camera.w - offset_lean.xy;
    let c = cos(camera.z);
    let s = sin(camera.z);
    return vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y) + camera.xy;
}

// Where a map point is shown (`view_translation`).
fn to_screen(map: vec2<f32>) -> vec2<f32> {
    let d = map - camera.xy;
    let c = cos(camera.z);
    let s = sin(camera.z);
    let turned = vec2<f32>(c * d.x + s * d.y, -s * d.x + c * d.y);
    let v = (turned + offset_lean.xy) * camera.w * params.z;
    return vec2<f32>(v.x, -v.y);
}

@fragment
fn fragment(in: VertexOutput) -> Fragment {
    let pixel = in.world_position.xy;
    let lean = offset_lean.zw;
    let top = params.x * camera.w;
    let solid = params.y > 0.5;
    // From the tallest top down to the floor, a screen pixel at a time: the
    // first wall found is the nearest.
    let steps = i32(ceil(top));
    var out: Fragment;
    for (var i = 0; i <= steps; i = i + 1) {
        let h = max(top - f32(i), 0.0);
        let map = to_map(pixel - lean * h);
        let cell = floor(map + 0.5);
        let local_cell = cell - frame.xy;
        if any(local_cell < vec2<f32>(0.0)) || any(local_cell >= frame.zw) {
            continue;
        }
        let slot = vec2<i32>(cell - frame.zw * floor(cell / frame.zw));
        let wall = textureLoad(cells, slot, 0);
        let height = wall.a * top;
        if wall.a == 0.0 || h > height {
            continue;
        }
        let texel = vec2<i32>(clamp(floor((map - cell + 0.5) * GLYPH_PX), vec2<f32>(0.0), vec2<f32>(GLYPH_PX - 1.0)));
        let glyph = i32(round(textureLoad(glyphs, slot, 0).r * 255.0));
        let origin = vec2<i32>(glyph % ATLAS_SLOTS, glyph / ATLAS_SLOTS) * i32(GLYPH_PX);
        let inked = textureLoad(atlas, origin + texel, 0).r > 0.5;
        // Within a pixel of its height: the wall's top, lit.
        let at_top = h > height - 1.0;
        var shade: f32;
        if solid {
            if at_top {
                shade = select(0.45, 1.0, inked);
            } else {
                shade = 0.18 + 0.12 * h / height;
            }
        } else {
            if !inked {
                continue;
            }
            shade = select(0.3 + 0.4 * h / height, 1.0, at_top);
        }
        // The draw order sprites get (`depth_order`), by the wall's cell.
        let z = 1.5 - dot(to_screen(cell), lean) / 16384.0;
        let clip = view.clip_from_world * vec4<f32>(pixel, z, 1.0);
        out.color = vec4<f32>(wall.rgb * shade, 1.0);
        out.depth = clip.z / clip.w;
        return out;
    }
    discard;
    return out;
}
