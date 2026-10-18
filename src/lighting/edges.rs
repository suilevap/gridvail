//! Shadow edges inside a cell.
//!
//! Light reaches a cell on the edge of a shadow only in part (its field of
//! view value is between 0 and 1). Which part is found by testing points
//! inside the cell, a [`POINTS`] by [`POINTS`] grid, so a renderer can draw
//! the shadow's edge finer than the grid.

use bevy::prelude::{IVec2, Vec2};

/// Points tested along each side of a cell.
pub const POINTS: usize = 4;

/// Every point of a cell lit.
pub const FULLY_LIT: u16 = u16::MAX;

/// Which points of `cell` a light at the centre of cell `light` reaches,
/// a bit per point, row by row from the top left (see [`point_lit`]):
/// those with no blocking cell between.
pub fn lit_points(light: IVec2, cell: IVec2, blocks: &impl Fn(IVec2) -> bool) -> u16 {
    let from = light.as_vec2() + Vec2::splat(0.5);
    let mut lit = 0;
    for row in 0..POINTS {
        for column in 0..POINTS {
            let to = cell.as_vec2() + (Vec2::new(column as f32, row as f32) + 0.5) / POINTS as f32;
            if clear_line(from, to, blocks) {
                lit |= 1 << (row * POINTS + column);
            }
        }
    }
    lit
}

/// Whether point `column`, `row` of a cell is lit in `lit`.
pub fn point_lit(lit: u16, column: usize, row: usize) -> bool {
    lit & 1 << (row * POINTS + column) != 0
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
