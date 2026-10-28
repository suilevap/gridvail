// How the text renderer's view maps screen to map, for its shaders: see
// `ViewCamera` and `view_translation`.
#define_import_path gridvail::view

// `camera` xy: the map point at the screen centre, z: rotation, w: zoom;
// `offset`: the view's offset in cells; `cell_px`: a cell in px at zoom 1.

// A 2D world offset (y up) as a map offset (`ViewCamera::to_map` without
// the translation).
fn to_map_offset(screen: vec2<f32>, camera: vec4<f32>, cell_px: f32) -> vec2<f32> {
    let v = vec2<f32>(screen.x, -screen.y) / (cell_px * camera.w);
    let c = cos(camera.z);
    let s = sin(camera.z);
    return vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y);
}

// The map point shown at a 2D world point.
fn to_map(screen: vec2<f32>, camera: vec4<f32>, offset: vec2<f32>, cell_px: f32) -> vec2<f32> {
    let v = -offset;
    let c = cos(camera.z);
    let s = sin(camera.z);
    return to_map_offset(screen, camera, cell_px) + vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y)
        + camera.xy;
}

// Where a map point is shown (`view_translation`).
fn to_screen(map: vec2<f32>, camera: vec4<f32>, offset: vec2<f32>, cell_px: f32) -> vec2<f32> {
    let d = map - camera.xy;
    let c = cos(camera.z);
    let s = sin(camera.z);
    let turned = vec2<f32>(c * d.x + s * d.y, -s * d.x + c * d.y);
    let v = (turned + offset) * camera.w * cell_px;
    return vec2<f32>(v.x, -v.y);
}

// Textures with a texel per frame cell (`frame` xy: the frame's origin,
// zw: its size) are blended between the four cell centres nearest a map
// point, so they change smoothly across cells. The shaders blend them
// themselves rather than through a sampler, the same on every GPU:
// `blend_corner` is the texel of the nearest centre up and left, and
// `blend` mixes the four texels from there.
fn blend_corner(map: vec2<f32>, frame: vec4<f32>) -> vec2<i32> {
    return vec2<i32>(floor(map - frame.xy));
}

fn blend(map: vec2<f32>, frame: vec4<f32>, texels: array<vec4<f32>, 4>) -> vec4<f32> {
    let at = map - frame.xy;
    let t = at - floor(at);
    return mix(mix(texels[0], texels[1], t.x), mix(texels[2], texels[3], t.x), t.y);
}

// Whether texel `cell` lies on a frame texture; off it is dark.
fn on_frame(cell: vec2<i32>, frame: vec4<f32>) -> bool {
    return all(cell >= vec2<i32>(0)) && all(cell < vec2<i32>(frame.zw));
}
