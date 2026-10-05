use bevy::prelude::*;

use super::path::grid::Cell;
use super::path::{PathSearch, StepFn};
use super::*;
use crate::app::GamePlugin;

const PLAYER: IVec2 = IVec2::new(8, 5);
const CLOSET_DOOR: IVec2 = IVec2::new(18, 4);
const CLOSET_KEY: IVec2 = IVec2::new(18, 5);

fn boot() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins(GamePlugin);
    app.update();
    app
}

/// The cheapest path between two map positions under `rules`.
fn find<TestRules: Rules<Node = Cell>>(
    nav: &NavMap,
    rules: &TestRules,
    from: IVec2,
    to: IVec2,
) -> Option<(TestRules::Cost, Vec<IVec2>)> {
    let mut path = Vec::new();
    PathSearch::new()
        .find(nav.grid(), rules, cell_of(from), cell_of(to), &mut path)
        .map(|cost| (cost, path.into_iter().map(pos_of).collect()))
}

#[test]
fn nav_map_mirrors_walls_and_closed_doors() {
    let app = boot();
    let nav = app.world().resource::<NavMap>();
    assert_eq!((nav.grid().width(), nav.grid().height()), (80, 24));
    assert_eq!(nav.at(IVec2::ZERO), NavCell::Wall);
    assert_eq!(nav.at(PLAYER), NavCell::Floor, "actors are not obstacles");
    assert_eq!(nav.at(CLOSET_DOOR), NavCell::ClosedDoor);
    assert_eq!(nav.at(CLOSET_KEY), NavCell::Floor);
}

#[test]
fn closed_doors_block_or_cost_extra() {
    let app = boot();
    let nav = app.world().resource::<NavMap>();
    assert_eq!(
        find(nav, &Terrain::walls_and_doors(nav), PLAYER, CLOSET_KEY),
        None,
        "the closet is sealed by its door"
    );
    let (cost, path) = find(nav, &Terrain::through_doors(nav, 5), PLAYER, CLOSET_KEY).unwrap();
    assert!(path.contains(&CLOSET_DOOR));
    assert_eq!(cost as usize, path.len() - 1 + 5);
}

#[test]
fn opening_a_door_refreshes_the_nav_map() {
    let mut app = boot();
    let world = app.world_mut();
    let door = world
        .query::<(Entity, &Pos, &Door)>()
        .iter(world)
        .find(|(_, pos, _)| pos.0 == CLOSET_DOOR)
        .map(|(entity, ..)| entity)
        .unwrap();
    // What `open_doors` does to the world.
    world.get_mut::<Door>(door).unwrap().open = true;
    world.resource_mut::<MapGrid>().clear(CLOSET_DOOR);
    world.entity_mut(door).remove::<Collider>();
    app.update();

    let nav = app.world().resource::<NavMap>();
    assert_eq!(nav.at(CLOSET_DOOR), NavCell::Floor);
    assert!(find(nav, &Terrain::walls_and_doors(nav), PLAYER, CLOSET_KEY).is_some());
}

/// Rules with state: at most `max` closed doors per path.
struct DoorLimit<'a> {
    nav: &'a NavMap,
    max: u8,
}

impl Rules for DoorLimit<'_> {
    type Node = Cell;
    type Cost = u32;
    type State = u8;

    fn state_count(&self) -> usize {
        self.max as usize + 1
    }

    fn state_index(&self, doors: &u8) -> usize {
        *doors as usize
    }

    fn start_state(&self, _start: Cell) -> u8 {
        0
    }

    fn step(&self, _from: Cell, to: Cell, doors: &u8) -> Option<(u32, u8)> {
        let doors = doors + u8::from(self.nav.cell(to) == NavCell::ClosedDoor);
        (doors <= self.max).then_some((0, doors))
    }
}

#[test]
fn game_rules_compose() {
    let app = boot();
    let nav = app.world().resource::<NavMap>();
    let doors = |path: &[IVec2]| {
        path.iter()
            .filter(|p| nav.at(**p) == NavCell::ClosedDoor)
            .count()
    };
    let terrain = Terrain::through_doors(nav, 0);

    // Doors are free here, but at most one may be passed.
    let (_, path) = find(
        nav,
        &(terrain, DoorLimit { nav, max: 1 }),
        PLAYER,
        CLOSET_KEY,
    )
    .unwrap();
    assert_eq!(doors(&path), 1);
    assert_eq!(
        find(
            nav,
            &(terrain, DoorLimit { nav, max: 0 }),
            PLAYER,
            CLOSET_KEY
        ),
        None
    );

    // Steer around a place: row 2 costs a lot to enter.
    let avoid_row = StepFn::new(0u32, |_, to: Cell| Some(if to.y == 2 { 50 } else { 0 }));
    let far = IVec2::new(25, 2);
    let (_, direct) = find(nav, &terrain, PLAYER, far).unwrap();
    let (_, steered) = find(nav, &(terrain, avoid_row), PLAYER, far).unwrap();
    let on_row = |path: &[IVec2]| path.iter().filter(|p| p.y == 2).count();
    assert!(on_row(&steered) < on_row(&direct));
    // Leaving the start room must cross the row once; then only the goal.
    assert_eq!(on_row(&steered), 2);
}

/// Counts closed doors passed, up to one; fewer beats more.
struct DoorsUsed<'a>(&'a NavMap);

impl Rules for DoorsUsed<'_> {
    type Node = Cell;
    type Cost = u32;
    type State = u8;

    fn state_count(&self) -> usize {
        2
    }

    fn state_index(&self, doors: &u8) -> usize {
        *doors as usize
    }

    fn start_state(&self, _start: Cell) -> u8 {
        0
    }

    fn step(&self, _from: Cell, to: Cell, doors: &u8) -> Option<(u32, u8)> {
        let doors = doors + u8::from(self.0.cell(to) == NavCell::ClosedDoor);
        (doors <= 1).then_some((0, doors))
    }

    fn dominates(&self, dominant: usize, dominated: usize) -> bool {
        dominant < dominated
    }
}

#[test]
fn one_search_offers_the_door_and_the_long_way_round() {
    let app = boot();
    let nav = app.world().resource::<NavMap>();
    // Either side of the green door, whose wall has a gap far to the west.
    let (from, to) = (IVec2::new(72, 14), IVec2::new(72, 18));
    let door = IVec2::new(72, 16);
    assert_eq!(nav.at(door), NavCell::ClosedDoor);
    let rules = (Terrain::through_doors(nav, 5), DoorsUsed(nav));

    let mut search = PathSearch::new();
    let mut path = Vec::new();
    let mut found = Vec::new();
    assert!(search.begin(nav.grid(), &rules, cell_of(from), cell_of(to)));
    while let Some((cost, (_, doors))) = search.next(nav.grid(), &rules, &mut path) {
        found.push((
            cost,
            doors,
            path.iter().copied().map(pos_of).collect::<Vec<_>>(),
        ));
    }

    assert_eq!(found.len(), 2, "through the door, then around");
    let (door_cost, door_count, through) = &found[0];
    assert_eq!((*door_cost, *door_count), (4 + 5, 1));
    assert!(through.contains(&door));
    let (around_cost, around_count, around) = &found[1];
    assert_eq!(*around_count, 0);
    assert!(!around.contains(&door));
    assert!(around_cost > door_cost);
    assert!(
        around.iter().any(|p| p.x < 45),
        "around goes by the western gap"
    );
    for (_, _, path) in &found {
        assert_eq!(path.first(), Some(&from));
        assert_eq!(path.last(), Some(&to));
        assert!(path
            .windows(2)
            .all(|step| (step[1] - step[0]).abs().element_sum() == 1));
    }
}

/// The real game with fixed 16 ms frames, where turns follow each other
/// immediately: the player holds no token, so nothing waits for it.
fn boot_walking() -> App {
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
fn place_enemy(app: &mut App, at: IVec2) -> Entity {
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
fn walk(app: &mut App, enemy: Entity, frames: usize) -> Vec<IVec2> {
    let mut cells = vec![app.world().get::<Pos>(enemy).unwrap().0];
    for _ in 0..frames {
        app.update();
        let pos = app.world().get::<Pos>(enemy).unwrap().0;
        if cells.last() != Some(&pos) {
            cells.push(pos);
        }
        if app.world().get::<PathFollow>(enemy).unwrap().goal.is_none() {
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
        .get_mut::<PathFollow>(enemy)
        .unwrap()
        .go_to(goal);
    let cells = walk(&mut app, enemy, 400);
    assert_eq!(cells.last(), Some(&goal), "walked {cells:?}");
    assert!(cells
        .windows(2)
        .all(|step| (step[1] - step[0]).abs().element_sum() == 1));
    let nav = app.world().resource::<NavMap>();
    let (cost, _) = find(nav, &Terrain::walls_and_doors(nav), start, goal).unwrap();
    assert_eq!(cells.len() - 1, cost as usize, "took the shortest way");
}

#[test]
fn an_enemy_with_a_key_opens_a_door_on_its_way() {
    let mut app = boot_walking();
    let enemy = place_enemy(&mut app, IVec2::new(18, 2));
    let key = app.world_mut().spawn((Active, Item, Key)).id();
    let world = app.world_mut();
    world.get_mut::<Inventory>(enemy).unwrap().0.push(key);
    world
        .get_mut::<PathFollow>(enemy)
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
        .get_mut::<PathFollow>(enemy)
        .unwrap()
        .go_to(CLOSET_KEY);
    let cells = walk(&mut app, enemy, 50);
    assert_eq!(cells, [IVec2::new(18, 2)], "no key, no way in");
    assert_eq!(app.world().get::<PathFollow>(enemy).unwrap().goal, None);
}

#[test]
fn wandering_enemies_keep_moving_to_new_goals() {
    let mut app = boot_walking();
    let world = app.world_mut();
    let enemies: Vec<(Entity, IVec2)> = world
        .query_filtered::<(Entity, &Pos), With<Enemy>>()
        .iter(world)
        .map(|(entity, pos)| (entity, pos.0))
        .collect();
    let mut goals = vec![Vec::new(); enemies.len()];
    for _ in 0..300 {
        app.update();
        for (i, (enemy, _)) in enemies.iter().enumerate() {
            let goal = app.world().get::<PathFollow>(*enemy).unwrap().goal;
            if let Some(goal) = goal {
                if goals[i].last() != Some(&goal) {
                    goals[i].push(goal);
                }
            }
        }
    }
    let nav = app.world().resource::<NavMap>();
    for (i, (enemy, start)) in enemies.iter().enumerate() {
        assert!(goals[i].len() >= 2, "enemy {i} had goals {:?}", goals[i]);
        assert!(goals[i].iter().all(|goal| nav.at(*goal) == NavCell::Floor));
        assert_ne!(app.world().get::<Pos>(*enemy).unwrap().0, *start);
    }
}
