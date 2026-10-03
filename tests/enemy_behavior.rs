//! Enemy behavior-tree gameplay, played through the real `GamePlugin` on small
//! maps: real turns, tokens, line of sight, and collision resolution.
//!
//! Each scenario records one snapshot per turn. Set `ENEMY_TRACE=1` to print
//! them as maps: `!` alert, `H` hunting, `A` attacking, `-` holding, `S`
//! searching, `p` patrolling, `z` resting, `g` going (navigation), `o`
//! opening a door, `@` the player, `k` a key, `+`/`'` a closed/open door.

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
use pav_ecs_game_bevy_port::lighting::YELLOW;
use pav_ecs_game_bevy_port::model::*;
use std::time::Duration;

#[derive(Clone, Debug)]
struct Snapshot {
    player: IVec2,
    enemies: Vec<(Entity, IVec2, Option<EnemyAct>)>,
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
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
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

    fn settle(&mut self) {
        for _ in 0..50 {
            if !self.app.world().resource::<TurnState>().simulation {
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
            .map(|(e, p, act)| (e, p.0, act.copied()))
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
    fn check_invariants(&self, enemies: &[(Entity, IVec2, Option<EnemyAct>)]) {
        let grid = self.app.world().resource::<MapGrid>();
        for &(enemy, pos, _) in enemies {
            assert_eq!(grid.get(pos), Some(enemy), "enemy off the occupancy grid");
        }
        if let Some(previous) = self.trace.last() {
            for (&(e, pos, _), &(pe, ppos, _)) in enemies.iter().zip(&previous.enemies) {
                assert_eq!(e, pe);
                let moved = (pos - ppos).abs();
                assert!(moved.x + moved.y <= 1, "enemy moved {moved} in one turn");
            }
        }
    }

    fn enemy(&self, turn: usize) -> (IVec2, Option<EnemyAct>) {
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
                    Some(EnemyAct::Alert) => '!',
                    Some(EnemyAct::Hunt(_)) => 'H',
                    Some(EnemyAct::Attack(_)) => 'A',
                    Some(EnemyAct::Hold) => '-',
                    Some(EnemyAct::Search(_)) => 'S',
                    Some(EnemyAct::Patrol(_)) => 'p',
                    Some(EnemyAct::Rest) => 'z',
                    Some(EnemyAct::GoTo(_)) => 'g',
                    Some(EnemyAct::Open(_)) => 'o',
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

fn is_idle(act: Option<EnemyAct>) -> bool {
    matches!(act, Some(EnemyAct::Patrol(_) | EnemyAct::Rest))
}

fn acts(game: &Game) -> Vec<Option<EnemyAct>> {
    game.trace.iter().map(|s| s.enemies[0].2).collect()
}

#[test]
fn a_sighting_freezes_the_enemy_then_it_chases_and_attacks() {
    let mut game = Game::new(
        "XXXXXXXXXXXX\n\
         Xp......e..X\n\
         XXXXXXXXXXXX\n",
    );
    // The alert beat: standing still, shown as a yellow `!`.
    let player = game.trace[0].player;
    assert_eq!(game.enemy(0), (IVec2::new(8, 1), Some(EnemyAct::Alert)));
    let world = game.app.world_mut();
    let glyph = *world
        .query_filtered::<&Glyph, With<Enemy>>()
        .single(world)
        .unwrap();
    assert_eq!((glyph.ch, glyph.color), ('!', YELLOW));

    game.turns(9);
    game.print("alert, chase, attack");
    // Then one step closer every turn, and bumps once adjacent.
    for turn in 1..game.trace.len() {
        let (pos, act) = game.enemy(turn);
        let before = manhattan(game.trace[turn - 1].enemies[0].1, player);
        let expected = if before == 1 {
            EnemyAct::Attack(IVec2::NEG_X)
        } else {
            EnemyAct::Hunt(IVec2::NEG_X)
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
    // Patrols are straight stretches, not a jitter: some last several turns.
    let longest = acts
        .windows(3)
        .filter(|w| matches!(w[0], Some(EnemyAct::Patrol(_))) && w[0] == w[1] && w[1] == w[2])
        .count();
    assert!(longest > 0, "no patrol held its direction: {acts:?}");
    assert!(
        acts.contains(&Some(EnemyAct::Rest)),
        "never rested: {acts:?}"
    );
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
    assert_eq!(acts[0], Some(EnemyAct::Alert));
    let searched = acts
        .iter()
        .position(|a| matches!(a, Some(EnemyAct::Search(_))))
        .expect("never searched");
    assert_eq!(acts[searched], Some(EnemyAct::Search(IVec2::NEG_X)));
    let found = acts[searched..]
        .iter()
        .any(|a| matches!(a, Some(EnemyAct::Hunt(_) | EnemyAct::Attack(_))));
    assert!(found, "never found the player again: {acts:?}");
    // One pursuit, one alert: re-sighting mid-chase does not freeze it again.
    let alerts = acts.iter().filter(|&&a| a == Some(EnemyAct::Alert)).count();
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
        .position(|a| matches!(a, Some(EnemyAct::Search(_))))
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
    game.turns(10);
    game.print("three chasers");
    let last = game.trace.last().unwrap();
    // Nobody queues behind an ally: each steps around and ends up attacking.
    for &(_, pos, act) in &last.enemies {
        assert!(matches!(act, Some(EnemyAct::Attack(_))), "{act:?}");
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
    let opener = game.enemy(opened).1;
    assert!(
        matches!(opener, Some(EnemyAct::Open(IVec2::X))),
        "{opener:?}"
    );
    let last = game.trace.last().unwrap();
    let (_, pos, act) = last.enemies[0];
    assert_eq!(manhattan(pos, last.player), 1);
    assert!(matches!(act, Some(EnemyAct::Attack(_))), "{act:?}");
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
    assert!(matches!(act, Some(EnemyAct::Attack(_))), "{act:?}");
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

#[test]
fn a_hunter_without_a_key_waits_at_the_locked_door() {
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
    assert_eq!(pos, IVec2::new(4, 2), "not waiting at the door");
    assert!(
        matches!(act, Some(EnemyAct::GoTo(_) | EnemyAct::Hold)),
        "{act:?}"
    );
}
