//! Pathfinding over the game map.
//!
//! [`NavMap`] is a snapshot of what each cell is to a walker (floor, wall, or
//! closed door), rebuilt only when the map's static blockers change. Its
//! [`Terrain`] cost model is the base for game searches; combine it with
//! further [`CostModel`]s (avoiding enemy sight, limiting doors, steering clear
//! of places) through the searches in [`crate::foundation::path`].
//!
//! Actors are not obstacles here: they move every turn, so a model that cares
//! about them should add their cost itself.

mod plugin;

pub use plugin::*;

use bevy::prelude::*;

use crate::foundation::path::{
    alternative_paths, find_path, k_shortest_paths, Alternatives, Cost, CostModel, GridShape, Path,
};
use crate::model::*;

/// What a cell is to a walker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavCell {
    #[default]
    Floor,
    Wall,
    /// Passable only by opening it, which takes a key.
    ClosedDoor,
}

/// Walkability of every map cell. The map wraps, like [`MapGrid`].
#[derive(Resource, Debug, Default)]
pub struct NavMap {
    width: i32,
    height: i32,
    cells: Vec<NavCell>,
    /// `MapGrid::blocker_revision` the cells were built from.
    revision: Option<u64>,
}

impl NavMap {
    pub fn shape(&self) -> GridShape {
        GridShape {
            width: self.width,
            height: self.height,
            wrap: true,
        }
    }

    /// The cell at `p`; outside the map is wall.
    pub fn cell(&self, p: IVec2) -> NavCell {
        self.index(p)
            .map_or(NavCell::Wall, |index| self.cells[index])
    }

    fn index(&self, p: IVec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height)
            .then_some((p.y * self.width + p.x) as usize)
    }

    /// The cheapest path under `model`; see [`find_path`].
    pub fn find_path(&self, model: &impl CostModel, from: IVec2, to: IVec2) -> Option<Path> {
        find_path(self.shape(), model, from, to)
    }

    /// The `k` cheapest paths under `model`; see [`k_shortest_paths`].
    pub fn k_shortest_paths(
        &self,
        model: &impl CostModel,
        from: IVec2,
        to: IVec2,
        k: usize,
    ) -> Vec<Path> {
        k_shortest_paths(self.shape(), model, from, to, k)
    }

    /// Routes that differ from each other; see [`alternative_paths`].
    pub fn alternative_paths(
        &self,
        model: &impl CostModel,
        from: IVec2,
        to: IVec2,
        options: Alternatives,
    ) -> Vec<Path> {
        alternative_paths(self.shape(), model, from, to, options)
    }

    /// Rebuilds the snapshot. Reuses its storage unless the map size changed.
    pub fn rebuild(
        &mut self,
        grid: &MapGrid,
        walls: impl IntoIterator<Item = IVec2>,
        closed_doors: impl IntoIterator<Item = IVec2>,
    ) {
        self.width = grid.width;
        self.height = grid.height;
        let cells = (grid.width * grid.height) as usize;
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
        if let Some(index) = self.index(p) {
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
    pub door_cost: Option<Cost>,
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
    pub fn through_doors(nav: &'a NavMap, door_cost: Cost) -> Self {
        Self {
            nav,
            door_cost: Some(door_cost),
        }
    }
}

impl CostModel for Terrain<'_> {
    type State = ();

    fn initial_state(&self, _start: IVec2) {}

    fn step(&self, _from: IVec2, to: IVec2, _state: &()) -> Option<(Cost, ())> {
        let cost = match self.nav.cell(to) {
            NavCell::Floor => 1,
            NavCell::Wall => return None,
            NavCell::ClosedDoor => 1 + self.door_cost?,
        };
        Some((cost, ()))
    }

    fn min_step_cost(&self) -> Cost {
        1
    }
}

#[cfg(test)]
mod tests;
