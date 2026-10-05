use bevy::prelude::*;

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
    world.entity_mut(enemy).remove::<Wander>();
    enemy
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
        if app
            .world()
            .get::<Destination>(enemy)
            .unwrap()
            .goal
            .is_none()
        {
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
fn an_unreachable_goal_is_dropped() {
    let mut app = boot_walking();
    let enemy = place_enemy(&mut app, IVec2::new(18, 2));
    app.world_mut()
        .get_mut::<Destination>(enemy)
        .unwrap()
        .go_to(CLOSET_KEY);
    let cells = walk(&mut app, enemy, 50);
    assert_eq!(cells, [IVec2::new(18, 2)], "no key, no way in");
    assert_eq!(app.world().get::<Destination>(enemy).unwrap().goal, None);
}
