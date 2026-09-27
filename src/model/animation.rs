use bevy::prelude::*;

/// Where an object is shown, written by the animation step.
///
/// `position` is in fractional grid cells and trails `Pos` while the object
/// moves between cells.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimatedPos {
    pub position: Vec2,
}

impl AnimatedPos {
    pub fn at(cell: IVec2) -> Self {
        Self {
            position: cell.as_vec2(),
        }
    }
}
