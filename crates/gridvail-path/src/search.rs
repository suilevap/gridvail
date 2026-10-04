use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::{PathCost, Rules, Space};

const NO_PARENT: u32 = u32::MAX;

/// Reusable A* over a [`Space`] and [`Rules`].
///
/// Working memory is one slot per node and rule state ("layer"): its best
/// cost so far, the slot it was reached from, and whether it is finished. A
/// slot belongs to the current search only when its stamp matches, so a new
/// search starts by bumping the stamp instead of clearing anything. Storage
/// and the open list grow to the largest search made, then are reused: after
/// that, searches allocate nothing.
///
/// [`PathSearch::find`] returns the cheapest path. [`PathSearch::begin`] and
/// [`PathSearch::next`] go on from there: each `next` returns the cheapest
/// path reaching the goal in a state not reached before (with doors counted
/// in the state: the cheapest way with 2 doors, then with 1, then with
/// none, in order of cost), so differing in what the path does along the
/// way, not in where it runs. Arrivals that [`Rules::dominates`] says an
/// earlier, no dearer arrival beats are skipped. Without dominance, a
/// counter-like state also yields detours that only change the count
/// (stepping into a door and back): declare dominance to drop those.
///
/// One `PathSearch` serves any space and rules with the same node, cost and
/// state types.
#[derive(Debug)]
pub struct PathSearch<Node, Cost, State> {
    stamp: u32,
    /// Stamp of the search that last reached each slot.
    reached: Vec<u32>,
    /// Stamp of the search that last finished each slot.
    finished: Vec<u32>,
    best: Vec<Cost>,
    parent: Vec<u32>,
    open: BinaryHeap<Open<Cost, State>>,
    expanded: usize,
    /// The search `next` continues, if any.
    query: Option<Query<Node>>,
    /// State layers of the goal arrivals returned so far.
    returned: Vec<usize>,
}

/// What [`PathSearch::begin`] set up.
#[derive(Clone, Copy, Debug)]
struct Query<Node> {
    goal: Node,
    layers: usize,
    jumps: bool,
}

impl<Node, Cost, State> Default for PathSearch<Node, Cost, State>
where
    Node: Copy + Eq,
    Cost: PathCost,
    State: Copy + Eq,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Node, Cost, State> PathSearch<Node, Cost, State>
where
    Node: Copy + Eq,
    Cost: PathCost,
    State: Copy + Eq,
{
    pub fn new() -> Self {
        Self {
            stamp: 0,
            reached: Vec::new(),
            finished: Vec::new(),
            best: Vec::new(),
            parent: Vec::new(),
            open: BinaryHeap::new(),
            expanded: 0,
            query: None,
            returned: Vec::new(),
        }
    }

    /// Room for `nodes` nodes with `states` layers allocated up front,
    /// including the open list's worst case for spaces whose nodes have at
    /// most four neighbours and no jumps, so such searches never grow it.
    pub fn with_capacity(nodes: usize, states: usize) -> Self {
        let mut search = Self::new();
        let slots = nodes * states.max(1);
        search.prepare(slots);
        search.open.reserve(slots * 4 + 1);
        search.returned.reserve(states.max(1));
        search
    }

    /// Whether this search already has the room [`PathSearch::with_capacity`]
    /// would give it.
    pub fn fits(&self, nodes: usize, states: usize) -> bool {
        let slots = nodes * states.max(1);
        self.reached.len() >= slots
            && self.open.capacity() > slots * 4
            && self.returned.capacity() >= states.max(1)
    }

    /// Slots finished so far by the current search.
    pub fn expanded(&self) -> usize {
        self.expanded
    }

    /// The cheapest path from `start` to `goal`, written into `path` (start
    /// and goal included; consecutive nodes are neighbours except across
    /// jumps), and its cost. `None`, with `path` empty, when no path exists
    /// or an end is not in the space.
    pub fn find<SearchSpace, SearchRules>(
        &mut self,
        space: &SearchSpace,
        rules: &SearchRules,
        start: Node,
        goal: Node,
        path: &mut Vec<Node>,
    ) -> Option<Cost>
    where
        SearchSpace: Space<Node = Node>,
        SearchRules: Rules<Node = Node, Cost = Cost, State = State>,
    {
        path.clear();
        if !self.begin(space, rules, start, goal) {
            return None;
        }
        let found = self.next(space, rules, path).map(|(cost, _)| cost);
        self.query = None;
        self.open.clear();
        found
    }

    /// Starts a search from `start` to `goal` whose paths [`PathSearch::next`]
    /// returns; false (and nothing to return) if an end is not in the space.
    pub fn begin<SearchSpace, SearchRules>(
        &mut self,
        space: &SearchSpace,
        rules: &SearchRules,
        start: Node,
        goal: Node,
    ) -> bool
    where
        SearchSpace: Space<Node = Node>,
        SearchRules: Rules<Node = Node, Cost = Cost, State = State>,
    {
        self.expanded = 0;
        self.returned.clear();
        self.query = None;
        let (Some(start_index), Some(_)) = (space.index(start), space.index(goal)) else {
            self.open.clear();
            return false;
        };
        let layers = rules.state_count().max(1);
        self.prepare(space.node_count() * layers);
        let query = Query {
            goal,
            layers,
            jumps: rules.has_jumps(),
        };
        let state = rules.start_state(start);
        let slot = slot_of(start_index, layers, rules.state_index(&state));
        self.reach(slot, Cost::ZERO, NO_PARENT);
        self.open.push(Open {
            estimate: estimate(space, rules, &query, start),
            cost: Cost::ZERO,
            slot: slot as u32,
            state,
        });
        self.query = Some(query);
        true
    }

    /// The next cheapest path to the goal of [`PathSearch::begin`] that
    /// reaches it in a state no earlier path did (and that no earlier path's
    /// state dominates), written into `path`, with its cost and final state.
    /// Costs never decrease from one call to the next. `None` once no such
    /// path remains. `space` and `rules` must be the ones given to `begin`.
    pub fn next<SearchSpace, SearchRules>(
        &mut self,
        space: &SearchSpace,
        rules: &SearchRules,
        path: &mut Vec<Node>,
    ) -> Option<(Cost, State)>
    where
        SearchSpace: Space<Node = Node>,
        SearchRules: Rules<Node = Node, Cost = Cost, State = State>,
    {
        path.clear();
        let query = self.query?;
        while let Some(Open {
            cost, slot, state, ..
        }) = self.open.pop()
        {
            let slot = slot as usize;
            // A slot may be queued again after a cheaper way to it was found.
            if self.finished[slot] == self.stamp || cost > self.best[slot] {
                continue;
            }
            self.finished[slot] = self.stamp;
            self.expanded += 1;
            let node = space.node(slot / query.layers);
            if node == query.goal {
                // Goals end paths: nothing continues from them.
                let layer = slot % query.layers;
                let dominated = self
                    .returned
                    .iter()
                    .any(|&earlier| rules.dominates(earlier, layer));
                if dominated {
                    continue;
                }
                self.returned.push(layer);
                self.trace(space, query.layers, slot, path);
                return Some((cost, state));
            }
            let from = Arrival {
                node,
                slot,
                cost,
                state,
            };
            space.for_each_neighbor(node, &mut |next| {
                self.relax(space, rules, &query, &from, next);
            });
            if query.jumps {
                rules.for_each_jump(node, &state, &mut |next| {
                    self.relax(space, rules, &query, &from, next);
                });
            }
        }
        self.query = None;
        None
    }

    /// Queues `next` if moving there from `from` is allowed and improves on
    /// every way to it known so far.
    fn relax<SearchSpace, SearchRules>(
        &mut self,
        space: &SearchSpace,
        rules: &SearchRules,
        query: &Query<Node>,
        from: &Arrival<Node, Cost, State>,
        next: Node,
    ) where
        SearchSpace: Space<Node = Node>,
        SearchRules: Rules<Node = Node, Cost = Cost, State = State>,
    {
        let Some(next_index) = space.index(next) else {
            return;
        };
        let Some((step, next_state)) = rules.step(from.node, next, &from.state) else {
            return;
        };
        let layers = query.layers;
        let layer = rules.state_index(&next_state);
        debug_assert!(
            layer < layers,
            "state index {layer} >= state_count {layers}"
        );
        let node_slot = slot_of(next_index, layers, 0);
        let next_slot = node_slot + layer;
        let next_cost = from.cost + step;
        if self.finished[next_slot] == self.stamp
            || (self.reached[next_slot] == self.stamp && self.best[next_slot] <= next_cost)
        {
            return;
        }
        // Another state at this node, reached no dearer, may make this one
        // unnecessary.
        if layers > 1
            && (0..layers).any(|other| {
                other != layer
                    && self.reached[node_slot + other] == self.stamp
                    && self.best[node_slot + other] <= next_cost
                    && rules.dominates(other, layer)
            })
        {
            return;
        }
        self.reach(next_slot, next_cost, from.slot as u32);
        self.open.push(Open {
            estimate: next_cost + estimate(space, rules, query, next),
            cost: next_cost,
            slot: next_slot as u32,
            state: next_state,
        });
    }

    /// Makes room for `slots` and starts a new stamp.
    fn prepare(&mut self, slots: usize) {
        assert!(slots < NO_PARENT as usize, "too many nodes × states");
        if self.reached.len() < slots {
            self.reached.resize(slots, 0);
            self.finished.resize(slots, 0);
            self.best.resize(slots, Cost::ZERO);
            self.parent.resize(slots, NO_PARENT);
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            // Stamps wrapped: forget every old one.
            self.reached.fill(0);
            self.finished.fill(0);
            self.stamp = 1;
        }
        self.open.clear();
    }

    fn reach(&mut self, slot: usize, cost: Cost, parent: u32) {
        self.reached[slot] = self.stamp;
        self.best[slot] = cost;
        self.parent[slot] = parent;
    }

    fn trace<SearchSpace: Space<Node = Node>>(
        &self,
        space: &SearchSpace,
        layers: usize,
        goal_slot: usize,
        path: &mut Vec<Node>,
    ) {
        let mut slot = goal_slot as u32;
        while slot != NO_PARENT {
            path.push(space.node(slot as usize / layers));
            slot = self.parent[slot as usize];
        }
        path.reverse();
    }
}

/// Lower bound on the cost from `node` to the goal; zero with jumps, which
/// can beat any distance based bound.
fn estimate<SearchSpace, SearchRules>(
    space: &SearchSpace,
    rules: &SearchRules,
    query: &Query<SearchSpace::Node>,
    node: SearchSpace::Node,
) -> SearchRules::Cost
where
    SearchSpace: Space,
    SearchRules: Rules<Node = SearchSpace::Node>,
{
    if query.jumps {
        SearchRules::Cost::ZERO
    } else {
        rules.heuristic(node, query.goal, space.min_moves(node, query.goal))
    }
}

/// The slot being expanded.
struct Arrival<Node, Cost, State> {
    node: Node,
    slot: usize,
    cost: Cost,
    state: State,
}

fn slot_of(node_index: usize, layers: usize, layer: usize) -> usize {
    node_index * layers + layer
}

/// An open-list entry. The heap pops the lowest estimate first and, among
/// equal estimates, the costliest so far: deeper paths first, which heads
/// straight for the goal instead of widening across ties.
#[derive(Debug)]
struct Open<Cost, State> {
    estimate: Cost,
    cost: Cost,
    slot: u32,
    state: State,
}

impl<Cost: Ord, State> PartialEq for Open<Cost, State> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<Cost: Ord, State> Eq for Open<Cost, State> {}

impl<Cost: Ord, State> PartialOrd for Open<Cost, State> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<Cost: Ord, State> Ord for Open<Cost, State> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .cmp(&self.estimate)
            .then_with(|| self.cost.cmp(&other.cost))
            .then_with(|| other.slot.cmp(&self.slot))
    }
}
