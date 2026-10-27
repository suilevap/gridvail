//! Searches over a wrapping grid of cells.

use bevy::prelude::*;

/// The cell nearest `p` (wrapped into a `size` grid, ring by ring) that
/// `free` accepts, or `None` if none does.
pub fn nearest_cell(p: IVec2, size: IVec2, free: impl Fn(IVec2) -> bool) -> Option<IVec2> {
    let size = size.max(IVec2::ONE);
    let wrap = |p: IVec2| p.rem_euclid(size);
    let reach = size.max_element();
    (0..=reach)
        .flat_map(|r| {
            (-r..=r)
                .flat_map(move |dy| (-r..=r).map(move |dx| IVec2::new(dx, dy)))
                .filter(move |d| d.abs().max_element() == r)
        })
        .map(|d| wrap(p + d))
        .find(|&cell| free(cell))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_free_cell_is_found_ring_by_ring_and_wraps() {
        let size = IVec2::new(5, 5);
        assert_eq!(
            nearest_cell(IVec2::new(2, 2), size, |_| true),
            Some(IVec2::new(2, 2))
        );
        let taken = |p: IVec2| p != IVec2::new(2, 2);
        let near = nearest_cell(IVec2::new(2, 2), size, taken).unwrap();
        assert_eq!((near - IVec2::new(2, 2)).abs().max_element(), 1);
        assert_eq!(
            nearest_cell(IVec2::new(6, -1), size, |p| p == IVec2::new(1, 4)),
            Some(IVec2::new(1, 4)),
            "wrapped into the grid"
        );
        assert_eq!(nearest_cell(IVec2::ZERO, size, |_| false), None);
    }
}
