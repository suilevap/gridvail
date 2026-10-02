//! Grid pathfinding with pluggable step costs, built on the `pathfinding`
//! crate's A* and Yen's k-shortest-paths.
//!
//! A [`CostModel`] decides what a single step costs, or forbids it. Models may
//! carry their own search state (a [`CostModel::State`]), so a rule like "pass
//! at most two closed doors" is a model whose state counts the doors so far:
//! the search then treats the same cell reached with a different count as a
//! different node. Models combine as tuples, `(a, b)`, adding their costs and
//! forbidding a step either one forbids.
//!
//! Three searches share every model:
//! - [`find_path`]: the cheapest path (A*).
//! - [`k_shortest_paths`]: the `k` cheapest distinct paths (Yen). On an open
//!   grid these are mostly the same route with a cell shifted, which suits
//!   picking among nearly equal options.
//! - [`alternative_paths`]: genuinely different routes, found by penalizing the
//!   cells of each route found so far and searching again.

use std::collections::HashMap;
use std::hash::Hash;

use bevy::prelude::*;

/// Step cost. Integer so that sums are exact and orderable.
pub type Cost = u32;

const DIRECTIONS: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

/// Size and edge behaviour of the searched grid (4-connected).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridShape {
    pub width: i32,
    pub height: i32,
    /// Whether moving off one edge enters the opposite one.
    pub wrap: bool,
}

impl GridShape {
    pub fn contains(&self, p: IVec2) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height
    }

    /// The cell one step from `p` in `direction`, if there is one.
    pub fn neighbor(&self, p: IVec2, direction: IVec2) -> Option<IVec2> {
        let next = p + direction;
        if self.wrap {
            Some(IVec2::new(
                next.x.rem_euclid(self.width),
                next.y.rem_euclid(self.height),
            ))
        } else {
            self.contains(next).then_some(next)
        }
    }

    /// Fewest steps between two cells, ignoring obstacles.
    pub fn distance(&self, a: IVec2, b: IVec2) -> u32 {
        let axis = |delta: i32, size: i32| {
            let delta = delta.unsigned_abs();
            if self.wrap {
                delta.min(size as u32 - delta)
            } else {
                delta
            }
        };
        axis(b.x - a.x, self.width) + axis(b.y - a.y, self.height)
    }
}

/// What moving between two adjacent cells costs.
pub trait CostModel {
    /// Search state carried along a path, `()` for plain per-step costs.
    /// Paths reaching a cell with different states are searched separately.
    type State: Clone + Eq + Hash;

    /// State at the start of every path.
    fn initial_state(&self, start: IVec2) -> Self::State;

    /// Cost of stepping from `from` to the adjacent `to` with `state` so far,
    /// and the state after the step; `None` forbids the step.
    fn step(&self, from: IVec2, to: IVec2, state: &Self::State) -> Option<(Cost, Self::State)>;

    /// A lower bound on any allowed step's cost, used by A*'s heuristic. Zero
    /// (the default) is always correct; higher bounds search faster.
    fn min_step_cost(&self) -> Cost {
        0
    }
}

impl<M: CostModel + ?Sized> CostModel for &M {
    type State = M::State;

    fn initial_state(&self, start: IVec2) -> Self::State {
        (**self).initial_state(start)
    }

    fn step(&self, from: IVec2, to: IVec2, state: &Self::State) -> Option<(Cost, Self::State)> {
        (**self).step(from, to, state)
    }

    fn min_step_cost(&self) -> Cost {
        (**self).min_step_cost()
    }
}

/// Both models apply: costs add, and either may forbid a step.
impl<A: CostModel, B: CostModel> CostModel for (A, B) {
    type State = (A::State, B::State);

    fn initial_state(&self, start: IVec2) -> Self::State {
        (self.0.initial_state(start), self.1.initial_state(start))
    }

    fn step(&self, from: IVec2, to: IVec2, state: &Self::State) -> Option<(Cost, Self::State)> {
        let (a_cost, a_state) = self.0.step(from, to, &state.0)?;
        let (b_cost, b_state) = self.1.step(from, to, &state.1)?;
        Some((a_cost + b_cost, (a_state, b_state)))
    }

    fn min_step_cost(&self) -> Cost {
        self.0.min_step_cost() + self.1.min_step_cost()
    }
}

/// A stateless model from a closure: `cost(from, to)`, `None` to forbid.
pub struct StepCost<F> {
    cost: F,
    min_step_cost: Cost,
}

impl<F: Fn(IVec2, IVec2) -> Option<Cost>> StepCost<F> {
    /// `min_step_cost` must not exceed any cost `cost` returns.
    pub fn new(min_step_cost: Cost, cost: F) -> Self {
        Self {
            cost,
            min_step_cost,
        }
    }
}

impl<F: Fn(IVec2, IVec2) -> Option<Cost>> CostModel for StepCost<F> {
    type State = ();

    fn initial_state(&self, _start: IVec2) {}

    fn step(&self, from: IVec2, to: IVec2, _state: &()) -> Option<(Cost, ())> {
        (self.cost)(from, to).map(|cost| (cost, ()))
    }

    fn min_step_cost(&self) -> Cost {
        self.min_step_cost
    }
}

/// A path from its start cell to its goal cell, both included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    pub cells: Vec<IVec2>,
    pub cost: Cost,
}

impl Path {
    /// Number of steps.
    pub fn len(&self) -> usize {
        self.cells.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn from_nodes<S>(nodes: Vec<(IVec2, S)>, cost: Cost) -> Self {
        Self {
            cells: nodes.into_iter().map(|(cell, _)| cell).collect(),
            cost,
        }
    }

    /// Whether some cell appears twice; possible when a model's state lets
    /// a path return to a cell.
    fn revisits(&self) -> bool {
        let mut seen = std::collections::HashSet::with_capacity(self.cells.len());
        !self.cells.iter().all(|cell| seen.insert(*cell))
    }
}

/// The cheapest path from `start` to `goal`, if one exists.
pub fn find_path<M: CostModel>(
    shape: GridShape,
    model: &M,
    start: IVec2,
    goal: IVec2,
) -> Option<Path> {
    let min_step = model.min_step_cost();
    pathfinding::directed::astar::astar(
        &(start, model.initial_state(start)),
        |node| successors(shape, model, node),
        |(cell, _)| shape.distance(*cell, goal) * min_step,
        |(cell, _)| *cell == goal,
    )
    .map(|(nodes, cost)| Path::from_nodes(nodes, cost))
}

/// Up to `k` cheapest paths from `start` to `goal`, cheapest first. No two
/// share their exact sequence of cells, and none visits a cell twice.
pub fn k_shortest_paths<M: CostModel>(
    shape: GridShape,
    model: &M,
    start: IVec2,
    goal: IVec2,
    k: usize,
) -> Vec<Path> {
    pathfinding::directed::yen::yen(
        &(start, model.initial_state(start)),
        |node| successors(shape, model, node),
        |(cell, _)| *cell == goal,
        k,
    )
    .into_iter()
    .map(|(nodes, cost)| Path::from_nodes(nodes, cost))
    .filter(|path| !path.revisits())
    .collect()
}

/// How different the routes of [`alternative_paths`] must be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alternatives {
    /// Most routes returned.
    pub count: usize,
    /// Extra cost per earlier route through a cell, while searching for the
    /// next. Larger values push later routes further away.
    pub penalty: Cost,
    /// Largest share (0 to 1) of a route's intermediate cells that may also
    /// lie on an earlier route; routes overlapping more are skipped.
    pub max_shared: f32,
}

impl Default for Alternatives {
    fn default() -> Self {
        Self {
            count: 3,
            penalty: 4,
            max_shared: 0.5,
        }
    }
}

/// Up to `options.count` routes from `start` to `goal` that differ from one
/// another, cheapest first. The first is the cheapest path; each later one is
/// the cheapest once the cells of earlier routes cost `options.penalty` more
/// per route through them. Returned costs are the model's own, without
/// penalties.
pub fn alternative_paths<M: CostModel>(
    shape: GridShape,
    model: &M,
    start: IVec2,
    goal: IVec2,
    options: Alternatives,
) -> Vec<Path> {
    let mut routes: Vec<Path> = Vec::with_capacity(options.count);
    let mut uses = HashMap::<IVec2, Cost>::new();
    // Penalties grow with every search, so a few more searches than routes
    // are enough to find what exists.
    for _ in 0..options.count * 4 {
        if routes.len() >= options.count {
            break;
        }
        let penalized = (
            model,
            Penalty {
                uses: &uses,
                penalty: options.penalty,
            },
        );
        let Some(found) = find_path(shape, &penalized, start, goal) else {
            break;
        };
        let Some(route) = rescore(model, found.cells) else {
            break;
        };
        let interior = interior(&route);
        for cell in interior {
            *uses.entry(*cell).or_default() += 1;
        }
        let distinct = routes.iter().all(|earlier| {
            earlier.cells != route.cells && shared(interior, earlier) <= options.max_shared
        });
        if distinct {
            routes.push(route);
        }
    }
    routes.sort_by_key(|route| route.cost);
    routes
}

fn successors<'a, M: CostModel>(
    shape: GridShape,
    model: &'a M,
    (at, state): &(IVec2, M::State),
) -> impl Iterator<Item = ((IVec2, M::State), Cost)> + use<'a, M> {
    let (at, state) = (*at, state.clone());
    DIRECTIONS.into_iter().filter_map(move |direction| {
        let to = shape.neighbor(at, direction)?;
        let (cost, next) = model.step(at, to, &state)?;
        Some(((to, next), cost))
    })
}

/// Extra cost for entering cells that earlier routes used.
struct Penalty<'a> {
    uses: &'a HashMap<IVec2, Cost>,
    penalty: Cost,
}

impl CostModel for Penalty<'_> {
    type State = ();

    fn initial_state(&self, _start: IVec2) {}

    fn step(&self, _from: IVec2, to: IVec2, _state: &()) -> Option<(Cost, ())> {
        let uses = self.uses.get(&to).copied().unwrap_or(0);
        Some((uses * self.penalty, ()))
    }
}

/// The model's own cost of walking `cells`, which it allowed before.
fn rescore<M: CostModel>(model: &M, cells: Vec<IVec2>) -> Option<Path> {
    let mut state = model.initial_state(*cells.first()?);
    let mut cost = 0;
    for pair in cells.windows(2) {
        let (step, next) = model.step(pair[0], pair[1], &state)?;
        cost += step;
        state = next;
    }
    Some(Path { cells, cost })
}

fn interior(path: &Path) -> &[IVec2] {
    match path.cells.len() {
        0..=2 => &[],
        n => &path.cells[1..n - 1],
    }
}

/// Share of `cells` that also lie on `other`.
fn shared(cells: &[IVec2], other: &Path) -> f32 {
    if cells.is_empty() {
        return 0.0;
    }
    let common = cells
        .iter()
        .filter(|cell| other.cells.contains(cell))
        .count();
    common as f32 / cells.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map from rows of text: `#` blocks, `D` is a closed door, `.` floor.
    struct Rows(Vec<Vec<char>>);

    impl Rows {
        fn new(rows: &[&str]) -> Self {
            Self(rows.iter().map(|row| row.chars().collect()).collect())
        }

        fn shape(&self) -> GridShape {
            GridShape {
                width: self.0[0].len() as i32,
                height: self.0.len() as i32,
                wrap: false,
            }
        }

        fn at(&self, p: IVec2) -> char {
            self.0[p.y as usize][p.x as usize]
        }

        fn find(&self, ch: char) -> IVec2 {
            for (y, row) in self.0.iter().enumerate() {
                if let Some(x) = row.iter().position(|c| *c == ch) {
                    return IVec2::new(x as i32, y as i32);
                }
            }
            panic!("no {ch}");
        }
    }

    impl CostModel for Rows {
        type State = ();

        fn initial_state(&self, _start: IVec2) {}

        fn step(&self, _from: IVec2, to: IVec2, _state: &()) -> Option<(Cost, ())> {
            (self.at(to) != '#').then_some((1, ()))
        }

        fn min_step_cost(&self) -> Cost {
            1
        }
    }

    /// Passes at most `max` closed doors: an example of a stateful rule.
    struct DoorLimit<'a> {
        rows: &'a Rows,
        max: u8,
    }

    impl CostModel for DoorLimit<'_> {
        type State = u8;

        fn initial_state(&self, _start: IVec2) -> u8 {
            0
        }

        fn step(&self, _from: IVec2, to: IVec2, doors: &u8) -> Option<(Cost, u8)> {
            let doors = doors + u8::from(self.rows.at(to) == 'D');
            (doors <= self.max).then_some((0, doors))
        }
    }

    // Two routes from S to G: through two doors (short) or around (long).
    const DOORS: [&str; 3] = ["S.D.D.G", ".#####.", "......."];

    #[test]
    fn finds_the_cheapest_path_around_walls() {
        let rows = Rows::new(&["S.#..", "..#..", "....G"]);
        let path = find_path(rows.shape(), &rows, rows.find('S'), rows.find('G')).unwrap();
        assert_eq!(path.cost, 6);
        assert_eq!(path.len(), 6);
        assert_eq!(path.cells.first(), Some(&rows.find('S')));
        assert_eq!(path.cells.last(), Some(&rows.find('G')));
        assert!(path.cells.iter().all(|cell| rows.at(*cell) != '#'));
    }

    #[test]
    fn unreachable_goals_have_no_path() {
        let rows = Rows::new(&["S#G"]);
        assert_eq!(
            find_path(rows.shape(), &rows, rows.find('S'), rows.find('G')),
            None
        );
    }

    #[test]
    fn wrapping_grids_step_across_the_edge() {
        let shape = GridShape {
            width: 10,
            height: 1,
            wrap: true,
        };
        let open = StepCost::new(1, |_, _| Some(1));
        let path = find_path(shape, &open, IVec2::new(1, 0), IVec2::new(8, 0)).unwrap();
        assert_eq!(path.cells, [1, 0, 9, 8].map(|x| IVec2::new(x, 0)));
        assert_eq!(shape.distance(IVec2::new(1, 0), IVec2::new(8, 0)), 3);
    }

    #[test]
    fn stateful_models_limit_closed_doors() {
        let rows = Rows::new(&DOORS);
        let (start, goal) = (rows.find('S'), rows.find('G'));
        let doors_on = |path: &Path| path.cells.iter().filter(|c| rows.at(**c) == 'D').count();

        let any = find_path(rows.shape(), &rows, start, goal).unwrap();
        assert_eq!((any.cost, doors_on(&any)), (6, 2));

        for max in [0, 1] {
            let limited = (&rows, DoorLimit { rows: &rows, max });
            let around = find_path(rows.shape(), &limited, start, goal).unwrap();
            assert_eq!(doors_on(&around), 0, "max {max} doors must avoid both");
            assert_eq!(around.cost, 10);
        }
    }

    #[test]
    fn closure_costs_steer_the_path() {
        let rows = Rows::new(&DOORS);
        let (start, goal) = (rows.find('S'), rows.find('G'));
        // Doors cost 10 extra: going around is cheaper.
        let doors_hurt = StepCost::new(0, |_, to| Some(if rows.at(to) == 'D' { 10 } else { 0 }));
        let path = find_path(rows.shape(), &(&rows, doors_hurt), start, goal).unwrap();
        assert_eq!(path.cost, 10);
    }

    #[test]
    fn k_shortest_paths_are_distinct_and_ordered() {
        let rows = Rows::new(&["S..", "...", "..G"]);
        let paths = k_shortest_paths(rows.shape(), &rows, rows.find('S'), rows.find('G'), 6);
        // Every monotone path through a 3x3 grid: C(4, 2) = 6, all 4 steps.
        assert_eq!(paths.len(), 6);
        assert!(paths.iter().all(|path| path.cost == 4));
        for (i, a) in paths.iter().enumerate() {
            assert!(paths[i + 1..].iter().all(|b| a.cells != b.cells));
        }

        let rows = Rows::new(&DOORS);
        let paths = k_shortest_paths(rows.shape(), &rows, rows.find('S'), rows.find('G'), 2);
        assert_eq!(paths.iter().map(|p| p.cost).collect::<Vec<_>>(), [6, 10]);
    }

    #[test]
    fn alternative_paths_take_different_routes() {
        let rows = Rows::new(&DOORS);
        let (start, goal) = (rows.find('S'), rows.find('G'));
        let routes = alternative_paths(rows.shape(), &rows, start, goal, Alternatives::default());
        assert_eq!(routes.len(), 2, "only two routes exist");
        assert_eq!(routes.iter().map(|p| p.cost).collect::<Vec<_>>(), [6, 10]);
        assert!(shared(interior(&routes[1]), &routes[0]) <= 0.5);

        // An open room: k-shortest paths hug each other, alternatives spread.
        let rows = Rows::new(&[
            "S........",
            ".........",
            ".........",
            ".........",
            "........G",
        ]);
        let (start, goal) = (rows.find('S'), rows.find('G'));
        let options = Alternatives {
            count: 3,
            penalty: 4,
            max_shared: 0.3,
        };
        let routes = alternative_paths(rows.shape(), &rows, start, goal, options);
        assert_eq!(routes.len(), 3);
        for (i, a) in routes.iter().enumerate() {
            for b in &routes[i + 1..] {
                assert!(shared(interior(b), a) <= 0.3);
            }
        }
    }
}
