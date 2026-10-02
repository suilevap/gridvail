//! ECS data model, grouped by gameplay responsibility.
//!
//! This layer contains data only. Systems live in `simulation`, `ai`, `vision`,
//! `lighting`, `animation`, and `presentation`; game-specific bundles live
//! in `app`.

mod actors;
mod ai;
mod animation;
mod items;
mod lighting;
mod presentation;
mod spatial;
mod vision;
mod world;

pub use actors::*;
pub use ai::*;
pub use animation::*;
pub use items::*;
pub use lighting::*;
pub use presentation::*;
pub use spatial::*;
pub use vision::*;
pub use world::*;
