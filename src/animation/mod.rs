//! Renderer-neutral animation of objects moving between grid cells.
//!
//! The simulation moves objects from cell to cell (one step per turn for
//! actors, or several cells at once). This step turns those moves into
//! continuous motion (`AnimatedPos`) for any renderer, and tells
//! `TurnPacing` how long the unfinished animations still run, so the next
//! turn waits for them (held input follows the animation). Without it the
//! simulation runs with no delay.

mod hierarchy;
mod motion;
mod plugin;

pub use hierarchy::*;
pub use motion::*;
pub use plugin::*;

#[cfg(test)]
mod tests;
