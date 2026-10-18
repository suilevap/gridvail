use crate::app::{BattlePlugin, GamePlugin, MapText};
use crate::lighting::{CYAN, RED};
use crate::model::*;
use bevy::prelude::*;
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
        .any(|text| text.0.contains("WAIT 0.3s")));
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
    assert_eq!(
        world
            .query_filtered::<&ActionTaken, With<Player>>()
            .single(world)
            .unwrap()
            .serial,
        1
    );
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
    app.world_mut().resource_mut::<TokenTimer>().0.reset();
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

fn actions(app: &mut App) -> u64 {
    let world = app.world_mut();
    world
        .query_filtered::<&ActionTaken, With<Player>>()
        .single(world)
        .unwrap()
        .serial
}

fn braced(app: &mut App) -> bool {
    let world = app.world_mut();
    world
        .query_filtered::<&Brace, With<Player>>()
        .single(world)
        .unwrap()
        .charges
        > 0
}

fn enemy(app: &mut App) -> (Entity, Health, AttackCycle) {
    let world = app.world_mut();
    let (entity, health, fighter) = world
        .query::<(Entity, &Health, &AttackCycle)>()
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
fn the_telegraph_stays_locked_until_the_decision_deadline() {
    let mut app = game("XXXXXXXX\nXp    bX\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    let start = *app.world().get::<Pos>(entity).unwrap();
    let intent = enemy(&mut app).2.area;
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(actions(&mut app), 0);
    assert_eq!(*app.world().get::<Pos>(entity).unwrap(), start);
    assert_eq!(enemy(&mut app).2.area, intent);
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
fn holding_space_counts_one_brace_but_does_not_stop_the_decision_clock() {
    let mut app = game("XXXXXXXX\nXp    bX\nX      X\nXXXXXXXX");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Space);
    for _ in 0..3 {
        app.update();
    }
    assert!(app.world().resource::<WaitHold>().progress() > 0.0);
    assert_eq!(actions(&mut app), 0);
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(actions(&mut app), 1);
    assert!(braced(&mut app));
    assert!(app.world().resource::<WaitHold>().latched);
    for _ in 0..20 {
        app.update();
    }
    assert!(actions(&mut app) > 1);
    assert!(!braced(&mut app));
    assert_eq!(app.world().resource::<WaitHold>().elapsed, 0.0);
}

#[test]
fn holding_a_direction_keeps_moving_and_releasing_stops_it() {
    let mut app = game("XXXXXXXXXXXX\nXp        bX\nX          X\nXXXXXXXXXXXX");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowRight);
    for _ in 0..4 {
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
    }
    let position = player(&mut app).0;
    assert_eq!(position, IVec2::new(5, 1));
    assert_eq!(actions(&mut app), 4);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(player(&mut app).0, position);
    assert_eq!(actions(&mut app), 4);
}

#[test]
fn a_missed_deadline_gives_enemies_an_unbraced_action() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    let pos = player(&mut app).0;
    for _ in 0..9 {
        app.update();
    }
    assert_eq!(actions(&mut app), 0);
    app.update();
    assert_eq!(actions(&mut app), 1);
    assert_eq!(player(&mut app).0, pos);
    assert_eq!(player(&mut app).1.current, 10);
    assert!(!braced(&mut app));
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(actions(&mut app), 2);
}

#[test]
fn manual_input_wins_on_the_deadline_frame_and_resets_the_clock() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    for _ in 0..9 {
        app.update();
    }
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(actions(&mut app), 1);
    assert_eq!(player(&mut app).1.current, 12);
    assert!(app.world().resource::<TokenTimer>().0.elapsed_secs() < 0.11);
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(actions(&mut app), 1);
    app.update();
    assert_eq!(actions(&mut app), 2);
}

#[test]
fn settings_pause_the_decision_clock_and_reset_starts_it_fresh() {
    let mut app = game("XXXXXXXX\nXp    bX\nX      X\nXXXXXXXX");
    for _ in 0..4 {
        app.update();
    }
    app.insert_resource(SettingsMenu {
        open: true,
        ..default()
    });
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(actions(&mut app), 0);
    assert!((app.world().resource::<TokenTimer>().0.elapsed_secs() - 0.4).abs() < 0.01);
    app.world_mut().resource_mut::<SettingsMenu>().open = false;
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(actions(&mut app), 0);
    app.update();
    assert_eq!(actions(&mut app), 1);
    press(&mut app, KeyCode::KeyR);
    assert_eq!(actions(&mut app), 0);
    assert!(app.world().resource::<TokenTimer>().0.elapsed_secs() < 0.21);
}

#[test]
fn the_clock_waits_for_animation_before_timing_the_next_decision() {
    let mut app = game("XXXXXXXXXXXX\nXp        bX\nX          X\nXXXXXXXXXXXX");
    press(&mut app, KeyCode::ArrowDown);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowRight);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    app.world_mut()
        .resource_mut::<TurnPacing>()
        .report_animation(Duration::from_secs(5));
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(actions(&mut app), 2);
    assert_eq!(app.world().resource::<TokenTimer>().0.elapsed_secs(), 0.0);
    app.world_mut()
        .resource_mut::<TurnPacing>()
        .report_animation(Duration::ZERO);
    app.update();
    assert!(app.world().resource::<TokenTimer>().0.remaining_secs() > 0.89);
    assert_eq!(actions(&mut app), 2);
}

#[test]
fn a_late_wait_hold_is_cancelled_by_the_deadline_without_granting_a_brace() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    for _ in 0..7 {
        app.update();
    }
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Space);
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(actions(&mut app), 1);
    assert_eq!(player(&mut app).1.current, 10);
    assert!(!braced(&mut app));
    assert!(app.world().resource::<WaitHold>().latched);
    assert_eq!(app.world().resource::<WaitHold>().elapsed, 0.0);
}

#[test]
fn the_hud_shows_the_decision_countdown_and_urgent_clock_bar() {
    let mut app = game_with_ui("XXXXXXXX\nXp    bX\nX      X\nXXXXXXXX", true);
    for _ in 0..8 {
        app.update();
    }
    let world = app.world_mut();
    assert!(world
        .query::<&Text>()
        .iter(world)
        .any(|text| text.0.contains("ACT 0.2s")));
    assert!(world.query::<(&Node, &BackgroundColor)>().iter(world).any(
        |(node, color)| matches!(node.width, Val::Percent(p) if (p - 20.0).abs() < 0.1)
            && color.0 == Color::srgb(1.0, 0.25, 0.25)
    ));
}

#[test]
fn spearman_threatens_two_cells_and_dodging_avoids_the_locked_strike() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    let cells = enemy(&mut app).2.area.cells;
    assert!(cells.contains(&Some(IVec2::new(2, 1))));
    assert!(cells.contains(&Some(IVec2::new(1, 1))));
    let buffers = app.world().resource::<RenderBuffers>();
    assert_eq!(
        buffers.ground[buffers.idx(IVec2::new(1, 1)).unwrap()].ch,
        '!'
    );
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(player(&mut app).1.current, 12);
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
}

#[test]
fn bump_attack_spends_a_turn_and_interrupts_the_enemy() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    let position = player(&mut app).0;
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 1);
    assert_eq!(player(&mut app).1.current, 12);
    assert_eq!(player(&mut app).0, position);
    assert_eq!(actions(&mut app), 1);
}

#[test]
fn bracing_reduces_one_hit_then_expires_and_brute_recovers() {
    let mut app = game("XXXXXXXX\nX pb   X\nX      X\nXXXXXXXX");
    wait_action(&mut app);
    assert_eq!(player(&mut app).1.current, 12);
    assert!(matches!(enemy(&mut app).2.phase, AttackPhase::Ready));
    wait_action(&mut app);
    assert_eq!(player(&mut app).1.current, 10);
    assert!(!braced(&mut app));
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
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
    assert_eq!(
        *app.world().resource::<EncounterOutcome>(),
        EncounterOutcome::Victory
    );
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(player(&mut app).0, IVec2::new(3, 1));
    press(&mut app, KeyCode::KeyR);
    assert_eq!(enemy(&mut app).1.current, 3);
    assert!(app.world().get::<Active>(entity).is_some());
    assert_eq!(
        app.world().resource::<MapGrid>().get(IVec2::new(3, 1)),
        Some(entity)
    );
    assert_eq!(actions(&mut app), 0);
}

#[test]
fn defeat_stops_actions_and_can_be_reset() {
    let mut app = game("XXXXXXXX\nX pb   X\nX      X\nXXXXXXXX");
    for _ in 0..30 {
        wait_action(&mut app);
    }
    assert_eq!(
        *app.world().resource::<EncounterOutcome>(),
        EncounterOutcome::Defeat
    );
    let position = player(&mut app).0;
    let round = actions(&mut app);
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(player(&mut app).0, position);
    assert_eq!(actions(&mut app), round);
    press(&mut app, KeyCode::KeyR);
    assert_eq!(player(&mut app).1.current, 12);
    assert_eq!(
        *app.world().resource::<EncounterOutcome>(),
        EncounterOutcome::Ongoing
    );
}

#[test]
fn provoking_a_spear_makes_it_lunge_on_a_miss_and_leaves_time_to_punish() {
    let mut app = game("XXXXXXXX\nX p s  X\nX      X\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    let attack = enemy(&mut app).2.area;
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(app.world().get::<Pos>(entity).unwrap().0, IVec2::new(3, 1));
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
    assert_eq!(enemy(&mut app).2.phase, AttackPhase::Recovering(2));
    assert_eq!(enemy(&mut app).2.spent, attack);
    assert_eq!(player(&mut app).1.current, 12);
    let frame = app.world().resource::<RenderBuffers>();
    let spent = frame.ground[frame.idx(IVec2::new(2, 1)).unwrap()];
    assert_eq!(spent.ch, '+');
    assert_eq!(spent.color, CYAN);
    press(&mut app, KeyCode::ArrowRight);
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
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
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
    assert_eq!(player(&mut app).1.current, 12);
}

#[test]
fn a_missed_bash_lowers_the_shield_even_for_a_frontal_counter() {
    let mut app = game("XXXXXXXX\nX pg   X\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    press(&mut app, KeyCode::ArrowDown);
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
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
    let entity = enemy(&mut app).0;
    let intent = enemy(&mut app).2.area;
    assert!(matches!(enemy(&mut app).2.phase, AttackPhase::Windup(_)));
    assert_eq!(intent.cells.into_iter().flatten().count(), 8);
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(app.world().get::<Glyph>(entity).unwrap().color, RED);
    assert!(matches!(enemy(&mut app).2.phase, AttackPhase::Ready));
    assert_eq!(enemy(&mut app).2.area.cells, intent.cells);
    press(&mut app, KeyCode::ArrowDown);
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
    assert_eq!(enemy(&mut app).2.spent, intent);
    assert_eq!(player(&mut app).1.current, 12);
    wait_action(&mut app);
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
    assert_eq!(enemy(&mut app).2.phase, AttackPhase::Recovering(2));
    wait_action(&mut app);
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
    assert_eq!(enemy(&mut app).2.phase, AttackPhase::Recovering(1));
}

#[test]
fn brute_wind_up_is_armoured_but_recovery_can_be_punished() {
    let mut app = game("XXXXXXXX\nX pb   X\nX      X\nXXXXXXXX");
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 4);
    assert!(matches!(enemy(&mut app).2.phase, AttackPhase::Ready));
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 3);
    assert_eq!(player(&mut app).1.current, 9);
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 0);
}

#[test]
fn baiting_a_brute_leaves_time_to_approach_and_land_two_opening_hits() {
    let mut app = game("XXXXXXXXX\nX       X\nX  pb   X\nX       X\nXXXXXXXXX");
    press(&mut app, KeyCode::ArrowLeft);
    wait_action(&mut app);
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
    press(&mut app, KeyCode::ArrowRight);
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 2);
    assert!(matches!(
        enemy(&mut app).2.phase,
        AttackPhase::Recovering(_)
    ));
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 0);
    assert_eq!(player(&mut app).1.current, 12);
}

#[test]
fn the_hud_explains_the_telegraph_and_counts_the_opening() {
    for (map, next, opening, count) in [
        (
            "XXXXXXXX\nX ps   X\nX      X\nX      X\nXXXXXXXX",
            "LUNGE | sidestep",
            "counter",
            2,
        ),
        (
            "XXXXXXXX\nX pg   X\nX      X\nX      X\nXXXXXXXX",
            "BASH | sidestep",
            "any side",
            2,
        ),
        (
            "XXXXXXXX\nX pb   X\nX      X\nX      X\nXXXXXXXX",
            "SLAM 2 turns | out",
            "counter",
            3,
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
        if next == "SLAM 2 turns | out" {
            assert!(has_text(&mut app, "SLAM NEXT"));
            press(&mut app, KeyCode::ArrowDown);
        }
        assert!(has_text(&mut app, opening), "missing {opening}");
        assert!(has_text(&mut app, &format!("OPEN {count}")));
        wait_action(&mut app);
        assert!(has_text(&mut app, &format!("OPEN {}", count - 1)));
    }
}

#[test]
fn a_spent_attack_never_hides_another_enemys_live_threat() {
    for spent_first in [false, true] {
        let mut app = game("XXXXXXXX\nX  s   X\nX p g  X\nX      X\nXXXXXXXX");
        let world = app.world_mut();
        for (index, mut fighter) in world
            .query::<&mut AttackCycle>()
            .iter_mut(world)
            .enumerate()
        {
            let mut cells = [None; 8];
            cells[0] = Some(IVec2::new(2, 2));
            let strike = AttackArea {
                origin: IVec2::new(3, 2),
                direction: IVec2::NEG_X,
                cells,
                damage: 2,
            };
            if (index == 0) == spent_first {
                fighter.phase = AttackPhase::Recovering(1);
                fighter.spent = strike;
            } else {
                fighter.phase = AttackPhase::Ready;
                fighter.area = strike;
            }
        }
        app.update();
        let frame = app.world().resource::<RenderBuffers>();
        let cell = frame.ground[frame.idx(IVec2::new(2, 2)).unwrap()];
        assert_eq!(cell.ch, '!');
        assert_eq!(cell.color, RED);
    }
}

#[test]
fn a_directional_guard_composes_with_a_lunge_and_lowers_for_recovery() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    app.world_mut().entity_mut(entity).insert((
        DirectionalGuard {
            window: AttackWindow::OutsideRecovery,
        },
        Facing(IVec2::NEG_X),
    ));
    // This is still a two-cell lunging attacker; adding a guard changes only
    // its defence. Its ordinary attack/recovery still advances on a block.
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 3);
    assert_eq!(player(&mut app).1.current, 10);
    assert_eq!(enemy(&mut app).2.phase, AttackPhase::Recovering(2));
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 0);
}

#[test]
fn armour_and_interrupt_resistance_compose_with_a_line_attack() {
    let mut app = game("XXXXXXXX\nX ps   X\nX      X\nXXXXXXXX");
    let entity = enemy(&mut app).0;
    app.world_mut().entity_mut(entity).insert((
        Armour {
            reduction: 1,
            window: AttackWindow::Always,
        },
        InterruptResistance {
            window: AttackWindow::Always,
        },
    ));
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 2);
    assert_eq!(player(&mut app).1.current, 10); // The hit did not interrupt.
    assert_eq!(enemy(&mut app).2.phase, AttackPhase::Recovering(2));
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(enemy(&mut app).1.current, 0); // Opening bonus and armour compose.
}

#[test]
fn a_longer_line_is_only_configuration_and_keeps_all_four_cells_locked() {
    let mut app = game("XXXXXXXXXX\nX p   s  X\nX        X\nXXXXXXXXXX");
    let entity = enemy(&mut app).0;
    app.world_mut()
        .entity_mut(entity)
        .insert(LineAttack { reach: 4 });
    app.update();
    let area = enemy(&mut app).2.area;
    assert_eq!(area.cells.into_iter().flatten().count(), 4);
    assert_eq!(area.cells[3], Some(IVec2::new(2, 1)));
    press(&mut app, KeyCode::ArrowDown);
    assert_eq!(enemy(&mut app).2.spent, area);
    assert_eq!(app.world().get::<Pos>(entity).unwrap().0, IVec2::new(5, 1));
    assert_eq!(player(&mut app).1.current, 12);
}

#[test]
fn damage_and_guards_work_without_an_enemy_type_or_tactical_turn_mode() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(MapText("XXXXXXXX\nXp     X\nX      X\nXXXXXXXX"))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )))
        .add_plugins((GamePlugin, crate::combat::CombatPlugin));
    app.update();
    let world = app.world_mut();
    let player = world
        .query_filtered::<Entity, With<Player>>()
        .single(world)
        .unwrap();
    world
        .entity_mut(player)
        .insert((Health::new(12), Team(0), BumpAttack { damage: 2 }));
    let target = world
        .spawn((
            Active,
            Collider,
            Pos(IVec2::new(2, 1)),
            Health::new(4),
            Facing(IVec2::NEG_X),
            DirectionalGuard {
                window: AttackWindow::Always,
            },
            Armour {
                reduction: 1,
                window: AttackWindow::Always,
            },
        ))
        .id();
    world
        .resource_mut::<MapGrid>()
        .set_with_blocking(IVec2::new(2, 1), target, false);
    assert_eq!(*world.resource::<TurnMode>(), TurnMode::Independent);
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(app.world().get::<Health>(target).unwrap().current, 4);
    app.world_mut().get_mut::<Facing>(target).unwrap().0 = IVec2::Y;
    press(&mut app, KeyCode::ArrowRight);
    assert_eq!(app.world().get::<Health>(target).unwrap().current, 3);
    for _ in 0..3 {
        press(&mut app, KeyCode::ArrowRight);
    }
    assert!(app.world().get_entity(target).is_err()); // Existing destruction pipeline.
    assert_eq!(
        app.world().resource::<MapGrid>().get(IVec2::new(2, 1)),
        None
    );
}

#[test]
fn existing_behavior_trees_still_run_beside_attack_capabilities() {
    let mut app = game("XXXXXXXXXXXXXXXXXXXXXXXX\nXp    h           s    X\nX                      X\nXXXXXXXXXXXXXXXXXXXXXXXX");
    let world = app.world_mut();
    let (hunter, start) = world
        .query_filtered::<(Entity, &Pos), With<Hunter>>()
        .single(world)
        .unwrap();
    let start = start.0;
    for _ in 0..20 {
        wait_action(&mut app);
    }
    assert!(app.world().get::<EnemyAct>(hunter).is_some());
    assert!(app
        .world()
        .get::<EnemyMind>(hunter)
        .unwrap()
        .last_seen
        .is_some());
    assert_ne!(app.world().get::<Pos>(hunter).unwrap().0, start);
}

#[test]
fn another_controller_can_aim_an_attack_without_a_player_or_enemy() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(MapText("XXXXXXXX\nX      X\nX      X\nXXXXXXXX"))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )))
        .add_plugins((GamePlugin, crate::combat::CombatPlugin));
    app.update();
    let world = app.world_mut();
    let source = world
        .spawn((
            Active,
            Collider,
            Pos(IVec2::new(4, 2)),
            PrevPos(IVec2::new(4, 2)),
            Facing::default(),
            Speed::default(),
            Friction(1),
            MoveCommand::default(),
            PendingPos::None,
            Tokens {
                count: 1,
                recharge: 1,
            },
            LineAttack { reach: 2 },
            AttackPower(2),
            AttackTiming {
                windup: 0,
                recovery: 2,
            },
            AttackTarget(Some(IVec2::new(2, 2))),
        ))
        .id();
    let target = world
        .spawn((Active, Collider, Pos(IVec2::new(2, 2)), Health::new(5)))
        .id();
    world
        .resource_mut::<MapGrid>()
        .set_with_blocking(IVec2::new(4, 2), source, false);
    world
        .resource_mut::<MapGrid>()
        .set_with_blocking(IVec2::new(2, 2), target, false);
    app.update(); // The independent target produces a footprint.
    assert_eq!(
        app.world().get::<AttackCycle>(source).unwrap().phase,
        AttackPhase::Ready
    );
    app.update(); // The normal token/movement pipeline executes it.
    assert_eq!(app.world().get::<Health>(target).unwrap().current, 3);
    assert_eq!(
        app.world().get::<AttackCycle>(source).unwrap().phase,
        AttackPhase::Recovering(2)
    );
    assert!(app.world().get::<Enemy>(source).is_none());
    assert!(app.world().get::<Player>(source).is_none());
}

#[test]
fn bump_attacks_reuse_grid_wrapping_and_keep_their_front_direction() {
    for (facing, expected_health) in [(None, 1), (Some(IVec2::X), 3), (Some(IVec2::NEG_X), 1)] {
        let mut app = game("p  s\n    ");
        let entity = enemy(&mut app).0;
        if let Some(facing) = facing {
            app.world_mut().entity_mut(entity).insert((
                DirectionalGuard {
                    window: AttackWindow::OutsideRecovery,
                },
                Facing(facing),
            ));
        }
        press(&mut app, KeyCode::ArrowLeft);
        assert_eq!(enemy(&mut app).1.current, expected_health);
        assert_eq!(player(&mut app).0, IVec2::ZERO); // Occupancy still blocks the wrapped move.
    }
}
