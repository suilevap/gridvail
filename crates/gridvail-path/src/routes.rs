use std::marker::PhantomData;

use crate::{Cell, Cost, Grid, PathSearch, Rules};

/// How different the routes of a [`RouteSearch`] must be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteOptions {
    /// Most routes streamed.
    pub max_routes: usize,
    /// Extra cost, in plain steps ([`Cost::steps`]), of entering a cell once
    /// per earlier search result through it. Larger values push later routes
    /// further away.
    pub penalty: u32,
    /// Largest share (0 to 1) of a route's intermediate cells that may lie on
    /// routes already streamed; candidates overlapping more are skipped.
    pub max_shared: f32,
    /// Searches allowed per streamed route before giving up.
    pub attempts_per_route: usize,
}

impl Default for RouteOptions {
    fn default() -> Self {
        Self {
            max_routes: 3,
            penalty: 4,
            max_shared: 0.5,
            attempts_per_route: 4,
        }
    }
}

/// Streams routes from a start to a goal that differ from one another.
///
/// [`RouteSearch::next`] returns the cheapest path first. Every later call
/// searches again with cells of earlier results costing
/// [`RouteOptions::penalty`] extra steps per result through them, and returns
/// the first result that shares at most [`RouteOptions::max_shared`] of its
/// cells with routes already returned. Returned costs are the rules' own,
/// without penalties; later routes usually, but not always, cost more.
///
/// Like [`PathSearch`], it reuses its memory: after warming up, a stream
/// allocates nothing.
#[derive(Debug)]
pub struct RouteSearch<C, S> {
    options: RouteOptions,
    search: PathSearch<C, (S, ())>,
    /// Per cell: search results through it so far.
    uses: Vec<u32>,
    /// Per cell: whether a returned route passes through it.
    on_route: Vec<bool>,
    grid: Grid,
    start: Cell,
    goal: Cell,
    returned: usize,
    attempts: usize,
}

impl<C: Cost, S: Copy + Eq> RouteSearch<C, S> {
    pub fn new(options: RouteOptions) -> Self {
        Self {
            options,
            search: PathSearch::new(),
            uses: Vec::new(),
            on_route: Vec::new(),
            grid: Grid::new(0, 0),
            start: Cell::default(),
            goal: Cell::default(),
            returned: 0,
            attempts: 0,
        }
    }

    pub fn options(&self) -> RouteOptions {
        self.options
    }

    /// Starts a new stream of routes from `start` to `goal`.
    pub fn begin(&mut self, grid: &Grid, start: Cell, goal: Cell) {
        let cells = grid.len();
        if self.uses.len() < cells {
            self.uses.resize(cells, 0);
            self.on_route.resize(cells, false);
        }
        self.uses[..cells].fill(0);
        self.on_route[..cells].fill(false);
        self.grid = *grid;
        self.start = start;
        self.goal = goal;
        self.returned = 0;
        self.attempts = 0;
    }

    /// The next route, written into `path`, and its cost; `None` once
    /// [`RouteOptions::max_routes`] were returned or no further different
    /// route is found. `rules` must be the same on every call of a stream.
    pub fn next<R>(&mut self, rules: &R, path: &mut Vec<Cell>) -> Option<C>
    where
        R: Rules<Cost = C, State = S>,
    {
        let options = self.options;
        let max_attempts = options.max_routes * options.attempts_per_route.max(1);
        while self.returned < options.max_routes && self.attempts < max_attempts {
            self.attempts += 1;
            let Self {
                search, uses, grid, ..
            } = self;
            let penalized = (
                rules,
                Penalty {
                    grid,
                    uses,
                    penalty: options.penalty,
                    cost: PhantomData,
                },
            );
            if search
                .find(grid, &penalized, self.start, self.goal, path)
                .is_none()
            {
                self.attempts = max_attempts;
                return None;
            }
            let cost = replay(rules, path).expect("rules allowed this path");
            let interior = match path.len() {
                0..=2 => &path[..0],
                n => &path[1..n - 1],
            };
            let shared = interior
                .iter()
                .filter(|cell| self.on_route[self.cell_index(**cell)])
                .count();
            for cell in interior {
                let index = self.cell_index(*cell);
                self.uses[index] += 1;
            }
            // A route without intermediate cells (start next to goal) can
            // only be returned once.
            let distinct = self.returned == 0
                || (!interior.is_empty()
                    && shared as f32 <= options.max_shared * interior.len() as f32);
            if distinct {
                for cell in interior {
                    let index = self.cell_index(*cell);
                    self.on_route[index] = true;
                }
                self.returned += 1;
                return Some(cost);
            }
        }
        path.clear();
        None
    }

    fn cell_index(&self, cell: Cell) -> usize {
        self.grid.index(cell).expect("path cell on grid")
    }
}

/// The rules' own cost of walking `path`, which they allowed before.
fn replay<R: Rules>(rules: &R, path: &[Cell]) -> Option<R::Cost> {
    let mut state = rules.start_state(*path.first()?);
    let mut cost = R::Cost::ZERO;
    for pair in path.windows(2) {
        let (step, next) = rules.step(pair[0], pair[1], &state)?;
        cost = cost + step;
        state = next;
    }
    Some(cost)
}

/// Extra cost for entering cells that earlier search results went through.
struct Penalty<'a, C> {
    grid: &'a Grid,
    uses: &'a [u32],
    penalty: u32,
    cost: PhantomData<C>,
}

impl<C: Cost> Rules for Penalty<'_, C> {
    type Cost = C;
    type State = ();

    fn state_count(&self) -> usize {
        1
    }

    fn state_index(&self, _state: &()) -> usize {
        0
    }

    fn start_state(&self, _start: Cell) {}

    fn step(&self, _from: Cell, to: Cell, _state: &()) -> Option<(C, ())> {
        let uses = self.uses[self.grid.index(to)?];
        Some((C::steps(uses.saturating_mul(self.penalty)), ()))
    }
}
