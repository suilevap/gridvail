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

// Where map point `map` falls on a texture with a texel per frame cell,
// for a frame at `origin` of `size` cells: cell centres are texel centres,
// so a linearly filtered read blends neighbouring cells.
fn frame_uv(map: vec2<f32>, origin: vec2<f32>, size: vec2<f32>) -> vec2<f32> {
    return (map - origin + 0.5) / size;
}

// Whether `uv` lies on the texture; off it is dark.
fn on_texture(uv: vec2<f32>) -> bool {
    return all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
}
