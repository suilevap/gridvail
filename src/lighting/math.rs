use crate::model::{LightCell, LightKind};

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

/// How much of a light reaches `(x, y)`, seen by it with visibility `fov`:
/// its base value fading as `1 - distance² / (radius + 1)²`.
pub fn light_amount(fov: f32, x: i32, y: i32, context: &LightContext) -> u8 {
    let dx = x - context.center_x;
    let dy = y - context.center_y;
    let distance_sq = dx * dx + dy * dy;
    if distance_sq > context.radius_sq {
        return 0;
    }
    (fov * (1.0 - distance_sq as f32 * context.inv_radius_sq) * context.base_value as f32) as u8
}

/// Adds a light to the cell at `(x, y)`, to the level of its kind: every
/// light counts, and the cell's colour mixes them all.
pub fn merge_light(target: &mut LightCell, fov: f32, x: i32, y: i32, context: &LightContext) {
    target.add(context.kind, light_amount(fov, x, y, context));
}
