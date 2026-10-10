use bevy::prelude::*;

/// The input the player used last. Each input module marks its own scheme;
/// hints read it without knowing how either input works.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlScheme {
    #[default]
    Keyboard,
    Touch,
}
