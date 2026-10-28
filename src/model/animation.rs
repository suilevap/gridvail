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
/// room raises): it rises at `from`, then moves to its cell (`Pos`). It is
/// in the world at its cell at once; only how it is shown rises and moves.
/// `from` is its own cell to rise in place.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rising {
    pub from: IVec2,
}

/// An object going down into the floor and out of the world (a wall a room
/// lowers). It is no longer on the map or in anyone's way; it is only shown
/// moving to its `Pos` (anywhere: where it is shown going, not a place on
/// the map) and then sinking there, and the animation step despawns it once
/// it has sunk.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Sinking;

/// An object that slides when it moves (a wall a room shifts), at a steady
/// pace, rather than stepping the way actors walk.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Sliding;

/// The most corners one slide can turn.
pub const MAX_CORNERS: usize = 8;

/// The cells a slide turns at between where it starts and where it ends, in
/// order: a wall riding rails that bend. None for a straight slide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Corners {
    cells: [IVec2; MAX_CORNERS],
    len: u8,
}

impl Corners {
    /// `None` with more than [`MAX_CORNERS`].
    pub fn new(cells: &[IVec2]) -> Option<Self> {
        let mut corners = Self::default();
        corners.cells.get_mut(..cells.len())?.copy_from_slice(cells);
        corners.len = cells.len() as u8;
        Some(corners)
    }

    pub fn as_slice(&self) -> &[IVec2] {
        &self.cells[..self.len as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Cells along the slide from `from` through the corners to `to`.
    pub fn length(&self, from: IVec2, to: IVec2) -> i32 {
        let mut length = 0;
        let mut at = from;
        for &next in self.as_slice().iter().chain([&to]) {
            length += (next - at).abs().element_sum();
            at = next;
        }
        length
    }
}

/// The corners of the slide a [`Sliding`] object is on, when it turns: the
/// animation step follows them instead of going straight.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlideRoute(pub Corners);
