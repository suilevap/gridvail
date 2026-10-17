use super::*;
use crate::app::{GamePlugin, MapText};
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

#[test]
fn wait_meter_shows_remaining_time_and_confirms_commitment() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(MapText("XXXXXXXX\nXp    bX\nX      X\nXXXXXXXX"))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )))
        .add_plugins((
            GamePlugin,
            BattlePlugin,
            crate::rendering::TextRendererPlugin,
        ));
    for _ in 0..4 {
        app.update();
    }
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Space);
    for _ in 0..3 {
        app.update();
    }
    let world = app.world_mut();
    assert!(world
        .query::<&Text>()
        .iter(world)
        .any(|text| text.0.contains("until WAIT counts")));
    assert!(world
        .query::<&Node>()
        .iter(world)
        .any(|node| matches!(node.width, Val::Percent(percent) if (percent - 50.0).abs() < 1.0)));
    for _ in 0..4 {
        app.update();
    }
    let world = app.world_mut();
    assert!(world
        .query::<&Text>()
        .iter(world)
        .any(|text| text.0.contains("WAIT COUNTED")));
    assert_eq!(world.resource::<BattleState>().round, 1);
}

fn game(map: &'static str) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(MapText(map))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )))
        .add_plugins((GamePlugin, BattlePlugin));
    for _ in 0..4 {
        app.update();
    }
    app
}

fn player(app: &mut App) -> (IVec2, Health) {
    let world = app.world_mut();
    let (pos, health) = world
        .query_filtered::<(&Pos, &Health), With<Player>>()
        .single(world)
        .unwrap();
    (pos.0, *health)
}

fn enemy(app: &mut App) -> (Entity, Health, Fighter) {
    let world = app.world_mut();
    let (entity, health, fighter) = world
        .query::<(Entity, &Health, &Fighter)>()
        .single(world)
        .unwrap();
    (entity, *health, *fighter)
}

fn press(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    app.update();
}

fn wait_action(app: &mut App) {
    let world = app.world_mut();
    let mut command = world
        .query_filtered::<&mut MoveCommand, With<Player>>()
        .single_mut(world)
        .unwrap();
    *command = MoveCommand {
        target: IVec2::ZERO,
        relative: true,
        active: true,
    };
    app.update();
    app.update();
}

#[test]
fn waiting_for_input_does_not_spend_rounds_or_move_enemies() {
    let mut app = game("XXXXXXXX\nXp    bX\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    let start = *app.world().get::<Pos>(entity).unwrap();
    let intent = enemy(&mut app).2.intent;
    for _ in 0..100 {
        app.update();
    }
    assert_eq!(app.world().resource::<BattleState>().round, 0);
    assert_eq!(*app.world().get::<Pos>(entity).unwrap(), start);
    assert_eq!(enemy(&mut app).2.intent, intent);
}

#[test]
fn wait_hold_shows_progress_cancels_and_commits_exactly_once() {
    let mut wait = WaitHold::default();
    assert!(!wait.update(true, true, 0.3));
    assert!((wait.progress() - 0.5).abs() < 0.001);
    assert!(!wait.update(false, true, 0.1));
    assert_eq!(wait.progress(), 0.0);
    assert!(!wait.update(true, false, 1.0));
    assert_eq!(wait.progress(), 0.0);
    assert!(wait.update(true, true, WaitHold::SECONDS));
    for _ in 0..10 {
        assert!(!wait.update(true, true, 1.0));
    }
    wait.update(false, true, 0.0);
    assert!(wait.update(true, true, WaitHold::SECONDS));
}

#[test]
fn holding_space_counts_one_guarded_turn_until_released() {
    let mut app = game("XXXXXXXX\nXp    bX\nX      X\nXXXXXXXX");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Space);
    for _ in 0..3 {
        app.update();
    }
    assert!(app.world().resource::<WaitHold>().progress() > 0.0);
    assert_eq!(app.world().resource::<BattleState>().round, 0);
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(app.world().resource::<BattleState>().round, 1);
    assert!(player(&mut app).1.guarded);
    assert!(app.world().resource::<WaitHold>().latched);
}

#[test]
fn a_held_movement_key_never_repeats_in_battle() {
    let mut app = game("XXXXXXXX\nXp    bX\nX      X\nXXXXXXXX");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowDown);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    let position = player(&mut app).0;
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(player(&mut app).0, position);
    assert_eq!(app.world().resource::<BattleState>().round, 1);
}

#[test]
fn spearman_threatens_two_cells_and_dodging_avoids_the_locked_strike() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    let cells = enemy(&mut app).2.intent.cells();
    assert!(cells.contains(&Some(IVec2::new(2, 1))));
    assert!(cells.contains(&Some(IVec2::new(1, 1))));
    let buffers = app.world().resource::<RenderBuffers>();
    assert_eq!(
        buffers.ground[buffers.idx(IVec2::new(1, 1)).unwrap()].ch,
        '!'
    );
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(player(&mut app).1.current, 12);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
}

#[test]
fn bump_attack_spends_a_turn_and_interrupts_the_enemy() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    let position = player(&mut app).0;
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 1);
    assert_eq!(player(&mut app).1.current, 12);
    assert_eq!(player(&mut app).0, position);
    assert_eq!(app.world().resource::<BattleState>().round, 1);
}

#[test]
fn bracing_reduces_one_hit_then_expires_and_brute_recovers() {
    let mut app = game("XXXXXXXX\nX pb   X\nX      X\nXXXXXXXX");
    wait_action(&mut app);
    assert_eq!(player(&mut app).1.current, 10);
    assert!(!player(&mut app).1.guarded);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    wait_action(&mut app);
    assert_eq!(player(&mut app).1.current, 10);
}

#[test]
fn shield_blocks_front_but_a_flank_interrupts_it() {
    let mut app = game("XXXXXXXX\nX pg   X\nX      X\nXXXXXXXX");
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 4);
    let mut app = game("XXXXXXXX\nX pg   X\nX      X\nXXXXXXXX");
    press(&mut app, KeyCode::ArrowDown);
    press(&mut app, KeyCode::ArrowRight);
    press(&mut app, KeyCode::ArrowUp);
    assert_eq!(enemy(&mut app).1.current, 2);
    assert_eq!(player(&mut app).1.current, 12);
}

#[test]
fn defeated_enemies_release_their_cells_and_reset_restores_them() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    press(&mut app, KeyCode::ArrowRight);
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 0);
    assert!(app.world().get::<Active>(entity).is_none());
    assert_eq!(
        app.world().resource::<MapGrid>().get(IVec2::new(3, 1)),
        None
    );
    assert!(app.world().resource::<BattleState>().won);
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(player(&mut app).0, IVec2::new(3, 1));
    press(&mut app, KeyCode::KeyR);
    assert_eq!(enemy(&mut app).1.current, 3);
    assert!(app.world().get::<Active>(entity).is_some());
    assert_eq!(
        app.world().resource::<MapGrid>().get(IVec2::new(3, 1)),
        Some(entity)
    );
    assert_eq!(app.world().resource::<BattleState>().round, 0);
}

#[test]
fn defeat_stops_actions_and_can_be_reset() {
    let mut app = game("XXXXXXXX\nX pb   X\nX      X\nXXXXXXXX");
    for _ in 0..15 {
        wait_action(&mut app);
    }
    assert!(app.world().resource::<BattleState>().defeated);
    let position = player(&mut app).0;
    let round = app.world().resource::<BattleState>().round;
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(player(&mut app).0, position);
    assert_eq!(app.world().resource::<BattleState>().round, round);
    press(&mut app, KeyCode::KeyR);
    assert_eq!(player(&mut app).1.current, 12);
    assert!(!app.world().resource::<BattleState>().defeated);
}
