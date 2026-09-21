//! Swappable output backends for composed game frames.

mod text;
mod walls_3d;

pub use text::{TextRenderStats, TextRendererPlugin};
pub use walls_3d::ExtrudedWallRendererPlugin;
