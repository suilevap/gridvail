use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::{Cell, Cost, Grid, Rules};

const NO_PARENT: u32 = u32::MAX;

/// Reusable A* over a [`Grid`] and [`Rules`].
///
/// Working memory is one slot per cell and rule state ("layer"): its best
/// cost so far, the slot it was reached from, and whether it is finished. A
/// slot belongs to the current search only when its stamp matches, so a new
/// search starts by bumping the stamp instead of clearing anything. Storage
/// and the open list grow to the largest search made, then are reused: after
/// that, searches allocate nothing.
///
/// One `PathSearch` serves any rules with the same cost and state types.
#[derive(Debug)]
pub struct PathSearch<C, S> {
    stamp: u32,
    /// Stamp of the search that last reached each slot.
    reached: Vec<u32>,
    /// Stamp of the search that last finished each slot.
    finished: Vec<u32>,
    best: Vec<C>,
    parent: Vec<u32>,
    open: BinaryHeap<Open<C, S>>,
    expanded: usize,
}

impl<C: Cost, S: Copy + Eq> Default for PathSearch<C, S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Cost, S: Copy + Eq> PathSearch<C, S> {
    pub fn new() -> Self {
        Self {
            stamp: 0,
            reached: Vec::new(),
            finished: Vec::new(),
            best: Vec::new(),
            parent: Vec::new(),
            open: BinaryHeap::new(),
            expanded: 0,
        }
    }

    /// Room for `grid` with `states` layers allocated up front, so the first
    /// search does not grow it. The open list still grows on first use.
    pub fn with_capacity(grid: &Grid, states: usize) -> Self {
        let mut search = Self::new();
        search.prepare(grid.len() * states.max(1));
        search
    }

    /// Slots finished by the last search.
    pub fn expanded(&self) -> usize {
        self.expanded
    }

    /// The cheapest path from `start` to `goal`, written into `path` (start
    /// and goal included), and its cost. `None`, with `path` empty, when no
    /// path exists or an end is off the grid.
    pub fn find<R>(
        &mut self,
        grid: &Grid,
        rules: &R,
        start: Cell,
        goal: Cell,
        path: &mut Vec<Cell>,
    ) -> Option<C>
    where
        R: Rules<Cost = C, State = S>,
    {
        path.clear();
        self.expanded = 0;
        let start_index = grid.index(start)?;
        grid.index(goal)?;
        let layers = rules.state_count().max(1);
        self.prepare(grid.len() * layers);

        let state = rules.start_state(start);
        let slot = slot_of(start_index, layers, rules.state_index(&state));
        self.reach(slot, C::ZERO, NO_PARENT);
        self.open.push(Open {
            estimate: rules.heuristic(grid, start, goal),
            cost: C::ZERO,
            slot: slot as u32,
            state,
        });

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
            let cell = grid.cell(slot / layers);
            if cell == goal {
                self.trace(grid, layers, slot, path);
                self.open.clear();
                return Some(cost);
            }
            for next in grid.neighbors(cell) {
                let Some((step, next_state)) = rules.step(cell, next, &state) else {
                    continue;
                };
                let layer = rules.state_index(&next_state);
                debug_assert!(
                    layer < layers,
                    "state index {layer} >= state_count {layers}"
                );
                let next_slot = slot_of(grid.index(next).expect("neighbor on grid"), layers, layer);
                let next_cost = cost + step;
                if self.finished[next_slot] == self.stamp
                    || (self.reached[next_slot] == self.stamp && self.best[next_slot] <= next_cost)
                {
                    continue;
                }
                self.reach(next_slot, next_cost, slot as u32);
                self.open.push(Open {
                    estimate: next_cost + rules.heuristic(grid, next, goal),
                    cost: next_cost,
                    slot: next_slot as u32,
                    state: next_state,
                });
            }
        }
        self.open.clear();
        None
    }

    /// Makes room for `slots` and starts a new stamp.
    fn prepare(&mut self, slots: usize) {
        assert!(slots < NO_PARENT as usize, "too many cells × states");
        if self.reached.len() < slots {
            self.reached.resize(slots, 0);
            self.finished.resize(slots, 0);
            self.best.resize(slots, C::ZERO);
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

    fn reach(&mut self, slot: usize, cost: C, parent: u32) {
        self.reached[slot] = self.stamp;
        self.best[slot] = cost;
        self.parent[slot] = parent;
    }

    fn trace(&self, grid: &Grid, layers: usize, goal: usize, path: &mut Vec<Cell>) {
        let mut slot = goal as u32;
        while slot != NO_PARENT {
            path.push(grid.cell(slot as usize / layers));
            slot = self.parent[slot as usize];
        }
        path.reverse();
    }
}

fn slot_of(cell: usize, layers: usize, layer: usize) -> usize {
    cell * layers + layer
}

/// An open-list entry. The heap pops the lowest estimate first and, among
/// equal estimates, the costliest so far: deeper paths first, which on grids
/// heads straight for the goal instead of widening across ties.
#[derive(Debug)]
struct Open<C, S> {
    estimate: C,
    cost: C,
    slot: u32,
    state: S,
}

impl<C: Ord, S> PartialEq for Open<C, S> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<C: Ord, S> Eq for Open<C, S> {}

impl<C: Ord, S> PartialOrd for Open<C, S> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<C: Ord, S> Ord for Open<C, S> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .cmp(&self.estimate)
            .then_with(|| self.cost.cmp(&other.cost))
            .then_with(|| other.slot.cmp(&self.slot))
    }
}
