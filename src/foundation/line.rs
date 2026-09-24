//! Grid line of sight.

use bevy::prelude::*;

/// Whether every cell strictly between `from` and `to` on a Bresenham line is
/// clear. The end points themselves are not tested.
pub fn line_clear(from: IVec2, to: IVec2, blocked: impl Fn(IVec2) -> bool) -> bool {
    let delta = (to - from).abs();
    let step = (to - from).signum();
    let mut error = delta.x - delta.y;
    let mut at = from;
    while at != to {
        let doubled = 2 * error;
        if doubled > -delta.y {
            error -= delta.y;
            at.x += step.x;
        }
        if doubled < delta.x {
            error += delta.x;
            at.y += step.y;
        }
        if at != to && blocked(at) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_ground_is_clear_both_ways() {
        let from = IVec2::new(0, 0);
        let to = IVec2::new(5, 2);
        assert!(line_clear(from, to, |_| false));
        assert!(line_clear(to, from, |_| false));
    }

    #[test]
    fn a_wall_between_blocks_but_one_at_the_end_does_not() {
        let wall = IVec2::new(2, 0);
        assert!(!line_clear(IVec2::ZERO, IVec2::new(4, 0), |p| p == wall));
        assert!(line_clear(IVec2::ZERO, wall, |p| p == wall));
    }
}
