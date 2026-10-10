//! Pathfinding over the game map, on top of the `gridvail-path` crate
//! (re-exported as [`path`]).
//!
//! [`NavMap`] is a snapshot of what each cell is to a walker (floor, wall, or
//! closed door), rebuilt only when the map's static blockers change. Its
//! [`Terrain`] rules are the base for game searches; combine them with further
//! [`Rules`] (avoiding enemy sight, limiting doors, steering clear of places)
//! as tuples, and search with a reused [`path::PathSearch`], for example a
//! system's `Local`.
//!
//! Actors are not obstacles here: they move every turn, so rules that care
//! about them should add their cost themselves. Searches do not wrap around
//! the map edges.

#![allow(clippy::type_complexity)]

mod paths;
mod planner;
mod plugin;

pub use gridvail_path as path;
pub use paths::*;
pub use planner::*;
pub use plugin::*;

use bevy::prelude::*;

use path::grid::{Cell, Grid, PortalLink, Portals};
use path::Rules;

use crate::model::*;

/// The pathfinding cell of a map position.
pub fn cell_of(p: IVec2) -> Cell {
    Cell::new(p.x, p.y)
}

/// The map position of a pathfinding cell.
pub fn pos_of(cell: Cell) -> IVec2 {
    IVec2::new(cell.x, cell.y)
}

/// What a cell is to a walker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavCell {
    #[default]
    Floor,
    Wall,
    /// Passable only by opening it, which takes a key.
    ClosedDoor,
}

/// Walkability of every map cell.
#[derive(Resource, Clone, Debug)]
pub struct NavMap {
    grid: Grid,
    cells: Vec<NavCell>,
    /// One link per portal face: from the floor in front of it, stepping
    /// into it, to the floor beyond its exit.
    portals: Portals,
    /// `MapGrid::blocker_revision` the cells were built from.
    revision: Option<u64>,
    /// `MapGrid::portal_revision` the portal links were built from.
    portal_revision: Option<u64>,
}

impl Default for NavMap {
    fn default() -> Self {
        Self {
            grid: Grid::new(0, 0),
            cells: Vec::new(),
            portals: Portals::default(),
            revision: None,
            portal_revision: None,
        }
    }
}

impl NavMap {
    /// The searched grid.
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// The cell at `cell`; off the map is wall.
    pub fn cell(&self, cell: Cell) -> NavCell {
        self.grid
            .index(cell)
            .map_or(NavCell::Wall, |index| self.cells[index])
    }

    /// The cell at map position `p`.
    pub fn at(&self, p: IVec2) -> NavCell {
        self.cell(cell_of(p))
    }

    /// The links through portal faces, for paths that may use them.
    pub fn portals(&self) -> &Portals {
        &self.portals
    }

    /// The step a walker at `from` takes to reach `to` next on a path: to a
    /// neighbouring cell, or into the portal face whose exit is `to`.
    /// `None` when `to` is neither.
    pub fn step_toward(&self, from: IVec2, to: IVec2) -> Option<IVec2> {
        let delta = to - from;
        if delta.abs().element_sum() == 1 {
            return Some(delta);
        }
        let (from, to) = (cell_of(from), cell_of(to));
        self.portals
            .links()
            .iter()
            .find(|link| link.from == from && link.to == to)
            .map(|link| pos_of(link.toward))
    }

    /// Rebuilds the snapshot. Reuses its storage unless the map grew.
    pub fn rebuild(
        &mut self,
        grid: &MapGrid,
        walls: impl IntoIterator<Item = IVec2>,
        closed_doors: impl IntoIterator<Item = IVec2>,
    ) {
        self.grid = Grid::new(grid.width.max(0) as u32, grid.height.max(0) as u32);
        let cells = self.grid.len();
        if self.cells.len() != cells {
            self.cells.resize(cells, NavCell::Floor);
        }
        self.cells.fill(NavCell::Floor);
        for p in walls {
            self.set(p, NavCell::Wall);
        }
        for p in closed_doors {
            self.set(p, NavCell::ClosedDoor);
        }
        let nav_grid = self.grid;
        self.portals
            .set(grid.portal_faces().filter_map(|(wall, face)| {
                let from = cell_of(wall + face.side);
                nav_grid.contains(from).then(|| PortalLink {
                    from,
                    toward: cell_of(-face.side),
                    // Where the simulation puts a walker stepping in.
                    to: cell_of(grid.safe_pos(face.through.apply(wall))),
                })
            }));
        self.revision = Some(grid.blocker_revision);
        self.portal_revision = Some(grid.portal_revision);
    }

    fn set(&mut self, p: IVec2, cell: NavCell) {
        if let Some(index) = self.grid.index(cell_of(p)) {
            self.cells[index] = cell;
        }
    }

    /// `MapGrid::blocker_revision` the cells were built from.
    pub fn revision(&self) -> Option<u64> {
        self.revision
    }

    /// `MapGrid::portal_revision` the portal links were built from.
    pub fn portal_revision(&self) -> Option<u64> {
        self.portal_revision
    }

    /// Whether the snapshot is older than the map's static blockers or its
    /// portals.
    pub fn is_stale(&self, grid: &MapGrid) -> bool {
        self.revision != Some(grid.blocker_revision)
            || self.portal_revision != Some(grid.portal_revision)
    }
}

/// Base walking cost: one per step, walls forbidden. Closed doors cost
/// `door_cost` extra to pass, or are forbidden when it is `None`.
#[derive(Clone, Copy, Debug)]
pub struct Terrain<'a> {
    pub nav: &'a NavMap,
    pub door_cost: Option<u32>,
}

impl<'a> Terrain<'a> {
    /// Walls and closed doors both block.
    pub fn walls_and_doors(nav: &'a NavMap) -> Self {
        Self {
            nav,
            door_cost: None,
        }
    }

    /// Closed doors pass for `door_cost` extra each.
    pub fn through_doors(nav: &'a NavMap, door_cost: u32) -> Self {
        Self {
            nav,
            door_cost: Some(door_cost),
        }
    }
}

impl Rules for Terrain<'_> {
    type Node = Cell;
    type Cost = u32;
    type State = ();

    fn state_count(&self) -> usize {
        1
    }

    fn state_index(&self, _state: &()) -> usize {
        0
    }

    fn start_state(&self, _start: Cell) {}

    fn step(&self, _from: Cell, to: Cell, _state: &()) -> Option<(u32, ())> {
        let cost = match self.nav.cell(to) {
            NavCell::Floor => 1,
            NavCell::Wall => return None,
            NavCell::ClosedDoor => 1 + self.door_cost?,
        };
        Some((cost, ()))
    }

    fn min_step_cost(&self) -> u32 {
        1
    }
}

/// Which map positions something stands on.
pub trait Occupancy {
    fn occupied(&self, p: IVec2) -> bool;
}

impl Occupancy for MapGrid {
    fn occupied(&self, p: IVec2) -> bool {
        self.get(p).is_some()
    }
}

/// Occupied cells copied from the [`MapGrid`], for planning away from it
/// (on another thread). Refreshed in place while nothing else holds it.
#[derive(Clone, Debug, Default)]
pub struct OccupancyMap {
    width: i32,
    height: i32,
    cells: Vec<bool>,
    /// `MapGrid::revision` the cells were copied at.
    revision: Option<u64>,
}

impl OccupancyMap {
    /// Copies `grid`'s occupancy unless it is already current.
    pub fn refresh(&mut self, grid: &MapGrid) {
        if self.revision == Some(grid.revision) {
            return;
        }
        self.width = grid.width;
        self.height = grid.height;
        self.cells.clear();
        self.cells.extend(
            (0..grid.height)
                .flat_map(|y| (0..grid.width).map(move |x| IVec2::new(x, y)))
                .map(|p| grid.get(p).is_some()),
        );
        self.revision = Some(grid.revision);
    }
}

impl Occupancy for OccupancyMap {
    fn occupied(&self, p: IVec2) -> bool {
        p.x >= 0
            && p.y >= 0
            && p.x < self.width
            && p.y < self.height
            && self.cells[(p.y * self.width + p.x) as usize]
    }
}

/// [`Terrain`] that also charges `cost` extra for entering a floor cell
/// another actor occupies, except the goal: walking into it is how a bump or
/// an attack happens. Actors move, so this steers around who is in the way
/// now rather than forbidding their cells.
#[derive(Clone, Copy, Debug)]
pub struct Crowd<'a, Occupied: ?Sized = MapGrid> {
    pub terrain: Terrain<'a>,
    pub occupied: &'a Occupied,
    pub goal: Cell,
    pub cost: u32,
}

impl<Occupied: Occupancy + ?Sized> Rules for Crowd<'_, Occupied> {
    type Node = Cell;
    type Cost = u32;
    type State = ();

    fn state_count(&self) -> usize {
        1
    }

    fn state_index(&self, _state: &()) -> usize {
        0
    }

    fn start_state(&self, _start: Cell) {}

    fn step(&self, from: Cell, to: Cell, state: &()) -> Option<(u32, ())> {
        let (cost, ()) = self.terrain.step(from, to, state)?;
        let crowded = to != self.goal
            && self.terrain.nav.cell(to) == NavCell::Floor
            && self.occupied.occupied(pos_of(to));
        Some((cost + if crowded { self.cost } else { 0 }, ()))
    }

    fn min_step_cost(&self) -> u32 {
        1
    }
}

#[cfg(test)]
mod tests;
