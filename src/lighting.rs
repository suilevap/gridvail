//! Light accumulation and palettes. Faithful port of `LightRenderSystem`
//! merge math plus `PrepareForRenderSystem` color tables.
//!
//! Rules kept verbatim:
//! - falloff `(1 - sqD / radiusSq) * base`, truncated to byte;
//! - same-kind lights add with saturation at 255;
//! - a different kind wins only when strictly brighter;
//! - palette lookup `table[len * value / 256]` (mirrors `GetByRate(byte)`).

use crate::components::{LightCell, LightKind};

/// Context of one contributing light (mirrors `LightDataContext`).
#[derive(Clone, Copy, Debug)]
pub struct LightContext {
    pub center_x: i32,
    pub center_y: i32,
    pub radius_sq: i32,
    pub inv_radius_sq: f32,
    pub base_value: u8,
    pub kind: LightKind,
}

impl LightContext {
    pub fn new(center: (i32, i32), radius: i32, base_value: u8, kind: LightKind) -> Self {
        let radius_sq = (radius + 1) * (radius + 1);
        Self {
            center_x: center.0,
            center_y: center.1,
            radius_sq,
            inv_radius_sq: 1.0 / radius_sq as f32,
            base_value,
            kind,
        }
    }
}

/// Merge one FOV sample into a light cell (mirrors `LightMerge`).
pub fn merge_light(target: &mut LightCell, fov: f32, x: i32, y: i32, ctx: &LightContext) {
    let dx = x - ctx.center_x;
    let dy = y - ctx.center_y;
    let sq_d = dx * dx + dy * dy;
    if sq_d > ctx.radius_sq {
        return;
    }
    let amount = (fov * (1.0 - sq_d as f32 * ctx.inv_radius_sq) * ctx.base_value as f32) as u8;
    if target.kind.overlaps(ctx.kind) {
        target.value = target.value.saturating_add(amount);
    } else if amount > target.value {
        target.value = amount;
        target.kind = ctx.kind;
    }
}

// ConsoleColor indices (matches System.ConsoleColor layout).
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

/// Map a light cell to a palette index (mirrors `ToConsoleColor`).
pub fn light_to_palette(cell: &LightCell) -> u8 {
    if cell.value == 0 {
        return BLACK;
    }
    let mut color = 0u8;
    match cell.kind {
        LightKind::Fire => color |= by_rate(FIRE, cell.value),
        LightKind::Electricity => color |= by_rate(ELECTRO, cell.value),
        LightKind::Acid => color |= by_rate(ACID, cell.value),
        LightKind::None => return by_rate(NONE, cell.value),
    }
    color
}

/// Approximate Bevy colors for the 16 console colors.
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
        _ => Color::srgb(1.0, 1.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(kind: LightKind, value: u8) -> LightContext {
        LightContext::new((0, 0), 4, value, kind)
    }

    #[test]
    fn falloff_is_full_at_center_and_fades() {
        let mut center = LightCell::default();
        merge_light(&mut center, 1.0, 0, 0, &ctx(LightKind::Fire, 100));
        assert_eq!(center.value, 100);
        assert_eq!(center.kind, LightKind::Fire);

        let mut edge = LightCell::default();
        // radius 4 -> radiusSq 25; dist 4 -> factor (1 - 16/25) = 0.36
        merge_light(&mut edge, 1.0, 4, 0, &ctx(LightKind::Fire, 100));
        assert_eq!(edge.value, 36);

        let mut outside = LightCell::default();
        merge_light(&mut outside, 1.0, 5, 0, &ctx(LightKind::Fire, 100));
        assert_eq!(outside.value, 0);
    }

    #[test]
    fn same_kind_saturates_at_255() {
        let mut cell = LightCell::default();
        let c = ctx(LightKind::Fire, 200);
        merge_light(&mut cell, 1.0, 0, 0, &c);
        merge_light(&mut cell, 1.0, 0, 0, &c);
        assert_eq!(cell.value, 255);
        assert_eq!(cell.kind, LightKind::Fire);
    }

    #[test]
    fn different_kind_wins_only_when_brighter() {
        let mut cell = LightCell::default();
        merge_light(&mut cell, 1.0, 0, 0, &ctx(LightKind::Fire, 100));
        // Dimmer acid does not displace fire.
        merge_light(&mut cell, 1.0, 0, 0, &ctx(LightKind::Acid, 50));
        assert_eq!((cell.value, cell.kind), (100, LightKind::Fire));
        // Brighter acid displaces fire.
        merge_light(&mut cell, 1.0, 0, 0, &ctx(LightKind::Acid, 150));
        assert_eq!((cell.value, cell.kind), (150, LightKind::Acid));
    }

    #[test]
    fn palette_lookup_matches_original_tables() {
        assert_eq!(light_to_palette(&LightCell::default()), BLACK);
        // Fire at full brightness -> white; dim fire -> dark yellow.
        assert_eq!(
            light_to_palette(&LightCell {
                value: 255,
                kind: LightKind::Fire
            }),
            WHITE
        );
        assert_eq!(
            light_to_palette(&LightCell {
                value: 1,
                kind: LightKind::Fire
            }),
            DARK_YELLOW
        );
        // Neutral light maps through the gray ramp.
        assert_eq!(
            light_to_palette(&LightCell {
                value: 1,
                kind: LightKind::None
            }),
            DARK_GRAY
        );
        assert_eq!(
            light_to_palette(&LightCell {
                value: 255,
                kind: LightKind::None
            }),
            WHITE
        );
    }
}
