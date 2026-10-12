use bevy::prelude::*;

use super::wall_bundle;
use crate::model::*;

/// Switches a room to the layout asked for, all in one pass. A room waits
/// while anything stands where a new wall goes. The walls around every
/// changed cell are autotiled again (`tile_system`).
///
/// Where it can, a wall slides to its new cell ([`plan_switch`]): it keeps
/// its entity, is marked [`Sliding`] and only its `Pos` changes. Other new
/// walls are [`Rising`], and other walls that go stop being walls at once
/// and are left [`Sinking`] for the animation step to show going down and
/// despawn; without one (no `AnimatedPos`) they are despawned here.
pub fn transform_rooms(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    mut rooms: Query<&mut Room>,
    walls: Query<Has<AnimatedPos>, With<Wall>>,
) {
    for mut room in &mut rooms {
        if room.current == room.target {
            continue;
        }
        let Room {
            layouts,
            current,
            target,
            ..
        } = &*room;
        let (from, to) = (&layouts[*current], &layouts[*target]);
        if to
            .iter()
            .any(|p| !from.contains(p) && grid.get(*p).is_some())
        {
            continue;
        }
        let wall_at = |grid: &MapGrid, p: IVec2| grid.get(p).filter(|&e| walls.contains(e));
        let plan = plan_switch(
            from,
            to,
            |p| grid.get(p).is_none(),
            |p| wall_at(&grid, p).is_some(),
        );

        let movers: Vec<_> = plan
            .slides
            .iter()
            .filter_map(|&(start, end)| Some((wall_at(&grid, start)?, end)))
            .collect();
        for &(start, _) in &plan.slides {
            grid.clear(start);
        }
        for &(wall, end) in &movers {
            grid.set(end, wall);
            commands
                .entity(wall)
                .insert((Pos(end), Sliding, Tile::wall()));
        }
        for &p in &plan.sunk {
            let Some(wall) = grid.get(p) else { continue };
            let Ok(animated) = walls.get(wall) else {
                continue;
            };
            grid.clear(p);
            if animated {
                commands
                    .entity(wall)
                    .remove::<(Wall, Collider, Active, Tile)>()
                    .insert(Sinking);
            } else {
                commands.entity(wall).despawn();
            }
        }
        for &p in &plan.risen {
            let wall = commands.spawn((wall_bundle(p), Rising)).id();
            grid.set(p, wall);
        }

        let changed = plan
            .slides
            .iter()
            .flat_map(|&(start, end)| [start, end])
            .chain(plan.sunk.iter().copied())
            .chain(plan.risen.iter().copied());
        for p in changed {
            for side in [IVec2::X, IVec2::Y, IVec2::NEG_X, IVec2::NEG_Y] {
                if let Some(wall) = wall_at(&grid, grid.safe_pos(p + side)) {
                    commands.entity(wall).insert(Tile::wall());
                }
            }
        }
        room.current = room.target;
    }
}

/// How a room's walls get from one layout to the next.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SwitchPlan {
    /// Walls that slide, from one cell to another.
    pub slides: Vec<(IVec2, IVec2)>,
    /// New walls that rise out of the floor.
    pub risen: Vec<IVec2>,
    /// Walls that sink into the floor and go.
    pub sunk: Vec<IVec2>,
}

/// Plans a switch from the walls of `from` to those of `to`. A cell the
/// new layout loses and one it gains, in the same row or column, become a
/// slide when every cell between them is
///
/// - empty (`free`) and floor in both layouts: the wall glides across, or
/// - wall in both layouts: the whole stretch shifts one cell, each wall
///   into the next one's place, as a wall whose gap moves along it.
///
/// The shortest slides are taken first, no cell is part of two, and only
/// cells holding a wall (`movable`) slide. What is left rises or sinks.
pub fn plan_switch(
    from: &[IVec2],
    to: &[IVec2],
    free: impl Fn(IVec2) -> bool,
    movable: impl Fn(IVec2) -> bool,
) -> SwitchPlan {
    let removed: Vec<IVec2> = from.iter().copied().filter(|p| !to.contains(p)).collect();
    let added: Vec<IVec2> = to.iter().copied().filter(|p| !from.contains(p)).collect();
    let both = |p: IVec2| from.contains(&p) && to.contains(&p);
    let floor = |p: IVec2| !from.contains(&p) && !to.contains(&p) && free(p);

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
            if moves.iter().all(|&(p, _)| movable(p)) {
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
        plan.slides.extend(moves);
    }
    plan.sunk = removed.into_iter().filter(|p| !used.contains(p)).collect();
    plan.risen = added.into_iter().filter(|p| !used.contains(p)).collect();
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(list: &[(i32, i32)]) -> Vec<IVec2> {
        list.iter().map(|&(x, y)| IVec2::new(x, y)).collect()
    }

    fn switch(from: &[(i32, i32)], to: &[(i32, i32)]) -> SwitchPlan {
        plan_switch(&cells(from), &cells(to), |_| true, |_| true)
    }

    #[test]
    fn a_wall_slides_across_empty_floor() {
        let plan = switch(&[(0, 0)], &[(3, 0)]);
        assert_eq!(plan.slides, vec![(IVec2::new(0, 0), IVec2::new(3, 0))]);
        assert!(plan.risen.is_empty() && plan.sunk.is_empty());
    }

    #[test]
    fn a_gap_moving_along_a_wall_shifts_the_walls_between() {
        // A wall from x = 0 to 4 with its gap at 1 moves the gap to 3.
        let plan = switch(
            &[(0, 0), (2, 0), (3, 0), (4, 0)],
            &[(0, 0), (1, 0), (2, 0), (4, 0)],
        );
        assert_eq!(
            plan.slides,
            vec![
                (IVec2::new(3, 0), IVec2::new(2, 0)),
                (IVec2::new(2, 0), IVec2::new(1, 0)),
            ]
        );
        assert!(plan.risen.is_empty() && plan.sunk.is_empty());
    }

    #[test]
    fn walls_off_line_or_blocked_rise_and_sink() {
        let plan = switch(&[(0, 0)], &[(1, 1)]);
        assert!(plan.slides.is_empty());
        assert_eq!(
            (plan.sunk, plan.risen),
            (cells(&[(0, 0)]), cells(&[(1, 1)]))
        );

        // Something stands between them.
        let plan = plan_switch(
            &cells(&[(0, 0)]),
            &cells(&[(2, 0)]),
            |p| p != IVec2::new(1, 0),
            |_| true,
        );
        assert!(plan.slides.is_empty());

        // A mix of wall and floor between them.
        let plan = switch(&[(0, 0), (1, 0)], &[(1, 0), (3, 0)]);
        assert!(plan.slides.is_empty());
    }

    #[test]
    fn the_shortest_slides_win_and_no_cell_slides_twice() {
        // Two walls can each slide to either of two cells: the closer
        // pairs are taken.
        let plan = switch(&[(0, 0), (0, 4)], &[(0, 1), (0, 3)]);
        assert_eq!(
            plan.slides,
            vec![
                (IVec2::new(0, 0), IVec2::new(0, 1)),
                (IVec2::new(0, 4), IVec2::new(0, 3)),
            ]
        );
        // One cell gained, two lost: one slides, the other sinks.
        let plan = switch(&[(0, 0), (2, 0)], &[(1, 0)]);
        assert_eq!(plan.slides.len(), 1);
        assert_eq!(plan.sunk.len(), 1);
    }
}
