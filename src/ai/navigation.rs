//! The navigation service: how a tree gets somewhere without seeing the map.
//!
//! A tree asks by reporting `EnemyAct::GoTo(dest)`. Right after the tick,
//! `navigate` answers every such request through the installed [`Router`]:
//! it stores this turn's step in the agent's `Route`, which `carry_out` then
//! spends, and an answer the gather copies into the blackboard for the next
//! turn (`EnemyMind::route`). So the step lands in the same turn and the tree
//! reacts to what the route ran into (a closed door, no way at all) on the
//! next one.
//!
//! The same shape fits any service a tree needs: a request in the act, a
//! system after the tick that does the work, and an answer the gather hands
//! back. The router is the replaceable part, picked by
//! `NavigationPlugin::<R>`.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::marker::PhantomData;

use bevy::ecs::component::Mutable;
use bevy::prelude::*;
use flatbt_bevy::prelude::BehaviorSystems;

use crate::model::*;
use crate::schedule::SimulationStep;

use super::carry_out;

/// What a cell is to a walker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Floor,
    Wall,
    /// Passable after opening it, which takes a key.
    ClosedDoor,
    /// Another actor: passable once it moves.
    Actor,
}

/// The map as a router sees it.
pub trait Terrain {
    /// Width and height; cells outside are walls.
    fn size(&self) -> IVec2;
    fn cell(&self, at: IVec2) -> Cell;
}

/// Answers one navigation request: where to step from `from` toward `to`.
///
/// Takes `&mut self` so a router can keep its search buffers between
/// requests and stay allocation-free once warm.
pub trait Router: Resource<Mutability = Mutable> {
    fn route(&mut self, terrain: &impl Terrain, from: IVec2, to: IVec2) -> (RouteStatus, IVec2);
}

/// Installs the navigation service with router `R`.
pub struct NavigationPlugin<R>(PhantomData<fn() -> R>);

impl<R> Default for NavigationPlugin<R> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<R: Router + Default> Plugin for NavigationPlugin<R> {
    fn build(&self, app: &mut App) {
        app.init_resource::<R>().add_systems(
            Update,
            navigate::<R>
                .after(BehaviorSystems)
                .before(carry_out)
                .in_set(SimulationStep::Decide),
        );
    }
}

/// Answers this turn's `GoTo` requests. Other acts leave the last answer in
/// place, so a tree can still read what its route ran into.
pub fn navigate<R: Router>(
    mut router: ResMut<R>,
    grid: Res<MapGrid>,
    doors: Query<&'static Door>,
    walls: Query<(), With<Wall>>,
    mut agents: Query<(&EnemyAct, &EnemyMind, &mut Route)>,
) {
    let terrain = GridTerrain {
        grid: &grid,
        doors: &doors,
        walls: &walls,
    };
    for (act, mind, mut route) in agents.iter_mut() {
        let EnemyAct::GoTo(dest) = *act else {
            continue;
        };
        if !mind.has_turn {
            continue;
        }
        let (status, step) = match mind.resolve(dest) {
            Some(to) => router.route(&terrain, mind.pos, to),
            None => (RouteStatus::Unreachable, IVec2::ZERO),
        };
        route.set_if_neq(Route {
            answer: Some(RouteAnswer { dest, status }),
            step,
        });
    }
}

struct GridTerrain<'a, 'w, 's> {
    grid: &'a MapGrid,
    doors: &'a Query<'w, 's, &'static Door>,
    walls: &'a Query<'w, 's, (), With<Wall>>,
}

impl Terrain for GridTerrain<'_, '_, '_> {
    fn size(&self) -> IVec2 {
        IVec2::new(self.grid.width, self.grid.height)
    }

    fn cell(&self, at: IVec2) -> Cell {
        let Some(occupant) = self.grid.get(at) else {
            return if self.grid.is_valid(at) {
                Cell::Floor
            } else {
                Cell::Wall
            };
        };
        if self.walls.contains(occupant) {
            Cell::Wall
        } else if self.doors.get(occupant).is_ok_and(|door| !door.open) {
            Cell::ClosedDoor
        } else {
            Cell::Actor
        }
    }
}

/// Extra cost of a step through a closed door: a detour this much longer
/// still wins.
pub const DOOR_COST: u32 = 8;
/// Extra cost of a step through another actor, which will likely move.
pub const ACTOR_COST: u32 = 2;

/// Interim router: Dijkstra over the grid, replanned every request.
///
/// Stands in until the `navigation` module's `NavMap` search lands; a router
/// over it replaces this by changing `NavigationPlugin::<GridRouter>`.
/// Buffers are kept between requests, so a warm search does not allocate.
#[derive(Resource, Default)]
pub struct GridRouter {
    cost: Vec<u32>,
    came_from: Vec<u32>,
    frontier: BinaryHeap<Reverse<(u32, u32)>>,
}

impl Router for GridRouter {
    fn route(&mut self, terrain: &impl Terrain, from: IVec2, to: IVec2) -> (RouteStatus, IVec2) {
        if from == to {
            return (RouteStatus::Arrived, IVec2::ZERO);
        }
        let size = terrain.size();
        let index = |p: IVec2| {
            (p.x >= 0 && p.y >= 0 && p.x < size.x && p.y < size.y)
                .then_some((p.y * size.x + p.x) as u32)
        };
        let cell_at = |i: u32| IVec2::new(i as i32 % size.x, i as i32 / size.x);
        let (Some(start), Some(goal)) = (index(from), index(to)) else {
            return (RouteStatus::Unreachable, IVec2::ZERO);
        };

        let cells = (size.x * size.y) as usize;
        self.cost.clear();
        self.cost.resize(cells, u32::MAX);
        self.came_from.clear();
        self.came_from.resize(cells, u32::MAX);
        self.frontier.clear();
        self.cost[start as usize] = 0;
        self.frontier.push(Reverse((0, start)));
        while let Some(Reverse((cost, at))) = self.frontier.pop() {
            if at == goal {
                break;
            }
            if cost > self.cost[at as usize] {
                continue;
            }
            for step in STEPS {
                let Some(next) = index(cell_at(at) + step) else {
                    continue;
                };
                // The destination itself is always enterable: stepping into
                // it is how a bump or an attack happens.
                let extra = if next == goal {
                    0
                } else {
                    match terrain.cell(cell_at(next)) {
                        Cell::Floor => 0,
                        Cell::Actor => ACTOR_COST,
                        Cell::ClosedDoor => DOOR_COST,
                        Cell::Wall => continue,
                    }
                };
                let reached = cost + 1 + extra;
                if reached < self.cost[next as usize] {
                    self.cost[next as usize] = reached;
                    self.came_from[next as usize] = at;
                    self.frontier.push(Reverse((reached, next)));
                }
            }
        }
        if self.cost[goal as usize] == u32::MAX {
            return (RouteStatus::Unreachable, IVec2::ZERO);
        }

        // Walk back to the first step, noting the closed door nearest to us.
        let (mut at, mut first) = (goal, goal);
        let mut door = None;
        while at != start {
            if at != goal && terrain.cell(cell_at(at)) == Cell::ClosedDoor {
                door = Some(cell_at(at));
            }
            first = at;
            at = self.came_from[at as usize];
        }
        let next = cell_at(first);
        match (first != goal).then(|| terrain.cell(next)) {
            Some(Cell::ClosedDoor) => (RouteStatus::Blocked(next), IVec2::ZERO),
            Some(Cell::Actor) => (RouteStatus::Waiting, IVec2::ZERO),
            _ => (
                door.map_or(RouteStatus::Moving, RouteStatus::Blocked),
                next - from,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `#` wall, `D` closed door, `a` actor, anything else floor.
    struct Ascii(Vec<Vec<char>>);

    impl Ascii {
        fn new(rows: &[&str]) -> Self {
            Self(rows.iter().map(|row| row.chars().collect()).collect())
        }
    }

    impl Terrain for Ascii {
        fn size(&self) -> IVec2 {
            IVec2::new(self.0[0].len() as i32, self.0.len() as i32)
        }

        fn cell(&self, at: IVec2) -> Cell {
            match self.0[at.y as usize][at.x as usize] {
                '#' => Cell::Wall,
                'D' => Cell::ClosedDoor,
                'a' => Cell::Actor,
                _ => Cell::Floor,
            }
        }
    }

    fn route(rows: &[&str], from: IVec2, to: IVec2) -> (RouteStatus, IVec2) {
        GridRouter::default().route(&Ascii::new(rows), from, to)
    }

    #[test]
    fn walks_around_a_wall() {
        let rows = ["......", ".###..", "......"];
        // Straight up the middle is walled; the first step goes around.
        let (status, step) = route(&rows, IVec2::new(2, 2), IVec2::new(2, 0));
        assert_eq!(status, RouteStatus::Moving);
        assert!(step == IVec2::X || step == IVec2::NEG_X, "{step}");
    }

    #[test]
    fn reports_the_door_on_the_only_way() {
        let rows = ["..#..", "..D..", "..#.."];
        let door = IVec2::new(2, 1);
        assert_eq!(
            route(&rows, IVec2::new(0, 1), IVec2::new(4, 1)),
            (RouteStatus::Blocked(door), IVec2::X)
        );
        // At the door there is no step to take until it opens.
        assert_eq!(
            route(&rows, IVec2::new(1, 1), IVec2::new(4, 1)),
            (RouteStatus::Blocked(door), IVec2::ZERO)
        );
    }

    #[test]
    fn prefers_a_modest_detour_to_a_door() {
        let rows = [".....", "..#..", "..D..", "..#.."];
        let (status, _) = route(&rows, IVec2::new(1, 2), IVec2::new(3, 2));
        assert_eq!(status, RouteStatus::Moving);
    }

    #[test]
    fn a_door_at_the_destination_is_not_in_the_way() {
        let rows = ["..D.."];
        let (status, step) = route(&rows, IVec2::new(0, 0), IVec2::new(2, 0));
        assert_eq!((status, step), (RouteStatus::Moving, IVec2::X));
    }

    #[test]
    fn waits_for_an_actor_on_the_only_way() {
        let rows = ["#.#", ".a.", "#.#"];
        let (status, _) = route(&rows, IVec2::new(1, 0), IVec2::new(1, 2));
        assert_eq!(status, RouteStatus::Waiting);
    }

    #[test]
    fn walled_off_is_unreachable() {
        let rows = ["..#.."];
        let (status, _) = route(&rows, IVec2::new(0, 0), IVec2::new(4, 0));
        assert_eq!(status, RouteStatus::Unreachable);
    }
}
