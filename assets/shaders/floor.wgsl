// The floor as tiles, in one of the `FloorStyle`s.
//
// Each pixel finds the map point it shows, the inverse of where the text
// renderer puts cells (`ViewCamera::to_map`), so the floor turns and zooms
// with the glyphs. A tile is a small mark like a glyph, several a cell or
// off the grid, laid out by map position so it stays put as the view
// moves. A tile is lit as a whole by the light at its middle, so shadows
// fall tile by tile.
//
// Floor seen directly is lit here: a ray from each lamp to the tile's
// middle through the cells that block light, and the game's falloff. A
// ray from the player tells whether they see that part of a cell they
// see; if not, it is drawn as remembered. Other floor shows its cell's
// light.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct Lamps {
    // x: how many are used.
    count: vec4<f32>,
    // x, y: where, in cells from the window's top-left corner; z: radius;
    // w: value.
    at: array<vec4<f32>, 32>,
    // x: kind: 0 none, 1 fire, 2 electricity, 3 acid.
    kind: array<vec4<f32>, 32>,
}

// x, y: the map point at the screen centre; z: rotation; w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// x, y: the camera's offset in view cells; z, w: the window's top-left cell.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> window: vec4<f32>;
// x, y: the window's size in cells; z: seconds; w: the style.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> style: vec4<f32>;
// Per cell: colour (sRGB) and brightness (alpha); no brightness where there
// is no floor.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var light: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var light_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<uniform> lamps: Lamps;
// Per cell: blocks light (r), lit here from the lamps (g), bounced light (b).
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var info: texture_2d<f32>;
// x, y: where the player is, in cells from the window's top-left corner.
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var<uniform> viewer: vec4<f32>;

const CELL: f32 = 16.0;

// Off pixels are left clear: the quad is blended.
const NOTHING = vec4<f32>(0.0);

// The light kinds' colour gradients (`lighting::palette`): stops of
// (light value, sRGB), the last repeated.
const UNLIT: f32 = 0.25;
const NONE_STOPS = array<vec4<f32>, 5>(
    vec4<f32>(1.0, UNLIT, UNLIT, UNLIT),
    vec4<f32>(48.0, 0.55, 0.55, 0.55),
    vec4<f32>(255.0, 1.0, 1.0, 1.0),
    vec4<f32>(256.0, 1.0, 1.0, 1.0),
    vec4<f32>(257.0, 1.0, 1.0, 1.0),
);
const FIRE_STOPS = array<vec4<f32>, 5>(
    vec4<f32>(1.0, UNLIT, UNLIT, UNLIT),
    vec4<f32>(70.0, 0.6, 0.5, 0.0),
    vec4<f32>(170.0, 1.0, 0.9, 0.2),
    vec4<f32>(255.0, 1.0, 1.0, 0.85),
    vec4<f32>(256.0, 1.0, 1.0, 0.85),
);
const ELECTRO_STOPS = array<vec4<f32>, 5>(
    vec4<f32>(1.0, UNLIT, UNLIT, UNLIT),
    vec4<f32>(32.0, 0.25, 0.32, 0.7),
    vec4<f32>(96.0, 0.2, 0.55, 0.75),
    vec4<f32>(160.0, 0.3, 0.5, 1.0),
    vec4<f32>(255.0, 0.4, 0.95, 0.95),
);
const ACID_STOPS = array<vec4<f32>, 5>(
    vec4<f32>(1.0, UNLIT, UNLIT, UNLIT),
    vec4<f32>(64.0, 0.2, 0.6, 0.2),
    vec4<f32>(192.0, 0.3, 0.95, 0.3),
    vec4<f32>(256.0, 0.3, 0.95, 0.3),
    vec4<f32>(257.0, 0.3, 0.95, 0.3),
);

fn along(stops: array<vec4<f32>, 5>, value: f32) -> vec3<f32> {
    var previous = stops[0];
    for (var i = 0; i < 5; i++) {
        let stop = stops[i];
        if value <= stop.x {
            let t = clamp((value - previous.x) / max(stop.x - previous.x, 1e-3), 0.0, 1.0);
            return mix(previous.yzw, stop.yzw, t);
        }
        previous = stop;
    }
    return previous.yzw;
}

// Linear colour of light `value` of `kind`.
fn gradient(kind: i32, value: f32) -> vec3<f32> {
    var srgb: vec3<f32>;
    if kind == 1 {
        srgb = along(FIRE_STOPS, value);
    } else if kind == 2 {
        srgb = along(ELECTRO_STOPS, value);
    } else if kind == 3 {
        srgb = along(ACID_STOPS, value);
    } else {
        srgb = along(NONE_STOPS, value);
    }
    return pow(srgb, vec3<f32>(2.2));
}

// How a lamp's light wavers: fire flickers, electricity buzzes, acid pulses.
fn waver(kind: i32, lamp: f32) -> f32 {
    let t = style.z;
    if kind == 1 {
        return 1.0 + 0.07 * sin(t * 9.0 + lamp * 1.7) + 0.04 * sin(t * 23.0 + lamp * 4.1);
    } else if kind == 2 {
        return 1.0 + 0.12 * sin(t * 41.0 + lamp * 3.0);
    } else if kind == 3 {
        return 1.0 + 0.1 * sin(t * 2.0 + lamp);
    }
    return 1.0;
}

fn cell_info(cell: vec2<i32>) -> vec4<f32> {
    if any(cell < vec2<i32>(0)) || any(cell >= vec2<i32>(style.xy)) {
        return vec4<f32>(0.0);
    }
    return textureLoad(info, cell, 0);
}

// Whether the segment `start`-`to` crosses no cell that blocks light between
// the cells it starts and ends in, walking the cells it passes through.
fn clear_line(start: vec2<f32>, to: vec2<f32>) -> bool {
    var cell = vec2<i32>(floor(start));
    let end = vec2<i32>(floor(to));
    let delta = to - start;
    let step = vec2<i32>(sign(delta));
    let each = vec2<f32>(
        select(1e9, 1.0 / abs(delta.x), delta.x != 0.0),
        select(1e9, 1.0 / abs(delta.y), delta.y != 0.0),
    );
    var next = vec2<f32>(
        select(
            select(1e9, (start.x - f32(cell.x)) * each.x, step.x < 0),
            (f32(cell.x) + 1.0 - start.x) * each.x,
            step.x > 0,
        ),
        select(
            select(1e9, (start.y - f32(cell.y)) * each.y, step.y < 0),
            (f32(cell.y) + 1.0 - start.y) * each.y,
            step.y > 0,
        ),
    );
    let steps = abs(end.x - cell.x) + abs(end.y - cell.y);
    for (var i = 0; i < 80; i++) {
        if i >= steps {
            break;
        }
        if next.x < next.y {
            cell.x += step.x;
            next.x += each.x;
        } else {
            cell.y += step.y;
            next.y += each.y;
        }
        if all(cell == end) {
            break;
        }
        if cell_info(cell).r > 0.5 {
            return false;
        }
    }
    return true;
}

// The cells' own light at `local`, blended between cells.
fn cell_light(local: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(light, light_sampler, local / style.xy, 0.0);
}

// The light at `local`: linear colour and brightness (0 to 1).
fn lit(local: vec2<f32>) -> vec4<f32> {
    let own = cell_light(local);
    let here = cell_info(vec2<i32>(floor(local)));
    if here.g < 0.5 {
        return own;
    }
    if !clear_line(viewer.xy, local) {
        // The part of a seen cell the player does not see: as remembered.
        return vec4<f32>(own.rgb * 0.3, own.a * 0.5);
    }
    var kinds = vec4<f32>(0.0);
    let count = i32(lamps.count.x);
    for (var i = 0; i < 32; i++) {
        if i >= count {
            break;
        }
        let lamp = lamps.at[i];
        let kind = i32(lamps.kind[i].x);
        let offset = local - lamp.xy;
        let reach = (lamp.z + 1.0) * (lamp.z + 1.0);
        let distance = dot(offset, offset);
        if distance < reach && clear_line(lamp.xy, local) {
            kinds[kind] += (1.0 - distance / reach) * lamp.w * waver(kind, f32(i));
        }
    }
    let bounced = here.b * 255.0;
    let total = kinds.x + kinds.y + kinds.z + kinds.w + bounced + 1.0;
    let shown = min(total, 255.0);
    var colour = own.rgb * bounced + gradient(0, 1.0);
    for (var kind = 0; kind < 4; kind++) {
        colour += gradient(kind, shown) * kinds[kind];
    }
    return vec4<f32>(colour / total, shown / 255.0);
}

// Whether there is floor at `local`: its own cell, unblended.
fn floor_at(local: vec2<f32>) -> bool {
    let cell = vec2<i32>(floor(local));
    if any(cell < vec2<i32>(0)) || any(cell >= vec2<i32>(style.xy)) {
        return false;
    }
    return textureLoad(light, cell, 0).a > 0.0;
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// The marks, each a few pixels from its top-left pixel.
const SPECK: i32 = 0;
const DOT: i32 = 1;
const DASH: i32 = 2;
const CROSS: i32 = 3;
const SQUARE: i32 = 4;
const COMMA: i32 = 5;
const TICK: i32 = 6;
const COLON: i32 = 7;

// Whether pixel `d` from a mark's top-left pixel is on the mark.
fn on_mark(mark: i32, d: vec2<f32>) -> bool {
    let inside = all(d >= vec2<f32>(0.0));
    if mark == SPECK {
        return all(d == vec2<f32>(0.0));
    } else if mark == DOT {
        return inside && all(d <= vec2<f32>(1.0));
    } else if mark == DASH {
        return inside && d.y == 0.0 && d.x <= 2.0;
    } else if mark == CROSS {
        return (d.x == 1.0 && d.y >= 0.0 && d.y <= 2.0) || (d.y == 1.0 && d.x >= 0.0 && d.x <= 2.0);
    } else if mark == SQUARE {
        return inside && all(d <= vec2<f32>(2.0));
    } else if mark == COMMA {
        return (inside && all(d <= vec2<f32>(1.0))) || all(d == vec2<f32>(0.0, 2.0));
    } else if mark == TICK {
        return all(d == vec2<f32>(1.0, 0.0)) || all(d == vec2<f32>(0.0, 1.0));
    } else if mark == COLON {
        return d.x == 0.0 && (d.y == 0.0 || d.y == 2.0);
    }
    return false;
}

const POINTS: f32 = 1.0;
const PEBBLES: f32 = 2.0;
const BRICKS: f32 = 3.0;
const CROSSES: f32 = 4.0;
const MOSAIC: f32 = 5.0;
const RAMP: f32 = 6.0;
const SCATTER: f32 = 7.0;

// The mark of the tile at `pixel` (map pixels), as its top-left pixel (x,
// y) and its mark (z), or z below 0 for none. The ramp's is the square its
// mark grows to with the light.
fn tile(pixel: vec2<f32>) -> vec3<f32> {
    if style.w == POINTS {
        // The text renderer's dot: a little below the middle of a cell.
        return vec3<f32>(floor(pixel / CELL) * CELL + vec2<f32>(7.0, 9.0), f32(DOT));
    }
    if style.w == BRICKS {
        let row = floor(pixel.y / 8.0);
        let shift = (row - 2.0 * floor(row / 2.0)) * 4.0;
        let column = floor((pixel.x + shift) / 8.0);
        return vec3<f32>(column * 8.0 - shift + 2.0, row * 8.0 + 4.0, f32(DASH));
    }
    if style.w == MOSAIC {
        return vec3<f32>(floor(pixel / 4.0) * 4.0, f32(SQUARE));
    }
    let cell = floor(pixel / 8.0);
    let corner = cell * 8.0;
    if style.w == CROSSES {
        return vec3<f32>(corner + 2.0, f32(CROSS));
    }
    let jitter = floor((vec2<f32>(hash(cell), hash(cell + 31.0)) - 0.5) * 5.0);
    let pick = hash(cell + 13.0);
    if style.w == PEBBLES {
        var mark = DOT;
        if pick > 0.75 {
            mark = DASH;
        } else if pick > 0.5 {
            mark = SPECK;
        }
        return vec3<f32>(corner + 3.0 + jitter, f32(mark));
    }
    if style.w == SCATTER {
        if pick < 0.35 {
            return vec3<f32>(0.0, 0.0, -1.0);
        }
        let marks = array<i32, 6>(SPECK, DOT, COMMA, TICK, DASH, COLON);
        let mark = marks[min(i32((pick - 0.35) / 0.65 * 6.0), 5)];
        return vec3<f32>(corner + 3.0 + jitter, f32(mark));
    }
    if style.w == RAMP {
        return vec3<f32>(corner + 2.0, f32(SQUARE));
    }
    return vec3<f32>(0.0, 0.0, -1.0);
}

// The ramp's mark for light `brightness`: speck, dot, cross, square.
fn ramp_mark(brightness: f32) -> i32 {
    if brightness < 0.2 {
        return SPECK;
    } else if brightness < 0.45 {
        return DOT;
    } else if brightness < 0.7 {
        return CROSS;
    }
    return SQUARE;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let view = vec2<f32>(mesh.world_position.x, -mesh.world_position.y) / CELL;
    let turn = vec2<f32>(cos(camera.z), sin(camera.z));
    let from_centre = view / camera.w - window.xy;
    let map = vec2<f32>(
        turn.x * from_centre.x - turn.y * from_centre.y,
        turn.y * from_centre.x + turn.x * from_centre.y,
    ) + camera.xy;
    // Cell (x, y) spans x - 0.5 to x + 0.5; `local` counts from the
    // window's corner, map pixels from the map's.
    let local = map - window.zw + 0.5;
    if !floor_at(local) {
        return NOTHING;
    }
    let pixel = floor((map + 0.5) * CELL);
    let found = tile(pixel);
    if found.z < 0.0 {
        return NOTHING;
    }
    var mark = i32(found.z);
    // A tile is lit as a whole, by the light at its middle.
    let middle = (found.xy + 1.5) / CELL - window.zw;
    if style.w == RAMP {
        if !on_mark(SQUARE, pixel - found.xy) {
            return NOTHING;
        }
        let here = lit(middle);
        mark = ramp_mark(here.a);
        // Smaller marks sit in the middle of the square.
        var offset = vec2<f32>(0.0);
        if mark == SPECK {
            offset = vec2<f32>(1.0);
        } else if mark == DOT {
            offset = vec2<f32>(1.0);
        }
        if !on_mark(mark, pixel - found.xy - offset) {
            return NOTHING;
        }
        return vec4<f32>(min(here.rgb * 1.7, vec3<f32>(1.0)), 1.0);
    }
    if !on_mark(mark, pixel - found.xy) {
        return NOTHING;
    }
    let here = lit(middle);
    // The mosaic covers most of the floor: dimmer, or it outshines the glyphs.
    let gain = select(1.7, 0.6, style.w == MOSAIC);
    return vec4<f32>(min(here.rgb * gain, vec3<f32>(1.0)), 1.0);
}
