//! Grid pathfinding with pluggable step rules and allocation-free repeated
//! searches.
//!
//! - [`Rules`] decide what each step between adjacent cells costs, or forbid
//!   it. Rules may carry a small search state (keys used, doors passed): the
//!   search keeps one layer per state, so a cell reached with different states
//!   is searched separately. Rules combine as tuples, `(a, b)`.
//! - [`PathSearch`] finds the cheapest path (A*). It owns its working memory
//!   and reuses it, so once warmed up to the largest search it allocates
//!   nothing; the path goes into a caller's `Vec`.
//! - [`RouteSearch`] streams routes that differ from one another, one per
//!   [`RouteSearch::next`] call: the cheapest first, then each next cheapest
//!   once cells of earlier routes cost extra.
//!
//! Costs are any [`Cost`]: unsigned integers, or [`Lex`] pairs compared
//! lexicographically ("fewest keys, then shortest").

mod cost;
mod grid;
mod routes;
mod rules;
mod search;

pub use cost::{Cost, Lex};
pub use grid::{Cell, Grid};
pub use routes::{RouteOptions, RouteSearch};
pub use rules::{Rules, StepFn};
pub use search::PathSearch;

#[cfg(test)]
mod tests;
