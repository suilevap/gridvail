//! Presentation data for a row of the shared reference sprite atlas.
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct CharacterSpec {
    pub row: u32,
    pub world_height: f32,
}
