use crate::PathCost;

/// What moving from a node to a neighbour costs.
///
/// A rule may carry search state along a path, such as how many keys it has
/// used. States are dense layers: every state maps to an index below
/// [`Rules::state_count`], and the search keeps a separate best cost and
/// "finished" mark for each node in each layer. Memory grows with
/// `nodes × state_count`, so states should be small counters or flags.
pub trait Rules {
    type Node: Copy + Eq;
    type Cost: PathCost;
    /// `()` for rules without state.
    type State: Copy + Eq;

    /// Number of distinct states; 1 for rules without state.
    fn state_count(&self) -> usize;

    /// The layer of `state`, below [`Rules::state_count`].
    fn state_index(&self, state: &Self::State) -> usize;

    /// State at the start of every path.
    fn start_state(&self, start: Self::Node) -> Self::State;

    /// Cost of moving from `from` to `to` with `state` so far, and the state
    /// after it; `None` forbids the move.
    fn step(
        &self,
        from: Self::Node,
        to: Self::Node,
        state: &Self::State,
    ) -> Option<(Self::Cost, Self::State)>;

    /// A lower bound on any allowed move's cost. Zero, the default, is always
    /// correct; higher bounds search faster.
    fn min_step_cost(&self) -> Self::Cost {
        Self::Cost::ZERO
    }

    /// A lower bound on the cost from `from` to `goal`, given that the space
    /// needs at least `min_moves` moves between them. It must also be
    /// consistent: never dropping by more than a move costs. The default,
    /// `min_moves` times [`Rules::min_step_cost`], is both. Ignored when
    /// [`Rules::has_jumps`] is true.
    fn heuristic(&self, _from: Self::Node, _goal: Self::Node, min_moves: u32) -> Self::Cost {
        self.min_step_cost().times(min_moves)
    }

    /// Whether reaching a node in state layer `dominant` at no higher cost
    /// makes reaching it in layer `dominated` unnecessary: everything
    /// possible from `dominated` is possible from `dominant`, for no more.
    /// For example, more keys left dominates fewer. The search then skips
    /// dominated arrivals. The default, never, is always correct.
    fn dominates(&self, _dominant: usize, _dominated: usize) -> bool {
        false
    }

    /// Nodes reachable from `from` in one move besides its neighbours in the
    /// space: teleports, ladders, doors between floors. Each is priced (or
    /// forbidden) by [`Rules::step`] like any move, by every combined rule.
    /// Requires [`Rules::has_jumps`].
    fn for_each_jump(
        &self,
        _from: Self::Node,
        _state: &Self::State,
        _visit: &mut dyn FnMut(Self::Node),
    ) {
    }

    /// Whether [`Rules::for_each_jump`] yields anything. A jump can cover
    /// more distance than its cost allows walking, which breaks distance
    /// based estimates, so searches with jumps ignore [`Rules::heuristic`]
    /// and expand by cost alone (Dijkstra).
    fn has_jumps(&self) -> bool {
        false
    }
}

/// Lets a combination borrow its rules, `(&terrain, &door_limit)`, instead of
/// owning them: rules that are expensive to build, not `Copy` (closures), or
/// used again in several combinations need not be cloned or rebuilt.
impl<Inner: Rules + ?Sized> Rules for &Inner {
    type Node = Inner::Node;
    type Cost = Inner::Cost;
    type State = Inner::State;

    fn state_count(&self) -> usize {
        (**self).state_count()
    }

    fn state_index(&self, state: &Self::State) -> usize {
        (**self).state_index(state)
    }

    fn start_state(&self, start: Self::Node) -> Self::State {
        (**self).start_state(start)
    }

    fn step(
        &self,
        from: Self::Node,
        to: Self::Node,
        state: &Self::State,
    ) -> Option<(Self::Cost, Self::State)> {
        (**self).step(from, to, state)
    }

    fn min_step_cost(&self) -> Self::Cost {
        (**self).min_step_cost()
    }

    fn heuristic(&self, from: Self::Node, goal: Self::Node, min_moves: u32) -> Self::Cost {
        (**self).heuristic(from, goal, min_moves)
    }

    fn dominates(&self, dominant: usize, dominated: usize) -> bool {
        (**self).dominates(dominant, dominated)
    }

    fn for_each_jump(
        &self,
        from: Self::Node,
        state: &Self::State,
        visit: &mut dyn FnMut(Self::Node),
    ) {
        (**self).for_each_jump(from, state, visit)
    }

    fn has_jumps(&self) -> bool {
        (**self).has_jumps()
    }
}

/// Both rules apply: costs add, either may forbid a move, the state is the
/// pair of states, either may add jumps, and a pair dominates another when
/// each part dominates or equals its counterpart.
impl<First, Second> Rules for (First, Second)
where
    First: Rules,
    Second: Rules<Node = First::Node, Cost = First::Cost>,
{
    type Node = First::Node;
    type Cost = First::Cost;
    type State = (First::State, Second::State);

    fn state_count(&self) -> usize {
        self.0.state_count() * self.1.state_count()
    }

    fn state_index(&self, state: &Self::State) -> usize {
        self.0.state_index(&state.0) * self.1.state_count() + self.1.state_index(&state.1)
    }

    fn start_state(&self, start: Self::Node) -> Self::State {
        (self.0.start_state(start), self.1.start_state(start))
    }

    fn step(
        &self,
        from: Self::Node,
        to: Self::Node,
        state: &Self::State,
    ) -> Option<(Self::Cost, Self::State)> {
        let (first_cost, first_state) = self.0.step(from, to, &state.0)?;
        let (second_cost, second_state) = self.1.step(from, to, &state.1)?;
        Some((first_cost + second_cost, (first_state, second_state)))
    }

    fn min_step_cost(&self) -> Self::Cost {
        self.0.min_step_cost() + self.1.min_step_cost()
    }

    fn heuristic(&self, from: Self::Node, goal: Self::Node, min_moves: u32) -> Self::Cost {
        self.0.heuristic(from, goal, min_moves) + self.1.heuristic(from, goal, min_moves)
    }

    fn dominates(&self, dominant: usize, dominated: usize) -> bool {
        let second_count = self.1.state_count();
        let (dominant_first, dominant_second) = (dominant / second_count, dominant % second_count);
        let (dominated_first, dominated_second) =
            (dominated / second_count, dominated % second_count);
        dominant != dominated
            && (dominant_first == dominated_first
                || self.0.dominates(dominant_first, dominated_first))
            && (dominant_second == dominated_second
                || self.1.dominates(dominant_second, dominated_second))
    }

    fn for_each_jump(
        &self,
        from: Self::Node,
        state: &Self::State,
        visit: &mut dyn FnMut(Self::Node),
    ) {
        self.0.for_each_jump(from, &state.0, visit);
        self.1.for_each_jump(from, &state.1, visit);
    }

    fn has_jumps(&self) -> bool {
        self.0.has_jumps() || self.1.has_jumps()
    }
}

/// Rules without state from a closure: `cost(from, to)`, `None` to forbid.
pub struct StepFn<Node, Cost, CostFn> {
    min_step_cost: Cost,
    cost: CostFn,
    node: std::marker::PhantomData<fn(Node)>,
}

impl<Node, Cost, CostFn> StepFn<Node, Cost, CostFn>
where
    Node: Copy + Eq,
    Cost: PathCost,
    CostFn: Fn(Node, Node) -> Option<Cost>,
{
    /// `min_step_cost` must not exceed any cost `cost` returns.
    pub fn new(min_step_cost: Cost, cost: CostFn) -> Self {
        Self {
            min_step_cost,
            cost,
            node: std::marker::PhantomData,
        }
    }
}

impl<Node, Cost, CostFn> Rules for StepFn<Node, Cost, CostFn>
where
    Node: Copy + Eq,
    Cost: PathCost,
    CostFn: Fn(Node, Node) -> Option<Cost>,
{
    type Node = Node;
    type Cost = Cost;
    type State = ();

    fn state_count(&self) -> usize {
        1
    }

    fn state_index(&self, _state: &()) -> usize {
        0
    }

    fn start_state(&self, _start: Node) {}

    fn step(&self, from: Node, to: Node, _state: &()) -> Option<(Cost, ())> {
        (self.cost)(from, to).map(|cost| (cost, ()))
    }

    fn min_step_cost(&self) -> Cost {
        self.min_step_cost
    }
}
