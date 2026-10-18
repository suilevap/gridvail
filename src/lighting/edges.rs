//! Shadow edges inside a cell.
//!
//! Light reaches a cell on the edge of a shadow only in part (its field of
//! view value is between 0 and 1). Which part is found by testing points
//! inside the cell: a 2 by 4 grid, in the order of the dots of a braille
//! character, so the result can be drawn as one. A cell then shows its
//! floor only where the light falls, drawing the shadow's edge finer than
//! the grid.

use bevy::prelude::{IVec2, Vec2};

/// Every point of a cell lit.
pub const FULLY_LIT: u8 = 0xFF;

/// The points tested in a cell, as (column, row) of a 2 by 4 grid, in the
/// order of braille dots 1 to 8 (bit 0 is dot 1).
const POINTS: [(u8, u8); 8] = [
    (0, 0),
    (0, 1),
    (0, 2),
    (1, 0),
    (1, 1),
    (1, 2),
    (0, 3),
    (1, 3),
];

/// Which points of `cell` a light at the centre of cell `light` reaches,
/// a bit per point (see [`POINTS`]): those with no blocking cell between.
pub fn lit_points(light: IVec2, cell: IVec2, blocks: &impl Fn(IVec2) -> bool) -> u8 {
    let from = light.as_vec2() + Vec2::splat(0.5);
    POINTS
        .iter()
        .enumerate()
        .filter(|(_, &(column, row))| {
            let to =
                cell.as_vec2() + Vec2::new((column as f32 + 0.5) / 2.0, (row as f32 + 0.5) / 4.0);
            clear_line(from, to, blocks)
        })
        .fold(0, |lit, (bit, _)| lit | 1 << bit)
}

/// Whether the segment `from`-`to` crosses no blocking cell between the
/// cells it starts and ends in, walking the cells it passes through.
fn clear_line(from: Vec2, to: Vec2, blocks: &impl Fn(IVec2) -> bool) -> bool {
    let mut cell = from.floor().as_ivec2();
    let end = to.floor().as_ivec2();
    let delta = to - from;
    let step = |d: f32| {
        if d > 0.0 {
            1
        } else if d < 0.0 {
            -1
        } else {
            0
        }
    };
    let step = IVec2::new(step(delta.x), step(delta.y));
    let first = |from: f32, cell: i32, step: i32, d: f32| match step {
        1 => (cell as f32 + 1.0 - from) / d,
        -1 => (from - cell as f32) / -d,
        _ => f32::INFINITY,
    };
    let mut next = Vec2::new(
        first(from.x, cell.x, step.x, delta.x),
        first(from.y, cell.y, step.y, delta.y),
    );
    let each = Vec2::new(1.0 / delta.x.abs(), 1.0 / delta.y.abs());
    // A straight walk crosses each column and row between once; rounding
    // could step past the end cell, so the walk stops there.
    for _ in 0..(end - cell).abs().element_sum() {
        if next.x < next.y {
            cell.x += step.x;
            next.x += each.x;
        } else {
            cell.y += step.y;
            next.y += each.y;
        }
        if cell == end {
            break;
        }
        if blocks(cell) {
            return false;
        }
    }
    true
}

/// The lit part as braille dots, thinned to a checkerboard so a cell
/// shows about as many dots as the floor's single dot does elsewhere. Too
/// little lit for the checkerboard, it falls back to [`lit_dot`].
pub fn lit_dots(lit: u8) -> char {
    const CHECKERBOARD: u8 = 0x01 | 0x10 | 0x04 | 0x80;
    match lit & CHECKERBOARD {
        0 => lit_dot(lit),
        dots => braille(dots),
    }
}

/// One braille dot where the lit part's middle is.
pub fn lit_dot(lit: u8) -> char {
    let lit_points: Vec2 = POINTS
        .iter()
        .enumerate()
        .filter(|(bit, _)| lit & 1 << bit != 0)
        .map(|(_, &(column, row))| Vec2::new(column as f32, row as f32))
        .fold(Vec2::ZERO, |sum, point| sum + point);
    let count = lit.count_ones().max(1) as f32;
    let middle = lit_points / count;
    let nearest = POINTS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let distance = |&&(column, row): &&(u8, u8)| {
                Vec2::new(column as f32, row as f32).distance_squared(middle)
            };
            distance(a).total_cmp(&distance(b))
        })
        .map_or(0, |(bit, _)| bit);
    braille(1 << nearest)
}

/// The lit part as quarter blocks: a quarter is lit when either of its two
/// points is.
pub fn lit_blocks(lit: u8) -> char {
    const QUARTERS: [char; 16] = [
        ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
    ];
    // Top left: dots 1, 2; top right: 4, 5; bottom left: 3, 7; bottom
    // right: 6, 8.
    let quarter = |mask: u8| (lit & mask != 0) as usize;
    QUARTERS[quarter(0x03) | quarter(0x18) << 1 | quarter(0x44) << 2 | quarter(0xA0) << 3]
}

fn braille(dots: u8) -> char {
    char::from_u32(0x2800 + dots as u32).unwrap_or(' ')
}
