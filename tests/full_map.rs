//! Headless integration: boot the real `map1` through `GamePlugin` and let
//! the full pipeline settle. Covers startup spawn, autotiling, FOV, light
//! layers, and render composition without opening a window.

use bevy::prelude::*;
use pav_ecs_game_bevy_port::components::*;
use pav_ecs_game_bevy_port::game::GamePlugin;
use pav_ecs_game_bevy_port::render::DynamicLight;

fn boot() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins(GamePlugin);
    app.update(); // startup + first pass
    for _ in 0..60 {
        app.update();
    }
    app
}

#[test]
fn map1_spawns_all_factions_and_settles() {
    let mut app = boot();
    let world = app.world_mut();

    // Roster: exactly one player, plus every Lite faction present on map1.
    assert_eq!(world.query::<&Player>().iter(world).count(), 1);
    assert!(world.query::<&Enemy>().iter(world).count() >= 1);
    assert!(world.query::<&Wall>().iter(world).count() > 100);
    assert!(world.query::<&Lamp>().iter(world).count() >= 1);
    assert!(world.query::<&AcidPool>().iter(world).count() >= 1);
    assert!(world.query::<&ElectroField>().iter(world).count() >= 1);

    // Autotiling finished: no Tile state left, walls show box glyphs.
    assert_eq!(world.query::<&Tile>().iter(world).count(), 0);
    assert!(world
        .query::<(&Wall, &Glyph)>()
        .iter(world)
        .all(|(_, g)| g.ch != '#'));

    // Player gained vision and the light layers are map-sized.
    let grid = world.resource::<MapGrid>();
    let (w, h) = (grid.width, grid.height);
    assert!((w, h) == (80, 24));
    let vis = world.query::<&VisibilityMap>().single(world).unwrap();
    assert!(vis.data.iter().any(|v| v.contains(Vis::KNOWN)));
    assert!(vis.data.iter().any(|v| v.contains(Vis::VISIBLE)));
    let dyn_light = world.resource::<DynamicLight>();
    assert_eq!(dyn_light.data.len(), (w * h) as usize);
    assert!(dyn_light.data.iter().any(|c| c.value > 1));

    // Occupancy is consistent: every collider owns its cell.
    let bodies: Vec<(Entity, IVec2)> = world
        .query::<(Entity, &Pos, Option<&Collider>)>()
        .iter(&*world)
        .filter(|(_, _, c)| c.is_some())
        .map(|(e, p, _)| (e, p.0))
        .collect();
    assert!(!bodies.is_empty());
    let grid = world.resource::<MapGrid>();
    for (e, p) in bodies {
        assert_eq!(grid.get(p), Some(e), "collider {e:?} lost its cell");
    }

    // The bound 'i' marker tracks its player parent.
    let bound = world.query::<&BoundTo>().iter(world).count();
    assert_eq!(bound, 1);
}
