//! The navigation service: how a tree gets somewhere without seeing the map.
//!
//! A tree asks by reporting `EnemyAct::GoTo(dest)`. The service splits the
//! work in two:
//!
//! - **Planning** is the expensive, replaceable part: a [`Planner`] finds a
//!   whole path over a [`MapSnapshot`]. It runs as a service [`Job`], so with
//!   `ServiceMode::Background` it takes as many frames as it needs on another
//!   thread while the game keeps rendering and taking turns. A smarter planner
//!   (one that weighs where enemies look, or where the player tends to go)
//!   plugs in here and may be slow.
//! - **Following** is cheap and happens every turn on the main thread: the
//!   next cell of the cached path, checked live for doors and actors.
//!
//! While a new plan is on its way the agent keeps following the old one, or
//! holds (`RouteStatus::Pending`) when it has none, so a slow plan costs that
//! agent a turn or two of thinking and never stalls the game. Path buffers
//! move between the agent and its job instead of being reallocated, so a warm
//! plan allocates nothing.
//!
//! The answer each turn lands in the agent's `Route`: the step `carry_out`
//! spends, and a `RouteAnswer` the gather copies into the blackboard for the
//! next turn.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::marker::PhantomData;
use std::sync::Arc;

use bevy::prelude::*;
use flatbt_bevy::prelude::BehaviorSystems;

use crate::model::*;
use crate::schedule::SimulationStep;

use super::carry_out;
use super::service::{Job, ServiceMode, Ticket};

/// What a cell is to a walker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cell {
    #[default]
    Floor,
    Wall,
    /// Passable after opening it, which takes a key.
    ClosedDoor,
    /// Another actor: passable once it moves.
    Actor,
}

/// The map as a planner or a follower sees it.
pub trait Terrain {
    /// Width and height; cells outside are walls.
    fn size(&self) -> IVec2;
    fn cell(&self, at: IVec2) -> Cell;
}

/// The map's static layout (walls and closed doors), owned, so a planning
/// job can take it to another thread. Actors are left out: they move every
/// turn, and following checks them live.
#[derive(Debug, Default)]
pub struct MapSnapshot {
    size: IVec2,
    cells: Vec<Cell>,
    /// `MapGrid::blocker_revision` this was taken at.
    revision: Option<u64>,
}

impl MapSnapshot {
    /// A snapshot with every cell read from `cell`; for tests and tools.
    pub fn from_fn(size: IVec2, cell: impl Fn(IVec2) -> Cell) -> Self {
        let cells = (0..size.y)
            .flat_map(|y| (0..size.x).map(move |x| IVec2::new(x, y)))
            .map(cell)
            .collect();
        Self {
            size,
            cells,
            revision: None,
        }
    }

    pub fn revision(&self) -> Option<u64> {
        self.revision
    }
}

impl Terrain for MapSnapshot {
    fn size(&self) -> IVec2 {
        self.size
    }

    fn cell(&self, at: IVec2) -> Cell {
        if at.x < 0 || at.y < 0 || at.x >= self.size.x || at.y >= self.size.y {
            return Cell::Wall;
        }
        self.cells[(at.y * self.size.x + at.x) as usize]
    }
}

/// The current snapshot, shared with every planning job.
#[derive(Resource, Default)]
pub struct NavSnapshot(pub Arc<MapSnapshot>);

/// Finds a path. May be slow: it runs as a service job.
pub trait Planner: Send + Sync + 'static {
    /// Fills `path` with the cells from `from` to `to`, both included, or
    /// leaves it empty when there is no way. `path` arrives cleared and keeps
    /// its capacity between plans.
    fn plan(&self, map: &MapSnapshot, from: IVec2, to: IVec2, path: &mut Vec<IVec2>);
}

/// Installs the navigation service with planner `P`.
pub struct NavigationPlugin<P>(PhantomData<fn() -> P>);

impl<P> Default for NavigationPlugin<P> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

/// The installed planner, shared with jobs.
#[derive(Resource)]
pub struct Navigation<P> {
    pub planner: Arc<P>,
}

impl<P: Planner + Default> Plugin for NavigationPlugin<P> {
    fn build(&self, app: &mut App) {
        app.init_resource::<ServiceMode>()
            .init_resource::<NavSnapshot>()
            .insert_resource(Navigation {
                planner: Arc::new(P::default()),
            })
            .add_systems(
                Update,
                (
                    (refresh_snapshot, collect_plans)
                        .chain()
                        .before(BehaviorSystems),
                    navigate::<P>.after(BehaviorSystems).before(carry_out),
                )
                    .in_set(SimulationStep::Decide),
            );
    }
}

/// The service's private state for one agent.
#[derive(Component, Default)]
pub struct Navigator {
    /// The plan being followed, from where it started to its destination.
    path: Vec<IVec2>,
    /// What `path` was planned for: destination and snapshot revision.
    planned: Option<(IVec2, Option<u64>)>,
    /// A buffer for the next plan.
    spare: Vec<IVec2>,
    /// A plan on its way.
    ticket: Option<Ticket<PlanResult>>,
    /// A plan that arrived since the agent last moved on.
    landed: Option<PlanResult>,
}

impl Navigator {
    /// Takes a plan that arrived, unless it starts somewhere the agent has
    /// already left while following the previous one: then the old path stays
    /// and a new plan is asked for from here.
    fn adopt(&mut self, result: PlanResult, at: IVec2) {
        if result.path.contains(&at) || !self.path.contains(&at) {
            self.spare = std::mem::replace(&mut self.path, result.path);
            self.planned = Some((result.to, result.revision));
        } else {
            self.spare = result.path;
        }
    }
}

#[derive(Default)]
struct PlanResult {
    to: IVec2,
    revision: Option<u64>,
    path: Vec<IVec2>,
}

struct PlanJob<P> {
    planner: Arc<P>,
    map: Arc<MapSnapshot>,
    from: IVec2,
    to: IVec2,
    path: Vec<IVec2>,
}

impl<P: Planner> Job for PlanJob<P> {
    type Output = PlanResult;

    fn run(mut self) -> PlanResult {
        self.path.clear();
        self.planner
            .plan(&self.map, self.from, self.to, &mut self.path);
        PlanResult {
            to: self.to,
            revision: self.map.revision,
            path: self.path,
        }
    }
}

/// Retakes the snapshot when walls or doors changed. Rare, and the only
/// place navigation allocates once warm.
pub fn refresh_snapshot(
    grid: Res<MapGrid>,
    doors: Query<&'static Door>,
    walls: Query<(), With<Wall>>,
    mut snapshot: ResMut<NavSnapshot>,
) {
    if snapshot.0.revision == Some(grid.blocker_revision) {
        return;
    }
    let live = LiveTerrain {
        grid: &grid,
        doors: &doors,
        walls: &walls,
    };
    let mut map = MapSnapshot::from_fn(live.size(), |at| match live.cell(at) {
        Cell::Actor => Cell::Floor,
        cell => cell,
    });
    map.revision = Some(grid.blocker_revision);
    snapshot.0 = Arc::new(map);
}

/// Picks up finished plans on whichever frame they finish. The agent adopts
/// one on its next turn.
pub fn collect_plans(mut agents: Query<&mut Navigator>) {
    for mut navigator in agents.iter_mut() {
        let Some(ticket) = navigator.ticket.as_mut() else {
            continue;
        };
        if let Some(result) = ticket.poll() {
            navigator.ticket = None;
            if let Some(stale) = navigator.landed.replace(result) {
                navigator.spare = stale.path;
            }
        }
    }
}

/// Answers this turn's `GoTo` requests: plans when the plan is missing or
/// stale and none is on its way, then follows whatever plan there is. Other
/// acts leave the last answer in place.
pub fn navigate<P: Planner>(
    mode: Res<ServiceMode>,
    navigation: Res<Navigation<P>>,
    snapshot: Res<NavSnapshot>,
    grid: Res<MapGrid>,
    doors: Query<&'static Door>,
    walls: Query<(), With<Wall>>,
    mut agents: Query<(&EnemyAct, &EnemyMind, &mut Route, &mut Navigator)>,
) {
    let live = LiveTerrain {
        grid: &grid,
        doors: &doors,
        walls: &walls,
    };
    for (act, mind, mut route, mut navigator) in agents.iter_mut() {
        let EnemyAct::GoTo(dest) = *act else {
            continue;
        };
        if !mind.has_turn {
            continue;
        }
        let Some(to) = mind.resolve(dest) else {
            *route = Route {
                answer: Some(RouteAnswer {
                    dest,
                    status: RouteStatus::Unreachable,
                }),
                step: IVec2::ZERO,
            };
            continue;
        };
        if let Some(result) = navigator.landed.take() {
            navigator.adopt(result, mind.pos);
        }
        let current = navigator.planned == Some((to, snapshot.0.revision))
            && navigator.path.contains(&mind.pos);
        if !current && navigator.ticket.is_none() {
            let job = PlanJob {
                planner: navigation.planner.clone(),
                map: snapshot.0.clone(),
                from: mind.pos,
                to,
                path: std::mem::take(&mut navigator.spare),
            };
            let mut ticket = Ticket::start(job, *mode);
            match ticket.poll() {
                Some(result) => navigator.adopt(result, mind.pos),
                None => navigator.ticket = Some(ticket),
            }
        }
        let (status, step) = follow(&navigator, &live, mind.pos, to);
        *route = Route {
            answer: Some(RouteAnswer { dest, status }),
            step,
        };
    }
}

/// This turn's step along the agent's plan, checked against the live map.
fn follow(
    navigator: &Navigator,
    live: &impl Terrain,
    at: IVec2,
    to: IVec2,
) -> (RouteStatus, IVec2) {
    if at == to {
        return (RouteStatus::Arrived, IVec2::ZERO);
    }
    let thinking = navigator.ticket.is_some();
    // Nothing left to follow while a new plan is on its way. An agent that
    // was already on its way keeps heading straight for the destination; one
    // that never had a plan holds.
    let rethink = || (RouteStatus::Pending, direct_step(live, at, to));
    let Some(here) = navigator.path.iter().position(|&cell| cell == at) else {
        return if !navigator.path.is_empty() {
            rethink()
        } else if thinking || navigator.planned.is_none() {
            (RouteStatus::Pending, IVec2::ZERO)
        } else {
            (RouteStatus::Unreachable, IVec2::ZERO)
        };
    };
    let ahead = &navigator.path[here + 1..];
    let Some(&next) = ahead.first() else {
        // At the end of an old plan whose destination has moved on.
        return rethink();
    };
    // The plan's last cell is always enterable: that is a bump or an attack.
    let last = navigator.path.last().copied();
    let closed = |cell: IVec2| Some(cell) != last && live.cell(cell) == Cell::ClosedDoor;
    if closed(next) {
        return (RouteStatus::Blocked(next), IVec2::ZERO);
    }
    if Some(next) != last && live.cell(next) == Cell::Actor {
        return (RouteStatus::Waiting, IVec2::ZERO);
    }
    let status = ahead
        .iter()
        .copied()
        .find(|&cell| closed(cell))
        .map_or(RouteStatus::Moving, RouteStatus::Blocked);
    (status, next - at)
}

/// A step that closes on `to` along the longer axis first, into a free cell
/// or `to` itself; `ZERO` when neither is free.
fn direct_step(live: &impl Terrain, at: IVec2, to: IVec2) -> IVec2 {
    let delta = to - at;
    let along_x = IVec2::new(delta.x.signum(), 0);
    let along_y = IVec2::new(0, delta.y.signum());
    let (first, second) = if delta.x.abs() >= delta.y.abs() {
        (along_x, along_y)
    } else {
        (along_y, along_x)
    };
    [first, second]
        .into_iter()
        .find(|&step| {
            step != IVec2::ZERO && (at + step == to || live.cell(at + step) == Cell::Floor)
        })
        .unwrap_or(IVec2::ZERO)
}

struct LiveTerrain<'a, 'w, 's> {
    grid: &'a MapGrid,
    doors: &'a Query<'w, 's, &'static Door>,
    walls: &'a Query<'w, 's, (), With<Wall>>,
}

impl Terrain for LiveTerrain<'_, '_, '_> {
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

/// Interim planner: Dijkstra over the snapshot, doors at a cost.
///
/// Stands in until the `navigation` module's `NavMap` search lands; a planner
/// over it replaces this by changing `NavigationPlugin::<GridPlanner>`. Its
/// search buffers are per thread, so a warm plan does not allocate on any.
#[derive(Default)]
pub struct GridPlanner;

#[derive(Default)]
struct Scratch {
    cost: Vec<u32>,
    came_from: Vec<u32>,
    frontier: BinaryHeap<Reverse<(u32, u32)>>,
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::default();
}

impl Planner for GridPlanner {
    fn plan(&self, map: &MapSnapshot, from: IVec2, to: IVec2, path: &mut Vec<IVec2>) {
        let size = map.size();
        let index = |p: IVec2| {
            (p.x >= 0 && p.y >= 0 && p.x < size.x && p.y < size.y)
                .then_some((p.y * size.x + p.x) as u32)
        };
        let cell_at = |i: u32| IVec2::new(i as i32 % size.x, i as i32 / size.x);
        let (Some(start), Some(goal)) = (index(from), index(to)) else {
            return;
        };
        SCRATCH.with_borrow_mut(|scratch| {
            let cells = (size.x * size.y) as usize;
            scratch.cost.clear();
            scratch.cost.resize(cells, u32::MAX);
            scratch.came_from.clear();
            scratch.came_from.resize(cells, u32::MAX);
            scratch.frontier.clear();
            scratch.cost[start as usize] = 0;
            scratch.frontier.push(Reverse((0, start)));
            while let Some(Reverse((cost, at))) = scratch.frontier.pop() {
                if at == goal {
                    break;
                }
                if cost > scratch.cost[at as usize] {
                    continue;
                }
                for step in STEPS {
                    let Some(next) = index(cell_at(at) + step) else {
                        continue;
                    };
                    // The destination itself is always enterable.
                    let extra = if next == goal {
                        0
                    } else {
                        match map.cell(cell_at(next)) {
                            Cell::Floor | Cell::Actor => 0,
                            Cell::ClosedDoor => DOOR_COST,
                            Cell::Wall => continue,
                        }
                    };
                    let reached = cost + 1 + extra;
                    if reached < scratch.cost[next as usize] {
                        scratch.cost[next as usize] = reached;
                        scratch.came_from[next as usize] = at;
                        scratch.frontier.push(Reverse((reached, next)));
                    }
                }
            }
            if scratch.cost[goal as usize] == u32::MAX {
                return;
            }
            let mut at = goal;
            path.push(cell_at(at));
            while at != start {
                at = scratch.came_from[at as usize];
                path.push(cell_at(at));
            }
            path.reverse();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `#` wall, `D` closed door, anything else floor.
    fn map(rows: &[&str]) -> MapSnapshot {
        let size = IVec2::new(rows[0].len() as i32, rows.len() as i32);
        MapSnapshot::from_fn(size, |at| {
            match rows[at.y as usize].as_bytes()[at.x as usize] {
                b'#' => Cell::Wall,
                b'D' => Cell::ClosedDoor,
                _ => Cell::Floor,
            }
        })
    }

    fn plan(rows: &[&str], from: IVec2, to: IVec2) -> Vec<IVec2> {
        let mut path = Vec::new();
        GridPlanner.plan(&map(rows), from, to, &mut path);
        path
    }

    #[test]
    fn walks_around_a_wall() {
        let rows = ["......", ".###..", "......"];
        let path = plan(&rows, IVec2::new(2, 2), IVec2::new(2, 0));
        assert_eq!(path.first(), Some(&IVec2::new(2, 2)));
        assert_eq!(path.last(), Some(&IVec2::new(2, 0)));
        assert!(!path.contains(&IVec2::new(2, 1)));
        assert_eq!(path.len(), 7, "{path:?}");
    }

    #[test]
    fn goes_through_the_door_on_the_only_way() {
        let rows = ["..#..", "..D..", "..#.."];
        let path = plan(&rows, IVec2::new(0, 1), IVec2::new(4, 1));
        assert!(path.contains(&IVec2::new(2, 1)), "{path:?}");
    }

    #[test]
    fn prefers_a_modest_detour_to_a_door() {
        let rows = [".....", "..#..", "..D..", "..#.."];
        let path = plan(&rows, IVec2::new(1, 2), IVec2::new(3, 2));
        assert!(!path.contains(&IVec2::new(2, 2)), "{path:?}");
    }

    #[test]
    fn walled_off_has_no_path() {
        assert!(plan(&["..#.."], IVec2::new(0, 0), IVec2::new(4, 0)).is_empty());
    }

    /// A planner run as a job keeps nothing but the buffer it was handed.
    #[test]
    fn plans_reuse_the_buffer_they_are_given() {
        let rows = [".....", "....."];
        let mut path = Vec::with_capacity(64);
        let capacity = path.capacity();
        GridPlanner.plan(&map(&rows), IVec2::ZERO, IVec2::new(4, 1), &mut path);
        assert_eq!(path.capacity(), capacity);
    }
}
