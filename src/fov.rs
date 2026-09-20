//! Field-of-view computation. Faithful port of `FieldOfViewComputation<T>`,
//! `FieldOfViewComputationInt2`, and `RangesCollectionV2`.
//!
//! The algorithm walks square rings around the observer. Each ring cell owns
//! an angular slice of the [0,1) circle; the occluded fraction of the slice
//! (from previously seen obstacles, with circular wrap at 0/1) determines
//! the fractional visibility `1 - occluded`. Results are deltas from the
//! observer plus a visibility value in [0,1].

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub start: f32,
    pub end: f32,
}

impl Range {
    /// -1 = entirely before, 1 = entirely after, 0 = overlapping/touching.
    fn compare(a: &Range, b: &Range) -> i32 {
        if a.end < b.start {
            -1
        } else if a.start > b.end {
            1
        } else {
            0
        }
    }
}

/// Sorted, merged interval set with circular [0,1) wrap.
#[derive(Clone, Debug, Default)]
pub struct Ranges {
    data: Vec<Range>,
    circular: bool,
}

impl Ranges {
    pub fn new_circular() -> Self {
        Self {
            data: Vec::new(),
            circular: true,
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn add(&mut self, mut range: Range) {
        if self.circular {
            if range.start < 0.0 {
                self.add_internal(Range {
                    start: range.start + 1.0,
                    end: 1.0,
                });
                range.start = 0.0;
            }
            if range.end > 1.0 {
                self.add_internal(Range {
                    start: 0.0,
                    end: range.end - 1.0,
                });
                range.end = 1.0;
            }
        }
        self.add_internal(range);
    }

    fn add_internal(&mut self, mut range: Range) {
        let mut insert_at = self.data.len();
        let mut i = 0;
        while i < self.data.len() {
            match Range::compare(&self.data[i], &range) {
                0 => {
                    range.start = range.start.min(self.data[i].start);
                    range.end = range.end.max(self.data[i].end);
                    self.data.remove(i);
                    insert_at = insert_at.min(i);
                }
                1 => {
                    insert_at = insert_at.min(i);
                    break;
                }
                _ => i += 1,
            }
        }
        self.data.insert(insert_at.min(self.data.len()), range);
    }

    pub fn intersect_length(&self, mut range: Range) -> f32 {
        let mut result = 0.0;
        if self.circular {
            if range.start < 0.0 {
                result += self.intersect_internal(Range {
                    start: range.start + 1.0,
                    end: 1.0,
                });
                range.start = 0.0;
            }
            if range.end > 1.0 {
                result += self.intersect_internal(Range {
                    start: 0.0,
                    end: range.end - 1.0,
                });
                range.end = 1.0;
            }
        }
        result += self.intersect_internal(range);
        result.min(1.0)
    }

    fn intersect_internal(&self, range: Range) -> f32 {
        let mut result = 0.0;
        for r in &self.data {
            match Range::compare(r, &range) {
                0 => {
                    result += range.end.min(r.end) - range.start.max(r.start);
                }
                1 => break,
                _ => {}
            }
        }
        result.min(1.0)
    }
}

/// Cached square-ring point sets (mirrors the static `_radiusPoints` cache).
#[derive(Debug, Default)]
pub struct RingCache {
    rings: Vec<Vec<IVec2>>,
}

impl RingCache {
    pub fn ring(&mut self, radius: i32) -> &[IVec2] {
        let r = radius.max(0) as usize;
        if self.rings.len() <= r {
            self.rings.resize(r + 1, Vec::new());
        }
        if self.rings[r].is_empty() && r > 0 {
            self.rings[r] = ring_points(radius);
        }
        &self.rings[r]
    }
}

/// Square ring of Chebyshev radius `r`, in `GetCirclePoints` order:
/// top row, right column, bottom row (mirrored), left column (mirrored).
fn ring_points(r: i32) -> Vec<IVec2> {
    let mut out = Vec::with_capacity((r * 8).max(0) as usize);
    for i in -r..r {
        out.push(IVec2::new(i, -r));
    }
    for i in -r..r {
        out.push(IVec2::new(r, i));
    }
    for i in -r..r {
        out.push(IVec2::new(-i, r));
    }
    for i in -r..r {
        out.push(IVec2::new(-r, -i));
    }
    out
}

/// One FOV sample: cell offset from the observer + visibility in [0,1].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FovSample {
    pub delta: IVec2,
    pub value: f32,
}

/// Stateful FOV computer (holds the occlusion set + ring cache).
#[derive(Debug, Default)]
pub struct FovComputer {
    ranges: Ranges,
    rings: RingCache,
}

impl FovComputer {
    pub fn new() -> Self {
        Self {
            ranges: Ranges::new_circular(),
            rings: RingCache::default(),
        }
    }

    /// Mirrors `FieldOfViewComputation.Compute`. `is_obstacle` receives the
    /// observer cell and the candidate delta.
    pub fn compute(
        &mut self,
        origin: IVec2,
        radius: i32,
        is_obstacle: impl Fn(IVec2, IVec2) -> bool,
        out: &mut Vec<FovSample>,
    ) {
        out.clear();
        self.ranges.clear();
        out.push(FovSample {
            delta: IVec2::ZERO,
            value: 1.0,
        });
        for r in 1..=radius.max(0) {
            let ring = self.rings.ring(r).to_vec();
            if ring.is_empty() {
                continue;
            }
            let cell = 1.0 / ring.len() as f32;
            for (index, delta) in ring.iter().enumerate() {
                let range = Range {
                    start: (index as f32 - 0.5) * cell,
                    end: (index as f32 + 0.5) * cell,
                };
                let occluded = self.ranges.intersect_length(range) / cell;
                out.push(FovSample {
                    delta: *delta,
                    value: 1.0 - occluded,
                });
                if occluded < 1.0 && is_obstacle(origin, *delta) {
                    self.ranges.add(range);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_order_and_count_match_original() {
        let ring = ring_points(1);
        assert_eq!(ring.len(), 8);
        assert_eq!(
            ring,
            vec![
                IVec2::new(-1, -1),
                IVec2::new(0, -1),
                IVec2::new(1, -1),
                IVec2::new(1, 0),
                IVec2::new(1, 1),
                IVec2::new(0, 1),
                IVec2::new(-1, 1),
                IVec2::new(-1, 0),
            ]
        );
        assert_eq!(ring_points(2).len(), 16);
    }

    #[test]
    fn open_field_is_fully_visible() {
        let mut fov = FovComputer::new();
        let mut out = Vec::new();
        fov.compute(IVec2::ZERO, 2, |_, _| false, &mut out);
        // 1 center + 8 ring-1 + 16 ring-2 samples, all unoccluded.
        assert_eq!(out.len(), 25);
        for s in &out {
            assert_eq!(s.value, 1.0, "delta {:?}", s.delta);
        }
    }

    #[test]
    fn obstacle_casts_partial_shadow() {
        let mut fov = FovComputer::new();
        let mut out = Vec::new();
        // Single obstacle directly east of the observer.
        fov.compute(
            IVec2::ZERO,
            3,
            |_, d| d == IVec2::new(1, 0),
            &mut out,
        );
        // The cell straight behind is fully covered by the obstacle slice...
        let blocked = out
            .iter()
            .find(|s| s.delta == IVec2::new(2, 0))
            .expect("sample behind obstacle");
        assert_eq!(blocked.value, 0.0);
        // ...while the diagonal neighbour is only partially shadowed.
        let partial = out
            .iter()
            .find(|s| s.delta == IVec2::new(2, 1))
            .expect("diagonal sample");
        assert!(
            partial.value < 1.0 && partial.value > 0.0,
            "expected partial shadow, got {}",
            partial.value
        );
        // Side cells stay fully visible.
        let side = out
            .iter()
            .find(|s| s.delta == IVec2::new(0, 2))
            .expect("side sample");
        assert_eq!(side.value, 1.0);
    }

    #[test]
    fn closed_ring_blocks_everything_behind() {
        let mut fov = FovComputer::new();
        let mut out = Vec::new();
        // Full ring of obstacles at Chebyshev distance 1.
        fov.compute(
            IVec2::ZERO,
            2,
            |_, d| d.x.abs().max(d.y.abs()) == 1,
            &mut out,
        );
        for s in out.iter().filter(|s| s.delta.x.abs().max(s.delta.y.abs()) == 2) {
            assert!(
                s.value.abs() < 1e-5,
                "delta {:?} should be fully blocked, got {}",
                s.delta,
                s.value
            );
        }
    }

    #[test]
    fn ranges_merge_and_wrap() {
        let mut r = Ranges::new_circular();
        r.add(Range { start: 0.1, end: 0.3 });
        r.add(Range { start: 0.2, end: 0.4 });
        assert!((r.intersect_length(Range { start: 0.0, end: 0.5 }) - 0.3).abs() < 1e-6);
        // Wrap-around range crossing 1 -> 0.
        r.add(Range { start: 0.9, end: 1.2 });
        assert!((r.intersect_length(Range { start: -0.2, end: 0.1 }) - 0.2).abs() < 1e-6);
    }
}
