//! Real GamePlugin regressions with deterministic frame time and keyboard input.
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::app::GamePlugin;
use pav_ecs_game_bevy_port::model::*;
use std::{collections::HashMap, time::Duration};

fn boot() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .add_plugins(GamePlugin);
    app.update();
    app
}

fn player_of(world: &mut World) -> (IVec2, IVec2, i32) {
    let mut q = world.query_filtered::<(&Pos, &Facing, &Tokens), With<Player>>();
    let (p, f, t) = q.single(world).unwrap();
    (p.0, f.0, t.count)
}

fn press_once(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    // MinimalPlugins has no InputPlugin to clear the frame's input edges.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
}

#[test]
fn arrow_key_steps_player_and_spends_token() {
    let mut app = boot();
    assert_eq!(player_of(app.world_mut()).0, IVec2::new(8, 5));
    press_once(&mut app, KeyCode::ArrowRight);
    assert_eq!(player_of(app.world_mut()), (IVec2::new(9, 5), IVec2::X, 0));
    let world = app.world_mut();
    let marker = world
        .query_filtered::<&Pos, With<BoundTo>>()
        .single(world)
        .unwrap();
    assert_eq!(marker.0, IVec2::new(10, 5));
    for _ in 0..7 {
        app.update();
    }
    assert_eq!(player_of(app.world_mut()).2, 1);
}

#[test]
fn tokenless_enemies_do_not_block_player_input() {
    let mut app = boot();
    for _ in 0..5 {
        app.update();
    }
    let (before, _, tokens) = player_of(app.world_mut());
    assert_eq!(tokens, 1);
    assert!(!app.world().resource::<TurnState>().simulation);
    press_once(&mut app, KeyCode::ArrowRight);
    assert_eq!(player_of(app.world_mut()).0, before + IVec2::X);
}

#[test]
fn held_key_repeats_without_delay_and_release_stops_it() {
    // No animation step is installed, so nothing holds the turns back.
    let mut app = boot();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowLeft);
    app.update();
    // Like InputPlugin: clear frame edges, but keep the key held.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    let after_press = player_of(app.world_mut()).0;
    app.update();
    let after_hold = player_of(app.world_mut()).0;
    assert_eq!(
        after_hold,
        after_press + IVec2::NEG_X,
        "held key did not move again on the next frame"
    );

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::ArrowLeft);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    for _ in 0..90 {
        app.update();
    }
    assert_eq!(player_of(app.world_mut()).0, after_hold);
}

#[test]
fn blocked_move_keeps_marker_relative_to_actual_position() {
    let mut app = boot();
    let wall_pos = IVec2::new(9, 5);
    let wall = app
        .world_mut()
        .spawn((Active, Collider, Pos(wall_pos)))
        .id();
    app.world_mut()
        .resource_mut::<MapGrid>()
        .set(wall_pos, wall);
    press_once(&mut app, KeyCode::ArrowRight);
    let world = app.world_mut();
    assert_eq!(player_of(world).0, IVec2::new(8, 5));
    let marker = world
        .query_filtered::<&Pos, With<BoundTo>>()
        .single(world)
        .unwrap();
    assert_eq!(marker.0, wall_pos);
    assert!(world
        .resource::<CollisionBuffer>()
        .0
        .iter()
        .any(|c| c.target == wall));
}

#[test]
fn enemies_wander_and_world_stays_consistent() {
    let mut app = boot();
    let world = app.world_mut();
    let before: HashMap<Entity, IVec2> = world
        .query_filtered::<(Entity, &Pos), With<Enemy>>()
        .iter(world)
        .map(|(e, p)| (e, p.0))
        .collect();
    assert!(!before.is_empty());
    // Three timed recharge intervals, while the player keeps its token.
    for _ in 0..190 {
        app.update();
    }
    let world = app.world_mut();
    let after: Vec<_> = world
        .query_filtered::<(Entity, &Pos), With<Enemy>>()
        .iter(world)
        .map(|(e, p)| (e, p.0))
        .collect();
    assert!(after.iter().any(|(e, p)| before[e] != *p));
    let grid = world.resource::<MapGrid>();
    for (e, p) in after {
        assert!(grid.is_valid(p));
        assert_eq!(grid.get(p), Some(e));
    }
}

#[test]
fn vision_and_light_fields_stay_sane() {
    let mut app = boot();
    for _ in 0..10 {
        app.update();
    }
    let world = app.world_mut();
    for fov in world.query::<&FovResult>().iter(&*world) {
        assert!(fov
            .data
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }
    let vis = world.query::<&VisibilityMap>().single(&*world).unwrap();
    let seen = vis.data.iter().filter(|v| v.contains(Vis::VISIBLE)).count();
    assert!(seen > 50, "only {seen} cells visible");
    let idx = (5 * world.resource::<MapGrid>().width + 8) as usize;
    let light = world.resource::<pav_ecs_game_bevy_port::presentation::DynamicLight>();
    assert!(light.data[idx].value > 1);
}

/// Presses `key` once and waits until the player can act again.
fn step(app: &mut App, key: KeyCode) {
    press_once(app, key);
    for _ in 0..120 {
        if player_of(app.world_mut()).2 > 0 && !app.world().resource::<TurnState>().simulation {
            return;
        }
        app.update();
    }
    panic!("player never got its token back");
}

fn carried_keys(world: &mut World) -> Vec<KeyColor> {
    let items = world
        .query_filtered::<&Inventory, With<Player>>()
        .single(world)
        .unwrap()
        .0
        .clone();
    items
        .into_iter()
        .filter_map(|item| world.get::<Key>(item).map(|key| key.0))
        .collect()
}

#[test]
fn red_key_opens_the_red_door_to_the_green_key() {
    use KeyCode::{ArrowDown as D, ArrowLeft as L, ArrowRight as R, ArrowUp as U};
    let mut app = boot();
    let door_pos = IVec2::new(18, 4);
    let door = {
        let world = app.world_mut();
        let (door, pos, glyph) = world
            .query::<(Entity, &Pos, &Door)>()
            .iter(world)
            .find(|(_, _, door)| door.color == KeyColor::Red)
            .map(|(entity, pos, door)| (entity, pos.0, *door))
            .unwrap();
        assert_eq!((pos, glyph.open), (door_pos, false));
        door
    };

    for key in [L, L, L, L, L, U, U, U] {
        step(&mut app, key);
    }
    assert_eq!(player_of(app.world_mut()).0, IVec2::new(3, 2));
    assert_eq!(carried_keys(app.world_mut()), [KeyColor::Red]);

    for _ in 0..15 {
        step(&mut app, R);
    }
    step(&mut app, D);
    assert_eq!(player_of(app.world_mut()).0, IVec2::new(18, 3));
    assert!(app.world().resource::<MapGrid>().blocks_vision(door_pos));

    // Bumping opens the door; the player walks through on the next step.
    step(&mut app, D);
    assert_eq!(player_of(app.world_mut()).0, IVec2::new(18, 3));
    assert!(app.world().get::<Door>(door).unwrap().open);
    assert!(!app.world().resource::<MapGrid>().blocks_vision(door_pos));

    step(&mut app, D);
    step(&mut app, D);
    assert_eq!(player_of(app.world_mut()).0, IVec2::new(18, 5));
    assert_eq!(
        carried_keys(app.world_mut()),
        [KeyColor::Red, KeyColor::Green]
    );
}
