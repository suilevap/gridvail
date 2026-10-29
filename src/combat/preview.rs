use bevy::prelude::*;

use crate::model::*;

/// What a maneuver would do, worked out without doing it, so it can be
/// shown before the player commits. It follows `perform_maneuvers` and the
/// movement passes: the target is knocked one cell the way of the second
/// direction (and stopped, with an impact, by whatever is there), the actor
/// follows if the move says so, or the two trade places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preview {
    /// The cell the second direction points at from the target: where the
    /// move would send it.
    pub cell: IVec2,
    pub target_to: IVec2,
    pub actor_to: IVec2,
    /// What the target would be knocked into, if anything.
    pub blocked_by: Option<Entity>,
    /// Damage the target would take.
    pub damage: i32,
}

/// What the target looks like to a preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetState {
    pub pos: IVec2,
    pub movable: bool,
    pub exposed: bool,
}

/// Works out `maneuver` by an actor at `actor` on `target`, picked in
/// direction `picked`, with second direction `dir`.
pub fn preview(
    grid: &MapGrid,
    maneuver: &Maneuver,
    actor: IVec2,
    target: TargetState,
    picked: IVec2,
    dir: IVec2,
    (critical, impact): (&CriticalHit, &ImpactDamage),
) -> Preview {
    let cell = grid.safe_pos(target.pos + dir);
    let mut outcome = Preview {
        cell,
        target_to: target.pos,
        actor_to: actor,
        blocked_by: None,
        damage: 0,
    };
    if target.movable && maneuver.swap {
        outcome.target_to = actor;
        outcome.actor_to = target.pos;
    } else if target.movable && maneuver.knock > 0 {
        match grid.get(cell) {
            Some(obstacle) => outcome.blocked_by = Some(obstacle),
            None => {
                outcome.target_to = cell;
                if maneuver.follow {
                    outcome.actor_to = grid.safe_pos(actor + picked);
                }
            }
        }
    }
    if maneuver.hit > 0 {
        outcome.damage += maneuver.hit;
        if target.exposed {
            outcome.damage += critical.bonus;
        }
    }
    if outcome.blocked_by.is_some() {
        outcome.damage += impact.0;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maneuver(knock: i32, follow: bool, swap: bool) -> Maneuver {
        Maneuver {
            name: "move".into(),
            hit: 1,
            knock,
            follow,
            swap,
        }
    }

    /// An actor at (2, 2), its target right of it, and `others` placed.
    fn scene(others: &[IVec2]) -> (MapGrid, Vec<Entity>) {
        let mut world = World::new();
        let mut grid = MapGrid::new(8, 8);
        grid.set(IVec2::new(2, 2), world.spawn_empty().id());
        grid.set(IVec2::new(3, 2), world.spawn_empty().id());
        let others = others
            .iter()
            .map(|&at| {
                let entity = world.spawn_empty().id();
                grid.set(at, entity);
                entity
            })
            .collect();
        (grid, others)
    }

    fn run(grid: &MapGrid, m: &Maneuver, dir: IVec2, exposed: bool) -> Preview {
        let target = TargetState {
            pos: IVec2::new(3, 2),
            movable: true,
            exposed,
        };
        let rules = (&CriticalHit::default(), &ImpactDamage::default());
        preview(grid, m, IVec2::new(2, 2), target, IVec2::X, dir, rules)
    }

    #[test]
    fn a_lunge_knocks_the_target_on_and_follows() {
        let (grid, _) = scene(&[]);
        let outcome = run(&grid, &maneuver(1, true, false), IVec2::X, false);
        assert_eq!(outcome.cell, IVec2::new(4, 2));
        assert_eq!(outcome.target_to, IVec2::new(4, 2));
        assert_eq!(outcome.actor_to, IVec2::new(3, 2));
        assert_eq!(outcome.damage, 1);
    }

    #[test]
    fn a_blocked_knock_stays_and_adds_the_impact() {
        let (grid, others) = scene(&[IVec2::new(3, 3)]);
        let outcome = run(&grid, &maneuver(1, false, false), IVec2::Y, true);
        assert_eq!(outcome.blocked_by, Some(others[0]));
        assert_eq!(outcome.target_to, IVec2::new(3, 2));
        // The hit, the critical bonus and the impact.
        assert_eq!(outcome.damage, 3);
    }

    #[test]
    fn a_swap_trades_places() {
        let (grid, _) = scene(&[]);
        let outcome = run(&grid, &maneuver(0, false, true), IVec2::NEG_X, false);
        assert_eq!(outcome.cell, IVec2::new(2, 2));
        assert_eq!(
            (outcome.target_to, outcome.actor_to),
            (IVec2::new(2, 2), IVec2::new(3, 2))
        );
    }
}
