//! Enemy behavior-tree gameplay, played through the real `GamePlugin` on small
//! maps: real turns, tokens, line of sight, and collision resolution.
//!
//! Each scenario records one snapshot per turn. Set `ENEMY_TRACE=1` to print
//! them as maps: `!` alert, `H` hunting, `A` attacking, `-` holding, `S`
//! searching, `p` patrolling, `z` resting, `g` going to an order, `@` the
//! player, `?` waiting for a path, `k` a key, `+`/`'` a closed/open door.
//! Walking acts (hunt,
//! search, patrol, go) are carried out by locomotion along planned paths.

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::ai::PortalPolicy;
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
use pav_ecs_game_bevy_port::lighting::YELLOW;
use pav_ecs_game_bevy_port::model::*;
use pav_ecs_game_bevy_port::navigation::{NavMap, PathService};
use pav_ecs_game_bevy_port::service::{Runner, ServiceSet, Services};
use std::time::Duration;

/// An enemy's act as the scenarios check it: a walk by its mood and where
/// its path leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Alert,
    Attack(IVec2),
    Hold,
    Rest,
    /// Waiting for a path.
    Think(Mood),
    Hunt(Option<IVec2>),
    Search(Option<IVec2>),
    Patrol(Option<IVec2>),
    Order(Option<IVec2>),
    Fetch(Option<IVec2>),
}

impl From<&EnemyAct> for Act {
    fn from(act: &EnemyAct) -> Self {
        match act {
            EnemyAct::Alert => Act::Alert,
            EnemyAct::Attack(step) => Act::Attack(*step),
            EnemyAct::Hold => Act::Hold,
            EnemyAct::Rest => Act::Rest,
            EnemyAct::Think(mood) => Act::Think(*mood),
            EnemyAct::Move(Mood::Hunt, path) => Act::Hunt(path.end()),
            EnemyAct::Move(Mood::Search, path) => Act::Search(path.end()),
            EnemyAct::Move(Mood::Patrol, path) => Act::Patrol(path.end()),
            EnemyAct::Move(Mood::Order, path) => Act::Order(path.end()),
            EnemyAct::Move(Mood::Fetch, path) => Act::Fetch(path.end()),
        }
    }
}

#[derive(Clone, Debug)]
struct Snapshot {
    player: IVec2,
    enemies: Vec<(Entity, IVec2, Option<Act>)>,
    doors: Vec<(IVec2, bool)>,
    keys: Vec<IVec2>,
}

struct Game {
    app: App,
    map: &'static str,
    trace: Vec<Snapshot>,
}

impl Game {
    fn new(map: &'static str) -> Self {
        Self::with_paths(map, Runner::Inline)
    }

    /// A game whose paths are planned as `runner` says.
    fn with_paths(map: &'static str, runner: Runner) -> Self {
        Self::with_policy(map, runner, PortalPolicy::default())
    }

    /// A game whose enemies use portals as `portals` says.
    fn with_policy(map: &'static str, runner: Runner, portals: PortalPolicy) -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(portals)
            .insert_resource(Services::new(ServiceSet {
                paths: PathService::with_runner(runner),
            }))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .insert_resource(MapText(map))
            .add_plugins(GamePlugin);
        let mut game = Self {
            app,
            map,
            trace: Vec::new(),
        };
        // Startup, then the first turn: tokens start empty and recharge at once.
        game.app.update();
        game.settle();
        game.record();
        game
    }

    /// Plays one turn: an optional player key, then frames until tokens
    /// recharge (enemies decide on that frame) and movement settles.
    fn turn(&mut self, key: Option<KeyCode>) -> &Snapshot {
        if let Some(key) = key {
            let mut input = self.app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            input.press(key);
            self.app.update();
            self.app
                .world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
        }
        let mut elapsed = self.timer();
        for _ in 0..200 {
            self.app.update();
            let now = self.timer();
            if now < elapsed {
                self.settle();
                self.record();
                return self.trace.last().unwrap();
            }
            elapsed = now;
        }
        panic!("no turn within 200 frames");
    }

    fn turns(&mut self, n: usize) {
        for _ in 0..n {
            self.turn(None);
        }
    }

    fn timer(&self) -> Duration {
        self.app.world().resource::<TokenTimer>().0.elapsed()
    }

    /// Frames until movement is done and no enemy still holds its turn open
    /// to think (it acts within a few frames, or waits the turn out).
    fn settle(&mut self) {
        for _ in 0..50 {
            let world = self.app.world_mut();
            let thinking = world
                .query::<(&Tokens, &EnemyAct)>()
                .iter(world)
                .any(|(tokens, act)| tokens.count > 0 && matches!(act, EnemyAct::Think(_)));
            if !thinking && !world.resource::<TurnState>().simulation {
                return;
            }
            self.app.update();
        }
        panic!("simulation did not settle");
    }

    fn record(&mut self) {
        let world = self.app.world_mut();
        let player = world
            .query_filtered::<&Pos, With<Player>>()
            .single(world)
            .unwrap()
            .0;
        let mut enemies: Vec<_> = world
            .query_filtered::<(Entity, &Pos, Option<&EnemyAct>), With<Enemy>>()
            .iter(world)
            .map(|(e, p, act)| (e, p.0, act.map(Act::from)))
            .collect();
        enemies.sort_by_key(|(e, ..)| *e);
        let doors = world
            .query::<(&Pos, &Door)>()
            .iter(world)
            .map(|(p, door)| (p.0, door.open))
            .collect();
        let keys = world
            .query_filtered::<&Pos, With<Key>>()
            .iter(world)
            .map(|p| p.0)
            .collect();
        self.check_invariants(&enemies);
        self.trace.push(Snapshot {
            player,
            enemies,
            doors,
            keys,
        });
    }

    /// Holds on every turn of every scenario.
    fn check_invariants(&self, enemies: &[(Entity, IVec2, Option<Act>)]) {
        let grid = self.app.world().resource::<MapGrid>();
        for &(enemy, pos, _) in enemies {
            assert_eq!(grid.get(pos), Some(enemy), "enemy off the occupancy grid");
        }
        if let Some(previous) = self.trace.last() {
            let nav = self.app.world().resource::<NavMap>();
            for (&(e, pos, _), &(pe, ppos, _)) in enemies.iter().zip(&previous.enemies) {
                assert_eq!(e, pe);
                let moved = (pos - ppos).abs();
                // One step a turn, or one step into a portal and out of it.
                let through_portal = nav.step_toward(ppos, pos).is_some();
                assert!(
                    moved.x + moved.y <= 1 || through_portal,
                    "enemy moved {moved} in one turn"
                );
            }
        }
    }

    fn enemy(&self, turn: usize) -> (IVec2, Option<Act>) {
        let (_, pos, act) = self.trace[turn].enemies[0];
        (pos, act)
    }

    fn collided(&mut self, source: Entity, target_is_player: bool) -> bool {
        let world = self.app.world_mut();
        let player = world
            .query_filtered::<Entity, With<Player>>()
            .single(world)
            .unwrap();
        world
            .resource::<CollisionBuffer>()
            .0
            .iter()
            .any(|c| c.source == source && (c.target == player) == target_is_player)
    }

    fn print(&self, title: &str) {
        if std::env::var_os("ENEMY_TRACE").is_none() {
            return;
        }
        eprintln!("=== {title}");
        let rows: Vec<Vec<char>> = self
            .map
            .lines()
            .map(|l| {
                l.chars()
                    .map(|c| if matches!(c, 'X' | 'x') { '#' } else { '.' })
                    .collect()
            })
            .collect();
        for (turn, snap) in self.trace.iter().enumerate() {
            let mut rows = rows.clone();
            let mut put = |p: IVec2, c: char| rows[p.y as usize][p.x as usize] = c;
            for &(door, open) in &snap.doors {
                put(door, if open { '\'' } else { '+' });
            }
            for &key in &snap.keys {
                put(key, 'k');
            }
            put(snap.player, '@');
            let mut acts = Vec::new();
            for &(_, pos, act) in &snap.enemies {
                let glyph = match act {
                    Some(Act::Alert) => '!',
                    Some(Act::Hunt(_)) => 'H',
                    Some(Act::Attack(_)) => 'A',
                    Some(Act::Hold) => '-',
                    Some(Act::Search(_)) => 'S',
                    Some(Act::Patrol(_)) => 'p',
                    Some(Act::Rest) => 'z',
                    Some(Act::Think(_)) => '?',
                    Some(Act::Order(_)) => 'g',
                    Some(Act::Fetch(_)) => 'f',
                    None => 'e',
                };
                put(pos, glyph);
                acts.push(format!("{act:?}"));
            }
            eprintln!("turn {turn}: {}", acts.join(", "));
            for row in rows {
                eprintln!("  {}", row.into_iter().collect::<String>());
            }
        }
    }
}

fn manhattan(a: IVec2, b: IVec2) -> i32 {
    (a - b).abs().element_sum()
}

fn is_idle(act: Option<Act>) -> bool {
    matches!(act, Some(Act::Patrol(_) | Act::Rest))
}

fn acts(game: &Game) -> Vec<Option<Act>> {
    game.trace.iter().map(|s| s.enemies[0].2).collect()
}

#[test]
fn a_sighting_freezes_the_enemy_then_it_chases_and_attacks() {
    let mut game = Game::new(
        "XXXXXXXXXXXX\n\
         Xp......e..X\n\
         XXXXXXXXXXXX\n",
    );
    // Turn 0 is the startup frame: the enemy has no field of view yet, so
    // it notices the player on its first look, on turn 1 at the latest.
    let player = game.trace[0].player;
    while game.enemy(game.trace.len() - 1).1 != Some(Act::Alert) {
        assert!(game.trace.len() < 2, "never alerted");
        game.turn(None);
    }
    let alert = game.trace.len() - 1;
    // The alert beat: standing still, shown as a yellow `!`.
    if alert > 0 {
        assert_eq!(game.enemy(alert).0, game.enemy(alert - 1).0);
    }
    let world = game.app.world_mut();
    let glyph = *world
        .query_filtered::<&Glyph, With<Enemy>>()
        .single(world)
        .unwrap();
    assert_eq!((glyph.ch, glyph.color), ('!', YELLOW));

    game.turns(9);
    game.print("alert, chase, attack");
    // Then one step closer every turn, and bumps once adjacent.
    for turn in alert + 1..game.trace.len() {
        let (pos, act) = game.enemy(turn);
        let before = manhattan(game.trace[turn - 1].enemies[0].1, player);
        let expected = if before == 1 {
            Act::Attack(IVec2::NEG_X)
        } else {
            Act::Hunt(Some(player))
        };
        assert_eq!(act, Some(expected), "turn {turn}");
        assert_eq!(manhattan(pos, player), (before - 1).max(1), "turn {turn}");
    }
    let enemy = game.trace[0].enemies[0].0;
    assert!(game.collided(enemy, true));
}

#[test]
fn an_unaware_enemy_patrols_and_rests() {
    let mut game = Game::new(
        "XXXXXXXXXXXXX\n\
         Xp...X......X\n\
         X....X......X\n\
         X....X...e..X\n\
         X....X......X\n\
         XXXXXXXXXXXXX\n",
    );
    game.turns(30);
    game.print("idle behind a wall");
    let acts = acts(&game);
    assert!(acts.iter().all(|&a| is_idle(a)), "{acts:?}");
    // Strolls are walks to a chosen cell, not a jitter: the same goal lasts
    // several turns, and the enemy gets around.
    let held = acts
        .windows(3)
        .filter(|w| matches!(w[0], Some(Act::Patrol(_))) && w[0] == w[1] && w[1] == w[2])
        .count();
    assert!(held > 0, "no stroll held its goal: {acts:?}");
    let cells: std::collections::HashSet<_> = game.trace.iter().map(|s| s.enemies[0].1).collect();
    assert!(cells.len() >= 4, "barely moved: {cells:?}");
    assert!(acts.contains(&Some(Act::Rest)), "never rested: {acts:?}");
}

#[test]
fn a_distant_player_is_not_noticed() {
    // Ten cells away down an open corridor: beyond the sight radius of 8.
    let game = Game::new(
        "XXXXXXXXXXXXXX\n\
         Xp.........e.X\n\
         XXXXXXXXXXXXXX\n",
    );
    game.print("out of range");
    let (_, act) = game.enemy(0);
    assert!(is_idle(act), "{act:?}");
}

#[test]
fn a_player_ducking_out_of_sight_is_searched_for_then_found() {
    // The player starts in the enemy's row, then steps down behind the wall.
    let mut game = Game::new(
        "XXXXXXXXXX\n\
         X.p....e.X\n\
         X.XXXXXXXX\n\
         X........X\n\
         XXXXXXXXXX\n",
    );
    game.turn(Some(KeyCode::ArrowLeft));
    game.turn(Some(KeyCode::ArrowDown));
    game.turns(8);
    game.print("duck out of sight");

    let acts = acts(&game);
    // Noticed on its first look (turn 0 is startup, before any).
    assert!(acts[..2].contains(&Some(Act::Alert)), "{acts:?}");
    let searched = acts
        .iter()
        .position(|a| matches!(a, Some(Act::Search(_))))
        .expect("never searched");
    // Heading for where the player was last seen, not where it is.
    let Some(Act::Search(Some(seen))) = acts[searched] else {
        unreachable!()
    };
    assert_ne!(seen, game.trace[searched].player);
    let found = acts[searched..]
        .iter()
        .any(|a| matches!(a, Some(Act::Hunt(_) | Act::Attack(_))));
    assert!(found, "never found the player again: {acts:?}");
    // One pursuit, one alert: re-sighting mid-chase does not freeze it again.
    let alerts = acts.iter().filter(|&&a| a == Some(Act::Alert)).count();
    assert_eq!(alerts, 1, "{acts:?}");
}

#[test]
fn a_player_who_gets_away_is_given_up_on() {
    // The player slips down the left shaft and along the bottom corridor,
    // out of any line from the row where it was last seen.
    let mut game = Game::new(
        "XXXXXXXXXXXX\n\
         X.p......e.X\n\
         X.XXXXXXXXXX\n\
         X.XXXXXXXXXX\n\
         X..........X\n\
         XXXXXXXXXXXX\n",
    );
    use KeyCode::{ArrowDown as D, ArrowLeft as L, ArrowRight as R};
    for key in [L, D, D, D, R, R, R, R, R, R, R, R] {
        game.turn(Some(key));
    }
    game.print("gets away");
    let acts = acts(&game);
    let searched = acts
        .iter()
        .position(|a| matches!(a, Some(Act::Search(_))))
        .expect("never searched");
    assert!(
        acts[searched..].iter().any(|&a| is_idle(a)),
        "never gave up: {acts:?}"
    );
}

#[test]
fn chasers_step_around_each_other() {
    let mut game = Game::new(
        "XXXXXXXXXXX\n\
         X....e....X\n\
         X.p.......X\n\
         X....e....X\n\
         X.....e...X\n\
         XXXXXXXXXXX\n",
    );
    game.turns(14);
    game.print("three chasers");
    let last = game.trace.last().unwrap();
    // Nobody queues behind an ally: each steps around and ends up attacking.
    for &(_, pos, act) in &last.enemies {
        assert!(matches!(act, Some(Act::Attack(_))), "{act:?}");
        assert_eq!(manhattan(pos, last.player), 1, "{pos} vs {}", last.player);
    }
}

#[test]
fn the_bundled_map_plays_consistently() {
    let mut game = Game::new(include_str!("../assets/maps/map1.txt"));
    let walk = [KeyCode::ArrowRight, KeyCode::ArrowDown, KeyCode::ArrowRight];
    for turn in 0..60 {
        game.turn(Some(walk[turn % walk.len()]));
    }
    // Invariants checked on every turn; every enemy has a standing act.
    for snap in &game.trace {
        assert!(snap.enemies.iter().all(|(_, _, act)| act.is_some()));
    }
}

// --- slow plans: the enemy thinks, the game goes on ----------------------

/// Planning that takes longer than a turn holds the enemy, not the game. Each
/// plan here lands 20 polls after it is asked for: the enemy thinks (`?`),
/// keeping its turn open only briefly, while the player keeps moving, then
/// walks its path and catches the player.
#[test]
fn a_slow_plan_holds_the_enemy_not_the_game() {
    let mut game = Game::with_paths(
        "XXXXXXXXXXXX\n\
         X..........X\n\
         Xe......p..X\n\
         X..........X\n\
         XXXXXXXXXXXX\n",
        Runner::Deferred(20),
    );
    use KeyCode::{ArrowLeft as L, ArrowRight as R};
    for key in [R, L].into_iter().cycle().take(30) {
        game.turn(Some(key));
    }
    game.print("slow planning");

    let acts = acts(&game);
    let thought = acts
        .iter()
        .position(|a| *a == Some(Act::Think(Mood::Hunt)))
        .expect("never thought about a hunt");
    let hunted = acts[thought..]
        .iter()
        .position(|a| matches!(a, Some(Act::Hunt(_))))
        .expect("never hunted")
        + thought;
    assert!(hunted >= thought + 2, "planned too fast to show: {acts:?}");
    // The game did not wait: the player moved while the enemy thought.
    let player_moves = game.trace[thought..=hunted]
        .windows(2)
        .filter(|w| w[0].player != w[1].player)
        .count();
    assert!(player_moves >= 2, "player stalled: {player_moves}");
    let last = game.trace.last().unwrap();
    assert_eq!(manhattan(last.enemies[0].1, last.player), 1);
}

/// The same chase with plans made on the async compute pool, as in the game.
/// When each plan lands depends on the threads, so this checks the outcome.
#[test]
fn an_enemy_planning_in_the_background_still_gets_there() {
    let mut game = Game::with_paths(
        "XXXXXXXXXXXX\n\
         X..........X\n\
         Xe......p..X\n\
         X..........X\n\
         XXXXXXXXXXXX\n",
        Runner::Background,
    );
    game.turns(16);
    let last = game.trace.last().unwrap();
    let (_, pos, act) = last.enemies[0];
    assert_eq!(manhattan(pos, last.player), 1, "{pos} vs {}", last.player);
    assert!(matches!(act, Some(Act::Attack(_))), "{act:?}");
}

// --- hunters: follow the order (the player), opening doors on the way ------

fn door_open(snap: &Snapshot) -> bool {
    snap.doors.iter().all(|&(_, open)| open)
}

#[test]
fn a_hunter_fetches_a_key_to_open_the_door_in_its_way() {
    let mut game = Game::new(
        "XXXXXXXXXXXXXX\n\
         X.k...X......X\n\
         X.....X......X\n\
         Xh....D....p.X\n\
         X.....X......X\n\
         XXXXXXXXXXXXXX\n",
    );
    game.turns(30);
    game.print("hunter: key, door, player");
    let picked = game
        .trace
        .iter()
        .position(|s| s.keys.is_empty())
        .expect("never picked up the key");
    let opened = game
        .trace
        .iter()
        .position(door_open)
        .expect("never opened the door");
    assert!(picked < opened, "opened before it had the key");
    // Walking to the order, which locomotion routed through the door.
    let opener = game.enemy(opened).1;
    assert!(matches!(opener, Some(Act::Order(_))), "{opener:?}");
    let last = game.trace.last().unwrap();
    let (_, pos, act) = last.enemies[0];
    assert_eq!(manhattan(pos, last.player), 1);
    assert!(matches!(act, Some(Act::Attack(_))), "{act:?}");
}

#[test]
fn a_hunter_walks_around_a_wall() {
    let mut game = Game::new(
        "XXXXXXXXXXX\n\
         X.........X\n\
         X....X....X\n\
         Xh...X..p.X\n\
         X....X....X\n\
         XXXXXXXXXXX\n",
    );
    game.turns(16);
    game.print("hunter: around a wall");
    let last = game.trace.last().unwrap();
    let (_, pos, act) = last.enemies[0];
    assert_eq!(manhattan(pos, last.player), 1, "{pos} vs {}", last.player);
    assert!(matches!(act, Some(Act::Attack(_))), "{act:?}");
}

#[test]
fn a_hunter_takes_an_open_detour_rather_than_spend_a_key() {
    let mut game = Game::new(
        "XXXXXXXXXXX\n\
         X.........X\n\
         X.k..X....X\n\
         Xh...D..p.X\n\
         X....X....X\n\
         XXXXXXXXXXX\n",
    );
    game.turns(16);
    game.print("hunter: detour over door");
    let last = game.trace.last().unwrap();
    assert!(!door_open(last), "opened the door");
    assert_eq!(last.keys.len(), 1, "fetched the key");
    let (_, pos, _) = last.enemies[0];
    assert_eq!(manhattan(pos, last.player), 1, "{pos} vs {}", last.player);
}

/// With no key anywhere, there is no way to the order: the hunter keeps
/// asking for it and holds where it is, ready to go once a way opens.
#[test]
fn a_hunter_without_a_key_holds_behind_a_locked_door() {
    let mut game = Game::new(
        "XXXXXXXXXXXX\n\
         X....X.....X\n\
         Xh...D...p.X\n\
         X....X.....X\n\
         XXXXXXXXXXXX\n",
    );
    game.turns(12);
    game.print("hunter: no key");
    let last = game.trace.last().unwrap();
    assert!(!door_open(last));
    let (_, pos, act) = last.enemies[0];
    assert_eq!(pos, IVec2::new(1, 2), "wandered off");
    assert_eq!(act, Some(Act::Hold));
}

// --- planning for keys: go the other way, chain doors, skip a sealed key ----

fn keys_left(snap: &Snapshot) -> usize {
    snap.keys.len()
}

fn doors_open(snap: &Snapshot) -> usize {
    snap.doors.iter().filter(|&&(_, open)| open).count()
}

/// The hunter catches the player: next to it and attacking.
fn assert_caught(game: &Game) {
    let last = game.trace.last().unwrap();
    let (_, pos, act) = last.enemies[0];
    assert_eq!(manhattan(pos, last.player), 1, "{pos} vs {}", last.player);
    assert!(matches!(act, Some(Act::Attack(_))), "{act:?}");
}

/// The key lies behind the hunter, away from the door it opens: it has to
/// walk away from its goal first.
#[test]
fn a_hunter_walks_away_from_its_goal_to_fetch_the_key() {
    let mut game = Game::new(
        "XXXXXXXXXXXXXXX\n\
         Xk....hX......X\n\
         X......D...p..X\n\
         X......X......X\n\
         XXXXXXXXXXXXXXX\n",
    );
    game.turns(36);
    game.print("hunter: key behind it");
    let start = game.trace[0].enemies[0].1;
    let picked = game
        .trace
        .iter()
        .position(|s| keys_left(s) == 0)
        .expect("never picked up the key");
    let opened = game
        .trace
        .iter()
        .position(|s| doors_open(s) == 1)
        .expect("never opened the door");
    assert!(picked < opened);
    // It went the other way: left, toward the key, at some point.
    let westmost = game.trace[..=picked]
        .iter()
        .map(|s| s.enemies[0].1.x)
        .min()
        .unwrap();
    assert!(westmost < start.x - 3, "never headed for the key");
    assert_caught(&game);
}

/// Each door spends a key, so two doors in a row take two keys.
#[test]
fn a_hunter_fetches_a_key_for_each_door() {
    let mut game = Game::new(
        "XXXXXXXXXXXXXXXX\n\
         Xk..h..X..X....X\n\
         Xk.....D..D..p.X\n\
         X......X..X....X\n\
         XXXXXXXXXXXXXXXX\n",
    );
    game.turns(50);
    game.print("hunter: two doors");
    let last = game.trace.last().unwrap();
    assert_eq!(keys_left(last), 0, "left a key behind");
    assert_eq!(doors_open(last), 2, "a door stayed shut");
    assert_caught(&game);
}

/// The closest key is walled in. The hunter gives it up and fetches one
/// that is farther away but reachable.
#[test]
fn a_hunter_skips_a_key_it_cannot_reach() {
    let mut game = Game::new(
        "XXXXXXXXXXXXXXXXXX\n\
         XXX.........X....X\n\
         XkX....h....D..p.X\n\
         XXX.........X....X\n\
         X.k.........X....X\n\
         XXXXXXXXXXXXXXXXXX\n",
    );
    game.turns(36);
    game.print("hunter: sealed key");
    let last = game.trace.last().unwrap();
    assert_eq!(last.keys, vec![IVec2::new(1, 2)], "took the wrong key");
    assert_eq!(doors_open(last), 1);
    assert_caught(&game);
}

/// `GetKey` asks for the keys one by one, nearest first: two walled-in keys
/// fail in turn before the farthest, reachable one is fetched.
#[test]
fn a_hunter_tries_every_key_before_giving_up() {
    let mut game = Game::new(
        "XXXXXXXXXXXXXXXXXX\n\
         XkX..........X...X\n\
         XXX.....h....D.p.X\n\
         XkX..........X...X\n\
         XXX..........X...X\n\
         X.k..........X...X\n\
         XXXXXXXXXXXXXXXXXX\n",
    );
    game.turns(45);
    game.print("hunter: third key");
    let last = game.trace.last().unwrap();
    assert_eq!(
        last.keys,
        vec![IVec2::new(1, 1), IVec2::new(1, 3)],
        "took the wrong key"
    );
    assert_eq!(doors_open(last), 1);
    assert_caught(&game);
}

/// When every key fails, so does `GetKey`: the hunter holds at the door
/// instead of trying the same keys again.
#[test]
fn a_hunter_with_no_key_in_reach_holds() {
    let mut game = Game::new(
        "XXXXXXXXXXXXXX\n\
         XkX......X...X\n\
         XXX..h...D.p.X\n\
         XkX......X...X\n\
         XXX......X...X\n\
         XXXXXXXXXXXXXX\n",
    );
    game.turns(20);
    game.print("hunter: no key in reach");
    let last = game.trace.last().unwrap();
    assert_eq!(keys_left(last), 2);
    assert!(!door_open(last));
    let settled: Vec<IVec2> = game.trace[game.trace.len() - 6..]
        .iter()
        .map(|snap| snap.enemies[0].1)
        .collect();
    assert!(
        settled.windows(2).all(|pair| pair[0] == pair[1]),
        "still pacing: {settled:?}"
    );
    assert_eq!(last.enemies[0].2, Some(Act::Hold));
}

/// The demo map (`--map assets/maps/hunter_keys.txt`): the nearest keys are
/// walled in or behind the first door, so the hunter walks away for the far
/// one, opens the first door, then fetches the key it could not reach before
/// for the second.
#[test]
fn a_hunter_works_through_the_demo_map() {
    let mut game = Game::new(include_str!("../assets/maps/hunter_keys.txt"));
    game.turns(70);
    game.print("hunter: demo map");
    let last = game.trace.last().unwrap();
    assert_eq!(last.keys, vec![IVec2::new(9, 1)], "took the wrong keys");
    assert_eq!(doors_open(last), 2);
    assert_caught(&game);
}

/// The same with the player pacing, so the order moves every turn: the
/// hunter still sees there is no way to it and goes for the keys.
#[test]
fn a_hunter_fetches_keys_while_the_player_paces() {
    let mut game = Game::new(include_str!("../assets/maps/hunter_keys.txt"));
    for turn in 0..70 {
        let key = if turn % 2 == 0 {
            KeyCode::ArrowLeft
        } else {
            KeyCode::ArrowRight
        };
        game.turn(Some(key));
    }
    game.print("hunter: demo map, pacing player");
    let last = game.trace.last().unwrap();
    assert_eq!(last.keys, vec![IVec2::new(9, 1)], "took the wrong keys");
    assert_eq!(doors_open(last), 2);
    assert_caught(&game);
}

/// Fetching the key with plans made off the frame, as in the game.
#[test]
fn a_hunter_planning_in_the_background_fetches_the_key() {
    let mut game = Game::with_paths(
        "XXXXXXXXXXXXXXX\n\
         Xk....hX......X\n\
         X......D...p..X\n\
         X......X......X\n\
         XXXXXXXXXXXXXXX\n",
        Runner::Background,
    );
    game.turns(45);
    let last = game.trace.last().unwrap();
    assert_eq!(keys_left(last), 0);
    assert_eq!(doors_open(last), 1);
    assert_caught(&game);
}

/// Two rooms joined only by a portal: `1` in the left room's east wall
/// opens west and leads out of the `1` in the right room's west wall.
const PORTAL_ROOMS: &str = "\
XXXXXXXXXXXXXXX\n\
X.....1XX.....X\n\
X.p...XXX..h..X\n\
X.....XX1.....X\n\
XXXXXXXXXXXXXXX\n";

#[test]
fn a_hunter_allowed_through_portals_reaches_the_player_through_one() {
    let mut game = Game::with_policy(PORTAL_ROOMS, Runner::Inline, PortalPolicy { enemies: true });
    game.turns(30);
    game.print("hunter through a portal");
    // It walked to the portal in its own room, came out of the other one
    // and went on to the player.
    let crossed = game
        .trace
        .windows(2)
        .any(|turns| manhattan(turns[0].enemies[0].1, turns[1].enemies[0].1) > 1);
    assert!(crossed, "never went through the portal");
    let last = game.trace.last().unwrap();
    let (_, pos, act) = last.enemies[0];
    assert_eq!(manhattan(pos, last.player), 1);
    assert!(matches!(act, Some(Act::Attack(_))), "{act:?}");
}

#[test]
fn a_hunter_kept_out_of_portals_cannot_reach_the_player() {
    let mut game = Game::new(PORTAL_ROOMS);
    game.turns(30);
    let last = game.trace.last().unwrap();
    let (_, pos, _) = last.enemies[0];
    assert!(pos.x > 7, "stayed in its own room: {pos}");
}

/// The player's room and the enemy's, joined only by portal `1`: the left
/// room's east wall leads out of the right room's west wall, on the same
/// row as both of them.
const SEEN_THROUGH_A_PORTAL: &str = "\
XXXXXXXXXXXXXXX\n\
X.....XXX.....X\n\
X.p...1X1...e.X\n\
X.....XXX.....X\n\
XXXXXXXXXXXXXXX\n";

#[test]
fn an_enemy_sees_the_player_through_a_portal() {
    let mut game = Game::new(SEEN_THROUGH_A_PORTAL);
    game.turns(2);
    game.print("seen through a portal");
    assert!(
        acts(&game).contains(&Some(Act::Alert)),
        "never noticed the player: {:?}",
        acts(&game)
    );

    // The same rooms with the portal walled up: nothing to see.
    let mut game = Game::new(&*SEEN_THROUGH_A_PORTAL.replace('1', "X").leak());
    game.turns(2);
    assert!(
        !acts(&game).contains(&Some(Act::Alert)),
        "noticed the player through a wall: {:?}",
        acts(&game)
    );
}
