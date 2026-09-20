//! Real GamePlugin regressions with deterministic frame time and input edges.
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::components::*;
use pav_ecs_game_bevy_port::game::GamePlugin;
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
    app.update();
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
fn one_key_press_does_not_repeat_after_recharge() {
    let mut app = boot();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowRight);
    app.update();
    // Like InputPlugin: clear frame edges, but keep the key held.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    let after_press = player_of(app.world_mut()).0;
    for _ in 0..90 {
        app.update();
    }
    assert_eq!(player_of(app.world_mut()).0, after_press);
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
    let light = world.resource::<pav_ecs_game_bevy_port::render::DynamicLight>();
    assert!(light.data[idx].value > 1);
}
