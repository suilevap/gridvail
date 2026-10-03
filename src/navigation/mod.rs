//! Pathfinding over the game map, on top of the `gridvail-path` crate
//! (re-exported as [`path`]).
//!
//! [`NavMap`] is a snapshot of what each cell is to a walker (floor, wall, or
//! closed door), rebuilt only when the map's static blockers change. Its
//! [`Terrain`] rules are the base for game searches; combine them with further
//! [`Rules`] (avoiding enemy sight, limiting doors, steering clear of places)
//! as tuples, and search with a reused [`path::PathSearch`] or
//! [`path::RouteSearch`], for example a system's `Local`.
//!
//! Actors are not obstacles here: they move every turn, so rules that care
//! about them should add their cost themselves. Searches do not wrap around
//! the map edges.

#![allow(clippy::type_complexity)]

mod follow;
mod plugin;

pub use follow::*;
pub use gridvail_path as path;
pub use plugin::*;

use bevy::prelude::*;

use path::{Cell, Grid, Rules};

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
#[derive(Resource, Debug)]
pub struct NavMap {
    grid: Grid,
    cells: Vec<NavCell>,
    /// `MapGrid::blocker_revision` the cells were built from.
    revision: Option<u64>,
}

impl Default for NavMap {
    fn default() -> Self {
        Self {
            grid: Grid::new(0, 0),
            cells: Vec::new(),
            revision: None,
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
        self.revision = Some(grid.blocker_revision);
    }

    fn set(&mut self, p: IVec2, cell: NavCell) {
        if let Some(index) = self.grid.index(cell_of(p)) {
            self.cells[index] = cell;
        }
    }

    /// Whether the snapshot is older than the map's static blockers.
    pub fn is_stale(&self, grid: &MapGrid) -> bool {
        self.revision != Some(grid.blocker_revision)
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

#[cfg(test)]
mod tests;
