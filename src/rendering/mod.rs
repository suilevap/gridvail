//! Swappable output backends for composed game frames.

mod extruded;
mod floor;
mod settings;
mod text;
// Desktop only: the web build leaves 3D rendering out to keep its download
// small.
#[cfg(not(target_family = "wasm"))]
mod walls_3d;

pub use floor::{FloorShaderPlugin, FLOOR_TILES};
pub use text::{TextRenderStats, TextRendererPlugin};
#[cfg(not(target_family = "wasm"))]
pub use walls_3d::ExtrudedWallRendererPlugin;
