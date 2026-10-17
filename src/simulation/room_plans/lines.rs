use bevy::prelude::*;

use super::{RoomPlanner, SwitchContext, SwitchPlan, WallMove};

/// Walls slide along rows and columns. A cell the new layout loses and one
/// it gains, in the same row or column, become a slide when every cell
/// between them is
///
/// - empty and floor in both layouts: the wall glides across, or
/// - wall in both layouts: the whole stretch shifts one cell, each wall
///   into the next one's place, as a wall whose gap moves along it.
///
/// The shortest slides are taken first, no cell is part of two, and only
/// cells holding a wall slide. What is left rises or sinks in place
/// (slides of no length).
#[derive(Clone, Copy, Debug, Default)]
pub struct LineSlides;

impl RoomPlanner for LineSlides {
    fn plan(&self, switch: &SwitchContext) -> SwitchPlan {
        let (from, to) = (switch.from, switch.to);
        let removed: Vec<IVec2> = switch.removed().collect();
        let added: Vec<IVec2> = switch.added().collect();
        let both = |p: IVec2| from.contains(&p) && to.contains(&p);
        let floor = |p: IVec2| !from.contains(&p) && !to.contains(&p) && switch.is_free(p);

        // Each candidate: its length and the walls it moves.
        let mut candidates: Vec<(i32, Vec<(IVec2, IVec2)>)> = Vec::new();
        for &start in &removed {
            for &end in &added {
                let delta = end - start;
                if delta.x != 0 && delta.y != 0 {
                    continue;
                }
                let step = delta.signum();
                let length = delta.x.abs() + delta.y.abs();
                let between: Vec<IVec2> = (1..length).map(|i| start + step * i).collect();
                let moves = if between.iter().all(|&p| floor(p)) {
                    vec![(start, end)]
                } else if between.iter().all(|&p| both(p)) {
                    std::iter::once(start)
                        .chain(between)
                        .map(|p| (p, p + step))
                        .collect()
                } else {
                    continue;
                };
                if moves.iter().all(|&(p, _)| switch.holds_wall(p)) {
                    candidates.push((length, moves));
                }
            }
        }
        candidates.sort_by_key(|(length, _)| *length);

        let mut plan = SwitchPlan::default();
        let mut used: Vec<IVec2> = Vec::new();
        for (_, moves) in candidates {
            let start = moves[0].0;
            let end = moves[moves.len() - 1].1;
            let cells = || {
                let delta = end - start;
                let step = delta.signum();
                (0..=delta.x.abs() + delta.y.abs()).map(move |i| start + step * i)
            };
            if cells().any(|p| used.contains(&p)) {
                continue;
            }
            used.extend(cells());
            plan.moves.extend(
                moves
                    .into_iter()
                    .map(|(from, to)| WallMove::slide(from, to)),
            );
        }
        plan.moves.extend(
            removed
                .into_iter()
                .filter(|p| !used.contains(p))
                .map(WallMove::sink_at),
        );
        plan.moves.extend(
            added
                .into_iter()
                .filter(|p| !used.contains(p))
                .map(WallMove::rise_at),
        );
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::plan_with;
    use super::super::MoveKind;
    use super::*;
    use crate::model::Room;

    fn switch(from: &[(i32, i32)], to: &[(i32, i32)]) -> SwitchPlan {
        plan_with(&LineSlides, from, to)
    }

    fn slides(plan: &SwitchPlan) -> Vec<(IVec2, IVec2)> {
        plan.moves
            .iter()
            .filter(|step| step.kind == MoveKind::Slide)
            .map(|step| (step.from, step.to))
            .collect()
    }

    #[test]
    fn a_wall_slides_across_empty_floor() {
        let plan = switch(&[(0, 0)], &[(3, 0)]);
        assert_eq!(plan.moves.len(), 1);
        assert_eq!(slides(&plan), vec![(IVec2::new(0, 0), IVec2::new(3, 0))]);
    }

    #[test]
    fn a_gap_moving_along_a_wall_shifts_the_walls_between() {
        // A wall from x = 0 to 4 with its gap at 1 moves the gap to 3.
        let plan = switch(
            &[(0, 0), (2, 0), (3, 0), (4, 0)],
            &[(0, 0), (1, 0), (2, 0), (4, 0)],
        );
        assert_eq!(plan.moves.len(), 2);
        assert_eq!(
            slides(&plan),
            vec![
                (IVec2::new(3, 0), IVec2::new(2, 0)),
                (IVec2::new(2, 0), IVec2::new(1, 0)),
            ]
        );
    }

    #[test]
    fn walls_off_line_or_blocked_rise_and_sink() {
        assert!(slides(&switch(&[(0, 0)], &[(1, 1)])).is_empty());
        // A mix of wall and floor between them.
        assert!(slides(&switch(&[(0, 0), (1, 0)], &[(1, 0), (3, 0)])).is_empty());

        // Something stands between them.
        let (from, to) = (vec![IVec2::new(0, 0)], vec![IVec2::new(2, 0)]);
        let room = Room::new(IVec2::ZERO, IVec2::splat(3), vec![]);
        let free = |p: IVec2| p != IVec2::new(1, 0);
        let wall = |p: IVec2| from.contains(&p);
        let plan = LineSlides.plan(&SwitchContext::new(&room, &from, &to, &free, &wall));
        assert!(slides(&plan).is_empty());
    }

    #[test]
    fn the_shortest_slides_win_and_no_cell_slides_twice() {
        // Two walls can each slide to either of two cells: the closer
        // pairs are taken.
        let plan = switch(&[(0, 0), (0, 4)], &[(0, 1), (0, 3)]);
        assert_eq!(
            slides(&plan),
            vec![
                (IVec2::new(0, 0), IVec2::new(0, 1)),
                (IVec2::new(0, 4), IVec2::new(0, 3)),
            ]
        );
        // One cell gained, two lost: one slides, the other sinks.
        let plan = switch(&[(0, 0), (2, 0)], &[(1, 0)]);
        assert_eq!(slides(&plan).len(), 1);
        assert_eq!(plan.moves.len(), 2);
    }
}
