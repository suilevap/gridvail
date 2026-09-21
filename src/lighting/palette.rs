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
        LightKind::None => by_rate(NONE, cell.value),
    }
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
