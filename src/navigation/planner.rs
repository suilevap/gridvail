//! Planning paths on the game map with reused memory.

use bevy::prelude::*;

use super::path::grid::Cell;
use super::path::{PathSearch, Rules};
use super::{cell_of, pos_of, NavMap, Terrain};

/// Working memory for planning, sized to the map so planning never
/// allocates.
#[derive(Resource, Default)]
pub struct PathPlanner {
    search: PathSearch<Cell, u32, ()>,
    cells: Vec<Cell>,
    /// Per portal link, a bound on the moves from its exit to the goal.
    bounds: Vec<u32>,
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
        self.plan_with(nav, &Terrain { nav, door_cost }, from, goal, steps)
    }

    /// Plans the cheapest path under `rules`, such as [`Terrain`] combined
    /// with further costs into one stateless rule; see [`PathPlanner::plan`].
    pub fn plan_with(
        &mut self,
        nav: &NavMap,
        rules: &impl Rules<Node = Cell, Cost = u32, State = ()>,
        from: IVec2,
        goal: IVec2,
        steps: &mut Vec<IVec2>,
    ) -> bool {
        self.plan_through(nav, rules, from, goal, false, steps)
    }

    /// Like [`PathPlanner::plan_with`], through portals too when `portals`
    /// is set: stepping into a portal face is one move to the floor beyond
    /// its exit. The search stays A*: its distance estimate knows the
    /// portals (see `Portals::space`), so it does not spread over the whole
    /// map as a search with jumps would.
    pub fn plan_through(
        &mut self,
        nav: &NavMap,
        rules: &impl Rules<Node = Cell, Cost = u32, State = ()>,
        from: IVec2,
        goal: IVec2,
        portals: bool,
        steps: &mut Vec<IVec2>,
    ) -> bool {
        self.fit(nav);
        let (start, goal) = (cell_of(from), cell_of(goal));
        let found = if portals && !nav.portals().is_empty() {
            nav.portals().bounds_to(nav.grid(), goal, &mut self.bounds);
            let space = nav.portals().space(nav.grid(), goal, &self.bounds);
            self.search
                .find(&space, rules, start, goal, &mut self.cells)
                .is_some()
        } else {
            self.search
                .find(nav.grid(), rules, start, goal, &mut self.cells)
                .is_some()
        };
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
        if self.bounds.capacity() < nav.portals().links().len() {
            self.bounds = Vec::with_capacity(nav.portals().links().len());
        }
    }
}
