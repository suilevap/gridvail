//! Reusable gameplay systems. The application schedule is assembled in
//! `app`; this module groups systems by responsibility.

#![allow(clippy::type_complexity)]

mod control;
mod lifecycle;
mod motion;
mod resolution;
mod tiles;
mod turn;

pub use control::*;
pub use lifecycle::*;
pub use motion::*;
pub use resolution::*;
pub use tiles::*;
pub use turn::*;

#[cfg(test)]
pub mod test_app;

#[cfg(test)]
mod tests;
