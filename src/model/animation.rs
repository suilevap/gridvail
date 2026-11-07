use bevy::prelude::*;

/// Where an object is shown, written by the animation step.
///
/// `position` is in fractional grid cells and trails `Pos` while the object
/// moves between cells; `pose` is how far a gesture (a thrust, a shake)
/// takes it off that position, which is drawn but not followed (the camera
/// stays on `position`); `lift` is its height above the ground in cells;
/// `height` is how far it stands out of the floor, from 0 (sunk into it) to
/// 1 (standing), for an object [`Rising`] or [`Sinking`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct AnimatedPos {
    pub position: Vec2,
    pub pose: Vec2,
    pub lift: f32,
    pub height: f32,
}

impl AnimatedPos {
    pub fn at(cell: IVec2) -> Self {
        Self {
            position: cell.as_vec2(),
            pose: Vec2::ZERO,
            lift: 0.0,
            height: 1.0,
        }
    }

    /// Where it is drawn: its position, posed.
    pub fn drawn(&self) -> Vec2 {
        self.position + self.pose
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

/// How a gesture moves: out toward `toward` (in cells) and back, in one of
/// a few shapes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GestureShape {
    /// Out and back in one smooth arch (a bump, a lean).
    Reach { toward: Vec2 },
    /// A pull back, a fast thrust out to `toward` at the middle of the
    /// gesture (the moment of contact), and back: an attack.
    Thrust { toward: Vec2 },
    /// Swung in an arc across `toward`, from the side opposite `across` (a
    /// unit direction) to its side, straight at `toward` at the middle: a
    /// sweep.
    Sweep { toward: Vec2, across: Vec2 },
    /// Out to `toward` by the middle, then pulled back past the start: a
    /// hook dragging what it caught.
    Hook { toward: Vec2 },
    /// Knocked out toward `toward`, then shaking back and forth smaller and
    /// smaller: taking a blow.
    Shake { toward: Vec2 },
}

/// A move out and back that does not change the object's cell, `secs`
/// long, starting `delay` seconds from now: a swing, a recoil. Several
/// gestures (and held moves) timed against each other play one exchange in
/// order. Shown by the animation step, if there is one.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Gesture {
    pub entity: Entity,
    pub shape: GestureShape,
    pub secs: f32,
    pub delay: f32,
}

/// Holds `entity`'s moves to new cells for `secs`: it keeps being shown
/// where it was going (and any gesture plays on) while the simulation has
/// already moved it, so a knocked body starts sliding when the blow lands,
/// not when it was decided. The longest hold asked for wins.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct HoldMoves {
    pub entity: Entity,
    pub secs: f32,
}

/// `entity` winds up `shape` (a swing) over `secs` and holds it wound up
/// until released: an anticipation that lasts as long as it needs to (an
/// aimed weapon, a picked target).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct WindUp {
    pub entity: Entity,
    pub shape: GestureShape,
    pub secs: f32,
}

/// Ends `entity`'s wind-up over `secs`: with `strike`, it plays the rest
/// of the swing (the strike, the follow-through, settling back), else it
/// eases back to rest.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Release {
    pub entity: Entity,
    pub secs: f32,
    pub strike: bool,
}

/// `entity`'s next move to a new cell is flung, whatever its own style:
/// fast from the start, easing in at the end, `secs` a cell (a body
/// knocked by a blow).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Fling {
    pub entity: Entity,
    pub secs: f32,
}
