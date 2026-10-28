use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use super::lines::line_slides;
use super::{Candidate, RoomPlanner, SwitchContext, SwitchPlan};

/// Walls move along the walls they belong to, as a gap travels along a
/// wall: a cell the new layout loses and one it gains, joined by a path of
/// cells that are wall in both layouts (stepping up, down, left or right,
/// around corners), become a shift where every wall on the path moves one
/// cell towards the gained one. A wall never leaves its structure for open
/// floor this way, so in a room inside a room the inner walls stay inner
/// walls and the outer ones outer.
///
/// The shortest paths are taken first, and no cell is part of two. Only
/// what no path joins slides across floor or along a line, as with
/// [`LineSlides`](super::LineSlides); what is left rises or sinks in place.
#[derive(Clone, Copy, Debug, Default)]
pub struct AlongWalls;

/// Cells farther along a wall than this are not worth shifting.
const MAX_PATH: i32 = 64;

impl RoomPlanner for AlongWalls {
    fn plan(&self, switch: &SwitchContext) -> SwitchPlan {
        let mut candidates = wall_paths(switch);
        candidates.extend(line_slides(switch).into_iter().map(|mut slide| {
            slide.rank.0 = 1;
            slide
        }));
        SwitchPlan::from_candidates(switch, candidates)
    }
}

/// From every lost cell, the shortest path through walls to each gained
/// cell it reaches.
fn wall_paths(switch: &SwitchContext) -> Vec<Candidate> {
    let from: HashSet<IVec2> = switch.from.iter().copied().collect();
    let to: HashSet<IVec2> = switch.to.iter().copied().collect();
    let added: HashSet<IVec2> = switch.added_in(&from).collect();
    let both = |p: IVec2| from.contains(&p) && to.contains(&p) && switch.holds_wall(p);

    let mut candidates = Vec::new();
    for start in switch.removed_in(&to).filter(|&p| switch.holds_wall(p)) {
        let mut came_from: HashMap<IVec2, IVec2> = HashMap::new();
        let mut queue = VecDeque::from([(start, 0)]);
        while let Some((p, length)) = queue.pop_front() {
            if length >= MAX_PATH {
                continue;
            }
            for step in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
                let next = p + step;
                if next == start || came_from.contains_key(&next) {
                    continue;
                }
                if added.contains(&next) {
                    came_from.insert(next, p);
                    candidates.push(shift(start, next, &came_from, length + 1));
                } else if both(next) {
                    came_from.insert(next, p);
                    queue.push_back((next, length + 1));
                }
            }
        }
    }
    candidates
}

/// The path from `start` to `end`, each wall on it moving one cell on.
fn shift(start: IVec2, end: IVec2, came_from: &HashMap<IVec2, IVec2>, length: i32) -> Candidate {
    let mut cells = vec![end];
    while let Some(&previous) = cells.last().and_then(|p| came_from.get(p)) {
        cells.push(previous);
        if previous == start {
            break;
        }
    }
    cells.reverse();
    let moves = cells.windows(2).map(|pair| (pair[0], pair[1])).collect();
    Candidate {
        rank: (0, length),
        cells,
        moves,
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
