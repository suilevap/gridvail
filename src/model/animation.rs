use bevy::prelude::*;

/// Where an object is shown, written by the animation step.
///
/// `position` is in fractional grid cells and trails `Pos` while the object
/// moves between cells; `lift` is its height above the ground in cells;
/// `height` is how far it stands out of the floor, from 0 (sunk into it) to
/// 1 (standing), for an object [`Rising`] or [`Sinking`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct AnimatedPos {
    pub position: Vec2,
    pub lift: f32,
    pub height: f32,
}

impl AnimatedPos {
    pub fn at(cell: IVec2) -> Self {
        Self {
            position: cell.as_vec2(),
            lift: 0.0,
            height: 1.0,
        }
    }
}

impl Default for AnimatedPos {
    fn default() -> Self {
        Self::at(IVec2::ZERO)
    }
}

/// An object that comes up out of the floor when it is placed (a wall a
/// room raises). It is in the world at once; only how it is shown rises.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Rising;

/// An object going down into the floor and out of the world (a wall a room
/// lowers). It is no longer on the map or in anyone's way; it is only shown
/// sinking, and the animation step despawns it once it has sunk.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Sinking;

/// An object that slides when it moves (a wall a room shifts), at a steady
/// pace, rather than stepping the way actors walk.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Sliding;
