// Standing objects for the text renderer: see `volume.rs`.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::view
#import gridvail::view::{blend, blend_corner, on_frame, to_map, to_map_offset, to_screen}

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
// The light each frame cell shows, a texel per cell (`LightTextures`):
// before its latest change, and the latest.
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var light_previous: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var light_current: texture_2d<f32>;
// x: 1 to light walls smoothly from the light maps, 0 for one colour a
// cell; y: how far the light has faded from the previous map to the current.
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var<uniform> lighting: vec4<f32>;
// xy: the previous light map's frame origin, zw: the current one's.
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var<uniform> light_origins: vec4<f32>;

fn previous_texel(cell: vec2<i32>) -> vec4<f32> {
    if !on_frame(cell, frame) {
        return vec4<f32>(0.0);
    }
    return textureLoad(light_previous, cell, 0);
}

fn current_texel(cell: vec2<i32>) -> vec4<f32> {
    if !on_frame(cell, frame) {
        return vec4<f32>(0.0);
    }
    return textureLoad(light_current, cell, 0);
}

// The latest light at map point `map`, blended between cells.
fn current_at(map: vec2<f32>) -> vec4<f32> {
    let at = vec4<f32>(light_origins.zw, frame.zw);
    let c = blend_corner(map, at);
    let texels = array<vec4<f32>, 4>(
        current_texel(c),
        current_texel(c + vec2<i32>(1, 0)),
        current_texel(c + vec2<i32>(0, 1)),
        current_texel(c + vec2<i32>(1, 1)),
    );
    return blend(map, at, texels);
}

// The light shown at map point `map`: faded from the previous light to the
// latest, each blended between cells.
fn light_at(map: vec2<f32>) -> vec4<f32> {
    let now = current_at(map);
    if lighting.y >= 1.0 {
        return now;
    }
    let at = vec4<f32>(light_origins.xy, frame.zw);
    let c = blend_corner(map, at);
    let texels = array<vec4<f32>, 4>(
        previous_texel(c),
        previous_texel(c + vec2<i32>(1, 0)),
        previous_texel(c + vec2<i32>(0, 1)),
        previous_texel(c + vec2<i32>(1, 1)),
    );
    return mix(blend(map, at, texels), now, lighting.y);
}

struct Fragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
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
    let floor_point = to_map(pixel, camera, offset_lean.xy, params.y);
    let rise = to_map_offset(lean, camera, params.y);
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
        var own = wall.rgb / wall.a;
        // With smooth light, a wall drawn black into the wall map takes its
        // colour wholly from the light maps, blended between cells (it
        // changes across a wall pixel by pixel, and fades from turn to
        // turn); one of another colour (a portal's) keeps it, and takes on
        // how the light shown here differs from its cell's latest.
        let cell = floor(map + 0.5);
        if lighting.x > 0.5 {
            let blended = light_at(map);
            if all(own < vec3<f32>(1.0 / 512.0)) {
                own = blended.rgb;
            } else {
                let flat = current_at(cell);
                own = max(own + blended.rgb - flat.rgb, vec3<f32>(0.0));
            }
        }
        // Within a pixel of its height: the object's top, in its own light;
        // its sides darker toward the floor.
        let at_top = h > height - 1.0;
        let color = own * select(0.3 + 0.4 * h / height, 1.0, at_top);
        // The draw order sprites get (`depth_order`), by the cell it is in.
        let z = 1.5 - dot(to_screen(cell, camera, offset_lean.xy, params.y), lean) / 16384.0;
        let clip = view.clip_from_world * vec4<f32>(pixel, z, 1.0);
        out.color = vec4<f32>(color, 1.0);
        out.depth = clip.z / clip.w;
        return out;
    }
    discard;
    return out;
}
