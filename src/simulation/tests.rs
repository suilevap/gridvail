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

fn spawn_mover(app: &mut App, at: IVec2, inventory: Vec<Entity>) -> Entity {
    let entity = app
        .world_mut()
        .spawn((
            Active,
            Collider,
            Pos(at),
            PrevPos(at),
            Speed::default(),
            PendingPos::default(),
            Inventory(inventory),
        ))
        .id();
    app.world_mut().resource_mut::<MapGrid>().set(at, entity);
    entity
}

fn spawn_key(app: &mut App, at: Option<IVec2>, color: KeyColor) -> Entity {
    let mut key = app.world_mut().spawn((Active, Item, Key(color)));
    if let Some(at) = at {
        key.insert(Pos(at));
    }
    key.id()
}

fn spawn_door(app: &mut App, at: IVec2, color: KeyColor) -> Entity {
    let door = app
        .world_mut()
        .spawn((
            Active,
            Collider,
            Door::closed(color),
            Pos(at),
            Glyph::new(Door::CLOSED_GLYPH, 1, 0),
        ))
        .id();
    app.world_mut()
        .resource_mut::<MapGrid>()
        .set_with_blocking(at, door, true);
    door
}

fn walk(app: &mut App, mover: Entity, step: IVec2) {
    app.world_mut().get_mut::<Speed>(mover).unwrap().0 = step;
    app.update();
    app.world_mut().get_mut::<Speed>(mover).unwrap().0 = IVec2::ZERO;
}

#[test]
fn stepping_onto_a_key_picks_it_up() {
    let mut app = test_app::headless();
    let mover = spawn_mover(&mut app, IVec2::new(1, 1), Vec::new());
    let key = spawn_key(&mut app, Some(IVec2::new(2, 1)), KeyColor::Red);
    walk(&mut app, mover, IVec2::X);
    assert_eq!(app.world().get::<Pos>(mover).unwrap().0, IVec2::new(2, 1));
    assert_eq!(app.world().get::<Inventory>(mover).unwrap().0, [key]);
    assert!(
        app.world().get::<Pos>(key).is_none(),
        "carried keys leave the map"
    );
}

#[test]
fn a_door_opens_only_for_a_key_of_its_color() {
    let mut app = test_app::headless();
    let green = spawn_key(&mut app, None, KeyColor::Green);
    let mover = spawn_mover(&mut app, IVec2::new(1, 1), vec![green]);
    let door = spawn_door(&mut app, IVec2::new(2, 1), KeyColor::Red);

    walk(&mut app, mover, IVec2::X);
    assert_eq!(app.world().get::<Pos>(mover).unwrap().0, IVec2::new(1, 1));
    assert!(!app.world().get::<Door>(door).unwrap().open);
    assert!(app
        .world()
        .resource::<MapGrid>()
        .blocks_vision(IVec2::new(2, 1)));

    let red = spawn_key(&mut app, None, KeyColor::Red);
    app.world_mut()
        .get_mut::<Inventory>(mover)
        .unwrap()
        .0
        .push(red);
    // The bump opens the door; the next step walks through it.
    walk(&mut app, mover, IVec2::X);
    assert_eq!(app.world().get::<Pos>(mover).unwrap().0, IVec2::new(1, 1));
    assert!(app.world().get::<Door>(door).unwrap().open);
    assert!(app.world().get::<Collider>(door).is_none());
    assert_eq!(app.world().get::<Glyph>(door).unwrap().ch, Door::OPEN_GLYPH);
    assert!(!app
        .world()
        .resource::<MapGrid>()
        .blocks_vision(IVec2::new(2, 1)));

    walk(&mut app, mover, IVec2::X);
    assert_eq!(app.world().get::<Pos>(mover).unwrap().0, IVec2::new(2, 1));
    assert_eq!(
        app.world().get::<Inventory>(mover).unwrap().0.len(),
        2,
        "keys are kept"
    );
}

#[test]
fn a_destroyed_actor_drops_everything_it_carries() {
    let mut app = test_app::headless();
    let red = spawn_key(&mut app, None, KeyColor::Red);
    let blue = spawn_key(&mut app, None, KeyColor::Blue);
    let doomed = spawn_mover(&mut app, IVec2::new(4, 4), vec![red, blue]);
    app.world_mut().entity_mut(doomed).insert(DestroyRequested);
    app.update();
    assert!(app.world().get_entity(doomed).is_err());
    for key in [red, blue] {
        assert_eq!(app.world().get::<Pos>(key).unwrap().0, IVec2::new(4, 4));
    }

    // Another actor walking onto the cell collects both.
    let finder = spawn_mover(&mut app, IVec2::new(3, 4), Vec::new());
    walk(&mut app, finder, IVec2::X);
    let mut found = app.world().get::<Inventory>(finder).unwrap().0.clone();
    found.sort();
    let mut expected = vec![red, blue];
    expected.sort();
    assert_eq!(found, expected);
}
