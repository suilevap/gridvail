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
    pub const ALL: [LightKind; 4] = [
        LightKind::None,
        LightKind::Fire,
        LightKind::Electricity,
        LightKind::Acid,
    ];

    /// Its place in `ALL` and in a `LightCell`'s levels.
    pub const fn index(self) -> usize {
        match self {
            LightKind::None => 0,
            LightKind::Fire => 1,
            LightKind::Electricity => 2,
            LightKind::Acid => 3,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct LightSource {
    pub radius: i32,
    pub kind: LightKind,
    pub value: u8,
}

/// The light reaching a map cell: how much of each kind, each summed over
/// every light of that kind and saturating at 255. Kinds are kept apart so
/// the cell's colour mixes them all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightCell {
    pub levels: [u8; LightKind::ALL.len()],
}

impl LightCell {
    /// `value` of one kind of light.
    pub fn of(kind: LightKind, value: u8) -> Self {
        let mut cell = Self::default();
        cell.levels[kind.index()] = value;
        cell
    }

    pub fn level(&self, kind: LightKind) -> u8 {
        self.levels[kind.index()]
    }

    pub fn add(&mut self, kind: LightKind, amount: u8) {
        let level = &mut self.levels[kind.index()];
        *level = level.saturating_add(amount);
    }

    /// All the light together, saturating at 255.
    pub fn total(&self) -> u8 {
        self.levels
            .iter()
            .fold(0u8, |total, &level| total.saturating_add(level))
    }
}
