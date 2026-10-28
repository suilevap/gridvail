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
// The light each frame cell shows, a texel per cell (`LightTextures`).
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var light_map: texture_2d<f32>;
// x: 1 to light walls smoothly from the light map, 0 for one colour a cell.
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var<uniform> lighting: vec4<f32>;

fn light_texel(cell: vec2<i32>) -> vec4<f32> {
    if !on_frame(cell, frame) {
        return vec4<f32>(0.0);
    }
    return textureLoad(light_map, cell, 0);
}

// The light map blended between cells at map point `map`.
fn light_at(map: vec2<f32>) -> vec4<f32> {
    let c = blend_corner(map, frame);
    let texels = array<vec4<f32>, 4>(
        light_texel(c),
        light_texel(c + vec2<i32>(1, 0)),
        light_texel(c + vec2<i32>(0, 1)),
        light_texel(c + vec2<i32>(1, 1)),
    );
    return blend(map, frame, texels);
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
        // colour wholly from the light map, blended between cells (it
        // changes across a wall pixel by pixel, and fades from turn to
        // turn); one of another colour (a portal's) keeps it, and takes on
        // how the light here differs from its cell's.
        let cell = floor(map + 0.5);
        if lighting.x > 0.5 {
            let blended = light_at(map);
            if all(own < vec3<f32>(1.0 / 512.0)) {
                own = blended.rgb;
            } else {
                let flat = light_texel(vec2<i32>(cell - frame.xy));
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
