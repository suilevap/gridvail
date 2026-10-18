use crate::model::{LightCell, LightKind};

pub const BLACK: u8 = 0;
pub const DARK_BLUE: u8 = 1;
pub const DARK_GREEN: u8 = 2;
pub const DARK_CYAN: u8 = 3;
pub const DARK_RED: u8 = 4;
pub const DARK_MAGENTA: u8 = 5;
pub const DARK_YELLOW: u8 = 6;
pub const GRAY: u8 = 7;
pub const DARK_GRAY: u8 = 8;
pub const BLUE: u8 = 9;
pub const GREEN: u8 = 10;
pub const CYAN: u8 = 11;
pub const RED: u8 = 12;
pub const MAGENTA: u8 = 13;
pub const YELLOW: u8 = 14;
pub const WHITE: u8 = 15;

const FIRE: &[u8] = &[
    DARK_YELLOW,
    DARK_YELLOW,
    DARK_YELLOW,
    DARK_YELLOW,
    DARK_YELLOW,
    DARK_YELLOW,
    DARK_YELLOW,
    YELLOW,
    YELLOW,
    YELLOW,
    YELLOW,
    YELLOW,
    YELLOW,
    YELLOW,
    YELLOW,
    WHITE,
];
const ELECTRO: &[u8] = &[DARK_BLUE, DARK_CYAN, BLUE, CYAN];
const ACID: &[u8] = &[DARK_GREEN, GREEN];
const NONE: &[u8] = &[
    DARK_GRAY, GRAY, GRAY, GRAY, GRAY, GRAY, GRAY, GRAY, GRAY, GRAY, GRAY, GRAY, GRAY, GRAY, WHITE,
];

fn by_rate(table: &[u8], value: u8) -> u8 {
    table[table.len() * value as usize / 256]
}

pub fn light_to_palette(cell: &LightCell) -> u8 {
    if cell.value == 0 {
        return BLACK;
    }
    match cell.kind {
        LightKind::Fire => by_rate(FIRE, cell.value),
        LightKind::Electricity => by_rate(ELECTRO, cell.value),
        LightKind::Acid => by_rate(ACID, cell.value),
        LightKind::None | LightKind::Sky | LightKind::Sun => by_rate(NONE, cell.value),
    }
}

/// The darker colour a remembered (not currently seen) cell is drawn in:
/// each bright colour's dark twin, and dark grey for the dark ones.
pub fn dimmed(index: u8) -> u8 {
    match index {
        BLUE..=YELLOW => index - 8,
        WHITE => GRAY,
        BLACK => BLACK,
        _ => DARK_GRAY,
    }
}

/// Each kind's gradient: colours near its palette colours (sRGB) at light
/// values around where its palette table reaches them, starting for every
/// kind from the dark grey of an unlit cell, so a light fades out into the
/// dark, and into other lights, without a step. Stronger light is never
/// darker: the dark blue and green of the palette are lifted, as they are
/// darker than the unlit grey.
const UNLIT: [f32; 3] = [0.25, 0.25, 0.25];
const FIRE_GRADIENT: &[(f32, [f32; 3])] = &[
    (1.0, UNLIT),
    (70.0, [0.6, 0.5, 0.0]),
    (170.0, [1.0, 0.9, 0.2]),
    (255.0, [1.0, 1.0, 0.85]),
];
const ELECTRO_GRADIENT: &[(f32, [f32; 3])] = &[
    (1.0, UNLIT),
    (32.0, [0.25, 0.32, 0.7]),
    (96.0, [0.2, 0.55, 0.75]),
    (160.0, [0.3, 0.5, 1.0]),
    (255.0, [0.4, 0.95, 0.95]),
];
const ACID_GRADIENT: &[(f32, [f32; 3])] = &[
    (1.0, UNLIT),
    (64.0, [0.2, 0.6, 0.2]),
    (192.0, [0.3, 0.95, 0.3]),
];
const NONE_GRADIENT: &[(f32, [f32; 3])] = &[
    (1.0, UNLIT),
    (48.0, [0.55, 0.55, 0.55]),
    (255.0, [1.0, 1.0, 1.0]),
];
const SKY_GRADIENT: &[(f32, [f32; 3])] = &[
    (1.0, UNLIT),
    (70.0, [0.4, 0.45, 0.58]),
    (150.0, [0.6, 0.67, 0.82]),
    (255.0, [0.8, 0.86, 1.0]),
];
/// Low sun is orange, high sun white.
const SUN_GRADIENT: &[(f32, [f32; 3])] = &[
    (1.0, UNLIT),
    (70.0, [0.55, 0.38, 0.25]),
    (150.0, [1.0, 0.62, 0.3]),
    (200.0, [1.0, 0.88, 0.62]),
    (240.0, [1.0, 0.98, 0.9]),
    (255.0, [1.0, 1.0, 1.0]),
];

fn along(gradient: &[(f32, [f32; 3])], value: f32) -> [f32; 3] {
    let mut previous = gradient[0];
    for &(at, color) in gradient {
        if value <= at {
            let span = at - previous.0;
            let t = if span > 0.0 {
                (value - previous.0) / span
            } else {
                1.0
            };
            return std::array::from_fn(|i| previous.1[i] + (color[i] - previous.1[i]) * t);
        }
        previous = (at, color);
    }
    previous.1
}

/// The exact colour of a lit cell (sRGB), the smooth counterpart of
/// [`light_to_palette`].
pub fn light_to_rgb(cell: &LightCell) -> [u8; 3] {
    if cell.value == 0 {
        return [0; 3];
    }
    let gradient = match cell.kind {
        LightKind::Fire => FIRE_GRADIENT,
        LightKind::Electricity => ELECTRO_GRADIENT,
        LightKind::Acid => ACID_GRADIENT,
        LightKind::None => NONE_GRADIENT,
        LightKind::Sky => SKY_GRADIENT,
        LightKind::Sun => SUN_GRADIENT,
    };
    along(gradient, cell.value as f32).map(|channel| (channel * 255.0).round() as u8)
}

/// The colour a remembered cell is drawn in: greyer and darker, the smooth
/// counterpart of [`dimmed`].
pub fn dimmed_rgb(rgb: [u8; 3]) -> [u8; 3] {
    let grey = (rgb[0] as f32 + rgb[1] as f32 + rgb[2] as f32) / 3.0;
    rgb.map(|channel| ((channel as f32 * 0.4 + grey * 0.6) * 0.5).round() as u8)
}

pub fn palette_color(index: u8) -> bevy::prelude::Color {
    use bevy::prelude::Color;
    match index {
        BLACK => Color::srgb(0.0, 0.0, 0.0),
        DARK_BLUE => Color::srgb(0.0, 0.0, 0.55),
        DARK_GREEN => Color::srgb(0.0, 0.5, 0.0),
        DARK_CYAN => Color::srgb(0.0, 0.5, 0.5),
        DARK_RED => Color::srgb(0.55, 0.0, 0.0),
        DARK_MAGENTA => Color::srgb(0.5, 0.0, 0.5),
        DARK_YELLOW => Color::srgb(0.6, 0.5, 0.0),
        GRAY => Color::srgb(0.55, 0.55, 0.55),
        DARK_GRAY => Color::srgb(0.25, 0.25, 0.25),
        BLUE => Color::srgb(0.2, 0.3, 1.0),
        GREEN => Color::srgb(0.2, 0.9, 0.2),
        CYAN => Color::srgb(0.2, 0.9, 0.9),
        RED => Color::srgb(1.0, 0.25, 0.25),
        MAGENTA => Color::srgb(0.9, 0.2, 0.9),
        YELLOW => Color::srgb(1.0, 0.9, 0.2),
        _ => Color::WHITE,
    }
}
