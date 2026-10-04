use bevy::math::StableInterpolate;

use super::Easing;

/// Eases one value (a number, a point) from where it is shown to a target.
///
/// Retargeting mid-way starts from the value shown at that moment, so a
/// tween that changes its mind never jumps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween<T> {
    from: T,
    to: T,
    elapsed: f32,
    duration: f32,
    easing: Easing,
}

impl<T: StableInterpolate + Copy> Tween<T> {
    /// At rest on `value`.
    pub fn at(value: T) -> Self {
        Self {
            from: value,
            to: value,
            elapsed: 0.0,
            duration: 0.0,
            easing: Easing::Linear,
        }
    }

    /// The value shown now.
    pub fn value(&self) -> T {
        if self.elapsed >= self.duration {
            return self.to;
        }
        let t = self.easing.apply(self.elapsed / self.duration);
        self.from.interpolate_stable(&self.to, t)
    }

    /// Where the tween ends.
    pub fn target(&self) -> T {
        self.to
    }

    pub fn done(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// Eases from the shown value to `to` over `duration` seconds; zero
    /// jumps there.
    pub fn ease_to(&mut self, to: T, duration: f32, easing: Easing) {
        self.from = self.value();
        self.to = to;
        self.elapsed = 0.0;
        self.duration = duration.max(0.0);
        self.easing = easing;
    }

    /// Jumps to `value` and rests there.
    pub fn snap(&mut self, value: T) {
        *self = Self::at(value);
    }

    pub fn advance(&mut self, dt: f32) {
        if !self.done() {
            self.elapsed += dt;
        }
    }
}
