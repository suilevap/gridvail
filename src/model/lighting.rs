use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightKind {
    #[default]
    None = 0,
    Fire = 1,
    Electricity = 2,
    Acid = 4,
}

impl LightKind {
    pub fn overlaps(self, other: LightKind) -> bool {
        (self as u8) & (other as u8) != 0
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct LightSource {
    pub radius: i32,
    pub kind: LightKind,
    pub value: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightCell {
    pub value: u8,
    pub kind: LightKind,
}
