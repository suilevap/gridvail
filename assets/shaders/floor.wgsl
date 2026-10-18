// The floor, drawn in one of the `FloorStyle`s.
//
// Each pixel finds the map point it shows, the inverse of where the text
// renderer puts cells (`ViewCamera::to_map`), so the floor turns and zooms
// with the glyphs. Patterns are laid out in cell units for the same reason.
//
// Floor seen directly is lit here a pixel at a time: a ray from each lamp
// to the pixel through the cells that block light, and the game's falloff,
// so shadows have exact edges and light fades smoothly inside cells. Other
// floor shows its cell's light.

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

const CELL: f32 = 16.0;

// Off pixels are left clear: the quad is blended.
const NOTHING = vec4<f32>(0.0);

const DOTS: f32 = 1.0;
const DITHER: f32 = 2.0;
const SCANLINES: f32 = 3.0;
const RIDGES: f32 = 4.0;

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

// The floor's dot at the middle of every cell, as the text renderer draws
// it: 2 by 2 pixels a little below the middle.
fn cell_dot(local: vec2<f32>) -> bool {
    let pixel = floor(fract(local) * CELL);
    return all(pixel >= vec2<f32>(7.0, 9.0)) && all(pixel < vec2<f32>(9.0, 11.0));
}

const BAYER = array<f32, 16>(
    0.0, 8.0, 2.0, 10.0,
    12.0, 4.0, 14.0, 6.0,
    3.0, 11.0, 1.0, 9.0,
    15.0, 7.0, 13.0, 5.0,
);

fn dots(local: vec2<f32>) -> vec4<f32> {
    // Four dots a cell, each as large as the light at it: 1 to 3 pixels.
    let quarter = floor(local * 2.0);
    let middle = (quarter + 0.5) / 2.0;
    let here = lit(middle);
    let side = 1.0 + floor(2.99 * sqrt(here.a));
    let distance = abs(local - middle) * CELL;
    if max(distance.x, distance.y) >= side / 2.0 {
        return NOTHING;
    }
    return vec4<f32>(here.rgb, 1.0);
}

fn dither(local: vec2<f32>) -> vec4<f32> {
    // Ordered dithering, a pixel at a time.
    let pixel = floor(local * CELL);
    let here = lit((pixel + 0.5) / CELL);
    let cell = vec2<u32>(pixel - 4.0 * floor(pixel / 4.0));
    let threshold = (BAYER[cell.y * 4u + cell.x] + 0.5) / 16.0;
    if here.a * 0.6 > threshold {
        return vec4<f32>(here.rgb, 1.0);
    }
    if cell_dot(local) {
        return vec4<f32>(here.rgb * 0.45, 1.0);
    }
    return NOTHING;
}

fn scanlines(local: vec2<f32>) -> vec4<f32> {
    let row = floor(local.y * CELL);
    if row - 3.0 * floor(row / 3.0) == 0.0 {
        let here = lit(local);
        // A brighter band rolls down the window every ten seconds.
        let band = fract(local.y / style.y - style.z * 0.1);
        let roll = 0.25 * exp(-pow(band * 12.0, 2.0));
        let shine = clamp(here.a * 0.9 + roll, 0.0, 1.0);
        if shine > 0.02 {
            return vec4<f32>(here.rgb * shine, 1.0);
        }
    }
    if cell_dot(local) {
        return vec4<f32>(cell_light(local).rgb * 0.45, 1.0);
    }
    return NOTHING;
}

fn ridges(local: vec2<f32>) -> vec4<f32> {
    // A line every half cell, lifted by the light under it and swaying where
    // it is lit. Nearer (lower) lines hide the ones behind them, as the
    // ground under a ridge does.
    let spacing = 0.5;
    let lift = 1.2;
    let half = 0.6 / CELL;
    var line = ceil((local.y + lift) / spacing);
    for (var i = 0; i < 6; i++) {
        let base = line * spacing;
        let under = lit(vec2<f32>(local.x, base));
        let sway = 0.05 * under.a * sin(style.z * 1.3 + local.x * 1.7 + line * 0.9);
        let y = base - lift * under.a + sway;
        if abs(local.y - y) < half {
            // Dim where it is dark, so the dark floor stays quiet.
            return vec4<f32>(under.rgb * (0.35 + 0.65 * sqrt(under.a)), 1.0);
        }
        if local.y > y {
            // Below this line: hidden by it.
            break;
        }
        line -= 1.0;
    }
    return NOTHING;
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
    // Cell (x, y) spans x - 0.5 to x + 0.5.
    let local = map - window.zw + 0.5;
    if !floor_at(local) {
        return NOTHING;
    }
    if style.w == DOTS {
        return dots(local);
    } else if style.w == DITHER {
        return dither(local);
    } else if style.w == SCANLINES {
        return scanlines(local);
    } else if style.w == RIDGES {
        return ridges(local);
    }
    return NOTHING;
}
