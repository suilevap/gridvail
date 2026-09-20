//! Scripted gameplay: drive the real pipeline with synthetic input and
//! assert observable behavior — movement, tokens, bindings, enemies,
//! vision, and light sanity on `map1`.

use bevy::prelude::*;
use pav_ecs_game_bevy_port::components::*;
use pav_ecs_game_bevy_port::game::GamePlugin;

fn boot() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins(GamePlugin);
    app.update();
    app
}

fn player_of(world: &mut World) -> (Entity, IVec2, IVec2, i32) {
    let mut q = world.query_filtered::<(Entity, &Pos, &Facing, &Tokens), With<Player>>();
    let (e, p, f, t) = q.single(world).unwrap();
    (e, p.0, f.0, t.count)
}

#[test]
fn arrow_key_steps_player_and_spends_token() {
    let mut app = boot();
    let (_, start, _, _) = player_of(app.world_mut());
    assert_eq!(start, IVec2::new(8, 5));

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowRight);
    app.update();
    // No input plugin headless, so release manually (else just_pressed sticks).
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::ArrowRight);

    // One step east, facing follows, token spent...
    let (_, pos, facing, tokens) = player_of(app.world_mut());
    assert_eq!(pos, IVec2::new(9, 5));
    assert_eq!(facing, IVec2::new(1, 0));
    assert_eq!(tokens, 0);

    // ...and the bound 'i' marker still sits on its old cell: bindings read
    // pre-move parent positions (same order as the original), so it trails
    // by exactly one pass.
    let world = app.world_mut();
    let marker = world
        .query::<(Entity, &Pos)>()
        .iter(&*world)
        .find(|(e, _)| world.get::<BoundTo>(*e).is_some())
        .map(|(_, p)| p.0);
    assert_eq!(marker, Some(IVec2::new(9, 5)));

    // One more pass: the marker catches up, and the player token comes back
    // once every actor has spent (all-spent recharge path).
    app.update();
    let world = app.world_mut();
    let marker = world
        .query::<(Entity, &Pos)>()
        .iter(&*world)
        .find(|(e, _)| world.get::<BoundTo>(*e).is_some())
        .map(|(_, p)| p.0);
    assert_eq!(marker, Some(IVec2::new(10, 5)));
    for _ in 0..6 {
        app.update();
    }
    let (_, _, _, tokens) = player_of(app.world_mut());
    assert_eq!(tokens, 1);
}

#[test]
fn enemies_wander_and_world_stays_consistent() {
    let mut app = boot();
    let before: Vec<IVec2> = app
        .world_mut()
        .query::<(&Enemy, &Pos)>()
        .iter(app.world())
        .map(|(_, p)| p.0)
        .collect();
    assert!(!before.is_empty());
    // Spend the player's hoarded token so the all-spent recharge path can
    // fire (under frozen test time the 1s timer never elapses; in real play
    // it recharges everyone regardless).
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowRight);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::ArrowRight);
    for _ in 0..40 {
        app.update();
    }
    let world = app.world_mut();
    let after: Vec<IVec2> = world
        .query::<(&Enemy, &Pos)>()
        .iter(&*world)
        .map(|(_, p)| p.0)
        .collect();
    // Seeded RNG wanders: at least one enemy left its start cell...
    assert!(after.iter().zip(&before).any(|(a, b)| a != b));
    // ...all remain on valid cells that the grid attributes to them.
    let grid = world.resource::<MapGrid>();
    for p in &after {
        assert!(grid.is_valid(*p));
        assert!(grid.get(*p).is_some());
    }
}

#[test]
fn vision_and_light_fields_stay_sane() {
    let mut app = boot();
    for _ in 0..10 {
        app.update();
    }
    let world = app.world_mut();
    // Fractional FOV values stay in range and finite. The lower bound admits
    // float epsilon: the ported math is op-for-op identical to the original,
    // whose `1 - occluded` can also dip a hair below zero.
    for fov in world.query::<&FovResult>().iter(&*world) {
        assert!(fov
            .data
            .iter()
            .all(|v| v.is_finite() && *v >= -1e-4 && *v <= 1.0));
    }
    // The player actually sees part of the map around (8, 5).
    let vis = world.query::<&VisibilityMap>().single(&*world).unwrap();
    let seen = vis
        .data
        .iter()
        .filter(|v| v.contains(Vis::VISIBLE))
        .count();
    assert!(seen > 50, "only {seen} cells visible");
    // The player's own lamp lights its surroundings above ambient.
    let grid = world.resource::<MapGrid>();
    let idx = (5 * grid.width + 8) as usize;
    let dyn_light = world.resource::<pav_ecs_game_bevy_port::render::DynamicLight>();
    assert!(dyn_light.data[idx].value > 1);
}
