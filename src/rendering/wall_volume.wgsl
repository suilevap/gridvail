// Standing walls for the text renderer: see `volume.rs`.

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

struct Fragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

// The map point shown at a 2D world point (`ViewCamera::to_map`).
fn to_map(screen: vec2<f32>) -> vec2<f32> {
    let view_cells = vec2<f32>(screen.x, -screen.y) / params.y;
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
    // From the tallest top down to the floor, a screen pixel at a time: the
    // first wall found is the nearest.
    let steps = i32(ceil(top));
    var out: Fragment;
    for (var i = 0; i <= steps; i = i + 1) {
        let h = max(top - f32(i), 0.0);
        let map = to_map(pixel - lean * h);
        let texel = vec2<i32>(floor((map - frame.xy + 0.5) * params.z));
        if any(texel < vec2<i32>(0)) || any(texel >= map_size) {
            continue;
        }
        let wall = textureLoad(wall_map, texel, 0);
        let height = wall.a * full;
        if wall.a < 0.004 || h > height {
            continue;
        }
        // Within a pixel of its height: the wall's top, lit.
        let at_top = h > height - 1.0;
        let shade = select(0.3 + 0.4 * h / height, 1.0, at_top);
        // The draw order sprites get (`depth_order`), by the cell it is in.
        let cell = floor(map + 0.5);
        let z = 1.5 - dot(to_screen(cell), lean) / 16384.0;
        let clip = view.clip_from_world * vec4<f32>(pixel, z, 1.0);
        out.color = vec4<f32>(wall.rgb / wall.a * shade, 1.0);
        out.depth = clip.z / clip.w;
        return out;
    }
    discard;
    return out;
}
