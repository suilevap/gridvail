//! Renderer-neutral camera direction.
//!
//! `ViewCamera` (in `model`) is what renderers draw: where the view is
//! centred, how it is turned and zoomed. The `CameraOperator` here decides
//! where it goes: it follows a target (the player by default) and eases
//! every change of rotation, zoom, offset or target, so the view never
//! jumps. Renderers never move the view themselves.

mod operator;
mod plugin;

pub use operator::*;
pub use plugin::*;

#[cfg(test)]
mod tests;
