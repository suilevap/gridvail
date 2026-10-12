//! Rooms that switch layouts when a pressure plate is stepped on.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::animation::ObjectAnimationPlugin;
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
use pav_ecs_game_bevy_port::model::*;

/// The player starts left of the plate. The room's first layout has walls
/// at (4, 1) and (4, 2); its second has them at (5, 1) and (6, 1). The top
/// wall slides one cell right; the one below, off that line, sinks, and the
/// one at (6, 1) rises.
const MAP: &str = "\
XXXXXXXX
Xp_.X..X
X...X..X
XXXXXXXX
@room 4 1
.XX
...
";
const PLATE: IVec2 = IVec2::new(2, 1);
const LEFT: IVec2 = IVec2::new(4, 1);
const RIGHT: IVec2 = IVec2::new(5, 1);
const FAR: IVec2 = IVec2::new(6, 1);
const BELOW: IVec2 = IVec2::new(4, 2);
/// The outer wall under `BELOW`, which joins up to it.
const BOTTOM: IVec2 = IVec2::new(4, 3);

fn boot() -> App {
    boot_with(|_| {})
}

/// Boots with the animation step, which shows walls rising and sinking.
fn boot_animated() -> App {
    boot_with(|app| {
        app.add_plugins(ObjectAnimationPlugin);
    })
}

fn boot_with(extra: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .insert_resource(MapText(MAP))
        .add_plugins(GamePlugin);
    extra(&mut app);
    app.update();
    app.update();
    app
}

fn player(app: &mut App) -> IVec2 {
    let world = app.world_mut();
    world
        .query_filtered::<&Pos, With<Player>>()
        .single(world)
        .unwrap()
        .0
}

/// Holds `key` until the player has taken one step.
fn step(app: &mut App, key: KeyCode) {
    let from = player(app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    for _ in 0..200 {
        app.update();
        if player(app) != from {
            break;
        }
    }
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.release(key);
    input.clear();
    assert_ne!(player(app), from, "the player should have stepped");
    // Let the turn finish.
    for _ in 0..5 {
        app.update();
    }
}

fn wall_at(app: &mut App, p: IVec2) -> Option<char> {
    let world = app.world_mut();
    let entity = world.resource::<MapGrid>().get(p)?;
    world
        .get::<Wall>(entity)
        .and(world.get::<Glyph>(entity))
        .map(|glyph| glyph.ch)
}

fn entity_at(app: &mut App, p: IVec2) -> Option<Entity> {
    app.world().resource::<MapGrid>().get(p)
}

#[test]
fn stepping_on_the_plate_switches_the_room_and_back() {
    let mut app = boot();
    let top = entity_at(&mut app, LEFT).unwrap();
    assert!(wall_at(&mut app, BELOW).is_some());
    assert!(wall_at(&mut app, RIGHT).is_none() && wall_at(&mut app, FAR).is_none());
    let bottom = wall_at(&mut app, BOTTOM).unwrap();
    let revision = app.world().resource::<MapGrid>().blocker_revision;

    step(&mut app, KeyCode::ArrowRight);
    assert_eq!(player(&mut app), PLATE);
    assert!(wall_at(&mut app, LEFT).is_none(), "the top wall moves");
    assert_eq!(entity_at(&mut app, RIGHT), Some(top), "it slid along");
    assert_eq!(app.world().get::<Pos>(top).unwrap().0, RIGHT);
    assert!(wall_at(&mut app, BELOW).is_none(), "the wall below goes");
    let far = wall_at(&mut app, FAR).expect("a new wall comes");
    assert_ne!(far, '#', "the new wall is autotiled");
    assert_ne!(
        wall_at(&mut app, BOTTOM).unwrap(),
        bottom,
        "the outer wall joins up again without the wall above it"
    );
    assert_ne!(
        app.world().resource::<MapGrid>().blocker_revision,
        revision,
        "sight, light and paths see the change"
    );

    // Standing on the plate does not press it again; stepping off and back
    // on does, and the room cycles back to its first layout.
    step(&mut app, KeyCode::ArrowDown);
    assert!(wall_at(&mut app, RIGHT).is_some());
    step(&mut app, KeyCode::ArrowUp);
    assert_eq!(entity_at(&mut app, LEFT), Some(top), "it slides back");
    assert!(wall_at(&mut app, BELOW).is_some());
    assert!(wall_at(&mut app, RIGHT).is_none() && wall_at(&mut app, FAR).is_none());
    assert_eq!(wall_at(&mut app, BOTTOM).unwrap(), bottom);
}

#[test]
fn a_room_waits_while_something_stands_where_a_wall_goes() {
    let mut app = boot();
    let blocker = app.world_mut().spawn((Active, Collider, Pos(FAR))).id();
    app.world_mut().resource_mut::<MapGrid>().set(FAR, blocker);

    step(&mut app, KeyCode::ArrowRight);
    assert!(wall_at(&mut app, LEFT).is_some(), "the room waits");
    assert!(wall_at(&mut app, FAR).is_none());

    app.world_mut().resource_mut::<MapGrid>().clear(FAR);
    app.world_mut().despawn(blocker);
    step(&mut app, KeyCode::ArrowDown);
    assert!(wall_at(&mut app, LEFT).is_none(), "and switches once clear");
    assert!(wall_at(&mut app, FAR).is_some());
}

fn height(app: &mut App, entity: Entity) -> f32 {
    app.world().get::<AnimatedPos>(entity).unwrap().height
}

fn shown_at(app: &mut App, entity: Entity) -> Vec2 {
    app.world().get::<AnimatedPos>(entity).unwrap().position
}

#[test]
fn with_animation_walls_slide_rise_and_sink() {
    let mut app = boot_animated();
    let top = entity_at(&mut app, LEFT).unwrap();
    let below = entity_at(&mut app, BELOW).unwrap();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowRight);
    for _ in 0..200 {
        app.update();
        if player(&mut app) == PLATE {
            break;
        }
    }
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::ArrowRight);
    app.update();

    // The switch is done in the world at once.
    assert_eq!(entity_at(&mut app, RIGHT), Some(top));
    assert!(app.world().get::<Sliding>(top).is_some());
    let far = entity_at(&mut app, FAR).expect("new wall");
    assert!(app.world().get::<Wall>(far).is_some());
    assert!(entity_at(&mut app, BELOW).is_none());
    assert!(
        app.world().get::<Wall>(below).is_none(),
        "the old wall is no wall"
    );
    assert!(app.world().get::<Sinking>(below).is_some());
    // Only how they are shown takes time.
    let slid = shown_at(&mut app, top).x;
    assert!(
        slid > LEFT.x as f32 && slid < RIGHT.x as f32,
        "sliding: {slid}"
    );
    let (rising, sinking) = (height(&mut app, far), height(&mut app, below));
    assert!(rising > 0.0 && rising < 1.0, "rising: {rising}");
    assert!(sinking > 0.0 && sinking < 1.0, "sinking: {sinking}");
    let waits = app.world().resource::<TurnPacing>().animation().unwrap();
    assert!(
        waits > Duration::from_millis(100),
        "the next turn waits for them: {waits:?}"
    );

    for _ in 0..40 {
        app.update();
    }
    assert_eq!(
        shown_at(&mut app, top),
        RIGHT.as_vec2(),
        "the wall has slid"
    );
    assert_eq!(height(&mut app, top), 1.0, "without rising or sinking");
    assert_eq!(height(&mut app, far), 1.0, "the new wall stands");
    assert!(
        app.world().get_entity(below).is_err(),
        "the old one is gone"
    );
}
