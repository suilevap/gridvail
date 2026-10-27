//! Ways to lay out walls: replaceable sources ([`LayoutSource`]) that each
//! turn a request (a map's size and a point to build around) into a
//! [`Layout`], the walls and the oval they fill.
//!
//! A layout is plain data: it knows nothing of entities, rooms or how the
//! walls appear. Whoever asks for one decides that (the game's intro shows
//! one and brings the first map together from it). Each source lives in its
//! own file here: text art ([`ArtLayout`]), cells scattered at random
//! ([`ScatterLayout`]), and [`Fallback`], which tries one source and then
//! another.

mod art;
mod fallback;
mod scatter;

use bevy::prelude::*;

pub use art::{Art, ArtLayout};
pub use fallback::Fallback;
pub use scatter::{scatter_in_oval, ScatterLayout};

/// What a layout is asked for: a map of `bounds` cells, built around
/// `centre`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutRequest {
    pub bounds: IVec2,
    pub centre: IVec2,
}

impl LayoutRequest {
    pub fn contains(&self, p: IVec2) -> bool {
        p.cmpge(IVec2::ZERO).all() && p.cmplt(self.bounds).all()
    }
}

/// An oval of cells: `centre` and `radii` in cells.
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// Walls laid out on a map, and the oval they fill (to look at them).
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub walls: Vec<IVec2>,
    pub frame: Oval,
}

/// A way to lay out walls. `None` when it cannot for this request (art
/// larger than the map, say), so another can be tried ([`Fallback`]).
pub trait LayoutSource: Send + Sync {
    fn layout(&self, request: &LayoutRequest) -> Option<Layout>;
}

impl<T: LayoutSource + ?Sized> LayoutSource for Box<T> {
    fn layout(&self, request: &LayoutRequest) -> Option<Layout> {
        (**self).layout(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every source keeps its walls on the map and inside its frame.
    pub(super) fn check(layout: &Layout, request: &LayoutRequest) {
        for &p in &layout.walls {
            assert!(request.contains(p), "{p} off the map");
            assert!(layout.frame.contains(p), "{p} outside the frame");
        }
    }

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
