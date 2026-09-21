use bevy::prelude::*;

use super::*;
use crate::model::*;

#[test]
fn direct_intents_wrap_and_support_first_placement() {
    let mut app = test_app::headless();
    let mover = app
        .world_mut()
        .spawn((
            Active,
            Collider,
            Pos(IVec2::new(1, 1)),
            PendingPos::MoveTo(IVec2::new(-1, 9)),
        ))
        .id();
    app.world_mut()
        .resource_mut::<MapGrid>()
        .set(IVec2::new(1, 1), mover);
    let newcomer = app
        .world_mut()
        .spawn((Active, Collider, PendingPos::MoveTo(IVec2::new(8, 0))))
        .id();
    app.update();
    let grid = app.world().resource::<MapGrid>();
    assert_eq!(app.world().get::<Pos>(mover).unwrap().0, IVec2::new(7, 1));
    assert_eq!(grid.get(IVec2::new(7, 1)), Some(mover));
    assert_eq!(grid.get(IVec2::new(1, 1)), None);
    assert_eq!(app.world().get::<Pos>(newcomer).unwrap().0, IVec2::ZERO);
    assert_eq!(grid.get(IVec2::ZERO), Some(newcomer));
}

#[test]
fn token_recharge_assigns_instead_of_adding() {
    let mut app = test_app::headless();
    let entity = app
        .world_mut()
        .spawn((
            Player(0),
            Tokens {
                count: 0,
                recharge: 1,
            },
        ))
        .id();
    app.update();
    assert_eq!(app.world().get::<Tokens>(entity).unwrap().count, 1);
}

#[test]
fn swaps_are_blocked_conservatively() {
    let mut app = test_app::headless();
    let a = app
        .world_mut()
        .spawn((
            Active,
            Collider,
            Pos(IVec2::new(1, 1)),
            PendingPos::MoveTo(IVec2::new(2, 1)),
        ))
        .id();
    let b = app
        .world_mut()
        .spawn((
            Active,
            Collider,
            Pos(IVec2::new(2, 1)),
            PendingPos::MoveTo(IVec2::new(1, 1)),
        ))
        .id();
    {
        let mut grid = app.world_mut().resource_mut::<MapGrid>();
        grid.set(IVec2::new(1, 1), a);
        grid.set(IVec2::new(2, 1), b);
    }
    app.update();
    assert_eq!(app.world().get::<Pos>(a).unwrap().0, IVec2::new(1, 1));
    assert_eq!(app.world().get::<Pos>(b).unwrap().0, IVec2::new(2, 1));
    assert_eq!(app.world().resource::<CollisionBuffer>().0.len(), 2);
}

#[test]
fn one_claimant_wins_a_free_cell() {
    let mut app = test_app::headless();
    let first = spawn_claimant(&mut app, IVec2::new(0, 0));
    let second = spawn_claimant(&mut app, IVec2::new(2, 0));
    app.update();
    let grid = app.world().resource::<MapGrid>();
    let winner = grid.get(IVec2::new(1, 0)).expect("winner");
    assert!(winner == first || winner == second);
    assert_eq!(app.world().resource::<CollisionBuffer>().0.len(), 1);
}

fn spawn_claimant(app: &mut App, at: IVec2) -> Entity {
    let entity = app
        .world_mut()
        .spawn((Active, Collider, Pos(at), PendingPos::MoveTo(IVec2::X)))
        .id();
    app.world_mut().resource_mut::<MapGrid>().set(at, entity);
    entity
}

#[test]
fn destruction_clears_the_map() {
    let mut app = test_app::headless();
    let entity = app
        .world_mut()
        .spawn((Active, Collider, Pos(IVec2::new(3, 3)), DestroyRequested))
        .id();
    app.world_mut()
        .resource_mut::<MapGrid>()
        .set(IVec2::new(3, 3), entity);
    app.update();
    assert_eq!(
        app.world().resource::<MapGrid>().get(IVec2::new(3, 3)),
        None
    );
    assert!(app.world().get_entity(entity).is_err());
}
