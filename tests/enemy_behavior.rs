//! Enemy behavior-tree gameplay, played through the real `GamePlugin` on small
//! maps: real turns, tokens, line of sight, and collision resolution.
//!
//! Each scenario records one snapshot per turn. Set `ENEMY_TRACE=1` to print
//! them as maps: `H` hunting, `S` searching, `w` wandering, `@` the player.

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
use pav_ecs_game_bevy_port::model::*;
use std::time::Duration;

#[derive(Clone, Debug)]
struct Snapshot {
    player: IVec2,
    enemies: Vec<(Entity, IVec2, Option<EnemyAct>)>,
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
        self.check_invariants(&enemies);
        self.trace.push(Snapshot { player, enemies });
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
            put(snap.player, '@');
            let mut acts = Vec::new();
            for &(_, pos, act) in &snap.enemies {
                let glyph = match act {
                    Some(EnemyAct::Hunt(_)) => 'H',
                    Some(EnemyAct::Search(_)) => 'S',
                    Some(EnemyAct::Wander(_)) => 'w',
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

#[test]
fn a_visible_player_is_chased_down_and_bumped() {
    let mut game = Game::new(
        "XXXXXXXXXXXX\n\
         Xp......e..X\n\
         XXXXXXXXXXXX\n",
    );
    game.turns(8);
    game.print("chase down a corridor");

    // One step closer every turn until adjacent, hunting all the way.
    let player = game.trace[0].player;
    for turn in 0..game.trace.len() {
        let (pos, act) = game.enemy(turn);
        assert!(
            matches!(act, Some(EnemyAct::Hunt(_))),
            "turn {turn}: {act:?}"
        );
        let expected = (6 - turn as i32).max(1);
        assert_eq!(manhattan(pos, player), expected, "turn {turn}");
    }
    // Adjacent: it keeps pressing into the player, which is a recorded bump.
    let enemy = game.trace[0].enemies[0].0;
    assert!(game.collided(enemy, true));
}

#[test]
fn a_wall_hides_the_player() {
    let mut game = Game::new(
        "XXXXXXXXXXX\n\
         Xp...X...eX\n\
         X....X....X\n\
         XXXXXXXXXXX\n",
    );
    game.turns(12);
    game.print("wall between");
    for turn in 0..game.trace.len() {
        let (_, act) = game.enemy(turn);
        assert!(
            matches!(act, Some(EnemyAct::Wander(_))),
            "turn {turn}: {act:?}"
        );
    }
}

#[test]
fn a_distant_player_is_not_noticed() {
    // Ten cells away down an open corridor: beyond the sight radius of 8.
    let game = Game::new(
        "XXXXXXXXXXXXXX\n\
         Xp.........e.X\n\
         XXXXXXXXXXXXXX\n",
    );
    let (_, act) = game.enemy(0);
    assert!(matches!(act, Some(EnemyAct::Wander(_))), "{act:?}");
    game.print("out of range");
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
    let (_, first) = game.enemy(0);
    assert!(matches!(first, Some(EnemyAct::Hunt(_))), "{first:?}");
    let seen_at = game.trace[0].player;

    // Step left, then down out of the row, then wait.
    game.turn(Some(KeyCode::ArrowLeft));
    game.turn(Some(KeyCode::ArrowDown));
    game.turns(8);
    game.print("duck out of sight");

    let acts: Vec<_> = game.trace.iter().map(|s| s.enemies[0].2).collect();
    let searched = acts
        .iter()
        .position(|a| matches!(a, Some(EnemyAct::Search(_))))
        .expect("never searched");
    // Searching heads for the last sighting, not the hidden player.
    let (_, Some(EnemyAct::Search(step))) = game.enemy(searched) else {
        unreachable!()
    };
    assert_eq!(step, IVec2::NEG_X);
    assert!(game.trace[searched].enemies[0].1.x > seen_at.x - 1);
    // Reaching the corner brings the player back into sight.
    assert!(
        acts[searched..]
            .iter()
            .any(|a| matches!(a, Some(EnemyAct::Hunt(_)))),
        "never found the player again: {acts:?}"
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
    // Nobody queues behind an ally: each steps around and ends up adjacent.
    for &(_, pos, act) in &last.enemies {
        assert!(matches!(act, Some(EnemyAct::Hunt(_))), "{act:?}");
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
