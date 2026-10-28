//! Rooms that switch layouts when a pressure plate is stepped on.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::animation::ObjectAnimationPlugin;
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
use pav_ecs_game_bevy_port::model::*;
use pav_ecs_game_bevy_port::simulation::room_plans::{
    RoomPlanner, RoomPlanners, SwitchContext, SwitchPlan, WallMove,
};

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
    boot_map(MAP, extra)
}

fn boot_map(map: &'static str, extra: impl FnOnce(&mut App)) -> App {
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

/// `MAP` with the room naming the planner that only rises and sinks.
const RISE_SINK_MAP: &str = "\
XXXXXXXX
Xp_.X..X
X...X..X
XXXXXXXX
@room 4 1 rise-sink
.XX
...
";

#[test]
fn a_room_can_name_its_planner() {
    let mut app = boot_with(|app| {
        app.insert_resource(MapText(RISE_SINK_MAP));
    });
    let top = entity_at(&mut app, LEFT).unwrap();
    step(&mut app, KeyCode::ArrowRight);
    assert!(wall_at(&mut app, LEFT).is_none());
    let right = entity_at(&mut app, RIGHT).expect("a wall at the new cell");
    assert_ne!(
        right, top,
        "a new wall rose rather than the old one sliding"
    );
    assert!(app.world().get_entity(top).is_err(), "the old one went");
}

/// A planner from outside the game: it counts its plans and carries out
/// whatever `inner` would.
struct Counting<P> {
    inner: P,
    plans: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl<P: RoomPlanner> RoomPlanner for Counting<P> {
    fn plan(&self, switch: &SwitchContext) -> SwitchPlan {
        self.plans
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.plan(switch)
    }
}

/// Plans one slide whatever the layouts, which does not give them.
struct Wrong;

impl RoomPlanner for Wrong {
    fn plan(&self, _: &SwitchContext) -> SwitchPlan {
        SwitchPlan {
            moves: vec![WallMove::slide(LEFT, FAR)],
        }
    }
}

#[test]
fn planners_added_from_outside_are_used_and_bad_plans_fall_back() {
    let plans = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = plans.clone();
    let mut app = boot_with(move |app| {
        let mut planners = app.world_mut().resource_mut::<RoomPlanners>();
        planners.add(
            "counting",
            Counting {
                inner: Wrong,
                plans: counted,
            },
        );
        planners.default = "counting".into();
    });
    let below = entity_at(&mut app, BELOW).unwrap();
    step(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        plans.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "it was asked"
    );
    // Its plan would leave the room wrong, so the walls rise and sink.
    assert!(wall_at(&mut app, LEFT).is_none() && wall_at(&mut app, BELOW).is_none());
    assert!(wall_at(&mut app, RIGHT).is_some() && wall_at(&mut app, FAR).is_some());
    assert!(app.world().get_entity(below).is_err());
}

/// Brings the new wall in from the floor below its cell and hides the wall
/// that goes inside the outer wall under it, while the top wall slides.
struct FromElsewhere;

/// Where `FromElsewhere` raises the new wall: floor in both layouts.
const ENTRY: IVec2 = IVec2::new(6, 2);

impl RoomPlanner for FromElsewhere {
    fn plan(&self, _: &SwitchContext) -> SwitchPlan {
        SwitchPlan {
            moves: vec![
                WallMove::slide(LEFT, RIGHT),
                WallMove::rise(ENTRY, FAR),
                WallMove::sink(BELOW, BOTTOM),
            ],
        }
    }
}

#[test]
fn walls_can_rise_elsewhere_and_sink_elsewhere() {
    let mut app = boot_with(|app| {
        app.add_plugins(ObjectAnimationPlugin);
        let mut planners = app.world_mut().resource_mut::<RoomPlanners>();
        planners.add("elsewhere", FromElsewhere);
        planners.default = "elsewhere".into();
    });
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

    // In the world the new wall is at its cell and the old one gone...
    let far = entity_at(&mut app, FAR).expect("new wall");
    assert_eq!(entity_at(&mut app, BOTTOM).map(|e| e == below), Some(false));
    assert!(entity_at(&mut app, BELOW).is_none());
    // ...while the new one is shown rising where it comes from, and the old
    // one sliding to where it goes, still standing.
    assert_eq!(shown_at(&mut app, far), ENTRY.as_vec2());
    assert!(height(&mut app, far) < 1.0);
    let going = shown_at(&mut app, below).y;
    assert!(
        going > BELOW.y as f32 && going < BOTTOM.y as f32,
        "going: {going}"
    );
    assert_eq!(height(&mut app, below), 1.0);

    // Once risen the new wall slides to its cell; once there the old one
    // sinks and goes. The turn waits for all of it.
    let mut risen_then_slid = false;
    for _ in 0..80 {
        app.update();
        let shown = shown_at(&mut app, far);
        if height(&mut app, far) == 1.0 && shown != ENTRY.as_vec2() && shown != FAR.as_vec2() {
            risen_then_slid = true;
        }
        if app.world().get_entity(below).is_err() {
            break;
        }
        assert!(
            app.world().resource::<TurnPacing>().animation().unwrap() > Duration::ZERO,
            "the turn waits"
        );
    }
    assert!(risen_then_slid, "the new wall rose before it slid");
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(shown_at(&mut app, far), FAR.as_vec2());
    assert!(
        app.world().get_entity(below).is_err(),
        "the old one sank and went"
    );
}

#[test]
fn walls_ride_rails_where_the_floor_has_them() {
    // Two rooms alike, switched by one plate: each has a pillar top left
    // and bottom right, going to top right and bottom left. Straight down
    // or up is shorter, and the bare room takes it; the top room has rails
    // along its top and bottom rows, and its pillars ride them.
    let mut app = boot_map(include_str!("../assets/maps/rails.txt"), |_| {});
    let (top, bottom) = (IVec2::new(2, 2), IVec2::new(2, 10));
    let (top_pillar, bottom_pillar) = (
        entity_at(&mut app, top).unwrap(),
        entity_at(&mut app, bottom).unwrap(),
    );

    // Rails lie flat and dim, joined like walls: under the pillar too.
    let world = app.world_mut();
    let rails: Vec<(IVec2, Glyph, Option<&Stature>)> = world
        .query_filtered::<(&Pos, &Glyph, Option<&Stature>), With<Rail>>()
        .iter(world)
        .map(|(pos, glyph, stature)| (pos.0, *glyph, stature))
        .collect();
    assert_eq!(rails.len(), 30);
    assert!(
        rails.iter().any(|(pos, _, _)| *pos == top),
        "under the pillar"
    );
    for (pos, glyph, stature) in &rails {
        assert_eq!(glyph.depth, 0, "{pos}: on the floor");
        assert_eq!(stature.map(|s| s.0), Some(0.0), "{pos}: flat");
        assert_ne!(glyph.ch, '=', "{pos}: autotiled");
        if (3..16).contains(&pos.x) {
            assert_eq!(glyph.ch, '═', "{pos}: a straight run");
        }
    }

    step(&mut app, KeyCode::ArrowRight);
    step(&mut app, KeyCode::ArrowRight);
    for _ in 0..5 {
        app.update();
    }
    let at = |app: &mut App, entity| app.world().get::<Pos>(entity).unwrap().0;
    assert_eq!(
        at(&mut app, top_pillar),
        IVec2::new(16, 2),
        "along the rail"
    );
    assert_eq!(
        at(&mut app, bottom_pillar),
        IVec2::new(2, 14),
        "straight down"
    );
}

#[test]
fn a_wall_gliding_most_of_the_way_across_the_map_glides_rather_than_jumps() {
    // `rails.txt` is 19 cells wide and its top pillar rides 14 cells: more
    // than half the map, which for anything else would be a step across the
    // map's wrapping edge, shown as a jump.
    let mut app = boot_map(include_str!("../assets/maps/rails.txt"), |app| {
        app.add_plugins(ObjectAnimationPlugin);
    });
    let pillar = entity_at(&mut app, IVec2::new(2, 2)).unwrap();
    step(&mut app, KeyCode::ArrowRight);
    step(&mut app, KeyCode::ArrowRight);
    let mut shown = Vec::new();
    for _ in 0..120 {
        app.update();
        shown.push(shown_at(&mut app, pillar).x);
    }
    assert!(
        shown.iter().any(|&x| x > 4.0 && x < 14.0),
        "seen on its way: {shown:?}"
    );
    assert_eq!(*shown.last().unwrap(), 16.0);
}
