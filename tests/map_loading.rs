//! Loading another map while the game runs.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::animation::ObjectAnimationPlugin;
use pav_ecs_game_bevy_port::app::{GamePlugin, LoadMap, MapText};
use pav_ecs_game_bevy_port::content::map::{parse_map, SpawnKind};
use pav_ecs_game_bevy_port::model::*;
use pav_ecs_game_bevy_port::navigation::NavMap;
use pav_ecs_game_bevy_port::rendering::TextRendererPlugin;

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
    let old_wall = world.resource::<MapGrid>().get(IVec2::ZERO).unwrap();
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
    for gone in [enemy, enemy_key, old_wall] {
        assert!(world.get_entity(gone).is_err(), "the old level is gone");
    }
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
