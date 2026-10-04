//! Grid pathfinding with pluggable step rules and allocation-free repeated
//! searches.
//!
//! - [`Rules`] decide what each step between adjacent cells costs, or forbid
//!   it. Rules may carry a small search state (keys used, doors passed): the
//!   search keeps one layer per state, so a cell reached with different states
//!   is searched separately. Rules combine as tuples, `(a, b)`.
//! - [`PathSearch`] finds the cheapest path (A*), and can go on to stream the
//!   cheapest path for each other state the goal is reachable in (with a
//!   door count as state: the best way through two doors, one, none), in
//!   order of cost, from the same search. It owns its working memory and
//!   reuses it, so once warmed up to the largest search it allocates nothing;
//!   paths go into a caller's `Vec`.
//!
//! Costs are any [`Cost`]: unsigned integers, or [`Lex`] pairs compared
//! lexicographically ("fewest keys, then shortest").

mod cost;
mod grid;
mod rules;
mod search;

pub use cost::{Cost, Lex};
pub use grid::{Cell, Grid};
pub use rules::{Rules, StepFn};
pub use search::PathSearch;

#[cfg(test)]
mod tests;
