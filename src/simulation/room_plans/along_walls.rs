use std::collections::HashSet;

use bevy::prelude::*;

use super::{Candidate, RoomPlanner, SwitchContext, SwitchPlan, WallMove};
use crate::model::Corners;
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
/// - across empty floor (floor in both layouts, nothing standing on it)
///   from wall to wall: the wall at the start glides to the end. Over bare
///   floor it glides straight; on rails it follows them, turning at a rail
///   onto any rail beside it (round a corner, at a T-junction or a
///   crossing).
///
/// Floor with a rail (`Rail`), reached along rails, counts apart:
/// of paths crossing the same bare floor, the one crossing the least floor
/// on rails wins, so where no path keeps off the floor, walls glide along
/// rails rather than off them. A path through walls always wins; with none,
/// walls shift as far as they can along their wall, then glide across the
/// floor, along rails where they can. In a room inside a room the inner
/// walls stay inner walls and the outer ones outer.
/// [`LineSlides`](super::LineSlides)' slides are the straight paths.
///
/// Paths are searched with the game's pathfinding (`gridvail_path`), the
/// way a wall glides being the search's state. Each lost cell looks for its
/// [`NEAREST`] nearest gained cells; the best paths are taken first, and no
/// cell is part of two. What is left rises or sinks in place.
#[derive(Clone, Copy, Debug, Default)]
pub struct AlongWalls;

/// How many gained cells, nearest first, each lost cell finds paths to:
/// enough for the best to be taken when the nearest is gone, few enough
/// that a switch across a whole map stays quick.
const NEAREST: usize = 8;

/// A path's cost: bare floor cells crossed, then floor cells on rails, then
/// steps.
type Cost = Lex<u32, Lex<u32, u32>>;

/// The way a wall glides across the floor: an index into [`STEPS`], or
/// [`STILL`] on a wall (or before a step).
type Heading = u8;
const STILL: Heading = 4;
const STEPS: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

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
                    if let Some(candidate) = moves.candidate(cost, cells) {
                        candidates.push(candidate);
                        left -= 1;
                    }
                    left > 0
                },
            );
        }
        SwitchPlan::from_candidates(switch, candidates)
    }
}

/// How walls may move in a switch, as pathfinding rules over the room's
/// cells (relative to its origin): steps into walls that stay or onto
/// gained cells, and glides over empty floor, straight or along rails.
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

    /// A wall in both layouts, there to shift along.
    fn wall(&self, p: IVec2) -> bool {
        self.from.contains(&p) && self.to.contains(&p) && self.switch.holds_wall(p)
    }

    /// Empty floor in both layouts, there to glide across.
    fn floor(&self, p: IVec2) -> bool {
        !self.from.contains(&p) && !self.to.contains(&p) && self.switch.is_free(p)
    }

    /// Where a wall stops: a wall to shift along, or a gained cell.
    fn landing(&self, p: IVec2) -> bool {
        self.wall(p) || self.added.contains(&p)
    }

    /// Whether a wall gliding `heading` over `p` may go on `step`: straight
    /// on, or turning from a rail at `p` onto a rail beside it.
    fn may_go(&self, p: IVec2, heading: Heading, step: IVec2) -> bool {
        let Some(&was) = STEPS.get(heading as usize) else {
            return true;
        };
        step == was
            || (step != -was
                && self.floor(p)
                && self.switch.has_rail(p)
                && self.switch.has_rail(p + step))
    }

    /// Whether a step from `from` onto the floor at `to` keeps to rails:
    /// onto a rail, from a rail or from a wall (which may stand on one).
    fn on_rails(&self, from: IVec2, to: IVec2) -> bool {
        self.switch.has_rail(to) && (self.switch.has_rail(from) || !self.floor(from))
    }

    /// A found path as moves: the walls on it (where it starts, and the
    /// walls it shifts through) each move on to the next, gliding over the
    /// floor between and turning where it turns. `None` if one would turn
    /// more often than a move can.
    fn candidate(&self, cost: Cost, cells: &[Cell]) -> Option<Candidate> {
        let cells: Vec<IVec2> = cells.iter().map(|&cell| self.pos(cell)).collect();
        let mut moves = Vec::new();
        let (mut from, mut corners) = (cells[0], Vec::new());
        for (i, &p) in cells.iter().enumerate().skip(1) {
            if self.landing(p) {
                moves.push(WallMove::slide(from, p).via(Corners::new(&corners)?));
                (from, corners) = (p, Vec::new());
            } else if cells[i + 1] - p != p - cells[i - 1] {
                corners.push(p);
            }
        }
        Some(Candidate {
            rank: (cost.0 as i32, cost.1 .0 as i32, cost.1 .1 as i32),
            cells,
            moves,
        })
    }
}

impl Rules for WallMoves<'_> {
    type Node = Cell;
    type Cost = Cost;
    type State = Heading;

    fn state_count(&self) -> usize {
        STEPS.len() + 1
    }

    fn state_index(&self, heading: &Heading) -> usize {
        *heading as usize
    }

    fn start_state(&self, _start: Cell) -> Heading {
        STILL
    }

    /// A step onto a wall or a gained cell costs one, and stops the glide;
    /// one over floor costs one, and a floor cell (on rails, or bare).
    fn step(&self, from: Cell, to: Cell, heading: &Heading) -> Option<(Cost, Heading)> {
        let (from, to) = (self.pos(from), self.pos(to));
        let step = to - from;
        if !self.may_go(from, *heading, step) {
            return None;
        }
        if self.landing(to) {
            return Some((Lex(0, Lex(0, 1)), STILL));
        }
        if !self.floor(to) {
            return None;
        }
        let heading = STEPS.iter().position(|&s| s == step)? as Heading;
        let cost = if self.on_rails(from, to) {
            Lex(0, Lex(1, 1))
        } else {
            Lex(1, Lex(0, 1))
        };
        Some((cost, heading))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{cells, plan_with};
    use super::super::MoveKind;
    use super::*;
    use crate::model::Room;

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
    fn walls_glide_along_rails_rather_than_across_bare_floor() {
        // One wall goes and two come: it can glide 3 cells right, or 4 down.
        // The other new wall rises.
        let from = cells(&[(1, 1)]);
        let to = cells(&[(4, 1), (1, 5)]);
        let room = Room::new(IVec2::ZERO, IVec2::splat(8), vec![]);
        let wall = |p: IVec2| from.contains(&p);
        let plain = SwitchContext::new(&room, &from, &to, &|_| true, &wall);
        let plan = AlongWalls.plan(&plain);
        plan.check(&plain).unwrap();
        assert_eq!(slides(&plan), vec![(IVec2::new(1, 1), IVec2::new(4, 1))]);

        // A rail down the column: the longer glide along it wins.
        let rail = |p: IVec2| p.x == 1 && (2..5).contains(&p.y);
        let railed = SwitchContext::new(&room, &from, &to, &|_| true, &wall).with_rails(&rail);
        let plan = AlongWalls.plan(&railed);
        plan.check(&railed).unwrap();
        assert_eq!(slides(&plan), vec![(IVec2::new(1, 1), IVec2::new(1, 5))]);
    }

    #[test]
    fn walls_follow_rails_round_their_corners() {
        // A wall goes at (1, 1) and one comes at (5, 4): no straight way
        // between them, but a rail runs right along row 1, turns at (5, 1)
        // and runs down column 5.
        let from = cells(&[(1, 1)]);
        let to = cells(&[(5, 4)]);
        let room = Room::new(IVec2::ZERO, IVec2::splat(8), vec![]);
        let wall = |p: IVec2| from.contains(&p);
        let plain = SwitchContext::new(&room, &from, &to, &|_| true, &wall);
        assert!(slides(&AlongWalls.plan(&plain)).is_empty(), "rise and sink");

        let rail = |p: IVec2| matches!((p.x, p.y), (2..=5, 1) | (5, 2..=3));
        let railed = SwitchContext::new(&room, &from, &to, &|_| true, &wall).with_rails(&rail);
        let plan = AlongWalls.plan(&railed);
        plan.check(&railed).unwrap();
        assert_eq!(plan.moves.len(), 1);
        let step = plan.moves[0];
        assert_eq!((step.from, step.to), (IVec2::new(1, 1), IVec2::new(5, 4)));
        assert_eq!(step.via.as_slice(), [IVec2::new(5, 1)]);
    }

    #[test]
    fn a_wall_does_not_turn_on_bare_floor_and_takes_any_branch_of_rails() {
        let from = cells(&[(1, 1)]);
        let to = cells(&[(5, 4)]);
        let room = Room::new(IVec2::ZERO, IVec2::splat(8), vec![]);
        let wall = |p: IVec2| from.contains(&p);
        // A rail along row 1 that stops short of column 5's: no turn on the
        // bare floor between.
        let gap = |p: IVec2| matches!((p.x, p.y), (2..=4, 1) | (5, 2..=3));
        let gapped = SwitchContext::new(&room, &from, &to, &|_| true, &wall).with_rails(&gap);
        assert!(slides(&AlongWalls.plan(&gapped)).is_empty());

        // A T: row 1 runs on past column 3, which branches down to row 4
        // and along to (5, 4). The wall takes the branch.
        let tee = |p: IVec2| matches!((p.x, p.y), (2..=6, 1) | (3, 2..=4) | (4, 4));
        let railed = SwitchContext::new(&room, &from, &to, &|_| true, &wall).with_rails(&tee);
        let plan = AlongWalls.plan(&railed);
        plan.check(&railed).unwrap();
        assert_eq!(
            plan.moves[0].via.as_slice(),
            [IVec2::new(3, 1), IVec2::new(3, 4)]
        );
    }

    #[test]
    fn the_test_rooms_rings_reshape_along_themselves() {
        use crate::content::map::{parse_map, parse_rooms};
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
