use bevy::prelude::*;

pub const VISIBILITY_THRESHOLD: f32 = 0.1;

#[derive(Component, Clone, Copy, Debug)]
pub struct VisualSensor {
    pub radius: i32,
}

#[derive(Component, Clone, Debug)]
pub struct FovResult {
    pub revision: u64,
    pub obstacle_revision: u64,
    pub pos: IVec2,
    pub radius: i32,
    pub data: Vec<f32>,
}

bitflags::bitflags! {
    #[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Vis: u8 {
        const VISIBLE = 1;
        const KNOWN = 2;
    }
}

#[derive(Component, Clone, Debug)]
pub struct VisibilityMap {
    pub revision: u64,
    pub data: Vec<Vis>,
}
