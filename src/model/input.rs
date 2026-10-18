use bevy::prelude::*;

/// The input the player used last. Each input module marks its own scheme;
/// hints read it without knowing how either input works.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlScheme {
    #[default]
    Keyboard,
    Touch,
}

/// Shared by keyboard and stationary touch holds. Each hold commits once.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct WaitHold {
    pub elapsed: f32,
    pub latched: bool,
    pub touch_held: bool,
}

impl WaitHold {
    pub const SECONDS: f32 = 0.6;

    pub fn progress(&self) -> f32 {
        (self.elapsed / Self::SECONDS).clamp(0.0, 1.0)
    }

    pub fn update(&mut self, held: bool, ready: bool, seconds: f32) -> bool {
        if !held {
            self.elapsed = 0.0;
            self.latched = false;
            return false;
        }
        if self.latched {
            return false;
        }
        if !ready {
            self.elapsed = 0.0;
            return false;
        }
        self.elapsed = (self.elapsed + seconds).min(Self::SECONDS);
        if self.elapsed >= Self::SECONDS {
            self.latched = true;
            true
        } else {
            false
        }
    }
}
