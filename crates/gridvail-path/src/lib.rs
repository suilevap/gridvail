//! Pathfinding with pluggable step rules and allocation-free repeated
//! searches.
//!
//! The core is independent of any map shape:
//! - A [`Space`] is the graph searched: nodes with dense indices and their
//!   neighbours. With the `grid` feature (on by default), [`grid::Grid`] is a
//!   4-connected grid of [`grid::Cell`]s.
//! - [`Rules`] decide what each move costs, or forbid it. Rules may carry a
//!   small search state (keys used, doors passed): the search keeps one layer
//!   of nodes per state, so a node reached with different states is searched
//!   separately. Rules combine as tuples, `(a, b)`.
//! - [`PathSearch`] finds the cheapest path (A*), and can go on to stream the
//!   cheapest path for each other state the goal is reachable in (with a
//!   door count as state: the best way through two doors, one, none), in
//!   order of cost, from the same search. It owns its working memory and
//!   reuses it, so once warmed up to the largest search it allocates nothing;
//!   paths go into a caller's `Vec`.
//!
//! Costs are any [`PathCost`]: unsigned integers, or [`Lex`] pairs compared
//! lexicographically ("fewest keys, then shortest").

mod cost;
mod rules;
mod search;
mod space;

#[cfg(feature = "grid")]
pub mod grid;

pub use cost::{Lex, PathCost};
pub use rules::{Rules, StepFn};
pub use search::PathSearch;
pub use space::Space;

#[cfg(all(test, feature = "grid"))]
mod tests;
