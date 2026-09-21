//! Reusable gameplay systems and the plugin that installs their schedule.

#![allow(clippy::type_complexity)]

mod control;
mod lifecycle;
mod motion;
mod plugin;
mod resolution;
mod tiles;
mod turn;

pub use control::*;
pub use lifecycle::*;
pub use motion::*;
pub use plugin::*;
pub use resolution::*;
pub use tiles::*;
pub use turn::*;

#[cfg(test)]
pub mod test_app;

#[cfg(test)]
mod tests;
