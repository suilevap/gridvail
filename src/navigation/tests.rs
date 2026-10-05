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

/// An open 5x3 room with one actor standing in the middle of row 1.
fn crowded_room() -> (MapGrid, NavMap) {
    let mut grid = MapGrid::new(5, 3);
    grid.set_with_blocking(IVec2::new(2, 1), Entity::from_raw_u32(7).unwrap(), false);
    let mut nav = NavMap::default();
    nav.rebuild(&grid, [], []);
    (grid, nav)
}

#[test]
fn a_crowd_cost_routes_around_actors() {
    let (grid, nav) = crowded_room();
    let (from, goal, actor) = (IVec2::new(0, 1), IVec2::new(4, 1), IVec2::new(2, 1));
    let mut planner = PathPlanner::default();
    let mut steps = Vec::new();

    assert!(planner.plan(&nav, from, goal, None, &mut steps));
    assert!(steps.contains(&actor), "actors are no obstacle: {steps:?}");

    let crowd = Crowd {
        terrain: Terrain::walls_and_doors(&nav),
        grid: &grid,
        goal: cell_of(goal),
        cost: 4,
    };
    assert!(planner.plan_with(&nav, &crowd, from, goal, &mut steps));
    assert!(!steps.contains(&actor), "walked through: {steps:?}");

    // An actor on the goal is the point of the walk, not in its way.
    let crowd = Crowd {
        goal: cell_of(actor),
        ..crowd
    };
    assert!(planner.plan_with(&nav, &crowd, from, actor, &mut steps));
    assert_eq!(steps.len(), 3, "{steps:?}");
}
