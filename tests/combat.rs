//! Combat through the real `GamePlugin` on small maps: picking a target,
//! the three maneuvers, slams, critical hits, telegraphed strikes and
//! friendly fire.

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
use pav_ecs_game_bevy_port::model::*;
use pav_ecs_game_bevy_port::navigation::PathService;
use pav_ecs_game_bevy_port::service::{Runner, ServiceSet, Services};
use std::time::Duration;

const UP: KeyCode = KeyCode::ArrowUp;
const DOWN: KeyCode = KeyCode::ArrowDown;
const LEFT: KeyCode = KeyCode::ArrowLeft;
const RIGHT: KeyCode = KeyCode::ArrowRight;

struct Game {
    app: App,
}

impl Game {
    fn new(map: &'static str) -> Self {
        Self::with_hp(map, 2)
    }

    fn with_hp(map: &'static str, player_hp: i32) -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Services::new(ServiceSet {
                paths: PathService::with_runner(Runner::Inline),
            }))
            .insert_resource(CombatRules {
                player_hp,
                ..default()
            })
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .insert_resource(MapText(map))
            .add_plugins(GamePlugin);
        let mut game = Self { app };
        game.app.update();
        game.frames(5);
        game
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.app.update();
        }
    }

    /// Presses `key` for one frame, then lets it go for one.
    fn press(&mut self, key: KeyCode) {
        self.app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        self.app.update();
        self.app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        self.frames(2);
    }

    /// Frames until the next turn starts and its moves settle.
    fn next_turn(&mut self) {
        let start = self.timer();
        let mut last = start;
        for _ in 0..200 {
            self.app.update();
            let now = self.timer();
            if now < last {
                self.frames(3);
                return;
            }
            last = now;
        }
        panic!("no turn within 200 frames");
    }

    fn timer(&self) -> Duration {
        self.app.world().resource::<TokenTimer>().0.elapsed()
    }

    fn player(&mut self) -> (Entity, IVec2, Health, i32, Option<Selection>) {
        let world = self.app.world_mut();
        let (entity, pos, health, tokens, selection) = world
            .query_filtered::<(Entity, &Pos, &Health, &Tokens, Option<&Selection>), With<Player>>()
            .single(world)
            .unwrap();
        (entity, pos.0, *health, tokens.count, selection.copied())
    }

    fn player_pos(&mut self) -> IVec2 {
        self.player().1
    }

    fn player_hp(&mut self) -> i32 {
        self.player().2.hp
    }

    /// The enemy standing at `at`, if any, with its hit points and whether
    /// it has a strike wound up (and toward where).
    fn enemy_at(&mut self, at: IVec2) -> Option<(Entity, i32, Option<IVec2>)> {
        let world = self.app.world_mut();
        world
            .query_filtered::<(Entity, &Pos, &Health, Option<&Telegraph>), With<Enemy>>()
            .iter(world)
            .find(|(_, pos, ..)| pos.0 == at)
            .map(|(entity, _, health, telegraph)| (entity, health.hp, telegraph.map(|t| t.dir)))
    }

    fn enemies(&mut self) -> usize {
        let world = self.app.world_mut();
        world
            .query_filtered::<(), (With<Enemy>, Without<DestroyRequested>)>()
            .iter(world)
            .count()
    }

    /// Turns until every enemy has noticed the player and is past its
    /// alert beat (no longer exposed), but no further.
    fn until_aware(&mut self) {
        for _ in 0..10 {
            let world = self.app.world_mut();
            let exposed = world
                .query::<&Exposed>()
                .iter(world)
                .any(|exposed| exposed.0);
            if !exposed {
                return;
            }
            self.next_turn();
        }
        panic!("enemies never noticed the player");
    }
}

/// An enemy right of the player, room to push it any way.
const OPEN: &str = "\
XXXXXXXXX
X.......X
X.......X
X..pe...X
X.......X
X.......X
XXXXXXXXX
";

#[test]
fn moving_into_an_enemy_picks_it_and_costs_nothing() {
    let mut game = Game::new(OPEN);
    let before = game.player_pos();
    game.press(RIGHT);
    let (_, pos, _, tokens, selection) = game.player();
    assert_eq!(pos, before, "bumped instead of picking");
    let selection = selection.expect("picked");
    assert_eq!(selection.dir, IVec2::X);
    assert_eq!(tokens, 1, "picking spent the turn");
}

#[test]
fn a_selection_carries_over_turns_and_a_cancel_drops_it() {
    let mut game = Game::with_hp(OPEN, 100);
    game.press(RIGHT);
    game.next_turn();
    game.next_turn();
    assert!(game.player().4.is_some(), "the selection lapsed");
    let tokens = game.player().3;
    game.press(KeyCode::KeyC);
    let (_, _, _, after, selection) = game.player();
    assert_eq!(selection, None);
    assert_eq!(after, tokens, "cancelling spent the turn");
}

#[test]
fn a_lunge_pushes_the_target_on_and_takes_its_cell() {
    let mut game = Game::with_hp(OPEN, 100);
    game.until_aware();
    let (player, enemy) = (IVec2::new(3, 3), IVec2::new(4, 3));
    game.press(RIGHT);
    game.press(RIGHT);
    assert_eq!(game.player_pos(), enemy);
    let (_, hp, _) = game.enemy_at(enemy + IVec2::X).expect("pushed on");
    assert_eq!(hp, 1);
    assert!(game.enemy_at(player).is_none());
}

#[test]
fn a_push_moves_the_target_aside_and_the_player_stays() {
    let mut game = Game::with_hp(OPEN, 100);
    game.until_aware();
    game.press(RIGHT);
    game.press(DOWN);
    assert_eq!(game.player_pos(), IVec2::new(3, 3));
    let (_, hp, _) = game.enemy_at(IVec2::new(4, 4)).expect("pushed aside");
    assert_eq!(hp, 1);
}

#[test]
fn a_yank_swaps_the_player_and_the_target() {
    let mut game = Game::with_hp(OPEN, 100);
    game.until_aware();
    game.press(RIGHT);
    game.press(LEFT);
    assert_eq!(game.player_pos(), IVec2::new(4, 3));
    let (_, hp, _) = game.enemy_at(IVec2::new(3, 3)).expect("yanked");
    assert_eq!(hp, 1);
}

#[test]
fn holding_the_key_that_picked_does_not_lunge() {
    let mut game = Game::with_hp(OPEN, 100);
    game.until_aware();
    game.app
        .world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(RIGHT);
    // Held through several turns, the frames between them included.
    game.frames(200);
    assert_eq!(game.player_pos(), IVec2::new(3, 3));
    assert!(game.player().4.is_some(), "the pick lapsed");
    assert_eq!(game.enemy_at(IVec2::new(4, 3)).unwrap().1, 2, "attacked");
}

#[test]
fn a_critical_hit_downs_an_enemy_on_its_alert_beat() {
    let mut game = Game::with_hp(OPEN, 100);
    // It has just noticed the player: frozen, `!`, and off guard.
    let world = game.app.world_mut();
    let exposed = world.query::<&Exposed>().single(world).unwrap().0;
    assert!(exposed, "not caught off guard on its alert beat");
    game.press(RIGHT);
    game.press(DOWN);
    game.frames(2);
    assert_eq!(game.enemies(), 0, "a critical hit should down it");
}

#[test]
fn a_slam_into_a_wall_adds_damage() {
    let mut game = Game::with_hp(
        "\
XXXXX
X...X
X.peX
X...X
XXXXX
",
        100,
    );
    game.until_aware();
    game.press(RIGHT);
    game.press(RIGHT);
    game.frames(2);
    assert_eq!(game.enemies(), 0, "1 + 1 for the slam should down it");
    assert_eq!(
        game.player_pos(),
        IVec2::new(2, 2),
        "a blocked lunge stays put"
    );
}

#[test]
fn a_slam_into_another_enemy_hurts_both() {
    let mut game = Game::with_hp(
        "\
XXXXXXX
X.....X
X.pee.X
X.....X
XXXXXXX
",
        100,
    );
    // On their alert beat, while neither moves: the near one is caught off
    // guard (2) and slammed (1), the far one only slammed.
    game.press(RIGHT);
    game.press(RIGHT);
    game.frames(2);
    assert!(
        game.enemy_at(IVec2::new(3, 2)).is_none(),
        "2 + 1 for the slam"
    );
    let (_, far, _) = game.enemy_at(IVec2::new(4, 2)).expect("stays");
    assert_eq!(far, 1, "what it was slammed into is hurt too");
    assert_eq!(game.player_pos(), IVec2::new(2, 2));
}

#[test]
fn an_enemy_next_to_the_player_winds_up_then_strikes() {
    let mut game = Game::new(OPEN);
    let enemy = IVec2::new(4, 3);
    for _ in 0..6 {
        if game.enemy_at(enemy).unwrap().2.is_some() {
            break;
        }
        game.next_turn();
    }
    let (_, _, telegraph) = game.enemy_at(enemy).unwrap();
    assert_eq!(
        telegraph,
        Some(IVec2::NEG_X),
        "no wind-up toward the player"
    );
    let marks = game.app.world().resource::<CellMarks>();
    assert!(marks.0.iter().any(|(cell, _)| *cell == IVec2::new(3, 3)));
    game.next_turn();
    assert_eq!(game.player_hp(), 1, "the strike missed");
}

#[test]
fn stepping_off_a_marked_cell_dodges_the_strike() {
    let mut game = Game::new(OPEN);
    let enemy = IVec2::new(4, 3);
    for _ in 0..6 {
        if game.enemy_at(enemy).unwrap().2.is_some() {
            break;
        }
        game.next_turn();
    }
    game.press(UP);
    game.next_turn();
    assert_eq!(game.player_hp(), 2, "hit after stepping away");
}

#[test]
fn a_pushed_strikers_blow_moves_with_it_and_hits_its_friend() {
    // `a` strikes down at the player, `b` stands beside the player. Pushing
    // `a` right puts `b` under its strike.
    let mut game = Game::with_hp(
        "\
XXXXXXX
X.....X
X..e..X
X..pe.X
X.....X
XXXXXXX
",
        100,
    );
    let (a, b) = (IVec2::new(3, 2), IVec2::new(4, 3));
    for _ in 0..6 {
        if game.enemy_at(a).unwrap().2.is_some() {
            break;
        }
        game.next_turn();
    }
    assert_eq!(game.enemy_at(a).unwrap().2, Some(IVec2::Y));
    let b_before = game.enemy_at(b).unwrap().1;
    game.press(UP);
    game.press(RIGHT);
    // The push spends the player's turn, so the next one starts and the
    // strike lands, from where `a` stands now.
    game.frames(5);
    let (_, a_hp, _) = game.enemy_at(IVec2::new(4, 2)).expect("pushed right");
    assert_eq!(a_hp, 1);
    let b_after = game.enemy_at(b).map_or(0, |(_, hp, _)| hp);
    assert_eq!(b_after, b_before - 1, "its friend was not hit");
}

#[test]
fn a_defeated_player_plays_the_map_again() {
    let mut game = Game::with_hp(OPEN, 1);
    let first = game.enemy_at(IVec2::new(4, 3)).unwrap().0;
    // Standing still next to it: struck down within a few turns.
    for _ in 0..8 {
        game.next_turn();
    }
    game.frames(120);
    let again = game.enemy_at(IVec2::new(4, 3)).map(|(enemy, ..)| enemy);
    assert!(
        again.is_some_and(|again| again != first),
        "the map was not played again"
    );
    let (_, pos, health, ..) = game.player();
    assert_eq!(health.hp, health.max);
    assert_eq!(pos, IVec2::new(3, 3));
}
