//! Loading another map while the game runs.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::animation::ObjectAnimationPlugin;
use pav_ecs_game_bevy_port::app::{bundled_map, CurrentMap, GamePlugin, LoadMap, MapText};
use pav_ecs_game_bevy_port::content::map::{parse_map, parse_next, SpawnKind};
use pav_ecs_game_bevy_port::model::*;
use pav_ecs_game_bevy_port::navigation::NavMap;
use pav_ecs_game_bevy_port::rendering::TextRendererPlugin;
use pav_ecs_game_bevy_port::transition::TransitionRoom;

/// A room with a key on the floor, an enemy and a lamp.
const FIRST: &str = "\
XXXXXXX
Xp.k.eX
X..i..X
XXXXXXX
";
/// Larger, with the start elsewhere and a pillar.
const SECOND: &str = "\
XXXXXXXXX
X.......X
X..XX.p.X
X.......X
XXXXXXXXX
";
const SECOND_START: IVec2 = IVec2::new(6, 2);

fn boot(map: &'static str, extra: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .insert_resource(MapText(map))
        .add_plugins(GamePlugin);
    extra(&mut app);
    app.update();
    app.update();
    app
}

fn load(app: &mut App, map: &'static str) {
    app.world_mut().write_message(LoadMap(map));
    for _ in 0..3 {
        app.update();
    }
}

fn player(app: &mut App) -> Entity {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<Player>>()
        .single(world)
        .unwrap()
}

fn walls(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&Wall>().iter(world).count()
}

fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query_filtered::<(), F>().iter(world).count()
}

#[test]
fn the_player_carries_over_and_the_old_level_goes() {
    let mut app = boot(FIRST, |_| {});
    let hero = player(&mut app);
    // The player carries a key; the enemy carries one too.
    let world = app.world_mut();
    let held = world.spawn((Active, Item, Key)).id();
    world.get_mut::<Inventory>(hero).unwrap().0.push(held);
    let enemy = world
        .query_filtered::<Entity, With<Enemy>>()
        .single(world)
        .unwrap();
    let enemy_key = world.spawn((Active, Item, Key)).id();
    world.get_mut::<Inventory>(enemy).unwrap().0.push(enemy_key);
    let revision = world.resource::<MapGrid>().blocker_revision;

    load(&mut app, SECOND);

    assert_eq!(player(&mut app), hero, "the same player");
    let world = app.world();
    assert_eq!(
        world.get::<Pos>(hero).unwrap().0,
        SECOND_START,
        "at the new start"
    );
    let grid = world.resource::<MapGrid>();
    assert_eq!(
        (grid.width, grid.height),
        (9, 5),
        "the grid takes the new size"
    );
    assert_eq!(grid.get(SECOND_START), Some(hero));
    assert!(grid.blocker_revision > revision, "revisions move on");
    assert_eq!(
        world.get::<Inventory>(hero).unwrap().0,
        [held],
        "with what they carry"
    );
    assert!(world.get_entity(held).is_ok());
    for gone in [enemy, enemy_key] {
        assert!(world.get_entity(gone).is_err(), "the old level is gone");
    }
    assert!(
        world.resource::<MapGrid>().get(IVec2::ZERO).is_some(),
        "the new map's walls hold their cells"
    );
    let walls_now = walls(&mut app);
    let expected = parse_map(SECOND)
        .2
        .iter()
        .filter(|cell| cell.kind == SpawnKind::Wall)
        .count();
    assert_eq!(walls_now, expected, "the new map's walls, and only those");
    assert_eq!(count::<With<Enemy>>(&mut app), 0);
    assert_eq!(count::<With<Lamp>>(&mut app), 0);

    // They remember nothing of the old map, and paths know the new one.
    let world = app.world_mut();
    let visibility = world.get::<VisibilityMap>(hero).unwrap();
    assert_eq!(visibility.data.len(), 9 * 5);
    assert!(visibility
        .data
        .iter()
        .all(|cell| !cell.contains(Vis::KNOWN) || cell.contains(Vis::VISIBLE)));
    assert!(visibility
        .data
        .iter()
        .any(|cell| cell.contains(Vis::VISIBLE)));
    let grid_revision = world.resource::<MapGrid>().blocker_revision;
    assert_eq!(world.resource::<NavMap>().revision(), Some(grid_revision));
}

#[test]
fn the_player_is_shown_at_the_new_start_at_once() {
    let mut app = boot(FIRST, |app| {
        app.add_plugins(ObjectAnimationPlugin);
    });
    let hero = player(&mut app);
    app.world_mut().write_message(LoadMap(SECOND));
    app.update();
    app.update();
    let shown = app.world().get::<AnimatedPos>(hero).unwrap().position;
    assert_eq!(shown, SECOND_START.as_vec2(), "no glide across the map");
}

#[test]
fn loading_again_and_again_leaves_nothing_behind() {
    let map1 = include_str!("../assets/maps/map1.txt");
    let mut app = boot(map1, |app| {
        app.add_plugins((ObjectAnimationPlugin, TextRendererPlugin));
    });
    for _ in 0..10 {
        app.update();
    }
    load(&mut app, SECOND);
    load(&mut app, map1);
    for _ in 0..10 {
        app.update();
    }
    let live = |app: &mut App| {
        let world = app.world_mut();
        world.query::<Entity>().iter(world).count()
    };
    let after_one = live(&mut app);
    load(&mut app, SECOND);
    load(&mut app, map1);
    for _ in 0..10 {
        app.update();
    }
    let after_two = live(&mut app);
    assert_eq!(after_one, after_two, "no entities or sprites pile up");
}

#[test]
fn a_walking_player_carries_over_to_every_bundled_map() {
    let maps = [
        include_str!("../assets/maps/map1_test.txt"),
        include_str!("../assets/maps/map2.txt"),
        include_str!("../assets/maps/chasers.txt"),
        include_str!("../assets/maps/portals.txt"),
        include_str!("../assets/maps/rooms.txt"),
        include_str!("../assets/maps/hunter_keys.txt"),
        include_str!("../assets/maps/map1.txt"),
    ];
    let mut app = boot(include_str!("../assets/maps/map1.txt"), |app| {
        app.add_plugins(ObjectAnimationPlugin);
    });
    for map in maps {
        // Walk, and load the next map mid-step.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowRight);
        for _ in 0..12 {
            app.update();
        }
        app.world_mut().write_message(LoadMap(map));
        for _ in 0..12 {
            app.update();
        }
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.release(KeyCode::ArrowRight);
        input.clear();
        for _ in 0..12 {
            app.update();
        }
    }
}

/// One step right of the start is an exit to `rooms`.
const WITH_EXIT: &str = "\
XXXXXX
Xp>..X
XXXXXX
@next rooms
";

/// Holds `key` until the player has stepped, then lets the turn finish.
fn step(app: &mut App, key: KeyCode) {
    let hero = player(app);
    let from = app.world().get::<Pos>(hero).unwrap().0;
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    for _ in 0..200 {
        app.update();
        if app.world().get::<Pos>(hero).unwrap().0 != from {
            break;
        }
    }
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.release(key);
    input.clear();
}

fn playing(app: &App) -> &'static str {
    app.world().resource::<CurrentMap>().text
}

#[test]
fn stepping_onto_an_exit_leads_to_the_next_map() {
    for animated in [false, true] {
        let mut app = boot(WITH_EXIT, |app| {
            if animated {
                app.add_plugins(ObjectAnimationPlugin);
            }
        });
        let hero = player(&mut app);
        let rooms = bundled_map("rooms").unwrap();
        assert_eq!(app.world().resource::<CurrentMap>().next, Some(rooms));
        // Placed on a map, the player has not stepped onto anything.
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(playing(&app), WITH_EXIT);

        step(&mut app, KeyCode::ArrowRight);
        for _ in 0..60 {
            app.update();
        }
        assert_eq!(playing(&app), rooms, "animated: {animated}");
        assert_eq!(player(&mut app), hero, "the same player");
        let start = parse_map(rooms)
            .2
            .into_iter()
            .find(|cell| cell.kind == SpawnKind::Player)
            .unwrap()
            .pos;
        assert_eq!(app.world().get::<Pos>(hero).unwrap().0, start);
    }
}

#[test]
fn the_bundled_maps_lead_from_one_to_the_next_and_round() {
    let mut map = "map1";
    for expected in ["map2", "map3", "map1"] {
        let text = bundled_map(map).unwrap();
        let cells = parse_map(text).2;
        assert!(
            cells.iter().any(|cell| cell.kind == SpawnKind::Exit),
            "{map} has an exit"
        );
        let next = parse_next(text, &cells).unwrap().unwrap();
        assert_eq!(next, expected, "{map} leads on");
        map = expected;
    }
}

#[test]
fn a_loaded_map_comes_together_from_the_one_before() {
    let map1 = bundled_map("map1").unwrap();
    let map2 = bundled_map("map2").unwrap();
    let mut app = boot(map1, |app| {
        app.add_plugins(ObjectAnimationPlugin);
    });
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().write_message(LoadMap(map2));
    app.update();
    app.update();

    // A transition plays: the player waits, a circle around them is in
    // sight beside what they see, and walls of map1 are on their way.
    assert!(app.world().contains_resource::<Cutscene>());
    let area = *app.world().resource::<RevealArea>();
    assert!(!area.hides_sight);
    assert_eq!(count::<With<TransitionRoom>>(&mut app), 1);
    let moving = count::<Or<(With<Sinking>, With<Sliding>)>>(&mut app);
    assert!(moving > 0, "the old walls go and move");

    let mut frames = 0;
    while count::<With<TransitionRoom>>(&mut app) > 0 {
        app.update();
        frames += 1;
        assert!(frames < 300, "the transition ends");
    }
    assert!(!app.world().contains_resource::<Cutscene>());
    assert!(!app.world().contains_resource::<RevealArea>());
    assert_eq!(count::<With<Sinking>>(&mut app), 0);
    let expected = parse_map(map2)
        .2
        .iter()
        .filter(|cell| matches!(cell.kind, SpawnKind::Wall | SpawnKind::Portal(_)))
        .count();
    assert_eq!(walls(&mut app), expected, "exactly map2's walls");
}
