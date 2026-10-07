//! Rigid grid transforms and the portal faces built from them.
//!
//! Map coordinates have y pointing down, so a quarter turn counter-clockwise
//! on screen takes east `(1, 0)` to up `(0, -1)`: `(x, y) -> (y, -x)`.

use bevy::prelude::*;

/// A rigid transform of whole cells: a rotation by quarter turns
/// (counter-clockwise on screen) followed by an offset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellTransform {
    /// Quarter turns counter-clockwise on screen, `0..4`.
    pub quarters: u8,
    pub offset: IVec2,
}

impl CellTransform {
    pub const IDENTITY: Self = Self {
        quarters: 0,
        offset: IVec2::ZERO,
    };

    pub fn translation(offset: IVec2) -> Self {
        Self {
            quarters: 0,
            offset,
        }
    }

    /// The transform of a portal: cells seen through the face of wall `a`
    /// whose open side is `side_a` continue from the floor in front of the
    /// face of wall `b` whose open side is `side_b`:
    /// `T(c) = b + side_b + R(c - a)`, with `R(-side_a) = side_b`.
    ///
    /// The wall `a` itself shows the floor in front of `b`, and the floor in
    /// front of `a` maps onto `b`. Sides must be unit orthogonal vectors.
    pub fn between_faces(a: IVec2, side_a: IVec2, b: IVec2, side_b: IVec2) -> Self {
        let quarters = (0..4)
            .find(|&q| turn(q, -side_a) == side_b)
            .expect("portal sides are unit orthogonal vectors");
        Self {
            quarters,
            offset: b + side_b - turn(quarters, a),
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// Where this transform takes a cell.
    pub fn apply(&self, cell: IVec2) -> IVec2 {
        self.turn(cell) + self.offset
    }

    /// Where this transform takes a point in fractional cells.
    pub fn apply_point(&self, point: Vec2) -> Vec2 {
        self.turn_vec(point) + self.offset.as_vec2()
    }

    /// Turns a direction (no offset).
    pub fn turn(&self, v: IVec2) -> IVec2 {
        turn(self.quarters, v)
    }

    /// Turns a direction in fractional cells (no offset).
    pub fn turn_vec(&self, v: Vec2) -> Vec2 {
        (0..self.quarters).fold(v, |v, _| Vec2::new(v.y, -v.x))
    }

    /// `self` first, then `next`.
    pub fn then(&self, next: &Self) -> Self {
        Self {
            quarters: (self.quarters + next.quarters) % 4,
            offset: next.turn(self.offset) + next.offset,
        }
    }

    pub fn inverse(&self) -> Self {
        let quarters = (4 - self.quarters) % 4;
        Self {
            quarters,
            offset: -turn(quarters, self.offset),
        }
    }
}

/// Turns `v` by `quarters` quarter turns counter-clockwise on screen.
fn turn(quarters: u8, v: IVec2) -> IVec2 {
    (0..quarters % 4).fold(v, |v, _| IVec2::new(v.y, -v.x))
}

/// One face of a wall cell that shows, and leads to, another place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalFace {
    /// The open side: from the wall towards the floor a viewer stands on.
    pub side: IVec2,
    /// Takes cells seen through the face to the cells they show (see
    /// [`CellTransform::between_faces`]).
    pub through: CellTransform,
}

impl PortalFace {
    /// Whether a viewer at `viewer` sees the face of the wall at `wall`,
    /// standing on its open side.
    pub fn faces(&self, wall: IVec2, viewer: IVec2) -> bool {
        (viewer - wall).dot(self.side) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turns_go_counter_clockwise_on_screen() {
        let east = CellTransform {
            quarters: 1,
            offset: IVec2::ZERO,
        };
        assert_eq!(east.apply(IVec2::X), IVec2::NEG_Y);
        assert_eq!(east.apply(IVec2::NEG_Y), IVec2::NEG_X);
        assert_eq!(east.turn_vec(Vec2::X), Vec2::NEG_Y);
    }

    #[test]
    fn facing_walls_join_by_translation() {
        // A wall open to the west at (14, 5) and one open to the east at
        // (16, 5): the cells behind the first continue past the second.
        let t = CellTransform::between_faces(
            IVec2::new(14, 5),
            IVec2::NEG_X,
            IVec2::new(16, 5),
            IVec2::X,
        );
        assert_eq!(t, CellTransform::translation(IVec2::new(3, 0)));
        assert_eq!(t.apply(IVec2::new(14, 5)), IVec2::new(17, 5));
        assert_eq!(t.apply(IVec2::new(13, 5)), IVec2::new(16, 5));
    }

    #[test]
    fn faces_at_right_angles_turn_the_view() {
        // Into a wall open to the south, out of a wall open to the east.
        let a = IVec2::new(3, 0);
        let b = IVec2::new(10, 4);
        let t = CellTransform::between_faces(a, IVec2::Y, b, IVec2::X);
        assert_eq!(t.apply(a), b + IVec2::X, "the wall shows the exit floor");
        assert_eq!(
            t.apply(a + IVec2::Y),
            b,
            "the entry floor maps onto the exit wall"
        );
        // Looking north into the wall continues east beyond the exit.
        assert_eq!(t.turn(IVec2::NEG_Y), IVec2::X);
        assert_eq!(t.apply(a + IVec2::NEG_Y), b + IVec2::new(2, 0));
    }

    #[test]
    fn transforms_compose_and_invert() {
        let a = CellTransform {
            quarters: 1,
            offset: IVec2::new(3, -2),
        };
        let b = CellTransform {
            quarters: 3,
            offset: IVec2::new(-5, 7),
        };
        for cell in [IVec2::ZERO, IVec2::new(4, 1), IVec2::new(-2, 9)] {
            assert_eq!(a.then(&b).apply(cell), b.apply(a.apply(cell)));
            assert_eq!(a.inverse().apply(a.apply(cell)), cell);
            let p = cell.as_vec2() + Vec2::new(0.25, -0.5);
            assert!(a.inverse().apply_point(a.apply_point(p)).distance(p) < 1e-5);
        }
        assert!(a.then(&a.inverse()).is_identity());
    }

    #[test]
    fn a_face_is_seen_only_from_its_open_side() {
        let face = PortalFace {
            side: IVec2::NEG_X,
            through: CellTransform::IDENTITY,
        };
        let wall = IVec2::new(5, 5);
        assert!(face.faces(wall, IVec2::new(2, 9)));
        assert!(!face.faces(wall, IVec2::new(5, 9)));
        assert!(!face.faces(wall, IVec2::new(7, 5)));
    }
}
