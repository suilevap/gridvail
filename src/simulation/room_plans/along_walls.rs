use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use bevy::prelude::*;

use super::{Candidate, RoomPlanner, SwitchContext, SwitchPlan};

/// Walls move along the walls they belong to, and across floor only as far
/// as they must. A cell the new layout loses and one it gains are joined by
/// the path that crosses the fewest floor cells, and of those the one with
/// the fewest steps. A path goes
///
/// - through cells that are wall in both layouts, one step at a time and
///   around corners: each wall on the way shifts one cell along, as a gap
///   travels along a wall;
/// - across empty floor (floor in both layouts, nothing standing on it) in
///   straight runs from wall to wall: the wall at the run's start glides
///   to its end.
///
/// So a path through walls always wins; with none, walls shift as far as
/// they can along their wall, then glide across the floor. In a room inside
/// a room the inner walls stay inner walls and the outer ones outer.
/// [`LineSlides`](super::LineSlides)' slides are the straight paths.
///
/// The best paths are taken first, and no cell is part of two. What is
/// left rises or sinks in place.
#[derive(Clone, Copy, Debug, Default)]
pub struct AlongWalls;

/// A path's cost: floor cells crossed, then steps.
type Cost = (i32, i32);

/// Paths longer than this, in steps, are not worth taking.
const MAX_STEPS: i32 = 64;

impl RoomPlanner for AlongWalls {
    fn plan(&self, switch: &SwitchContext) -> SwitchPlan {
        SwitchPlan::from_candidates(switch, paths(switch))
    }
}

/// From every lost cell, the cheapest path to each gained cell it reaches.
fn paths(switch: &SwitchContext) -> Vec<Candidate> {
    let from: HashSet<IVec2> = switch.from.iter().copied().collect();
    let to: HashSet<IVec2> = switch.to.iter().copied().collect();
    let added: HashSet<IVec2> = switch.added_in(&from).collect();
    let (low, high) = (switch.room.origin, switch.room.origin + switch.room.size);
    let inside = |p: IVec2| p.cmpge(low).all() && p.cmplt(high).all();
    let wall = |p: IVec2| from.contains(&p) && to.contains(&p) && switch.holds_wall(p);
    let floor = |p: IVec2| inside(p) && !from.contains(&p) && !to.contains(&p) && switch.is_free(p);

    let mut candidates = Vec::new();
    for start in switch.removed_in(&to).filter(|&p| switch.holds_wall(p)) {
        // Where each node was reached from, at what cost.
        let mut best: HashMap<IVec2, (Cost, IVec2)> = HashMap::new();
        let mut queue = BinaryHeap::from([Reverse(((0, 0), (start.x, start.y)))]);
        while let Some(Reverse((cost, (x, y)))) = queue.pop() {
            let p = IVec2::new(x, y);
            if p != start && best.get(&p).is_some_and(|&(known, _)| known < cost) {
                continue;
            }
            if added.contains(&p) {
                candidates.push(path(start, p, &best));
                continue;
            }
            for step in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
                // Over a straight run of floor (maybe none) to a wall or a
                // gained cell.
                let mut next = p + step;
                let mut crossed = 0;
                while floor(next) && crossed < MAX_STEPS {
                    next += step;
                    crossed += 1;
                }
                if next == start || !(wall(next) || added.contains(&next)) {
                    continue;
                }
                let reached = (cost.0 + crossed, cost.1 + crossed + 1);
                if reached.1 > MAX_STEPS
                    || best.get(&next).is_some_and(|&(known, _)| known <= reached)
                {
                    continue;
                }
                best.insert(next, (reached, p));
                queue.push(Reverse((reached, (next.x, next.y))));
            }
        }
    }
    candidates
}

/// The path from `start` to `end`: each wall on it moves on to the next
/// node, and it takes up every cell it passes.
fn path(start: IVec2, end: IVec2, best: &HashMap<IVec2, (Cost, IVec2)>) -> Candidate {
    let mut nodes = vec![end];
    while let Some(&(_, previous)) = nodes.last().and_then(|p| best.get(p)) {
        nodes.push(previous);
        if previous == start {
            break;
        }
    }
    nodes.reverse();
    let mut cells = vec![start];
    for pair in nodes.windows(2) {
        let step = (pair[1] - pair[0]).signum();
        let mut p = pair[0];
        while p != pair[1] {
            p += step;
            cells.push(p);
        }
    }
    Candidate {
        rank: best[&end].0,
        cells,
        moves: nodes.windows(2).map(|pair| (pair[0], pair[1])).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::plan_with;
    use super::super::MoveKind;
    use super::*;

    fn switch(from: &[(i32, i32)], to: &[(i32, i32)]) -> SwitchPlan {
        plan_with(&AlongWalls, from, to)
    }

    fn slides(plan: &SwitchPlan) -> Vec<(IVec2, IVec2)> {
        plan.moves
            .iter()
            .filter(|step| step.kind == MoveKind::Slide)
            .map(|step| (step.from, step.to))
            .collect()
    }

    #[test]
    fn a_gap_travels_around_a_corner() {
        // An L of walls with its gap at the end of one arm moves the gap
        // to the end of the other: every wall on the way shifts one cell.
        let plan = switch(
            &[(0, 0), (1, 0), (2, 0), (0, 1)],
            &[(1, 0), (2, 0), (0, 1), (0, 2)],
        );
        assert_eq!(
            slides(&plan),
            vec![
                (IVec2::new(0, 0), IVec2::new(0, 1)),
                (IVec2::new(0, 1), IVec2::new(0, 2)),
            ]
        );
    }

    #[test]
    fn walls_keep_to_their_own_wall_rather_than_cross_the_floor() {
        // Two walls, rows 0 and 2. Row 0 opens a gap at x = 2 and closes
        // its end at x = 5; row 2 the other way round. Straight across the
        // floor is shorter, but each row's walls shift along that row.
        let row = |y: i32, gap: i32| (0..6).filter(move |&x| x != gap).map(move |x| (x, y));
        let from: Vec<_> = row(0, 5).chain(row(2, 2)).collect();
        let to: Vec<_> = row(0, 2).chain(row(2, 5)).collect();
        let plan = switch(&from, &to);
        assert!(
            slides(&plan).iter().all(|(from, to)| from.y == to.y),
            "{:?}",
            slides(&plan)
        );
        assert_eq!(
            plan.moves
                .iter()
                .filter(|m| m.kind != MoveKind::Slide)
                .count(),
            0
        );
    }

    #[test]
    fn with_no_wall_to_move_along_a_wall_still_slides_across_the_floor() {
        let plan = switch(&[(0, 0)], &[(3, 0)]);
        assert_eq!(slides(&plan), vec![(IVec2::new(0, 0), IVec2::new(3, 0))]);
    }

    #[test]
    fn walls_shift_as_far_as_they_can_along_their_wall_then_cross_the_floor() {
        // A wall along row 0 loses its left end; the cell gained is below
        // its right end. The walls shift right and the last one glides down.
        let plan = switch(
            &[(0, 0), (1, 0), (2, 0), (3, 0)],
            &[(1, 0), (2, 0), (3, 0), (3, 3)],
        );
        assert_eq!(
            slides(&plan),
            vec![
                (IVec2::new(0, 0), IVec2::new(1, 0)),
                (IVec2::new(1, 0), IVec2::new(2, 0)),
                (IVec2::new(2, 0), IVec2::new(3, 0)),
                (IVec2::new(3, 0), IVec2::new(3, 3)),
            ]
        );
    }

    #[test]
    fn the_test_rooms_rings_reshape_along_themselves() {
        use crate::content::map::{parse_map, parse_rooms};
        use crate::model::Room;
        let text = include_str!("../../../assets/maps/rooms.txt");
        let spec = parse_rooms(text, &parse_map(text).2).unwrap().remove(0);
        let room = Room::new(spec.origin, spec.size, spec.layouts.clone());
        for (from, to) in [(0, 1), (1, 0)] {
            let (from, to) = (&spec.layouts[from], &spec.layouts[to]);
            let wall = |p: IVec2| from.contains(&p);
            let switch = SwitchContext::new(&room, from, to, &|_| true, &wall);
            let plan = AlongWalls.plan(&switch);
            plan.check(&switch).unwrap();
            // Every wall that slides shifts one cell along its ring: none
            // crosses the floor between the rings.
            let slides = slides(&plan);
            assert!(!slides.is_empty());
            for (from, to) in slides {
                assert_eq!((to - from).abs().element_sum(), 1, "{from} to {to}");
            }
        }
    }
}
