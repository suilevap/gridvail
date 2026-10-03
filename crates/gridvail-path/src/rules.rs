use crate::{Cell, Cost, Grid};

/// What moving between two adjacent cells costs.
///
/// A rule may carry search state along a path, such as how many keys it has
/// used. States are dense layers: every state maps to an index below
/// [`Rules::state_count`], and the search keeps a separate best cost and
/// "finished" mark for each cell in each layer. Memory grows with
/// `cells × state_count`, so states should be small counters or flags.
pub trait Rules {
    type Cost: Cost;
    /// `()` for rules without state.
    type State: Copy + Eq;

    /// Number of distinct states; 1 for rules without state.
    fn state_count(&self) -> usize;

    /// The layer of `state`, below [`Rules::state_count`].
    fn state_index(&self, state: &Self::State) -> usize;

    /// State at the start of every path.
    fn start_state(&self, start: Cell) -> Self::State;

    /// Cost of stepping from `from` to the adjacent `to` with `state` so far,
    /// and the state after it; `None` forbids the step.
    fn step(&self, from: Cell, to: Cell, state: &Self::State) -> Option<(Self::Cost, Self::State)>;

    /// A lower bound on any allowed step's cost. Zero, the default, is always
    /// correct; higher bounds search faster.
    fn min_step_cost(&self) -> Self::Cost {
        Self::Cost::ZERO
    }

    /// A lower bound on the cost from `from` to `goal`. It must also be
    /// consistent: never dropping by more than a step costs. The default,
    /// distance times [`Rules::min_step_cost`], is both.
    fn heuristic(&self, grid: &Grid, from: Cell, goal: Cell) -> Self::Cost {
        self.min_step_cost().times(grid.distance(from, goal))
    }
}

impl<R: Rules + ?Sized> Rules for &R {
    type Cost = R::Cost;
    type State = R::State;

    fn state_count(&self) -> usize {
        (**self).state_count()
    }

    fn state_index(&self, state: &Self::State) -> usize {
        (**self).state_index(state)
    }

    fn start_state(&self, start: Cell) -> Self::State {
        (**self).start_state(start)
    }

    fn step(&self, from: Cell, to: Cell, state: &Self::State) -> Option<(Self::Cost, Self::State)> {
        (**self).step(from, to, state)
    }

    fn min_step_cost(&self) -> Self::Cost {
        (**self).min_step_cost()
    }

    fn heuristic(&self, grid: &Grid, from: Cell, goal: Cell) -> Self::Cost {
        (**self).heuristic(grid, from, goal)
    }
}

/// Both rules apply: costs add, either may forbid a step, and the state is
/// the pair of states.
impl<A: Rules, B: Rules<Cost = A::Cost>> Rules for (A, B) {
    type Cost = A::Cost;
    type State = (A::State, B::State);

    fn state_count(&self) -> usize {
        self.0.state_count() * self.1.state_count()
    }

    fn state_index(&self, state: &Self::State) -> usize {
        self.0.state_index(&state.0) * self.1.state_count() + self.1.state_index(&state.1)
    }

    fn start_state(&self, start: Cell) -> Self::State {
        (self.0.start_state(start), self.1.start_state(start))
    }

    fn step(&self, from: Cell, to: Cell, state: &Self::State) -> Option<(Self::Cost, Self::State)> {
        let (a_cost, a_state) = self.0.step(from, to, &state.0)?;
        let (b_cost, b_state) = self.1.step(from, to, &state.1)?;
        Some((a_cost + b_cost, (a_state, b_state)))
    }

    fn min_step_cost(&self) -> Self::Cost {
        self.0.min_step_cost() + self.1.min_step_cost()
    }

    fn heuristic(&self, grid: &Grid, from: Cell, goal: Cell) -> Self::Cost {
        self.0.heuristic(grid, from, goal) + self.1.heuristic(grid, from, goal)
    }
}

/// Rules without state from a closure: `cost(from, to)`, `None` to forbid.
pub struct StepFn<C, F> {
    min_step_cost: C,
    cost: F,
}

impl<C: Cost, F: Fn(Cell, Cell) -> Option<C>> StepFn<C, F> {
    /// `min_step_cost` must not exceed any cost `cost` returns.
    pub fn new(min_step_cost: C, cost: F) -> Self {
        Self {
            min_step_cost,
            cost,
        }
    }
}

impl<C: Cost, F: Fn(Cell, Cell) -> Option<C>> Rules for StepFn<C, F> {
    type Cost = C;
    type State = ();

    fn state_count(&self) -> usize {
        1
    }

    fn state_index(&self, _state: &()) -> usize {
        0
    }

    fn start_state(&self, _start: Cell) {}

    fn step(&self, from: Cell, to: Cell, _state: &()) -> Option<(C, ())> {
        (self.cost)(from, to).map(|cost| (cost, ()))
    }

    fn min_step_cost(&self) -> C {
        self.min_step_cost
    }
}
