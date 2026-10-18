use bevy::prelude::*;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EncounterOutcome {
    #[default]
    Ongoing,
    Victory,
    Defeat,
}
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct RestartRequest(pub bool);
#[derive(Component, Clone, Copy, Debug)]
pub struct RespawnPoint(pub IVec2);

#[derive(Component, Clone, Copy, Debug)]
pub struct RespawnGlyph(pub super::Glyph);
