//! Planning paths on the game map with reused memory.

use bevy::prelude::*;

use super::path::grid::Cell;
use super::path::PathSearch;
use super::{cell_of, pos_of, NavMap, Terrain};

/// Working memory for planning, sized to the map so planning never
/// allocates.
#[derive(Resource, Default)]
pub struct PathPlanner {
    search: PathSearch<Cell, u32, ()>,
    cells: Vec<Cell>,
}

impl PathPlanner {
    /// Plans the cheapest path from `from` to `goal` into `steps` (both ends
    /// included). Closed doors cost `door_cost` extra steps each, or block
    /// when it is `None`. False, with `steps` empty, if there is no path.
    pub fn plan(
        &mut self,
        nav: &NavMap,
        from: IVec2,
        goal: IVec2,
        door_cost: Option<u32>,
        steps: &mut Vec<IVec2>,
    ) -> bool {
        self.fit(nav);
        let terrain = Terrain { nav, door_cost };
        let found = self
            .search
            .find(
                nav.grid(),
                &terrain,
                cell_of(from),
                cell_of(goal),
                &mut self.cells,
            )
            .is_some();
        steps.clear();
        steps.extend(self.cells.iter().copied().map(pos_of));
        found
    }

    fn fit(&mut self, nav: &NavMap) {
        if !self.search.fits(nav.grid().len(), 1) {
            self.search = PathSearch::with_capacity(nav.grid().len(), 1);
        }
        if self.cells.capacity() < nav.grid().len() {
            self.cells = Vec::with_capacity(nav.grid().len());
        }
    }
}
