//! Renderer-neutral animation of objects moving between grid cells.
//!
//! The simulation moves objects from cell to cell (one step per turn for
//! actors, or several cells at once). This step turns those moves into
//! continuous motion (`AnimatedPos`) for any renderer.

mod motion;
mod plugin;

pub use motion::*;
pub use plugin::*;

#[cfg(test)]
mod tests;
