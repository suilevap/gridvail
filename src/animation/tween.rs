use bevy::prelude::*;

/// Eases one value (a number, a point, a rotation) from where it is shown
/// to a target, along Bevy's `EasingCurve`.
///
/// Retargeting mid-way starts a new curve from the value shown at that
/// moment, so a tween that changes its mind never jumps.
#[derive(Clone, Debug)]
pub struct Tween<T> {
    curve: EasingCurve<T>,
    to: T,
    elapsed: f32,
    duration: f32,
}

impl<T: Ease + Clone> Tween<T> {
    /// At rest on `value`.
    pub fn at(value: T) -> Self {
        Self {
            curve: EasingCurve::new(value.clone(), value.clone(), EaseFunction::Linear),
            to: value,
            elapsed: 0.0,
            duration: 0.0,
        }
    }

    /// The value shown now.
    pub fn value(&self) -> T {
        if self.done() {
            return self.to.clone();
        }
        self.curve.sample_clamped(self.elapsed / self.duration)
    }

    /// Where the tween ends.
    pub fn target(&self) -> T {
        self.to.clone()
    }

    pub fn done(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// Eases from the shown value to `to` over `duration` seconds; zero
    /// jumps there.
    pub fn ease_to(&mut self, to: T, duration: f32, easing: EaseFunction) {
        self.curve = EasingCurve::new(self.value(), to.clone(), easing);
        self.to = to;
        self.elapsed = 0.0;
        self.duration = duration.max(0.0);
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
