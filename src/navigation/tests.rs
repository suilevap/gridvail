use bevy::prelude::*;

use super::path::{PathSearch, RouteOptions, RouteSearch, StepFn};
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
fn find<R: Rules>(
    nav: &NavMap,
    rules: &R,
    from: IVec2,
    to: IVec2,
) -> Option<(R::Cost, Vec<IVec2>)> {
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
    let avoid_row = StepFn::new(0, |_, to: Cell| Some(if to.y == 2 { 50 } else { 0 }));
    let far = IVec2::new(25, 2);
    let (_, direct) = find(nav, &terrain, PLAYER, far).unwrap();
    let (_, steered) = find(nav, &(terrain, avoid_row), PLAYER, far).unwrap();
    let on_row = |path: &[IVec2]| path.iter().filter(|p| p.y == 2).count();
    assert!(on_row(&steered) < on_row(&direct));
    // Leaving the start room must cross the row once; then only the goal.
    assert_eq!(on_row(&steered), 2);
}

#[test]
fn several_routes_between_two_places() {
    let app = boot();
    let nav = app.world().resource::<NavMap>();
    let terrain = Terrain::walls_and_doors(nav);
    let goal = IVec2::new(25, 8);
    let (cheapest, _) = find(nav, &terrain, PLAYER, goal).unwrap();

    let mut routes = RouteSearch::new(RouteOptions::default());
    routes.begin(nav.grid(), cell_of(PLAYER), cell_of(goal));
    let mut path = Vec::new();
    let mut found = Vec::new();
    while let Some(cost) = routes.next(&terrain, &mut path) {
        found.push((cost, path.clone()));
    }
    assert!(found.len() >= 2, "found {} routes", found.len());
    assert_eq!(found[0].0, cheapest);
    for (cost, path) in &found {
        assert_eq!(path.first(), Some(&cell_of(PLAYER)));
        assert_eq!(path.last(), Some(&cell_of(goal)));
        assert_eq!(*cost as usize, path.len() - 1);
        assert!(path.iter().all(|cell| nav.cell(*cell) == NavCell::Floor));
        assert!(path
            .windows(2)
            .all(|step| nav.grid().distance(step[0], step[1]) == 1));
    }
}
