//! Presentation data for a character's four-view PNG.
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct CharacterSpec {
    pub world_height: f32,
}
