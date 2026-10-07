//! Paths as a service: [`PathService::plan`] answers with a [`Task`] that
//! lands a [`Path`], inline or in the background as [`ServiceMode`] says.
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
use std::sync::{Arc, Mutex, Weak};

use bevy::prelude::*;

use super::{cell_of, Crowd, NavMap, OccupancyMap, PathPlanner, Terrain};
use crate::model::{MapGrid, TraversalPrefs};
use crate::service::{ServiceMode, Task};

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

/// Plans paths as tasks, over copies of the map it keeps.
#[derive(Resource, Clone)]
pub struct PathService {
    nav: Arc<NavMap>,
    occupancy: Arc<OccupancyMap>,
    pool: Arc<PathPool>,
    mode: ServiceMode,
}

impl Default for PathService {
    fn default() -> Self {
        Self {
            nav: Arc::default(),
            occupancy: Arc::default(),
            pool: Arc::new(PathPool {
                free: Mutex::new(Vec::with_capacity(POOLED_PATHS)),
            }),
            mode: ServiceMode::default(),
        }
    }
}

impl fmt::Debug for PathService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathService")
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

thread_local! {
    /// Planning memory, one per thread that plans, sized to the map once.
    static PLANNER: RefCell<PathPlanner> = RefCell::default();
}

impl PathService {
    /// The cheapest path from `from` to `goal` for a walker with `prefs`:
    /// what doors cost it, and what cells others stand on cost it. Lands
    /// `None` when there is no way.
    pub fn plan(&self, from: IVec2, goal: IVec2, prefs: TraversalPrefs) -> Task<Option<Path>> {
        let nav = self.nav.clone();
        let occupancy = self.occupancy.clone();
        let pool = self.pool.clone();
        Task::spawn(self.mode, move || {
            plan_path(&nav, &occupancy, &pool, from, goal, prefs)
        })
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

/// Keeps the service's copies of the map current: the nav map, retaken when
/// walls or doors change, and occupancy, refreshed in place each frame unless
/// a plan in the background still reads the previous copy.
pub fn share_paths(
    mode: Res<ServiceMode>,
    nav: Res<NavMap>,
    grid: Res<MapGrid>,
    mut paths: ResMut<PathService>,
) {
    let paths = paths.bypass_change_detection();
    paths.mode = *mode;
    if paths.nav.revision() != nav.revision() {
        paths.nav = Arc::new(nav.clone());
    }
    Arc::make_mut(&mut paths.occupancy).refresh(&grid);
}
