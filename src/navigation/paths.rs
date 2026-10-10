//! Paths as a service: [`PathService::plan`] promises a [`Path`], planned as
//! its [`Runner`] says (inline by default).
//!
//! The service plans over its own copies of the map (the [`NavMap`] and who
//! stands where, [`OccupancyMap`]), so a plan can run on another thread while
//! the game goes on. [`share_paths`] keeps those copies current.
//!
//! A [`Path`] is shared, not copied: clones are cheap handles to the same
//! cells, compared by identity. Its buffer goes back to a pool when the last
//! handle drops, so planning reuses memory instead of allocating each time.

use std::cell::RefCell;
use std::fmt;
use std::sync::{Arc, Mutex, RwLock, Weak};

use bevy::prelude::*;

use super::{cell_of, Crowd, NavMap, OccupancyMap, PathPlanner, Terrain};
use crate::model::{MapGrid, TraversalPrefs};
use crate::service::{Promise, Runner, Services};

/// Buffers kept for reuse, enough for every enemy's path and a few in flight.
const POOLED_PATHS: usize = 64;

/// A planned path: the cells from where it was planned to the goal.
pub struct Path {
    cells: Option<Arc<PathCells>>,
}

struct PathCells {
    cells: Vec<IVec2>,
    pool: Weak<PathPool>,
}

#[derive(Default)]
struct PathPool {
    free: Mutex<Vec<Arc<PathCells>>>,
}

impl Path {
    pub fn cells(&self) -> &[IVec2] {
        self.cells.as_ref().map_or(&[], |cells| &cells.cells)
    }

    /// Where the path leads.
    pub fn end(&self) -> Option<IVec2> {
        self.cells().last().copied()
    }

    /// The cell after `pos`, if `pos` is on the path and not its end.
    pub fn next_after(&self, pos: IVec2) -> Option<IVec2> {
        let cells = self.cells();
        let here = cells.iter().position(|&cell| cell == pos)?;
        cells.get(here + 1).copied()
    }

    pub fn contains(&self, pos: IVec2) -> bool {
        self.cells().contains(&pos)
    }
}

impl Clone for Path {
    fn clone(&self) -> Self {
        Self {
            cells: self.cells.clone(),
        }
    }
}

/// The same path, not merely the same cells: comparing is cheap, and a fresh
/// plan is a new path even where it repeats the old one.
impl PartialEq for Path {
    fn eq(&self, other: &Self) -> bool {
        match (&self.cells, &other.cells) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl Eq for Path {}

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cells = self.cells();
        write!(f, "Path({} cells", cells.len())?;
        if let (Some(start), Some(end)) = (cells.first(), cells.last()) {
            write!(f, ", {start} to {end}")?;
        }
        write!(f, ")")
    }
}

impl Drop for Path {
    /// The last handle gives the buffer back to its pool.
    fn drop(&mut self) {
        let Some(cells) = self.cells.take() else {
            return;
        };
        if Arc::strong_count(&cells) != 1 {
            return;
        }
        if let Some(pool) = cells.pool.upgrade() {
            if let Ok(mut free) = pool.free.lock() {
                free.push(cells);
            }
        }
    }
}

/// Plans paths, over copies of the map it keeps.
pub struct PathService {
    map: RwLock<SharedMap>,
    pool: Arc<PathPool>,
    runner: Runner,
}

/// The copies plans run over; each plan holds on to the ones it started with.
#[derive(Default)]
struct SharedMap {
    nav: Arc<NavMap>,
    occupancy: Arc<OccupancyMap>,
}

impl Default for PathService {
    /// Plans inline: the path is there on the tick that asked.
    fn default() -> Self {
        Self::with_runner(Runner::Inline)
    }
}

impl fmt::Debug for PathService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathService")
            .field("runner", &self.runner)
            .finish_non_exhaustive()
    }
}

thread_local! {
    /// Planning memory, one per thread that plans, sized to the map once.
    static PLANNER: RefCell<PathPlanner> = RefCell::default();
}

impl PathService {
    /// A service that runs its plans as `runner` says.
    pub fn with_runner(runner: Runner) -> Self {
        Self {
            map: RwLock::default(),
            pool: Arc::new(PathPool {
                free: Mutex::new(Vec::with_capacity(POOLED_PATHS)),
            }),
            runner,
        }
    }

    /// The cheapest path from `from` to `goal` for a walker with `prefs`:
    /// what doors cost it, and what cells others stand on cost it. Lands
    /// `None` when there is no way.
    pub fn plan(&self, from: IVec2, goal: IVec2, prefs: TraversalPrefs) -> Promise<Option<Path>> {
        let (nav, occupancy) = match self.map.read() {
            Ok(map) => (map.nav.clone(), map.occupancy.clone()),
            Err(_) => return Promise::ready(None),
        };
        let pool = self.pool.clone();
        self.runner
            .run(move || plan_path(&nav, &occupancy, &pool, from, goal, prefs))
    }

    /// Whether the service has a map to plan on yet: not on the very first
    /// frame, before the nav map is built. Until then every plan finds no
    /// way, which says nothing about the map.
    pub fn knows_map(&self) -> bool {
        self.map.read().is_ok_and(|map| !map.nav.grid().is_empty())
    }

    /// Takes in changed walls and doors, and who stands where now. Occupancy
    /// is refreshed in place unless a plan still reads the previous copy.
    fn refresh(&self, nav: &NavMap, grid: &MapGrid) {
        let Ok(mut map) = self.map.write() else {
            return;
        };
        if map.nav.revision() != nav.revision() {
            map.nav = Arc::new(nav.clone());
        }
        Arc::make_mut(&mut map.occupancy).refresh(grid);
    }
}

fn plan_path(
    nav: &NavMap,
    occupancy: &OccupancyMap,
    pool: &Arc<PathPool>,
    from: IVec2,
    goal: IVec2,
    prefs: TraversalPrefs,
) -> Option<Path> {
    if nav.grid().is_empty() {
        return None;
    }
    let mut buffer = pool
        .free
        .lock()
        .ok()
        .and_then(|mut free| free.pop())
        .unwrap_or_else(|| {
            Arc::new(PathCells {
                cells: Vec::new(),
                pool: Arc::downgrade(pool),
            })
        });
    let cells = &mut Arc::get_mut(&mut buffer)
        .expect("a pooled path has no other handles")
        .cells;
    let terrain = Terrain {
        nav,
        door_cost: prefs.door_cost,
    };
    let found = PLANNER.with_borrow_mut(|planner| match prefs.crowd_cost {
        None => planner.plan_with(nav, &terrain, from, goal, cells),
        Some(cost) => {
            let crowd = Crowd {
                terrain,
                occupied: occupancy,
                goal: cell_of(goal),
                cost,
            };
            planner.plan_with(nav, &crowd, from, goal, cells)
        }
    });
    let path = Path {
        cells: Some(buffer),
    };
    // A path that was not found goes straight back to the pool.
    found.then_some(path)
}

/// Keeps the path service's copies of the map current.
pub fn share_paths(nav: Res<NavMap>, grid: Res<MapGrid>, services: Res<Services>) {
    services.paths().refresh(&nav, &grid);
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::tasks::{block_on, poll_once};

    use super::{Path, PathService};
    use crate::model::{MapGrid, TraversalPrefs};
    use crate::navigation::NavMap;
    use crate::service::Runner;

    /// A 5x3 room; `walls` stand in it.
    fn shared(walls: &[IVec2]) -> PathService {
        let grid = MapGrid::new(5, 3);
        let mut nav = NavMap::default();
        nav.rebuild(&grid, walls.iter().copied(), []);
        let paths = PathService::with_runner(Runner::Inline);
        paths.refresh(&nav, &grid);
        paths
    }

    fn plan(paths: &PathService, from: IVec2, goal: IVec2) -> Option<Path> {
        let mut promise = paths.plan(from, goal, TraversalPrefs::default());
        block_on(poll_once(&mut promise)).expect("planned inline")
    }

    #[test]
    fn a_path_is_planned_on_the_shared_map() {
        let paths = shared(&[]);
        let path = plan(&paths, IVec2::new(0, 1), IVec2::new(3, 1)).expect("a way");
        assert_eq!(path.cells().first(), Some(&IVec2::new(0, 1)));
        assert_eq!(path.end(), Some(IVec2::new(3, 1)));
        assert_eq!(path.cells().len(), 4);
        assert_eq!(path.next_after(IVec2::new(0, 1)), Some(IVec2::new(1, 1)));
    }

    #[test]
    fn no_way_lands_none() {
        let wall: Vec<IVec2> = (0..3).map(|y| IVec2::new(2, y)).collect();
        let paths = shared(&wall);
        assert!(plan(&paths, IVec2::new(0, 1), IVec2::new(4, 1)).is_none());
    }

    #[test]
    fn the_map_is_unknown_until_shared() {
        let paths = PathService::with_runner(Runner::Inline);
        assert!(!paths.knows_map());
        assert!(plan(&paths, IVec2::ZERO, IVec2::X).is_none());
        assert!(shared(&[]).knows_map());
    }

    /// A path that is dropped gives its buffer back, and the next plan reuses
    /// it; clones share one buffer and compare equal.
    #[test]
    fn paths_reuse_pooled_buffers() {
        let paths = shared(&[]);
        let first = plan(&paths, IVec2::new(0, 1), IVec2::new(4, 1)).unwrap();
        let copy = first.clone();
        assert_eq!(first, copy);
        let buffer = first.cells().as_ptr();
        drop((first, copy));
        let second = plan(&paths, IVec2::new(0, 2), IVec2::new(4, 2)).unwrap();
        assert_eq!(second.cells().as_ptr(), buffer);
        let third = plan(&paths, IVec2::new(0, 2), IVec2::new(4, 2)).unwrap();
        assert_ne!(second, third, "the same cells, but a new path");
    }
}
