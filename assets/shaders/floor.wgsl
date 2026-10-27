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
// x, y: the window's size in cells; z: seconds.
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
    vec4<f32>(70.0, 0.62, 0.36, 0.12),
    vec4<f32>(170.0, 1.0, 0.62, 0.22),
    vec4<f32>(255.0, 1.0, 0.9, 0.6),
    vec4<f32>(256.0, 1.0, 0.9, 0.6),
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

// The light where nothing shines: a dark night teal (linear).
const AMBIENT = vec3<f32>(0.018, 0.04, 0.045);

// How much of a lamp's light reaches `offset` from it: a lamp with a
// facing is a beam, bright in a cone that fades at its edges and dim
// around; one without shines all round.
fn beam(facing: vec2<f32>, offset: vec2<f32>) -> f32 {
    if dot(facing, facing) < 0.01 {
        return 1.0;
    }
    let along = dot(normalize(offset + vec2<f32>(1e-4)), facing);
    return 0.3 + 2.0 * smoothstep(0.86, 0.97, along);
}

// The ground between glyphs: the light there, dim and smooth, glowing
// more where the light is strong, so lit places glow under their glyphs.
fn ground(local: vec2<f32>) -> vec4<f32> {
    let here = lit(local);
    return vec4<f32>(here.rgb * (0.04 + 0.2 * here.a * here.a), 1.0);
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
            kinds[kind] += (1.0 - distance / reach) * lamp.w * waver(kind, f32(i))
                * beam(lamps.kind[i].yz, offset);
        }
    }
    let bounced = here.b * 255.0;
    let total = kinds.x + kinds.y + kinds.z + kinds.w + bounced + 1.0;
    let shown = min(total, 255.0);
    var colour = own.rgb * bounced + AMBIENT;
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

struct Tiles {
    // x: slots along a cell's side; y: jitter, in glyph pixels; z: the
    // share left empty; w: how many slots every other row moves along.
    arrange: vec4<f32>,
    // x, y: the block's size in slots; z: how many glyphs; w: how they are
    // picked: 0 at random, 1 as a block, 2 by brightness.
    pick: vec4<f32>,
    // x: how bright lit glyphs are.
    gain: vec4<f32>,
    // Glyph indices, four to an element.
    glyphs: array<vec4<f32>, 16>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(8) var<uniform> tiles: Tiles;
// The glyphs side by side, 8 by 8 pixels each.
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var glyph_atlas: texture_2d<f32>;

fn glyph(index: i32) -> i32 {
    return i32(tiles.glyphs[index / 4][index % 4]);
}

// A glyph's pixel: its tint where it is on (alpha 1), nothing where off.
fn glyph_pixel(glyph: i32, pixel: vec2<f32>) -> vec4<f32> {
    return textureLoad(glyph_atlas, vec2<i32>(glyph * 8 + i32(pixel.x), i32(pixel.y)), 0);
}

fn whole_mod(value: f32, by: f32) -> f32 {
    return value - by * floor(value / by);
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

    // The slot the pixel is in: every other row moved along.
    let slot = CELL / tiles.arrange.x;
    let row = floor(pixel.y / slot);
    let shift = whole_mod(row, 2.0) * tiles.arrange.w * slot;
    let column = floor((pixel.x + shift) / slot);
    let place = vec2<f32>(column, row);
    if hash(place + 71.0) < tiles.arrange.z {
        return ground(local);
    }
    // Where in its glyph, which may stray by up to the jitter.
    let within = vec2<f32>(pixel.x + shift - column * slot, pixel.y - row * slot);
    let reach = tiles.arrange.y;
    let jitter = floor((vec2<f32>(hash(place), hash(place + 31.0))) * (2.0 * reach + 1.0)) - reach;
    let at = floor(within * 8.0 / slot) - jitter;
    if any(at < vec2<f32>(0.0)) || any(at > vec2<f32>(7.0)) {
        return ground(local);
    }
    // A glyph is lit as a whole, by the light at its slot's middle.
    let middle = (vec2<f32>(column * slot - shift, row * slot) + slot / 2.0) / CELL - window.zw;

    let count = i32(tiles.pick.z);
    var index = 0;
    var here = vec4<f32>(0.0);
    if tiles.pick.w == 2.0 {
        here = lit(middle);
        index = min(i32(here.a * f32(count) * 1.4), count - 1);
    } else if tiles.pick.w == 1.0 {
        let block = vec2<f32>(whole_mod(column, tiles.pick.x), whole_mod(row, tiles.pick.y));
        index = i32(block.x + block.y * tiles.pick.x);
    } else {
        index = min(i32(hash(place + 13.0) * f32(count)), count - 1);
    }
    let mark = glyph_pixel(glyph(index), at);
    if mark.a < 0.5 {
        return ground(local);
    }
    if tiles.pick.w != 2.0 {
        here = lit(middle);
    }
    return vec4<f32>(min(here.rgb * mark.rgb * tiles.gain.x, vec3<f32>(1.0)), 1.0);
}
