//! Reusable gameplay systems and the plugin that installs their schedule.

#![allow(clippy::type_complexity)]

mod control;
mod items;
mod lifecycle;
mod motion;
mod plates;
mod plugin;
mod portals;
mod resolution;
pub mod room_plans;
mod rooms;
mod tiles;
mod turn;

pub use control::*;
pub use items::*;
pub use lifecycle::*;
pub use motion::*;
pub use plates::*;
pub use plugin::*;
pub use portals::*;
pub use resolution::*;
pub use rooms::*;
pub use tiles::*;
pub use turn::*;

#[cfg(test)]
pub mod test_app;

#[cfg(test)]
mod tests;
