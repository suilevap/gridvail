//! Combat through the real `GamePlugin` on small maps: moves that meet
//! something attack it (lunge, sweeps, backing off), plain walks, impacts,
//! critical hits, wound-up strikes, strike patterns and friendly fire. The
//! player starts facing north. Actors are the bundled kinds: `e` a guard
//! with a sweep (the row in front), `s` a spearman (two cells ahead).

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::app::{ActorKind, ActorKinds, GamePlugin, MapText};
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

    /// The map with the kinds of actor changed by `setup` first. The
    /// player's attacks are made at once, not readied first (the tests of
    /// readying use `readying`).
    fn with(map: &'static str, setup: impl FnOnce(&mut ActorKinds)) -> Self {
        Self::readying(map, |kinds| {
            kinds.player_mut().readies = None;
            setup(kinds);
        })
    }

    /// The map with the kinds of actor changed by `setup` first, the
    /// player's attacks readied as the bundled player's are.
    fn readying(map: &'static str, setup: impl FnOnce(&mut ActorKinds)) -> Self {
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

    /// The attack the player has readied, if any.
    fn readied(&mut self) -> Option<Attack> {
        let world = self.app.world_mut();
        world
            .query_filtered::<&Readied, With<Player>>()
            .iter(world)
            .next()
            .map(|readied| readied.attack)
    }

    fn marked(&self) -> Vec<IVec2> {
        let marks = self.app.world().resource::<CellMarks>();
        marks.0.iter().map(|(cell, _)| *cell).collect()
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

    fn player(&mut self) -> (IVec2, Health, i32, IVec2) {
        let world = self.app.world_mut();
        let (pos, health, tokens, facing) = world
            .query_filtered::<(&Pos, &Health, &Tokens, &Facing), With<Player>>()
            .single(world)
            .unwrap();
        (pos.0, *health, tokens.count, facing.0)
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
    kinds.by_glyph.get_mut(&'e').unwrap().weapon = jab;
}

/// An enemy right of the player (to its right as it faces north), room to
/// go any way.
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

/// An enemy right ahead of the player.
const AHEAD: &str = "\
XXXXXXXXX
X.......X
X.......X
X...e...X
X...p...X
X.......X
XXXXXXXXX
";
const BEHIND: IVec2 = IVec2::new(4, 4);
const FRONT: IVec2 = IVec2::new(4, 3);

#[test]
fn a_walk_that_meets_nothing_turns_the_player_to_face_it() {
    let mut game = Game::new(OPEN, 2);
    game.press(LEFT);
    let (pos, _, _, facing) = game.player();
    assert_eq!(pos, PLAYER + IVec2::NEG_X);
    assert_eq!(facing, IVec2::NEG_X);
}

#[test]
fn a_lunge_knocks_what_is_ahead_on_and_takes_its_cell() {
    let mut game = Game::new(AHEAD, 100);
    game.until_aware();
    game.press(UP);
    assert_eq!(game.player_pos(), FRONT);
    // Knocked on by the swing and by the step it carries: two cells.
    let (_, hp, _) = game.enemy_at(FRONT + IVec2::NEG_Y * 2).expect("knocked on");
    assert_eq!(hp, 1);
}

#[test]
fn a_sweep_knocks_what_is_ahead_aside_and_steps_the_player_that_way() {
    let mut game = Game::new(AHEAD, 100);
    game.until_aware();
    game.press(RIGHT);
    let (pos, _, _, facing) = game.player();
    assert_eq!((pos, facing), (BEHIND + IVec2::X, IVec2::X));
    let swept = FRONT + IVec2::X * 2;
    assert_eq!(game.enemy_at(swept).expect("swept right").1, 1);
}

#[test]
fn a_move_toward_something_beside_sweeps_it_out_of_the_way() {
    // Facing north, the enemy east of the player is to its right: swept back
    // round (south) and on the way the player steps (east), it leaves the
    // cell the player steps into.
    let mut game = Game::new(OPEN, 100);
    game.until_aware();
    game.press(RIGHT);
    let (pos, _, _, facing) = game.player();
    assert_eq!((pos, facing), (ENEMY, IVec2::X));
    let swept = ENEMY + IVec2::ONE;
    assert_eq!(game.enemy_at(swept).expect("swept away").1, 1);
}

#[test]
fn backing_off_steps_back_still_facing_what_is_ahead() {
    let mut game = Game::new(AHEAD, 100);
    game.until_aware();
    game.press(DOWN);
    let (pos, _, _, facing) = game.player();
    assert_eq!((pos, facing), (BEHIND + IVec2::Y, IVec2::NEG_Y));
    assert_eq!(game.enemy_at(FRONT).expect("left alone").1, 2);
}

#[test]
fn a_critical_hit_downs_an_enemy_on_its_alert_beat() {
    let mut game = Game::new(AHEAD, 100);
    let world = game.app.world_mut();
    let exposed = world.query::<&Exposed>().single(world).unwrap().0;
    assert!(exposed, "not caught off guard on its alert beat");
    game.press(RIGHT);
    game.frames(2);
    assert_eq!(game.enemies(), 0, "a critical hit should down it");
}

#[test]
fn an_enemy_knocked_into_a_wall_takes_the_impact() {
    let mut game = Game::new("XXXXX\nX.e.X\nX.p.X\nX...X\nXXXXX\n", 100);
    game.until_aware();
    game.press(UP);
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
    let mut game = Game::with("XXXXX\nX...X\nX.e.X\nX.e.X\nX.p.X\nXXXXX\n", |kinds| {
        kinds.player_mut().hp = 100;
        kinds.by_glyph.get_mut(&'e').unwrap().hp = 4;
    });
    // On their alert beat, while neither moves: the near one is caught off
    // guard (1 + 1) and takes the impact (1), the far one the impact.
    game.press(UP);
    game.frames(4);
    assert_eq!(game.enemy_at(IVec2::new(2, 3)).expect("stays").1, 1);
    assert_eq!(game.enemy_at(IVec2::new(2, 2)).expect("stays").1, 3);
    assert_eq!(
        game.player_pos(),
        IVec2::new(2, 4),
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
/// them after they step `key` (the player faces north, away from it, so
/// stepping up or left is a plain walk).
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

/// Whether the player is hit when answering a guard's wound-up sweep, from
/// right ahead, with the move `key`.
fn hit_after(key: KeyCode) -> bool {
    let mut game = Game::new(AHEAD, 100);
    game.until_wound_up(FRONT);
    let before = game.player_hp();
    game.press(key);
    game.frames(5);
    game.player_hp() < before
}

#[test]
fn against_a_sweep_lunging_or_backing_off_is_safe() {
    // A lunge knocks the guard two cells on, so its sweep covers the row
    // ahead of the player; a sweep steps the player along the row the
    // guard's sweep covers; backing off leaves it.
    assert!(!hit_after(UP), "lunge");
    assert!(hit_after(LEFT), "sweep left");
    assert!(hit_after(RIGHT), "sweep right");
    assert!(!hit_after(DOWN), "back off");
}

#[test]
fn a_knocked_strikers_blow_moves_with_it_and_hits_its_friend() {
    // `a`, ahead of the player, strikes down at it; `b`, a dummy that stays
    // put, stands two cells to the player's right. Sweeping `a` right (two cells: the swing and the
    // step) puts `b` under its strike.
    let mut game = Game::with(
        "\
XXXXXXX
X.....X
X..e..X
X..p.dX
X.....X
XXXXXXX
",
        |kinds| {
            jabbing_guards(kinds);
            kinds.player_mut().hp = 100;
            let dummy = ActorKind {
                tree: None,
                weapon: None,
                ..kinds.get('e').clone()
            };
            kinds.by_glyph.insert('d', dummy);
        },
    );
    let (a, b) = (IVec2::new(3, 2), IVec2::new(5, 3));
    game.until_wound_up(a);
    assert_eq!(game.enemy_at(a).unwrap().2, Some(IVec2::Y));
    let b_before = game.enemy_at(b).unwrap().1;
    game.press(RIGHT);
    // The sweep takes the player's turn, so the next one starts and the
    // strike lands, from where `a` stands now.
    game.next_turn();
    assert_eq!(game.enemy_at(IVec2::new(5, 2)).expect("swept right").1, 1);
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

#[test]
fn a_lethal_blow_still_knocks_its_target_before_it_dies() {
    let mut game = Game::new("XXXXX\nX...X\nX.e.X\nX.e.X\nX.p.X\nXXXXX\n", 100);
    // On its alert beat the near guard is caught off guard: the lunge's
    // critical hit downs it, but it is knocked into the far guard first.
    game.press(UP);
    game.frames(4);
    assert!(
        game.enemy_at(IVec2::new(2, 3)).is_none(),
        "the near one lives"
    );
    assert_eq!(
        game.enemy_at(IVec2::new(2, 2)).expect("stays").1,
        1,
        "the dead body did not hit the far one"
    );
}

/// Frames (of 16ms) the bundled player's readied attack is held for, and a
/// few more.
const READIED_FRAMES: usize = 25;

#[test]
fn a_move_that_would_attack_is_readied_and_shown_then_made() {
    let mut game = Game::readying(AHEAD, |kinds| kinds.player_mut().hp = 100);
    game.until_aware();
    game.press(UP);
    let readied = game.readied().expect("not readied");
    assert_eq!(readied.relation, Relation::Forward);
    assert_eq!(game.player_pos(), BEHIND, "made before it was shown");
    assert_eq!(game.enemy_at(FRONT).expect("hit too soon").1, 2);
    assert!(
        game.marked().contains(&FRONT),
        "what it strikes is not shown"
    );
    game.frames(READIED_FRAMES);
    assert!(game.readied().is_none());
    assert_eq!(game.player_pos(), FRONT, "never made");
    assert_eq!(
        game.enemy_at(FRONT + IVec2::NEG_Y * 2).expect("knocked").1,
        1
    );
}

#[test]
fn a_cancel_drops_the_readied_attack_and_takes_no_turn() {
    let mut game = Game::readying(AHEAD, |kinds| kinds.player_mut().hp = 100);
    game.until_aware();
    game.press(UP);
    assert!(game.readied().is_some());
    game.press(KeyCode::Space);
    assert!(game.readied().is_none(), "not dropped");
    game.frames(READIED_FRAMES);
    let (pos, _, tokens, _) = game.player();
    assert_eq!((pos, tokens), (BEHIND, 1), "the dropped move was made");
    assert_eq!(game.enemy_at(FRONT).expect("left alone").1, 2);
}

#[test]
fn the_same_move_again_makes_the_readied_attack_at_once() {
    let mut game = Game::readying(AHEAD, |kinds| kinds.player_mut().hp = 100);
    game.until_aware();
    game.press(UP);
    game.press(UP);
    assert_eq!(game.player_pos(), FRONT, "not made at once");
    assert_eq!(
        game.enemy_at(FRONT + IVec2::NEG_Y * 2).expect("knocked").1,
        1
    );
}

#[test]
fn another_move_takes_the_readied_attacks_place() {
    let mut game = Game::readying(AHEAD, |kinds| kinds.player_mut().hp = 100);
    game.until_aware();
    game.press(UP);
    // Backing off strikes nothing, so it is made at once, in place of the
    // lunge.
    game.press(DOWN);
    assert!(game.readied().is_none());
    game.frames(READIED_FRAMES);
    assert_eq!(game.player_pos(), BEHIND + IVec2::Y);
    assert_eq!(game.enemy_at(FRONT).expect("left alone").1, 2);
}
