use bevy::prelude::*;

use flatbt_bevy::prelude::BehaviorCommands;

use crate::ai::enemy_tree;
use crate::app::GamePlugin;
use crate::model::*;
use crate::navigation::{NavMap, PathPlanner};

const CLOSET_DOOR: IVec2 = IVec2::new(18, 4);
const CLOSET_KEY: IVec2 = IVec2::new(18, 5);

/// The real game with fixed 16 ms frames, where turns follow each other
/// immediately: the player holds no token, so nothing waits for it.
pub(crate) fn boot_walking() -> App {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_plugins(GamePlugin);
    app.update();
    let world = app.world_mut();
    for mut tokens in world
        .query_filtered::<&mut Tokens, With<Player>>()
        .iter_mut(world)
    {
        *tokens = Tokens {
            count: 0,
            recharge: 0,
        };
    }
    app
}

/// Moves `enemy` to `at`, keeping the occupancy grid consistent, and stops
/// every other enemy so none gets in the way or holds up turns.
pub(crate) fn place_enemy(app: &mut App, at: IVec2) -> Entity {
    let world = app.world_mut();
    let enemies: Vec<Entity> = world
        .query_filtered::<Entity, With<Enemy>>()
        .iter(world)
        .collect();
    let enemy = enemies[0];
    for &other in &enemies[1..] {
        world.entity_mut(other).remove::<Active>();
        // Unspent tokens would make every turn wait for the timeout.
        *world.get_mut::<Tokens>(other).unwrap() = Tokens {
            count: 0,
            recharge: 0,
        };
    }
    let old = world.get::<Pos>(enemy).unwrap().0;
    let mut grid = world.resource_mut::<MapGrid>();
    grid.clear(old);
    grid.set_with_blocking(at, enemy, false);
    world.get_mut::<Pos>(enemy).unwrap().0 = at;
    world.get_mut::<PrevPos>(enemy).unwrap().0 = at;
    // Under test control: no wandering, no tree setting destinations, and
    // paths that ignore actors, as locomotion does by default.
    world
        .entity_mut(enemy)
        .remove::<Wander>()
        .stop_behavior(enemy_tree);
    world.get_mut::<TraversalPrefs>(enemy).unwrap().crowd_cost = None;
    enemy
}

pub(crate) fn status(app: &App, enemy: Entity) -> WalkStatus {
    app.world().get::<PathFollow>(enemy).unwrap().status
}

/// Runs frames until `enemy` stops walking; returns every cell it entered.
pub(crate) fn walk(app: &mut App, enemy: Entity, frames: usize) -> Vec<IVec2> {
    let mut cells = vec![app.world().get::<Pos>(enemy).unwrap().0];
    for _ in 0..frames {
        app.update();
        let pos = app.world().get::<Pos>(enemy).unwrap().0;
        if cells.last() != Some(&pos) {
            cells.push(pos);
        }
        if status(app, enemy).is_done() {
            break;
        }
    }
    cells
}

#[test]
fn an_enemy_walks_its_path_to_the_goal() {
    let mut app = boot_walking();
    let start = IVec2::new(3, 5);
    let goal = IVec2::new(25, 8);
    let enemy = place_enemy(&mut app, start);
    app.world_mut()
        .get_mut::<Destination>(enemy)
        .unwrap()
        .go_to(goal);
    let cells = walk(&mut app, enemy, 400);
    assert_eq!(cells.last(), Some(&goal), "walked {cells:?}");
    assert_eq!(status(&app, enemy), WalkStatus::Arrived);
    assert_eq!(
        app.world().get::<Destination>(enemy).unwrap().goal(),
        Some(goal),
        "locomotion reports, it does not change the destination"
    );
    assert!(cells
        .windows(2)
        .all(|step| (step[1] - step[0]).abs().element_sum() == 1));
    let nav = app.world().resource::<NavMap>();
    let mut shortest = Vec::new();
    assert!(PathPlanner::default().plan(nav, start, goal, None, &mut shortest));
    assert_eq!(cells, shortest, "took the planned shortest way");
}

#[test]
fn an_enemy_with_a_key_opens_a_door_on_its_way() {
    let mut app = boot_walking();
    let enemy = place_enemy(&mut app, IVec2::new(18, 2));
    let key = app.world_mut().spawn((Active, Item, Key)).id();
    let world = app.world_mut();
    world.get_mut::<Inventory>(enemy).unwrap().0.push(key);
    world
        .get_mut::<Destination>(enemy)
        .unwrap()
        .go_to(CLOSET_KEY);
    let cells = walk(&mut app, enemy, 400);

    assert_eq!(cells.last(), Some(&CLOSET_KEY), "walked {cells:?}");
    assert_eq!(status(&app, enemy), WalkStatus::Arrived);
    assert!(cells.contains(&CLOSET_DOOR));
    let world = app.world();
    assert!(
        world.get_entity(key).is_err(),
        "the key was spent on the door"
    );
    let carried = &world.get::<Inventory>(enemy).unwrap().0;
    assert_eq!(carried.len(), 1, "picked up the closet's key");
    assert_ne!(carried[0], key);
}

#[test]
fn an_unreachable_goal_is_reported() {
    let mut app = boot_walking();
    let enemy = place_enemy(&mut app, IVec2::new(18, 2));
    app.world_mut()
        .get_mut::<Destination>(enemy)
        .unwrap()
        .go_to(CLOSET_KEY);
    let cells = walk(&mut app, enemy, 50);
    assert_eq!(cells, [IVec2::new(18, 2)], "no key, no way in");
    assert_eq!(status(&app, enemy), WalkStatus::Unreachable);
    assert_eq!(
        app.world().get::<Destination>(enemy).unwrap().goal(),
        Some(CLOSET_KEY)
    );

    // Gaining a key changes what doors cost, which means planning again.
    let key = app.world_mut().spawn((Active, Item, Key)).id();
    app.world_mut()
        .get_mut::<Inventory>(enemy)
        .unwrap()
        .0
        .push(key);
    let cells = walk(&mut app, enemy, 400);
    assert_eq!(cells.last(), Some(&CLOSET_KEY), "walked {cells:?}");
    assert_eq!(status(&app, enemy), WalkStatus::Arrived);
}

#[test]
fn a_blocked_walk_is_reported_and_can_be_retried() {
    let mut app = boot_walking();
    let start = IVec2::new(3, 5);
    let goal = IVec2::new(3, 1);
    let enemy = place_enemy(&mut app, start);
    // Something in the way that the map snapshot does not know about.
    let in_the_way = IVec2::new(3, 4);
    let obstacle = app
        .world_mut()
        .spawn((Active, Collider, Pos(in_the_way)))
        .id();
    app.world_mut()
        .resource_mut::<MapGrid>()
        .set(in_the_way, obstacle);
    app.world_mut()
        .get_mut::<Destination>(enemy)
        .unwrap()
        .go_to(goal);
    let cells = walk(&mut app, enemy, 200);
    assert_eq!(cells, [start], "never got past the obstacle");
    assert_eq!(status(&app, enemy), WalkStatus::Blocked);

    // Once it is gone, asking again for the same goal is a new request.
    app.world_mut().resource_mut::<MapGrid>().clear(in_the_way);
    app.world_mut().despawn(obstacle);
    app.world_mut()
        .get_mut::<Destination>(enemy)
        .unwrap()
        .go_to(goal);
    let cells = walk(&mut app, enemy, 200);
    assert_eq!(cells.last(), Some(&goal), "walked {cells:?}");
    assert_eq!(status(&app, enemy), WalkStatus::Arrived);
}

/// The portals map with an enemy at `at` in room B under test control.
/// Rooms A and B are joined only by portals.
fn boot_portals(enemy_portals: bool, at: IVec2) -> (App, Entity) {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .insert_resource(crate::app::MapText(include_str!(
            "../../assets/maps/portals.txt"
        )))
        .insert_resource(crate::ai::PortalPolicy {
            enemies: enemy_portals,
        })
        .add_plugins(GamePlugin);
    app.update();
    let world = app.world_mut();
    for mut tokens in world
        .query_filtered::<&mut Tokens, With<Player>>()
        .iter_mut(world)
    {
        *tokens = Tokens {
            count: 0,
            recharge: 0,
        };
    }
    let enemy = place_enemy(&mut app, at);
    (app, enemy)
}

#[test]
fn an_enemy_allowed_through_portals_walks_through_one() {
    // From room B to the middle of room A: only portals lead there.
    let start = IVec2::new(45, 17);
    let goal = IVec2::new(20, 8);
    let (mut app, enemy) = boot_portals(true, start);
    app.update();
    assert!(app.world().get::<TraversalPrefs>(enemy).unwrap().portals);
    app.world_mut()
        .get_mut::<Destination>(enemy)
        .unwrap()
        .go_to(goal);
    let cells = walk(&mut app, enemy, 600);
    assert_eq!(cells.last(), Some(&goal), "walked {cells:?}");
    assert_eq!(status(&app, enemy), WalkStatus::Arrived);
    // Every move is a step, except where it came out of a portal.
    let nav = app.world().resource::<NavMap>();
    let hops = cells
        .windows(2)
        .filter(|step| (step[1] - step[0]).abs().element_sum() != 1)
        .inspect(|step| {
            assert!(
                nav.step_toward(step[0], step[1]).is_some(),
                "{:?} is neither a step nor a portal",
                step
            )
        })
        .count();
    assert!(hops >= 1, "went through no portal: {cells:?}");
}

#[test]
fn enemies_keep_out_of_portals_unless_allowed() {
    let start = IVec2::new(45, 17);
    let (mut app, enemy) = boot_portals(false, start);
    app.update();
    assert!(!app.world().get::<TraversalPrefs>(enemy).unwrap().portals);
    app.world_mut()
        .get_mut::<Destination>(enemy)
        .unwrap()
        .go_to(IVec2::new(20, 8));
    walk(&mut app, enemy, 100);
    assert_eq!(status(&app, enemy), WalkStatus::Unreachable);
    assert_eq!(app.world().get::<Pos>(enemy).unwrap().0, start);
}
