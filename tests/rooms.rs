//! Rooms that switch layouts when a pressure plate is stepped on.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
use pav_ecs_game_bevy_port::model::*;

/// The player starts left of the plate. The room's first layout has walls
/// at (4, 1) and (4, 2); its second moves the top one to (5, 1).
const MAP: &str = "\
XXXXXXX
Xp_.X.X
X...X.X
XXXXXXX
@room 4 1
.X
X.
";
const PLATE: IVec2 = IVec2::new(2, 1);
const LEFT: IVec2 = IVec2::new(4, 1);
const RIGHT: IVec2 = IVec2::new(5, 1);
const BELOW: IVec2 = IVec2::new(4, 2);

fn boot() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .insert_resource(MapText(MAP))
        .add_plugins(GamePlugin);
    app.update();
    app.update();
    app
}

fn player(app: &mut App) -> IVec2 {
    let world = app.world_mut();
    world
        .query_filtered::<&Pos, With<Player>>()
        .single(world)
        .unwrap()
        .0
}

/// Holds `key` until the player has taken one step.
fn step(app: &mut App, key: KeyCode) {
    let from = player(app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    for _ in 0..200 {
        app.update();
        if player(app) != from {
            break;
        }
    }
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.release(key);
    input.clear();
    assert_ne!(player(app), from, "the player should have stepped");
    // Let the turn finish.
    for _ in 0..5 {
        app.update();
    }
}

fn wall_at(app: &mut App, p: IVec2) -> Option<char> {
    let world = app.world_mut();
    let entity = world.resource::<MapGrid>().get(p)?;
    world
        .get::<Wall>(entity)
        .and(world.get::<Glyph>(entity))
        .map(|glyph| glyph.ch)
}

#[test]
fn stepping_on_the_plate_switches_the_room_and_back() {
    let mut app = boot();
    assert!(wall_at(&mut app, LEFT).is_some());
    assert!(wall_at(&mut app, RIGHT).is_none());
    let below = wall_at(&mut app, BELOW).unwrap();
    let revision = app.world().resource::<MapGrid>().blocker_revision;

    step(&mut app, KeyCode::ArrowRight);
    assert_eq!(player(&mut app), PLATE);
    assert!(wall_at(&mut app, LEFT).is_none(), "the old wall goes");
    let right = wall_at(&mut app, RIGHT).expect("the new wall comes");
    assert_ne!(right, '#', "the new wall is autotiled");
    assert_ne!(
        wall_at(&mut app, BELOW).unwrap(),
        below,
        "its neighbour joins up again without the old wall"
    );
    assert_ne!(
        app.world().resource::<MapGrid>().blocker_revision,
        revision,
        "sight, light and paths see the change"
    );

    // Standing on the plate does not press it again; stepping off and back
    // on does, and the room cycles back to its first layout.
    step(&mut app, KeyCode::ArrowDown);
    assert!(wall_at(&mut app, RIGHT).is_some());
    step(&mut app, KeyCode::ArrowUp);
    assert!(wall_at(&mut app, LEFT).is_some());
    assert!(wall_at(&mut app, RIGHT).is_none());
    assert_eq!(wall_at(&mut app, BELOW).unwrap(), below);
}

#[test]
fn a_room_waits_while_something_stands_where_a_wall_goes() {
    let mut app = boot();
    let blocker = app.world_mut().spawn((Active, Collider, Pos(RIGHT))).id();
    app.world_mut()
        .resource_mut::<MapGrid>()
        .set(RIGHT, blocker);

    step(&mut app, KeyCode::ArrowRight);
    assert!(wall_at(&mut app, LEFT).is_some(), "the room waits");
    assert!(wall_at(&mut app, RIGHT).is_none());

    app.world_mut().resource_mut::<MapGrid>().clear(RIGHT);
    app.world_mut().despawn(blocker);
    step(&mut app, KeyCode::ArrowDown);
    assert!(wall_at(&mut app, LEFT).is_none(), "and switches once clear");
    assert!(wall_at(&mut app, RIGHT).is_some());
}
