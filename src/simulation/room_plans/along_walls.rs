use std::collections::HashSet;

use bevy::prelude::*;

use super::{Candidate, RoomPlanner, SwitchContext, SwitchPlan};
use crate::navigation::path::grid::{Cell, Grid};
use crate::navigation::path::{Lex, PathSearch, Rules};

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
/// Paths are searched with the game's pathfinding (`gridvail_path`): a
/// floor run is a jump. Each lost cell looks for its [`NEAREST`] nearest
/// gained cells; the best paths are taken first, and no cell is part of
/// two. What is left rises or sinks in place.
#[derive(Clone, Copy, Debug, Default)]
pub struct AlongWalls;

/// How many gained cells, nearest first, each lost cell finds paths to:
/// enough for the best to be taken when the nearest is gone, few enough
/// that a switch across a whole map stays quick.
const NEAREST: usize = 8;

/// A path's cost: floor cells crossed, then steps.
type Cost = Lex<u32, u32>;

impl RoomPlanner for AlongWalls {
    fn plan(&self, switch: &SwitchContext) -> SwitchPlan {
        let from: HashSet<IVec2> = switch.from.iter().copied().collect();
        let to: HashSet<IVec2> = switch.to.iter().copied().collect();
        let moves = WallMoves {
            switch,
            added: switch.added_in(&from).collect(),
            from: &from,
            to: &to,
        };
        let size = switch.room.size.max(IVec2::ONE).as_uvec2();
        let grid = Grid::new(size.x, size.y);
        let mut search = PathSearch::new();
        let mut path = Vec::new();
        let mut candidates = Vec::new();
        for start in switch.removed_in(&to).filter(|&p| switch.holds_wall(p)) {
            let mut left = NEAREST;
            search.find_each(
                &grid,
                &moves,
                moves.cell(start),
                |cell| moves.added.contains(&moves.pos(cell)),
                &mut path,
                |cost, cells| {
                    candidates.push(moves.candidate(cost, cells));
                    left -= 1;
                    left > 0
                },
            );
        }
        SwitchPlan::from_candidates(switch, candidates)
    }
}

/// How walls may move in a switch, as pathfinding rules over the room's
/// cells (relative to its origin): a step into a wall that stays, or onto
/// a gained cell, and jumps over straight runs of empty floor.
struct WallMoves<'a> {
    switch: &'a SwitchContext<'a>,
    from: &'a HashSet<IVec2>,
    to: &'a HashSet<IVec2>,
    added: HashSet<IVec2>,
}

impl WallMoves<'_> {
    fn cell(&self, p: IVec2) -> Cell {
        let p = p - self.switch.room.origin;
        Cell::new(p.x, p.y)
    }

    fn pos(&self, cell: Cell) -> IVec2 {
        self.switch.room.origin + IVec2::new(cell.x, cell.y)
    }

    fn inside(&self, p: IVec2) -> bool {
        let room = self.switch.room;
        p.cmpge(room.origin).all() && p.cmplt(room.origin + room.size).all()
    }

    /// A wall in both layouts, there to shift along.
    fn wall(&self, p: IVec2) -> bool {
        self.from.contains(&p) && self.to.contains(&p) && self.switch.holds_wall(p)
    }

    /// Empty floor in both layouts, there to glide across.
    fn floor(&self, p: IVec2) -> bool {
        self.inside(p) && !self.from.contains(&p) && !self.to.contains(&p) && self.switch.is_free(p)
    }

    /// Where a path may go next: a wall to shift along, or a gained cell.
    fn landing(&self, p: IVec2) -> bool {
        self.wall(p) || self.added.contains(&p)
    }

    /// A found path as slides: each wall on it moves on to the next node,
    /// and it takes up every cell it passes.
    fn candidate(&self, cost: Cost, nodes: &[Cell]) -> Candidate {
        let nodes: Vec<IVec2> = nodes.iter().map(|&cell| self.pos(cell)).collect();
        let mut cells = vec![nodes[0]];
        for pair in nodes.windows(2) {
            let step = (pair[1] - pair[0]).signum();
            let mut p = pair[0];
            while p != pair[1] {
                p += step;
                cells.push(p);
            }
        }
        Candidate {
            rank: (cost.0 as i32, cost.1 as i32),
            cells,
            moves: nodes.windows(2).map(|pair| (pair[0], pair[1])).collect(),
        }
    }
}

impl Rules for WallMoves<'_> {
    type Node = Cell;
    type Cost = Cost;
    type State = ();

    fn state_count(&self) -> usize {
        1
    }

    fn state_index(&self, _state: &()) -> usize {
        0
    }

    fn start_state(&self, _start: Cell) {}

    /// A step costs one; a jump costs the floor it crosses, and its length.
    fn step(&self, from: Cell, to: Cell, _state: &()) -> Option<(Cost, ())> {
        let (from, to) = (self.pos(from), self.pos(to));
        if !self.landing(to) {
            return None;
        }
        let length = (to - from).abs().element_sum() as u32;
        Some((Lex(length - 1, length), ()))
    }

    fn for_each_jump(&self, from: Cell, _state: &(), visit: &mut dyn FnMut(Cell)) {
        let from = self.pos(from);
        for step in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
            let mut next = from + step;
            if !self.floor(next) {
                continue;
            }
            while self.floor(next) {
                next += step;
            }
            if self.landing(next) {
                visit(self.cell(next));
            }
        }
    }

    fn has_jumps(&self) -> bool {
        true
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
