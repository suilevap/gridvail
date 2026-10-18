// The floor, drawn from `FloorLight` in one of the `FloorStyle`s.
//
// Each pixel finds the map point it shows, the inverse of where the text
// renderer puts cells (`ViewCamera::to_map`), so the floor turns and zooms
// with the glyphs. Patterns are laid out in cell units for the same reason.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// x, y: the map point at the screen centre; z: rotation; w: zoom.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> camera: vec4<f32>;
// x, y: the camera's offset in view cells; z, w: the light's top-left cell.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> window: vec4<f32>;
// x, y: the light's size in cells; z: seconds; w: the style.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> style: vec4<f32>;
// Colour (sRGB) and brightness (alpha) a quarter cell at a time; no
// brightness where there is no floor.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var light: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var light_sampler: sampler;

const CELL: f32 = 16.0;
const TEXELS: f32 = 4.0;

// Off pixels are left clear: the quad is blended.
const NOTHING = vec4<f32>(0.0);

const DOTS: f32 = 1.0;
const DITHER: f32 = 2.0;
const SCANLINES: f32 = 3.0;
const RIDGES: f32 = 4.0;

// Light at `local`, in cells from the light's top-left corner, blended
// between quarter cells.
fn light_at(local: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(light, light_sampler, local / style.xy, 0.0);
}

// Whether there is floor at `local`: its own quarter cell, unblended.
fn floor_at(local: vec2<f32>) -> bool {
    let size = vec2<i32>(style.xy * TEXELS);
    let texel = vec2<i32>(floor(local * TEXELS));
    if any(texel < vec2<i32>(0)) || any(texel >= size) {
        return false;
    }
    return textureLoad(light, texel, 0).a > 0.0;
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
    let here = light_at(middle);
    let side = 1.0 + floor(2.99 * sqrt(here.a));
    let distance = abs(local - middle) * CELL;
    if max(distance.x, distance.y) >= side / 2.0 {
        return NOTHING;
    }
    return vec4<f32>(here.rgb, 1.0);
}

fn dither(local: vec2<f32>) -> vec4<f32> {
    // Ordered dithering in 2 by 2 pixel blocks.
    let block = floor(local * CELL / 2.0);
    let here = light_at((block + 0.5) * 2.0 / CELL);
    let cell = vec2<u32>(block - 4.0 * floor(block / 4.0));
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
    let here = light_at(local);
    let row = floor(local.y * CELL);
    if row - 3.0 * floor(row / 3.0) == 0.0 {
        // A brighter band rolls down the window every ten seconds.
        let band = fract(local.y / style.y - style.z * 0.1);
        let roll = 0.25 * exp(-pow(band * 12.0, 2.0));
        let shine = clamp(here.a * 0.9 + roll, 0.0, 1.0);
        if shine > 0.02 {
            return vec4<f32>(here.rgb * shine, 1.0);
        }
    }
    if cell_dot(local) {
        return vec4<f32>(here.rgb * 0.45, 1.0);
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
        let under = light_at(vec2<f32>(local.x, base));
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
