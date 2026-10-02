use bevy::prelude::*;

use super::*;
use crate::app::GamePlugin;
use crate::foundation::path::StepCost;

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

#[test]
fn nav_map_mirrors_walls_and_closed_doors() {
    let app = boot();
    let nav = app.world().resource::<NavMap>();
    assert_eq!(nav.shape().width, 80);
    assert_eq!(nav.cell(IVec2::ZERO), NavCell::Wall);
    assert_eq!(nav.cell(PLAYER), NavCell::Floor, "actors are not obstacles");
    assert_eq!(nav.cell(CLOSET_DOOR), NavCell::ClosedDoor);
    assert_eq!(nav.cell(CLOSET_KEY), NavCell::Floor);
}

#[test]
fn closed_doors_block_or_cost_extra() {
    let app = boot();
    let nav = app.world().resource::<NavMap>();
    assert_eq!(
        nav.find_path(&Terrain::walls_and_doors(nav), PLAYER, CLOSET_KEY),
        None,
        "the closet is sealed by its door"
    );
    let path = nav
        .find_path(&Terrain::through_doors(nav, 5), PLAYER, CLOSET_KEY)
        .unwrap();
    assert!(path.cells.contains(&CLOSET_DOOR));
    assert_eq!(path.cost as usize, path.len() + 5);
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
    assert_eq!(nav.cell(CLOSET_DOOR), NavCell::Floor);
    assert!(nav
        .find_path(&Terrain::walls_and_doors(nav), PLAYER, CLOSET_KEY)
        .is_some());
}

/// A rule carrying its own state: at most `max` closed doors per path.
struct DoorLimit<'a> {
    nav: &'a NavMap,
    max: u8,
}

impl CostModel for DoorLimit<'_> {
    type State = u8;

    fn initial_state(&self, _start: IVec2) -> u8 {
        0
    }

    fn step(&self, _from: IVec2, to: IVec2, doors: &u8) -> Option<(Cost, u8)> {
        let doors = doors + u8::from(self.nav.cell(to) == NavCell::ClosedDoor);
        (doors <= self.max).then_some((0, doors))
    }
}

#[test]
fn game_models_compose() {
    let app = boot();
    let nav = app.world().resource::<NavMap>();
    let doors = |path: &Path| {
        path.cells
            .iter()
            .filter(|cell| nav.cell(**cell) == NavCell::ClosedDoor)
            .count()
    };
    let terrain = Terrain::through_doors(nav, 0);

    // Doors are free here, but at most one may be passed.
    let one_door = (terrain, DoorLimit { nav, max: 1 });
    let path = nav.find_path(&one_door, PLAYER, CLOSET_KEY).unwrap();
    assert_eq!(doors(&path), 1);
    let none = (terrain, DoorLimit { nav, max: 0 });
    assert_eq!(nav.find_path(&none, PLAYER, CLOSET_KEY), None);

    // Steer around a place: row 2 costs a lot to enter.
    let avoid_row = StepCost::new(0, |_, to: IVec2| Some(if to.y == 2 { 50 } else { 0 }));
    let far = IVec2::new(25, 2);
    let direct = nav.find_path(&terrain, PLAYER, far).unwrap();
    let steered = nav.find_path(&(terrain, avoid_row), PLAYER, far).unwrap();
    let on_row = |path: &Path| path.cells.iter().filter(|cell| cell.y == 2).count();
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

    let shortest = nav.k_shortest_paths(&terrain, PLAYER, goal, 4);
    assert_eq!(shortest.len(), 4);
    assert!(shortest.windows(2).all(|pair| pair[0].cost <= pair[1].cost));

    let routes = nav.alternative_paths(&terrain, PLAYER, goal, Alternatives::default());
    assert!(routes.len() >= 2, "found {} routes", routes.len());
    assert_eq!(routes[0].cost, shortest[0].cost);
    for path in shortest.iter().chain(&routes) {
        assert_eq!(path.cells.first(), Some(&PLAYER));
        assert_eq!(path.cells.last(), Some(&goal));
        assert!(path
            .cells
            .iter()
            .all(|cell| nav.cell(*cell) == NavCell::Floor));
        assert!(path
            .cells
            .windows(2)
            .all(|step| nav.shape().distance(step[0], step[1]) == 1));
    }
}
