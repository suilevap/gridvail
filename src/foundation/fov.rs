//! Field-of-view computation. Faithful port of `FieldOfViewComputation<T>`,
//! `FieldOfViewComputationInt2`, and `RangesCollectionV2`.
//!
//! The algorithm walks square rings around the observer. Each ring cell owns
//! an angular slice of the [0,1) circle; the occluded fraction of the slice
//! (from previously seen obstacles, with circular wrap at 0/1) determines
//! the fractional visibility `1 - occluded`. Results are deltas from the
//! observer plus a visibility value in [0,1].

use bevy::prelude::*;

use super::portal::{CellTransform, PortalFace};

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
            data: Vec::with_capacity(64),
            circular: true,
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Becomes a copy of `other`, reusing this set's storage.
    pub fn copy_from(&mut self, other: &Ranges) {
        self.data.clear();
        self.data.extend_from_slice(&other.data);
        self.circular = other.circular;
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
#[derive(Debug)]
pub struct FovComputer {
    ranges: Ranges,
    rings: RingCache,
}

impl Default for FovComputer {
    fn default() -> Self {
        Self::new()
    }
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
        let FovComputer { ranges, rings } = self;
        ranges.clear();
        out.push(FovSample {
            delta: IVec2::ZERO,
            value: 1.0,
        });
        for r in 1..=radius.max(0) {
            let ring = rings.ring(r);
            if ring.is_empty() {
                continue;
            }
            let cell = 1.0 / ring.len() as f32;
            for (index, delta) in ring.iter().enumerate() {
                let range = Range {
                    start: (index as f32 - 0.5) * cell,
                    end: (index as f32 + 0.5) * cell,
                };
                let occluded = ranges.intersect_length(range) / cell;
                out.push(FovSample {
                    delta: *delta,
                    // Roundoff in interval division can exceed 1 by a few
                    // ulps. Keep the public visibility field in [0, 1].
                    value: (1.0 - occluded).clamp(0.0, 1.0),
                });
                if occluded < 1.0 && is_obstacle(origin, *delta) {
                    ranges.add(range);
                }
            }
        }
    }
}

/// One cell of a view through portals: the cell `delta` from the viewer
/// shows the map cell `world`, seen through `transform` (the identity for
/// cells seen directly), with visibility `value` in [0, 1]. `portal` is set
/// where the view passes through a portal face: the cell where the face's
/// wall stands, which shows the floor beyond the exit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewSample {
    pub delta: IVec2,
    pub world: IVec2,
    pub transform: CellTransform,
    pub value: f32,
    pub portal: bool,
}

/// Part of the view seen through a portal (or directly, for the first):
/// cells in it show `transform` of the viewer's own cells, and `occluded`
/// holds the angles hidden in it.
#[derive(Debug)]
struct Window {
    transform: CellTransform,
    occluded: Ranges,
}

/// Field of view through portals.
///
/// The same ring walk as [`FovComputer`], with a list of windows. Each
/// window has its own occluded angles: it starts from what its parent had
/// hidden when the portal was reached, plus every angle outside the
/// portal's slice. A ring cell is shown from the window that sees most of
/// it, so a cell split between a portal and the wall around it goes to the
/// side that covers more of it, and the part of a cell outside the portal
/// stays hidden. A visible portal face opens a window (nested inside the
/// window it is seen in) and hides its slice in the parent like any wall.
/// Each ring is visited once, so the radius bounds how deep portals nest.
///
/// Without portals the samples equal [`FovComputer`]'s, with
/// `world = origin + delta`.
#[derive(Debug)]
pub struct PortalFovComputer {
    rings: RingCache,
    windows: Vec<Window>,
    open: usize,
}

impl Default for PortalFovComputer {
    fn default() -> Self {
        Self::new()
    }
}

impl PortalFovComputer {
    pub fn new() -> Self {
        Self {
            rings: RingCache::default(),
            windows: vec![Window {
                transform: CellTransform::IDENTITY,
                occluded: Ranges::new_circular(),
            }],
            open: 1,
        }
    }

    /// Computes the view from `origin`. `is_obstacle` and `portal_at` are
    /// asked about map cells (after portal transforms).
    pub fn compute(
        &mut self,
        origin: IVec2,
        radius: i32,
        is_obstacle: impl Fn(IVec2) -> bool,
        portal_at: impl Fn(IVec2) -> Option<PortalFace>,
        out: &mut Vec<ViewSample>,
    ) {
        out.clear();
        let PortalFovComputer {
            rings,
            windows,
            open,
        } = self;
        *open = 1;
        windows[0].transform = CellTransform::IDENTITY;
        windows[0].occluded.clear();
        out.push(ViewSample {
            delta: IVec2::ZERO,
            world: origin,
            transform: CellTransform::IDENTITY,
            value: 1.0,
            portal: false,
        });
        for r in 1..=radius.max(0) {
            let ring = rings.ring(r);
            if ring.is_empty() {
                continue;
            }
            let cell = 1.0 / ring.len() as f32;
            for (index, delta) in ring.iter().enumerate() {
                let range = Range {
                    start: (index as f32 - 0.5) * cell,
                    end: (index as f32 + 0.5) * cell,
                };
                // The window that sees most of this cell; the first (direct
                // view) on ties.
                let (mut seen_in, mut value) = (0, -1.0_f32);
                for (i, window) in windows[..*open].iter().enumerate() {
                    let visible = 1.0 - window.occluded.intersect_length(range) / cell;
                    if visible > value {
                        (seen_in, value) = (i, visible);
                    }
                }
                let mut transform = windows[seen_in].transform;
                let mut world = transform.apply(origin + *delta);
                let mut portal = false;
                if value > 0.0 {
                    let viewer = transform.apply(origin);
                    match portal_at(world) {
                        Some(face) if face.faces(world, viewer) => {
                            transform = transform.then(&face.through);
                            world = face.through.apply(world);
                            portal = true;
                            open_window(windows, open, seen_in, transform, range);
                            windows[seen_in].occluded.add(range);
                        }
                        _ if is_obstacle(world) => windows[seen_in].occluded.add(range),
                        _ => {}
                    }
                }
                out.push(ViewSample {
                    delta: *delta,
                    world,
                    transform,
                    // Roundoff in interval division can exceed 1 by a few
                    // ulps. Keep the public visibility field in [0, 1].
                    value: value.clamp(0.0, 1.0),
                    portal,
                });
            }
        }
    }
}

/// Opens a window through a portal whose slice is `range`, seen in window
/// `parent`: it hides what the parent hid and everything outside `range`.
/// Window storage is reused between computes.
fn open_window(
    windows: &mut Vec<Window>,
    open: &mut usize,
    parent: usize,
    transform: CellTransform,
    range: Range,
) {
    if *open == windows.len() {
        windows.push(Window {
            transform,
            occluded: Ranges::new_circular(),
        });
    }
    let (opened, unopened) = windows.split_at_mut(*open);
    let window = &mut unopened[0];
    window.transform = transform;
    window.occluded.copy_from(&opened[parent].occluded);
    // Slices start no lower than -half a cell and end below 1, so the
    // complement runs from the end round to the start.
    window.occluded.add(Range {
        start: range.end,
        end: range.start + 1.0,
    });
    *open += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_wraps_northwest_shadows_like_new() {
        let mut default = FovComputer::default();
        let mut explicit = FovComputer::new();
        let mut a = Vec::new();
        let mut b = Vec::new();
        default.compute(IVec2::ZERO, 4, |_, d| d == IVec2::new(-1, -1), &mut a);
        explicit.compute(IVec2::ZERO, 4, |_, d| d == IVec2::new(-1, -1), &mut b);
        assert_eq!(a, b);
        assert_eq!(
            a.iter()
                .find(|s| s.delta == IVec2::new(-4, -3))
                .unwrap()
                .value,
            0.0
        );
    }

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
        fov.compute(IVec2::ZERO, 3, |_, d| d == IVec2::new(1, 0), &mut out);
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
        for s in out
            .iter()
            .filter(|s| s.delta.x.abs().max(s.delta.y.abs()) == 2)
        {
            assert!(
                s.value.abs() < 1e-5,
                "delta {:?} should be fully blocked, got {}",
                s.delta,
                s.value
            );
        }
    }

    fn sample_at(samples: &[ViewSample], delta: IVec2) -> ViewSample {
        *samples
            .iter()
            .find(|s| s.delta == delta)
            .unwrap_or_else(|| panic!("no sample at {delta}"))
    }

    #[test]
    fn without_portals_the_portal_view_is_the_plain_view() {
        // Pseudo-random walls; the same samples from several origins.
        let wall = |p: IVec2| (p.x * 7 + p.y * 13 + p.x * p.y).rem_euclid(5) == 0;
        let mut plain = FovComputer::new();
        let mut portal = PortalFovComputer::new();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for origin in [IVec2::new(0, 0), IVec2::new(3, -2), IVec2::new(-4, 5)] {
            plain.compute(origin, 9, |o, d| wall(o + d), &mut a);
            portal.compute(origin, 9, wall, |_| None, &mut b);
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(&b) {
                assert_eq!(a.delta, b.delta);
                assert_eq!(a.value, b.value, "at {}", a.delta);
                assert_eq!(b.world, origin + b.delta);
                assert!(b.transform.is_identity());
                assert!(!b.portal);
            }
        }
    }

    /// A wall along x = 3 with a portal at (3, 0) open to the west, leading
    /// out of a wall at (20, 0) open to the east; one pillar at (23, 0).
    fn portal_world() -> (impl Fn(IVec2) -> bool, impl Fn(IVec2) -> Option<PortalFace>) {
        let portal = IVec2::new(3, 0);
        let face = PortalFace {
            side: IVec2::NEG_X,
            through: CellTransform::between_faces(
                portal,
                IVec2::NEG_X,
                IVec2::new(20, 0),
                IVec2::X,
            ),
        };
        let wall = |p: IVec2| p.x == 3 || p == IVec2::new(23, 0) || p.x == 20;
        (wall, move |p: IVec2| (p == portal).then_some(face))
    }

    #[test]
    fn a_portal_shows_what_lies_beyond_its_exit() {
        let (wall, portal_at) = portal_world();
        let mut fov = PortalFovComputer::new();
        let mut out = Vec::new();
        fov.compute(IVec2::ZERO, 8, &wall, portal_at, &mut out);
        let through = CellTransform::translation(IVec2::new(18, 0));

        // The portal wall itself shows the floor in front of the exit.
        let face = sample_at(&out, IVec2::new(3, 0));
        assert_eq!(
            (face.world, face.transform, face.value),
            (IVec2::new(21, 0), through, 1.0)
        );
        // It is the threshold, and the only one: cells beyond it are not.
        assert!(face.portal);
        assert_eq!(out.iter().filter(|sample| sample.portal).count(), 1);
        assert_eq!(sample_at(&out, IVec2::new(4, 0)).world, IVec2::new(22, 0));
        // The pillar beyond the exit is seen, and casts a shadow there.
        let pillar = sample_at(&out, IVec2::new(5, 0));
        assert_eq!((pillar.world, pillar.value), (IVec2::new(23, 0), 1.0));
        assert_eq!(sample_at(&out, IVec2::new(6, 0)).value, 0.0);
        // The rest of the wall still hides what is behind it.
        let behind = sample_at(&out, IVec2::new(5, 3));
        assert_eq!((behind.world, behind.value), (IVec2::new(5, 3), 0.0));
        // Everything seen directly is seen exactly as if the portal were a
        // plain wall: no cell behind the wall leaks through the portal's
        // slice.
        let mut plain = FovComputer::new();
        let mut walls_only = Vec::new();
        plain.compute(IVec2::ZERO, 8, |o, d| wall(o + d), &mut walls_only);
        for (sample, plain) in out.iter().zip(&walls_only) {
            if sample.transform.is_identity() {
                assert_eq!(sample.value, plain.value, "at {}", sample.delta);
            } else {
                assert_eq!(sample.transform, through);
            }
        }
    }

    #[test]
    fn a_turning_portal_shows_what_lies_beyond_its_exit_turned() {
        // Looking east into a wall open to the west at (3, 0) comes out of
        // a wall open to the south at (20, 0), looking south: a quarter
        // turn clockwise on screen. A pillar stands two cells beyond it.
        let portal = IVec2::new(3, 0);
        let through =
            CellTransform::between_faces(portal, IVec2::NEG_X, IVec2::new(20, 0), IVec2::Y);
        assert_eq!(through.quarters, 3);
        let face = PortalFace {
            side: IVec2::NEG_X,
            through,
        };
        let wall = |p: IVec2| p.x == 3 || (p.y == 0 && p.x >= 15) || p == IVec2::new(20, 3);
        let portal_at = move |p: IVec2| (p == portal).then_some(face);
        let mut fov = PortalFovComputer::new();
        let mut out = Vec::new();
        fov.compute(IVec2::ZERO, 8, wall, portal_at, &mut out);

        let face = sample_at(&out, IVec2::new(3, 0));
        assert_eq!(
            (face.world, face.transform, face.value),
            (IVec2::new(20, 1), through, 1.0)
        );
        assert_eq!(sample_at(&out, IVec2::new(4, 0)).world, IVec2::new(20, 2));
        // East of the viewer runs south beyond the exit, and south runs
        // west.
        assert_eq!(sample_at(&out, IVec2::new(4, 1)).world, IVec2::new(19, 2));
        let pillar = sample_at(&out, IVec2::new(5, 0));
        assert_eq!((pillar.world, pillar.value), (IVec2::new(20, 3), 1.0));
        assert_eq!(sample_at(&out, IVec2::new(6, 0)).value, 0.0);
        // Seen directly, the portal is a plain wall.
        let mut plain = FovComputer::new();
        let mut walls_only = Vec::new();
        plain.compute(IVec2::ZERO, 8, |o, d| wall(o + d), &mut walls_only);
        for (sample, plain) in out.iter().zip(&walls_only) {
            if sample.transform.is_identity() {
                assert_eq!(sample.value, plain.value, "at {}", sample.delta);
            } else {
                assert_eq!(sample.transform, through);
                assert_eq!(sample.world, through.apply(sample.delta));
            }
        }
    }

    #[test]
    fn a_portal_seen_from_behind_is_a_wall() {
        let (wall, portal_at) = portal_world();
        let mut fov = PortalFovComputer::new();
        let mut out = Vec::new();
        fov.compute(IVec2::new(6, 0), 8, wall, portal_at, &mut out);
        let back = sample_at(&out, IVec2::new(-3, 0));
        assert_eq!(
            (back.world, back.transform, back.portal),
            (IVec2::new(3, 0), CellTransform::IDENTITY, false)
        );
        assert_eq!(sample_at(&out, IVec2::new(-4, 0)).value, 0.0);
    }

    #[test]
    fn a_portal_into_its_own_corridor_repeats_it() {
        // Corridor x = 0..4 between walls at y = -1 and 1. Its east end
        // (5, 0) leads out of its west end (-1, 0).
        let east = IVec2::new(5, 0);
        let face = PortalFace {
            side: IVec2::NEG_X,
            through: CellTransform::between_faces(east, IVec2::NEG_X, IVec2::new(-1, 0), IVec2::X),
        };
        let wall = |p: IVec2| p.y.abs() == 1 || p.x == 5 || p.x == -1;
        let portal_at = |p: IVec2| (p == east).then_some(face);
        let mut fov = PortalFovComputer::new();
        let mut out = Vec::new();
        fov.compute(IVec2::new(2, 0), 13, wall, portal_at, &mut out);

        // Looking east: the corridor again, then again through the nested
        // portal, each time 5 cells further (its length).
        let once = sample_at(&out, IVec2::new(4, 0));
        assert_eq!(once.world, IVec2::new(1, 0));
        assert_eq!(
            once.transform,
            CellTransform::translation(IVec2::new(-5, 0))
        );
        let twice = sample_at(&out, IVec2::new(9, 0));
        assert_eq!(twice.world, IVec2::new(1, 0));
        assert_eq!(
            twice.transform,
            CellTransform::translation(IVec2::new(-10, 0))
        );
        assert!(twice.value > 0.9);
        // The viewer sees their own cell five cells ahead.
        assert_eq!(sample_at(&out, IVec2::new(5, 0)).world, IVec2::new(2, 0));
    }

    #[test]
    fn ranges_merge_and_wrap() {
        let mut r = Ranges::new_circular();
        r.add(Range {
            start: 0.1,
            end: 0.3,
        });
        r.add(Range {
            start: 0.2,
            end: 0.4,
        });
        assert!(
            (r.intersect_length(Range {
                start: 0.0,
                end: 0.5
            }) - 0.3)
                .abs()
                < 1e-6
        );
        // Wrap-around range crossing 1 -> 0.
        r.add(Range {
            start: 0.9,
            end: 1.2,
        });
        assert!(
            (r.intersect_length(Range {
                start: -0.2,
                end: 0.1
            }) - 0.2)
                .abs()
                < 1e-6
        );
    }
}
