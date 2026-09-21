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

pub fn merge_light(target: &mut LightCell, fov: f32, x: i32, y: i32, context: &LightContext) {
    let dx = x - context.center_x;
    let dy = y - context.center_y;
    let distance_sq = dx * dx + dy * dy;
    if distance_sq > context.radius_sq {
        return;
    }
    let amount = (fov
        * (1.0 - distance_sq as f32 * context.inv_radius_sq)
        * context.base_value as f32) as u8;
    if target.kind.overlaps(context.kind) {
        target.value = target.value.saturating_add(amount);
    } else if amount > target.value {
        target.value = amount;
        target.kind = context.kind;
    }
}
