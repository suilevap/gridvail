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
    /// distance times [`Rules::min_step_cost`], is both. Ignored when
    /// [`Rules::has_jumps`] is true.
    fn heuristic(&self, grid: &Grid, from: Cell, goal: Cell) -> Self::Cost {
        self.min_step_cost().times(grid.distance(from, goal))
    }

    /// Whether reaching a cell in state layer `a` at no higher cost makes
    /// reaching it in layer `b` unnecessary: everything possible from `b`
    /// is possible from `a`, for no more. For example, more keys left
    /// dominates fewer. The search then skips dominated arrivals, which can
    /// shrink a stateful search a lot. The default, never, is always correct.
    fn dominates(&self, _a: usize, _b: usize) -> bool {
        false
    }

    /// Cells reachable from `from` in one move other than the four adjacent
    /// ones: teleports, ladders, doors between floors. Each is priced (or
    /// forbidden) by [`Rules::step`] like any move, by every combined rule.
    /// Requires [`Rules::has_jumps`].
    fn for_each_jump(&self, _from: Cell, _state: &Self::State, _visit: &mut dyn FnMut(Cell)) {}

    /// Whether [`Rules::for_each_jump`] yields anything. A jump can cover
    /// more distance than its cost allows walking, which breaks distance
    /// based estimates, so searches with jumps ignore [`Rules::heuristic`]
    /// and expand by cost alone (Dijkstra).
    fn has_jumps(&self) -> bool {
        false
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

    fn dominates(&self, a: usize, b: usize) -> bool {
        (**self).dominates(a, b)
    }

    fn for_each_jump(&self, from: Cell, state: &Self::State, visit: &mut dyn FnMut(Cell)) {
        (**self).for_each_jump(from, state, visit)
    }

    fn has_jumps(&self) -> bool {
        (**self).has_jumps()
    }
}

/// Both rules apply: costs add, either may forbid a step, the state is the
/// pair of states, either may add jumps, and a pair dominates another when
/// each part dominates or equals its counterpart.
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

    fn dominates(&self, a: usize, b: usize) -> bool {
        let count = self.1.state_count();
        let (a0, a1, b0, b1) = (a / count, a % count, b / count, b % count);
        a != b && (a0 == b0 || self.0.dominates(a0, b0)) && (a1 == b1 || self.1.dominates(a1, b1))
    }

    fn for_each_jump(&self, from: Cell, state: &Self::State, visit: &mut dyn FnMut(Cell)) {
        self.0.for_each_jump(from, &state.0, visit);
        self.1.for_each_jump(from, &state.1, visit);
    }

    fn has_jumps(&self) -> bool {
        self.0.has_jumps() || self.1.has_jumps()
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
