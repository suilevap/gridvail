//! Settings the player chooses in game, and the menu that changes them.

use std::f32::consts::TAU;

use bevy::prelude::*;

/// When the compass (which way north is on screen) is shown.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompassMode {
    /// Only while the view is turned away from north up.
    #[default]
    WhenTurned,
    Always,
    Off,
}

impl CompassMode {
    pub const ALL: [Self; 3] = [Self::WhenTurned, Self::Always, Self::Off];

    pub fn name(self) -> &'static str {
        match self {
            Self::WhenTurned => "when-turned",
            Self::Always => "always",
            Self::Off => "off",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }

    /// The next mode, or the previous one with `step` -1.
    pub fn step(self, step: i32) -> Self {
        let index = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0) as i32;
        let len = Self::ALL.len() as i32;
        Self::ALL[(index + step).rem_euclid(len) as usize]
    }

    /// Whether the compass shows for a view turned `rotation` radians.
    pub fn shown(self, rotation: f32) -> bool {
        match self {
            Self::WhenTurned => {
                let off_north = rotation - (rotation / TAU).round() * TAU;
                off_north.abs() > 1e-3
            }
            Self::Always => true,
            Self::Off => false,
        }
    }
}

/// The settings menu. While it is open it takes the arrow keys, so the
/// player and the camera wait.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsMenu {
    pub open: bool,
    /// The line being changed.
    pub selected: usize,
}

impl SettingsMenu {
    pub fn is_open(menu: Option<&Self>) -> bool {
        menu.is_some_and(|menu| menu.open)
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, TAU};

    use super::*;

    #[test]
    fn the_compass_shows_when_turned_unless_set_otherwise() {
        let turned = CompassMode::WhenTurned;
        assert!(!turned.shown(0.0));
        assert!(!turned.shown(TAU), "a whole turn is north up again");
        assert!(!turned.shown(-2.0 * TAU));
        assert!(turned.shown(FRAC_PI_2));
        assert!(turned.shown(-0.1));
        assert!(CompassMode::Always.shown(0.0));
        assert!(!CompassMode::Off.shown(FRAC_PI_2));
        assert_eq!(CompassMode::Off.step(1), CompassMode::WhenTurned);
        assert_eq!(CompassMode::WhenTurned.step(-1), CompassMode::Off);
        for mode in CompassMode::ALL {
            assert_eq!(CompassMode::from_name(mode.name()), Some(mode));
        }
    }
}
