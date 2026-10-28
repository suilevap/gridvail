use bevy::prelude::*;

use super::*;

/// A grid with the engager at (2, 2) and its target right of it, plus
/// whatever `others` stand where.
fn setup(others: &[IVec2]) -> (MapGrid, Entity, Entity, Vec<Entity>) {
    let mut world = World::new();
    let mut grid = MapGrid::new(6, 6);
    let engager = world.spawn_empty().id();
    let target = world.spawn_empty().id();
    grid.set(IVec2::new(2, 2), engager);
    grid.set(IVec2::new(3, 2), target);
    let others = others
        .iter()
        .map(|&at| {
            let other = world.spawn_empty().id();
            grid.set(at, other);
            other
        })
        .collect();
    (grid, engager, target, others)
}

fn plan(
    grid: &MapGrid,
    engager: Entity,
    second: IVec2,
    movable: bool,
    exposed: bool,
) -> ManeuverOutcome {
    let rules = CombatRules::default();
    plan_maneuver(
        grid,
        &rules,
        &rules.maneuvers,
        engager,
        IVec2::new(2, 2),
        IVec2::X,
        second,
        movable,
        exposed,
    )
}

#[test]
fn the_target_always_goes_the_way_of_the_second_direction() {
    let (grid, engager, ..) = setup(&[]);
    let lunge = plan(&grid, engager, IVec2::X, true, false);
    assert_eq!(lunge.maneuver.name, "lunge");
    assert_eq!(lunge.target_to, Some(IVec2::new(4, 2)));
    assert_eq!(lunge.engager_to, Some(IVec2::new(3, 2)));

    let push = plan(&grid, engager, IVec2::Y, true, false);
    assert_eq!(push.maneuver.name, "push");
    assert_eq!(push.target_to, Some(IVec2::new(3, 3)));
    assert_eq!(push.engager_to, None);

    let yank = plan(&grid, engager, IVec2::NEG_X, true, false);
    assert_eq!(yank.maneuver.name, "yank");
    assert_eq!(
        yank.target_to,
        Some(IVec2::new(2, 2)),
        "into the engager's cell"
    );
    assert_eq!(yank.engager_to, Some(IVec2::new(3, 2)));
    assert_eq!(yank.damage, 1);
}

#[test]
fn a_blocked_way_slams_the_target_and_keeps_everyone_in_place() {
    let (grid, engager, _, others) = setup(&[IVec2::new(4, 2)]);
    let lunge = plan(&grid, engager, IVec2::X, true, false);
    assert_eq!(lunge.target_to, None);
    assert_eq!(lunge.engager_to, None);
    assert_eq!(lunge.damage, 2);
    assert_eq!(lunge.slammed, Some(Some(others[0])));
}

#[test]
fn an_exposed_target_takes_a_critical_hit() {
    let (grid, engager, ..) = setup(&[]);
    assert_eq!(plan(&grid, engager, IVec2::Y, true, true).damage, 2);
}

#[test]
fn an_immovable_target_only_takes_the_hit() {
    let (grid, engager, ..) = setup(&[]);
    let outcome = plan(&grid, engager, IVec2::X, false, false);
    assert_eq!((outcome.target_to, outcome.engager_to), (None, None));
    assert_eq!(outcome.damage, 1);
    assert_eq!(outcome.slammed, None);
}

#[test]
fn a_target_with_its_own_maneuvers_answers_to_them() {
    let (grid, engager, ..) = setup(&[]);
    let rules = CombatRules::default();
    // A crate, say: shoved without harm, never followed.
    let shove = Maneuver {
        name: "shove",
        follow: false,
        damage: 0,
    };
    let own = Maneuvers {
        forward: shove,
        side: shove,
        back: shove,
    };
    let outcome = plan_maneuver(
        &grid,
        &rules,
        &own,
        engager,
        IVec2::new(2, 2),
        IVec2::X,
        IVec2::X,
        true,
        false,
    );
    assert_eq!(outcome.maneuver.name, "shove");
    assert_eq!(outcome.engager_to, None);
    assert_eq!(outcome.damage, 0);
}
