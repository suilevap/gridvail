//! ECS data model, grouped by gameplay responsibility.
//!
//! This layer contains data only. Systems live in `simulation`, `vision`,
//! `lighting`, `animation`, and `presentation`; game-specific bundles live
//! in `app`.

mod actors;
mod ai;
mod animation;
mod camera;
mod input;
mod items;
mod lighting;
mod locomotion;
mod presentation;
mod spatial;
mod vision;
mod world;

pub use actors::*;
pub use ai::*;
pub use animation::*;
pub use camera::*;
pub use input::*;
pub use items::*;
pub use lighting::*;
pub use locomotion::*;
pub use presentation::*;
pub use spatial::*;
pub use vision::*;
pub use world::*;
