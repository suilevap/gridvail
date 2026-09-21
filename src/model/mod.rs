//! ECS data model, grouped by gameplay responsibility.
//!
//! This layer contains data only. Systems live in `simulation`, `vision`,
//! `lighting`, and `presentation`; game-specific bundles live in `app`.

mod actors;
mod lighting;
mod presentation;
mod spatial;
mod vision;
mod world;

pub use actors::*;
pub use lighting::*;
pub use presentation::*;
pub use spatial::*;
pub use vision::*;
pub use world::*;
