//! Combat through the real `GamePlugin` on small maps: picking a target,
//! the three maneuvers, impacts, critical hits, wound-up strikes, strike
//! patterns and friendly fire. Actors are the bundled roster's: `e` a guard
//! with a sweep (the row in front), `s` a spearman (two cells ahead).

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::app::{ActorKinds, GamePlugin, MapText};
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
    /// The map with the player at `player_hp`.
    fn new(map: &'static str, player_hp: i32) -> Self {
        Self::with(map, |kinds| kinds.player_mut().hp = player_hp)
    }

    /// The map with the roster changed by `setup` first.
    fn with(map: &'static str, setup: impl FnOnce(&mut ActorKinds)) -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Services::new(ServiceSet {
                paths: PathService::with_runner(Runner::Inline),
            }))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .insert_resource(MapText(map))
            .add_plugins(GamePlugin);
        setup(&mut app.world_mut().resource_mut::<ActorKinds>());
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

    /// Presses `key` for one frame, then lets it go for two.
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

    /// Frames until the next turn starts and settles.
    fn next_turn(&mut self) {
        let mut last = self.timer();
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

    fn player(&mut self) -> (IVec2, Health, i32, Option<Selected>) {
        let world = self.app.world_mut();
        let (pos, health, tokens, selected) = world
            .query_filtered::<(&Pos, &Health, &Tokens, Option<&Selected>), With<Player>>()
            .single(world)
            .unwrap();
        (pos.0, *health, tokens.count, selected.copied())
    }

    fn player_pos(&mut self) -> IVec2 {
        self.player().0
    }

    fn player_hp(&mut self) -> i32 {
        self.player().1.hp
    }

    /// The enemy at `at`: its hit points and where its strike is aimed.
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
    /// alert beat (no longer exposed).
    fn until_aware(&mut self) {
        for _ in 0..10 {
            let world = self.app.world_mut();
            if !world
                .query::<&Exposed>()
                .iter(world)
                .any(|exposed| exposed.0)
            {
                return;
            }
            self.next_turn();
        }
        panic!("enemies never noticed the player");
    }

    /// Turns until the enemy at `at` has a strike wound up.
    fn until_wound_up(&mut self, at: IVec2) {
        for _ in 0..10 {
            if self.enemy_at(at).is_some_and(|(.., aimed)| aimed.is_some()) {
                return;
            }
            self.next_turn();
        }
        panic!("the enemy at {at} never wound up");
    }
}

/// Gives the guard `e` the hunter's jab (the cell in front) for tests about
/// one struck cell.
fn jabbing_guards(kinds: &mut ActorKinds) {
    let jab = kinds.get('h').weapon.clone();
    kinds.0.get_mut(&'e').unwrap().weapon = jab;
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
const PLAYER: IVec2 = IVec2::new(3, 3);
const ENEMY: IVec2 = IVec2::new(4, 3);

#[test]
fn moving_into_an_enemy_picks_it_and_costs_nothing() {
    let mut game = Game::new(OPEN, 2);
    game.press(RIGHT);
    let (pos, _, tokens, selected) = game.player();
    assert_eq!(pos, PLAYER, "bumped instead of picking");
    assert_eq!(selected.expect("picked").dir, IVec2::X);
    assert_eq!(tokens, 1, "picking spent the turn");
}

#[test]
fn a_pick_carries_over_turns_and_a_cancel_drops_it() {
    let mut game = Game::new(OPEN, 100);
    game.press(RIGHT);
    game.next_turn();
    game.next_turn();
    assert!(game.player().3.is_some(), "the pick lapsed");
    let tokens = game.player().2;
    game.press(KeyCode::KeyC);
    let (_, _, after, selected) = game.player();
    assert_eq!(selected, None);
    assert_eq!(after, tokens, "cancelling spent the turn");
}

#[test]
fn a_lunge_knocks_the_target_on_and_takes_its_cell() {
    let mut game = Game::new(OPEN, 100);
    game.until_aware();
    game.press(RIGHT);
    game.press(RIGHT);
    assert_eq!(game.player_pos(), ENEMY);
    let (_, hp, _) = game.enemy_at(ENEMY + IVec2::X).expect("knocked on");
    assert_eq!(hp, 1);
}

#[test]
fn a_push_knocks_the_target_aside_and_the_player_stays() {
    let mut game = Game::new(OPEN, 100);
    game.until_aware();
    game.press(RIGHT);
    game.press(DOWN);
    assert_eq!(game.player_pos(), PLAYER);
    assert_eq!(game.enemy_at(ENEMY + IVec2::Y).expect("pushed").1, 1);
}

#[test]
fn a_yank_swaps_the_player_and_the_target() {
    let mut game = Game::new(OPEN, 100);
    game.until_aware();
    game.press(RIGHT);
    game.press(LEFT);
    assert_eq!(game.player_pos(), ENEMY);
    assert_eq!(game.enemy_at(PLAYER).expect("yanked").1, 1);
}

#[test]
fn holding_the_key_that_picked_does_not_lunge() {
    let mut game = Game::new(OPEN, 100);
    game.until_aware();
    game.app
        .world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(RIGHT);
    // Held through several turns, the frames between them included.
    game.frames(200);
    assert_eq!(game.player_pos(), PLAYER);
    assert!(game.player().3.is_some(), "the pick lapsed");
    assert_eq!(game.enemy_at(ENEMY).unwrap().1, 2, "attacked");
}

#[test]
fn a_critical_hit_downs_an_enemy_on_its_alert_beat() {
    let mut game = Game::new(OPEN, 100);
    let world = game.app.world_mut();
    let exposed = world.query::<&Exposed>().single(world).unwrap().0;
    assert!(exposed, "not caught off guard on its alert beat");
    game.press(RIGHT);
    game.press(DOWN);
    game.frames(2);
    assert_eq!(game.enemies(), 0, "a critical hit should down it");
}

#[test]
fn an_enemy_knocked_into_a_wall_takes_the_impact() {
    let mut game = Game::new("XXXXX\nX...X\nX.peX\nX...X\nXXXXX\n", 100);
    game.until_aware();
    game.press(RIGHT);
    game.press(RIGHT);
    game.frames(2);
    assert_eq!(game.enemies(), 0, "1 + 1 for the impact should down it");
    assert_eq!(
        game.player_pos(),
        IVec2::new(2, 2),
        "a blocked lunge stays put"
    );
}

#[test]
fn an_enemy_knocked_into_another_hurts_both() {
    // Guards tough enough to live through the hit, so the knocked one moves
    // and runs into the other.
    let mut game = Game::with("XXXXXXX\nX.....X\nX.pee.X\nX.....X\nXXXXXXX\n", |kinds| {
        kinds.player_mut().hp = 100;
        kinds.0.get_mut(&'e').unwrap().hp = 4;
    });
    // On their alert beat, while neither moves: the near one is caught off
    // guard (1 + 1) and takes the impact (1), the far one the impact.
    game.press(RIGHT);
    game.press(RIGHT);
    game.frames(4);
    assert_eq!(game.enemy_at(IVec2::new(3, 2)).expect("stays").1, 1);
    assert_eq!(game.enemy_at(IVec2::new(4, 2)).expect("stays").1, 3);
    assert_eq!(
        game.player_pos(),
        IVec2::new(2, 2),
        "a blocked lunge stays put"
    );
}

#[test]
fn a_wound_up_strike_is_marked_then_lands() {
    let mut game = Game::with(OPEN, jabbing_guards);
    game.until_wound_up(ENEMY);
    assert_eq!(game.enemy_at(ENEMY).unwrap().2, Some(IVec2::NEG_X));
    let marks = game.app.world().resource::<CellMarks>();
    assert!(marks.0.iter().any(|(cell, _)| *cell == PLAYER));
    game.next_turn();
    assert_eq!(game.player_hp(), 1, "the strike missed");
}

#[test]
fn stepping_off_a_jab_dodges_it() {
    let mut game = Game::with(OPEN, jabbing_guards);
    game.until_wound_up(ENEMY);
    game.press(UP);
    game.next_turn();
    assert_eq!(game.player_hp(), 2);
}

/// Whether a wound-up enemy of kind `glyph` right of the player still hits
/// them after they step `key`.
fn dodge(glyph: char, key: KeyCode) -> bool {
    let map: &'static str = Box::leak(OPEN.replace('e', &glyph.to_string()).into_boxed_str());
    let mut game = Game::new(map, 100);
    game.until_wound_up(ENEMY);
    let before = game.player_hp();
    game.press(key);
    game.next_turn();
    game.player_hp() < before
}

#[test]
fn a_sweep_cannot_be_sidestepped_but_can_be_backed_away_from() {
    assert!(dodge('e', UP), "stepping aside escaped the sweep");
    assert!(!dodge('e', LEFT), "stepping back did not escape the sweep");
}

#[test]
fn a_spear_cannot_be_backed_away_from_but_can_be_sidestepped() {
    assert!(dodge('s', LEFT), "stepping back escaped the spear");
    assert!(!dodge('s', UP), "stepping aside did not escape the spear");
}

/// Whether the player is hit when answering a guard's wound-up sweep with
/// the maneuver `second`.
fn hit_after(second: KeyCode) -> bool {
    let mut game = Game::new(OPEN, 100);
    game.until_wound_up(ENEMY);
    let before = game.player_hp();
    game.press(RIGHT);
    game.press(second);
    game.frames(5);
    game.player_hp() < before
}

#[test]
fn against_a_sweep_only_a_yank_is_safe() {
    // A lunge or a push leaves the player in the row the sweep covers once
    // the guard is knocked; a yank turns its sweep away.
    assert!(hit_after(RIGHT), "lunge");
    assert!(hit_after(DOWN), "push");
    assert!(!hit_after(LEFT), "yank");
}

#[test]
fn a_knocked_strikers_blow_moves_with_it_and_hits_its_friend() {
    // `a` strikes down at the player, `b` stands beside the player. Pushing
    // `a` right puts `b` under its strike.
    let mut game = Game::with(
        "\
XXXXXXX
X.....X
X..e..X
X..pe.X
X.....X
XXXXXXX
",
        |kinds| {
            jabbing_guards(kinds);
            kinds.player_mut().hp = 100;
        },
    );
    let (a, b) = (IVec2::new(3, 2), IVec2::new(4, 3));
    game.until_wound_up(a);
    assert_eq!(game.enemy_at(a).unwrap().2, Some(IVec2::Y));
    let b_before = game.enemy_at(b).unwrap().1;
    game.press(UP);
    game.press(RIGHT);
    // The push takes the player's turn, so the next one starts and the
    // strike lands, from where `a` stands now.
    game.frames(5);
    assert_eq!(game.enemy_at(IVec2::new(4, 2)).expect("pushed right").1, 1);
    let b_after = game.enemy_at(b).map_or(0, |(_, hp, _)| hp);
    assert_eq!(b_after, b_before - 1, "its friend was not hit");
}

#[test]
fn a_dead_player_plays_the_map_again() {
    let mut game = Game::new(OPEN, 1);
    let first = game.enemy_at(ENEMY).unwrap().0;
    for _ in 0..8 {
        game.next_turn();
    }
    game.frames(120);
    let again = game.enemy_at(ENEMY).map(|(enemy, ..)| enemy);
    assert!(
        again.is_some_and(|again| again != first),
        "the map was not played again"
    );
    let (pos, health, ..) = game.player();
    assert_eq!(health.hp, health.max);
    assert_eq!(pos, PLAYER);
}

// --- feedback: what the player is shown ---------------------------------

/// The colour the frame's marks give `cell`, if any (the last mark wins).
fn mark_at(game: &Game, cell: IVec2) -> Option<Rgb> {
    let marks = game.app.world().resource::<CellMarks>();
    marks
        .0
        .iter()
        .rev()
        .find(|(at, _)| *at == cell)
        .map(|(_, mark)| mark.tint)
}

#[test]
fn a_pick_shows_what_each_direction_would_do() {
    use pav_ecs_game_bevy_port::lighting::{CYAN, RED};
    let mut game = Game::new(OPEN, 100);
    game.until_wound_up(ENEMY);
    game.press(RIGHT);
    assert!(game.player().3.is_some(), "not picked");
    // The guard's sweep covers the row in front of it (the player's
    // column): a lunge or a push leaves the player in it, a yank does not.
    assert_eq!(mark_at(&game, ENEMY + IVec2::X), Some(RED), "lunge");
    assert_eq!(mark_at(&game, ENEMY + IVec2::Y), Some(RED), "push down");
    assert_eq!(mark_at(&game, ENEMY + IVec2::NEG_Y), Some(RED), "push up");
    assert_eq!(mark_at(&game, PLAYER), Some(CYAN), "yank");
    // What the marks say is what happens.
    assert!(hit_after(DOWN), "push");
    assert!(!hit_after(LEFT), "yank");
}

#[test]
fn a_blow_that_would_down_the_target_shows_green() {
    use pav_ecs_game_bevy_port::lighting::GREEN;
    let mut game = Game::new(OPEN, 100);
    // On its alert beat a hit is critical: any move downs it.
    game.press(RIGHT);
    assert_eq!(mark_at(&game, ENEMY + IVec2::Y), Some(GREEN));
}

#[test]
fn a_hit_pops_up_its_damage_above_the_target() {
    let mut game = Game::new(OPEN, 100);
    game.until_aware();
    game.press(RIGHT);
    game.press(DOWN);
    let world = game.app.world_mut();
    let popups: Vec<(IVec2, char)> = world
        .query_filtered::<(&Pos, &Glyph), With<pav_ecs_game_bevy_port::feedback::Popup>>()
        .iter(world)
        .map(|(pos, glyph)| (pos.0, glyph.ch))
        .collect();
    // Pushed down to (4, 4): its 1 damage shows above it. (The guard's
    // sweep then catches the player too: the push was the unsafe answer.)
    assert!(popups.contains(&(IVec2::new(4, 3), '1')), "{popups:?}");
}
