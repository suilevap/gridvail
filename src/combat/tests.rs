use super::*;
use crate::app::{GamePlugin, MapText};
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

#[test]
fn wait_meter_shows_remaining_time_and_confirms_commitment() {
    let mut app = game_with_ui("XXXXXXXX\nXp    bX\nX      X\nXXXXXXXX", true);
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
    game_with_ui(map, false)
}

fn game_with_ui(map: &'static str, ui: bool) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(MapText(map))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )))
        .add_plugins((GamePlugin, BattlePlugin));
    if ui {
        app.add_plugins(crate::rendering::TextRendererPlugin);
    }
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
    let glyph = app.world().get::<Glyph>(entity).unwrap();
    assert_eq!(glyph.ch, 'B');
    let buffers = app.world().resource::<RenderBuffers>();
    let sprite = buffers
        .objects
        .iter()
        .find(|object| object.entity == entity)
        .unwrap();
    assert_eq!(sprite.cell.ch, 'B');
    assert_eq!(sprite.cell.color, glyph.color);
    assert_eq!(
        buffers.current[buffers.idx(start.0).unwrap()].color,
        glyph.color
    );
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
    assert_eq!(player(&mut app).1.current, 12);
    assert!(matches!(
        enemy(&mut app).2.intent,
        BattleIntent::Slam { .. }
    ));
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
    assert_eq!(enemy(&mut app).1.current, 1);
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
    for _ in 0..30 {
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

#[test]
fn provoking_a_spear_makes_it_lunge_on_a_miss_and_leaves_time_to_punish() {
    let mut app = game("XXXXXXXX\nX p s  X\nX      X\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    let attack = enemy(&mut app).2.intent;
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(app.world().get::<Pos>(entity).unwrap().0, IVec2::new(3, 1));
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    assert_eq!(enemy(&mut app).2.recovery_left, 1);
    assert_eq!(enemy(&mut app).2.last_attack, attack);
    assert_eq!(player(&mut app).1.current, 12);
    let frame = app.world().resource::<RenderBuffers>();
    let spent = frame.ground[frame.idx(IVec2::new(2, 1)).unwrap()];
    assert_eq!(spent.ch, '+');
    assert_eq!(spent.color, CYAN);
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    press(&mut app, KeyCode::ArrowUp);
    assert_eq!(enemy(&mut app).1.current, 0); // Opening deals 3, rather than 2.
    assert_eq!(player(&mut app).1.current, 12);
}

#[test]
fn a_spear_lunge_respects_walls_and_never_wraps_its_threat() {
    let mut app = game("XXXXXXXX\nX p s  X\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    // Put a wall in the locked lane after it was planned (a changed room).
    let wall = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<MapGrid>()
        .set_with_blocking(IVec2::new(3, 1), wall, true);
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(app.world().get::<Pos>(entity).unwrap().0, IVec2::new(4, 1));
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    assert_eq!(player(&mut app).1.current, 12);
}

#[test]
fn a_missed_bash_lowers_the_shield_even_for_a_frontal_counter() {
    let mut app = game("XXXXXXXX\nX pg   X\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    assert_eq!(app.world().get::<Glyph>(entity).unwrap().ch, 'g');
    let world = app.world_mut();
    assert!(world
        .query::<(&BoundTo, &Glyph)>()
        .iter(world)
        .any(|(bound, glyph)| bound.parent == entity && glyph.ch == ' '));
    press(&mut app, KeyCode::ArrowUp);
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 1);
    assert_eq!(player(&mut app).1.current, 12);
}

#[test]
fn brute_finishes_the_original_area_slam_after_the_player_leaves() {
    let mut app = game("XXXXXXXX\nX pb   X\nX      X\nX      X\nXXXXXXXX");
    let intent = enemy(&mut app).2.intent;
    assert!(matches!(intent, BattleIntent::WindUp { .. }));
    assert_eq!(intent.cells().into_iter().flatten().count(), 8);
    press(&mut app, KeyCode::ArrowDown);
    assert!(matches!(
        enemy(&mut app).2.intent,
        BattleIntent::Slam { .. }
    ));
    assert_eq!(enemy(&mut app).2.intent.cells(), intent.cells());
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    assert!(matches!(
        enemy(&mut app).2.last_attack,
        BattleIntent::Slam { .. }
    ));
    assert_eq!(player(&mut app).1.current, 12);
    wait_action(&mut app);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    assert_eq!(enemy(&mut app).2.recovery_left, 1);
    wait_action(&mut app);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    assert_eq!(enemy(&mut app).2.recovery_left, 0);
}

#[test]
fn brute_wind_up_is_armoured_but_recovery_can_be_punished() {
    let mut app = game("XXXXXXXX\nX pb   X\nX      X\nXXXXXXXX");
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 4);
    assert!(matches!(
        enemy(&mut app).2.intent,
        BattleIntent::Slam { .. }
    ));
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 3);
    assert_eq!(player(&mut app).1.current, 9);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 0);
}

#[test]
fn baiting_a_brute_leaves_time_to_approach_and_land_two_opening_hits() {
    let mut app = game("XXXXXXXXX\nX       X\nX p b   X\nX       X\nXXXXXXXXX");
    wait_action(&mut app);
    wait_action(&mut app);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    press(&mut app, KeyCode::ArrowRight);
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 2);
    assert_eq!(enemy(&mut app).2.intent, BattleIntent::Recover);
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 0);
    assert_eq!(player(&mut app).1.current, 12);
}

#[test]
fn the_hud_explains_the_telegraph_and_counts_the_opening() {
    for (map, next, opening, count) in [
        (
            "XXXXXXXX\nX ps   X\nX      X\nX      X\nXXXXXXXX",
            "LUNGE NEXT",
            "EXTENDED",
            "2 actions OPEN",
        ),
        (
            "XXXXXXXX\nX pg   X\nX      X\nX      X\nXXXXXXXX",
            "BASH NEXT",
            "SHIELD DOWN (any side)",
            "2 actions OPEN",
        ),
        (
            "XXXXXXXX\nX pb   X\nX      X\nX      X\nXXXXXXXX",
            "SLAM IN 2 ACTIONS",
            "RECOVERING",
            "3 actions OPEN",
        ),
    ] {
        let mut app = game_with_ui(map, true);
        let has_text = |app: &mut App, expected: &str| {
            let world = app.world_mut();
            world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0.contains(expected))
        };
        assert!(has_text(&mut app, next), "missing {next}");
        press(&mut app, KeyCode::ArrowDown);
        if next == "SLAM IN 2 ACTIONS" {
            assert!(has_text(&mut app, "SLAM NEXT"));
            press(&mut app, KeyCode::ArrowDown);
        }
        assert!(has_text(&mut app, opening), "missing {opening}");
        assert!(has_text(&mut app, count), "missing {count}");
        wait_action(&mut app);
        assert!(has_text(
            &mut app,
            if count.starts_with('3') {
                "2 actions OPEN"
            } else {
                "1 action OPEN"
            }
        ));
    }
}

#[test]
fn a_spent_attack_never_hides_another_enemys_live_threat() {
    for spent_first in [false, true] {
        let mut app = game("XXXXXXXX\nX  s   X\nX p g  X\nX      X\nXXXXXXXX");
        let world = app.world_mut();
        for (index, mut fighter) in world.query::<&mut Fighter>().iter_mut(world).enumerate() {
            let strike = BattleIntent::Strike {
                origin: IVec2::new(3, 2),
                direction: IVec2::NEG_X,
                reach: 1,
                damage: 2,
            };
            fighter.needs_plan = false;
            if (index == 0) == spent_first {
                fighter.intent = BattleIntent::Recover;
                fighter.last_attack = strike;
            } else {
                fighter.intent = strike;
            }
        }
        app.update();
        let frame = app.world().resource::<RenderBuffers>();
        let cell = frame.ground[frame.idx(IVec2::new(2, 2)).unwrap()];
        assert_eq!(cell.ch, '!');
        assert_eq!(cell.color, RED);
    }
}
