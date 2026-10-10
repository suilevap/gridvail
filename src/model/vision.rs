use bevy::prelude::*;

use crate::foundation::fov::ViewSample;

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

/// What the player sees around them, through portals: one sample per cell
/// within the sensor radius, giving the map cell it shows (see
/// `PortalFovComputer`). Recomputed when the player moves or the walls or
/// portals change.
#[derive(Component, Clone, Debug)]
pub struct PlayerView {
    pub revision: u64,
    pub pos: IVec2,
    pub radius: i32,
    pub obstacle_revision: u64,
    pub portal_revision: u64,
    pub samples: Vec<ViewSample>,
}

impl PlayerView {
    /// Empty, with room for every sample of a `radius` view.
    pub fn with_radius(radius: i32) -> Self {
        let side = (2 * radius.max(0) + 1) as usize;
        Self {
            revision: 0,
            pos: IVec2::splat(i32::MIN),
            radius: -1,
            obstacle_revision: u64::MAX,
            portal_revision: u64::MAX,
            samples: Vec::with_capacity(side * side),
        }
    }
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
