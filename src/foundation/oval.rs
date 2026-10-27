//! An oval of grid cells.

use bevy::prelude::*;

/// An oval of cells: `centre` and `radii` in cells. It holds no cell when
/// either radius is zero.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Oval {
    pub centre: Vec2,
    pub radii: Vec2,
}

impl Oval {
    pub fn contains(&self, p: IVec2) -> bool {
        self.radii.min_element() > 0.0
            && ((p.as_vec2() - self.centre) / self.radii).length_squared() <= 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ovals_hold_the_cells_within_them() {
        let oval = Oval {
            centre: Vec2::new(5.0, 5.0),
            radii: Vec2::new(2.0, 1.0),
        };
        assert!(oval.contains(IVec2::new(7, 5)) && oval.contains(IVec2::new(5, 4)));
        assert!(!oval.contains(IVec2::new(5, 3)) && !oval.contains(IVec2::new(7, 6)));
        let empty = Oval {
            radii: Vec2::ZERO,
            ..oval
        };
        assert!(!empty.contains(IVec2::new(5, 5)));
    }
}
