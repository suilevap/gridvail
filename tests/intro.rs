//! The intro: the map's walls come together from the title when it loads.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::animation::ObjectAnimationPlugin;
use pav_ecs_game_bevy_port::app::{
    GamePlugin, Intro, IntroClosing, IntroLayout, IntroRoom, MapText,
};
use pav_ecs_game_bevy_port::content::map::{parse_map, SpawnKind};
use pav_ecs_game_bevy_port::model::*;

const MAP: &str = include_str!("../assets/maps/map1.txt");

fn boot(animated: bool, extra: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .insert_resource(MapText(MAP))
        .insert_resource(Intro(IntroLayout::Title));
    extra(&mut app);
    app.add_plugins(GamePlugin);
    if animated {
        app.add_plugins(ObjectAnimationPlugin);
    }
    app
}

/// The map's plain walls (portal walls are not part of the intro).
fn map_walls() -> Vec<IVec2> {
    parse_map(MAP)
        .2
        .into_iter()
        .filter(|cell| cell.kind == SpawnKind::Wall)
        .map(|cell| cell.pos)
        .collect()
}

fn walls(app: &mut App) -> Vec<IVec2> {
    let world = app.world_mut();
    let mut walls: Vec<IVec2> = world
        .query_filtered::<&Pos, With<Wall>>()
        .iter(world)
        .map(|pos| pos.0)
        .collect();
    walls.sort_by_key(|p| (p.y, p.x));
    walls
}

fn all_walls_in_place(app: &mut App) -> bool {
    let walls = walls(app);
    let grid = app.world().resource::<MapGrid>();
    map_walls()
        .iter()
        .all(|p| walls.contains(p) && grid.get(*p).is_some())
}

fn player(app: &mut App) -> IVec2 {
    let world = app.world_mut();
    world
        .query_filtered::<&Pos, With<Player>>()
        .single(world)
        .unwrap()
        .0
}

fn playing_intro(app: &mut App) -> bool {
    let world = app.world_mut();
    world.contains_resource::<Cutscene>()
        || world.query::<&IntroRoom>().iter(world).next().is_some()
}

#[test]
fn without_animation_the_map_comes_together_after_the_title_has_stood() {
    let mut app = boot(false, |_| {});
    app.update();
    assert!(playing_intro(&mut app), "the title stands");
    assert!(!all_walls_in_place(&mut app));
    // 1.5 s of 16 ms frames.
    for _ in 0..100 {
        app.update();
    }
    assert!(all_walls_in_place(&mut app));
    let portals = parse_map(MAP)
        .2
        .iter()
        .filter(|cell| matches!(cell.kind, SpawnKind::Portal(_)))
        .count();
    assert_eq!(
        walls(&mut app).len(),
        map_walls().len() + portals,
        "and no others"
    );
    assert!(!playing_intro(&mut app), "the intro is over");
    // The oval in sight closes over 0.8 s.
    for _ in 0..60 {
        app.update();
    }
    assert!(!app.world().contains_resource::<RevealArea>());
    assert!(!app.world().contains_resource::<IntroClosing>());
    assert!(!app.world().contains_resource::<RevealAll>(), "never all");
}

#[test]
fn the_intro_holds_the_player_with_the_title_in_sight_then_closes_onto_them() {
    let mut app = boot(true, |_| {});
    app.update();
    assert!(playing_intro(&mut app));
    assert!(
        !app.world().contains_resource::<RevealAll>(),
        "not all in sight"
    );
    let area = *app.world().resource::<RevealArea>();
    let title = walls(&mut app);
    assert!(
        title.iter().all(|&p| area.contains(p)),
        "the title is in sight"
    );
    assert!(!area.contains(IVec2::new(79, 23)), "the far corner is not");
    assert!(!title.is_empty() && !all_walls_in_place(&mut app));
    let world = app.world_mut();
    let rising = world
        .query::<&AnimatedPos>()
        .iter(world)
        .filter(|shown| shown.height < 1.0)
        .count();
    assert!(rising >= title.len(), "the title rises");

    let start = player(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowRight);
    let mut frames = 0;
    let mut switched = false;
    while playing_intro(&mut app) {
        assert_eq!(player(&mut app), start, "the player waits");
        app.update();
        frames += 1;
        assert!(frames < 600, "the intro ends");
        if !switched && all_walls_in_place(&mut app) {
            // The world switches at once once the title has stood; only
            // the walls' animation plays on.
            switched = true;
            assert!(frames >= 90, "the title stood first: {frames} frames");
            let world = app.world_mut();
            assert!(
                world.query::<&Sinking>().iter(world).count() > 0,
                "title walls go"
            );
        }
    }
    assert!(switched);
    let world = app.world_mut();
    assert_eq!(world.query::<&Sinking>().iter(world).count(), 0);
    assert!(all_walls_in_place(&mut app));

    // The oval closes onto the player while the held key already walks.
    let mut radii = area.radii;
    while let Some(area) = app.world().get_resource::<RevealArea>().copied() {
        assert!(area.radii.x <= radii.x, "it only shrinks");
        radii = area.radii;
        app.update();
        frames += 1;
        assert!(frames < 700, "it closes");
    }
    assert!(radii.x < 2.0, "down to nothing: {radii}");
    assert!(!app.world().contains_resource::<IntroClosing>());
    assert_ne!(player(&mut app), start, "the game has started");
}

#[test]
fn a_reveal_asked_for_stays_after_the_intro() {
    let mut app = boot(false, |app| {
        app.insert_resource(RevealAll);
    });
    for _ in 0..100 {
        app.update();
    }
    assert!(!playing_intro(&mut app));
    assert!(app.world().contains_resource::<RevealAll>());
}

#[test]
fn every_bundled_map_comes_together_from_the_intro() {
    for (name, text) in [
        ("map1", include_str!("../assets/maps/map1.txt")),
        ("map2", include_str!("../assets/maps/map2.txt")),
        ("map3", include_str!("../assets/maps/map3.txt")),
        ("lightTest", include_str!("../assets/maps/lightTest.txt")),
        ("portals", include_str!("../assets/maps/portals.txt")),
        ("rooms", include_str!("../assets/maps/rooms.txt")),
        ("chasers", include_str!("../assets/maps/chasers.txt")),
        (
            "hunter_keys",
            include_str!("../assets/maps/hunter_keys.txt"),
        ),
    ] {
        let mut app = boot(true, |app| {
            app.insert_resource(MapText(text));
        });
        app.update();
        let mut frames = 0;
        while playing_intro(&mut app) {
            app.update();
            frames += 1;
            assert!(frames < 900, "{name}: the intro ends");
        }
        let cells = parse_map(text).2;
        let world = app.world_mut();
        let walls: Vec<IVec2> = world
            .query_filtered::<&Pos, With<Wall>>()
            .iter(world)
            .map(|pos| pos.0)
            .collect();
        let expected: Vec<IVec2> = cells
            .iter()
            .filter(|cell| matches!(cell.kind, SpawnKind::Wall | SpawnKind::Portal(_)))
            .map(|cell| cell.pos)
            .collect();
        assert_eq!(walls.len(), expected.len(), "{name}: no extra walls");
        assert!(
            expected.iter().all(|p| walls.contains(p)),
            "{name}: every wall"
        );
        let grid = world.resource::<MapGrid>();
        let portals = cells
            .iter()
            .filter(|cell| matches!(cell.kind, SpawnKind::Portal(_)))
            .count();
        let faces = expected
            .iter()
            .filter(|p| grid.portal_at(**p).is_some())
            .count();
        assert_eq!(faces, portals, "{name}: portals untouched");
    }
}
