//! How animated objects travel between grid cells.
//!
//! A move can span any number of cells (a sliding wall, a push, a dash):
//! it takes proportionally longer, at the same speed as a one-cell step.
//! Each move tweens from wherever the object is shown, so a move that
//! arrives mid-glide continues without a jump.

use bevy::prelude::*;

/// Shape of a tween from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    /// Fast start, gentle arrival.
    EaseOut,
    /// Gentle start and arrival.
    EaseInOut,
    /// Passes the cell and settles back; `overshoot` 1.70158 is ~10%.
    Back {
        overshoot: f32,
    },
}

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseOut => t * (2.0 - t),
            Self::EaseInOut => t * t * (3.0 - 2.0 * t),
            Self::Back { overshoot } => {
                let u = t - 1.0;
                1.0 + (overshoot + 1.0) * u * u * u + overshoot * u * u
            }
        }
    }
}

/// How objects move between cells.
///
/// The resource is the style for every object: insert one before adding
/// `ObjectAnimationPlugin`, or change it at runtime. An [`ObjectMotion`]
/// component overrides it for one entity.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub enum MotionStyle {
    /// Jump straight to the new cell.
    Snap,
    /// Interpolation from the displayed position, `duration` per cell.
    Tween { duration: f32, easing: Easing },
}

/// Per-entity motion style, overriding the `MotionStyle` resource (for a
/// sliding wall that should not move like the player, say).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ObjectMotion(pub MotionStyle);

impl Default for MotionStyle {
    fn default() -> Self {
        Self::PRESETS[0].1
    }
}

impl MotionStyle {
    /// Named styles, in the order the runtime toggle cycles through them.
    pub const PRESETS: [(&'static str, Self); 5] = [
        (
            "ease-out",
            Self::Tween {
                duration: 0.1,
                easing: Easing::EaseOut,
            },
        ),
        (
            "linear",
            Self::Tween {
                duration: 0.1,
                easing: Easing::Linear,
            },
        ),
        (
            "ease-in-out",
            Self::Tween {
                duration: 0.12,
                easing: Easing::EaseInOut,
            },
        ),
        (
            "overshoot",
            Self::Tween {
                duration: 0.18,
                easing: Easing::Back { overshoot: 1.70158 },
            },
        ),
        ("snap", Self::Snap),
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Self::PRESETS
            .iter()
            .find(|(preset, _)| *preset == name)
            .map(|(_, motion)| *motion)
    }

    /// Preset name, or "custom" for hand-tuned values.
    pub fn name(&self) -> &'static str {
        Self::PRESETS
            .iter()
            .find(|(_, motion)| motion == self)
            .map_or("custom", |(name, _)| name)
    }

    /// The next preset after this one (the first one for custom values).
    pub fn next(&self) -> Self {
        let index = Self::PRESETS
            .iter()
            .position(|(_, motion)| motion == self)
            .map_or(0, |index| (index + 1) % Self::PRESETS.len());
        Self::PRESETS[index].1
    }
}

/// Displayed position of one object, in fractional grid cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionState {
    pub position: Vec2,
    from: Vec2,
    to: Vec2,
    elapsed: f32,
    /// Length of the current move in cells (at least 1).
    cells: f32,
}

impl MotionState {
    pub fn at(cell: Vec2) -> Self {
        Self {
            position: cell,
            from: cell,
            to: cell,
            // At rest: the last step has long finished.
            elapsed: f32::INFINITY,
            cells: 1.0,
        }
    }

    pub fn target(&self) -> Vec2 {
        self.to
    }

    /// Whether nothing is moving: resting objects cost almost nothing.
    pub fn at_rest(&self) -> bool {
        self.position == self.to
    }

    /// Starts moving toward `cell`, any number of cells away, from wherever
    /// the object is shown now. Longer moves take proportionally longer.
    pub fn retarget(&mut self, cell: Vec2, _motion: MotionStyle) {
        self.cells = (cell - self.to).length().max(1.0);
        self.from = self.position;
        self.to = cell;
        self.elapsed = 0.0;
    }

    pub fn advance(&mut self, motion: MotionStyle, dt: f32) {
        if self.at_rest() {
            return;
        }
        match motion {
            MotionStyle::Snap => self.settle(),
            MotionStyle::Tween { duration, easing } => {
                self.elapsed += dt;
                let duration = duration * self.cells;
                let t = if duration > 0.0 {
                    self.elapsed / duration
                } else {
                    1.0
                };
                if t >= 1.0 {
                    self.settle();
                } else {
                    self.position = self.from.lerp(self.to, easing.apply(t));
                }
            }
        }
    }

    /// Rest exactly on the target.
    fn settle(&mut self) {
        self.position = self.to;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    fn ease_out() -> MotionStyle {
        MotionStyle::default()
    }

    /// Steps along `path` every `every` frames, then runs `rest` more frames.
    /// Returns the x position after each frame.
    fn walk(motion: MotionStyle, path: &[Vec2], every: usize, rest: usize) -> Vec<f32> {
        let mut state = MotionState::at(Vec2::ZERO);
        let mut xs = Vec::new();
        for cell in path {
            state.retarget(*cell, motion);
            for _ in 0..every {
                state.advance(motion, FRAME);
                xs.push(state.position.x);
            }
        }
        for _ in 0..rest {
            state.advance(motion, FRAME);
            xs.push(state.position.x);
        }
        xs
    }

    fn peak(xs: &[f32]) -> f32 {
        xs.iter().copied().fold(f32::MIN, f32::max)
    }

    #[test]
    fn every_preset_arrives_exactly() {
        for (name, motion) in MotionStyle::PRESETS {
            let path = walk(motion, &[Vec2::X], 1, 60);
            assert_eq!(*path.last().unwrap(), 1.0, "{name} did not settle");
        }
    }

    #[test]
    fn only_the_overshoot_style_passes_the_cell() {
        let peak_of = |name| {
            peak(&walk(
                MotionStyle::from_name(name).unwrap(),
                &[Vec2::X],
                1,
                60,
            ))
        };
        for name in ["ease-out", "linear", "ease-in-out", "snap"] {
            assert!(peak_of(name) <= 1.0, "{name} overshot");
        }
        assert!(peak_of("overshoot") > 1.05);
    }

    #[test]
    fn a_new_step_continues_from_the_shown_position() {
        let mut state = MotionState::at(Vec2::ZERO);
        state.retarget(Vec2::X, ease_out());
        state.advance(ease_out(), FRAME);
        let before = state.position;
        state.retarget(Vec2::new(2.0, 0.0), ease_out());
        assert_eq!(state.position, before, "retarget jumped");
        state.advance(ease_out(), FRAME);
        assert!(state.position.x > before.x && state.position.x < 2.0);
    }

    #[test]
    fn multi_cell_moves_glide_at_the_same_speed() {
        let linear = MotionStyle::from_name("linear").unwrap();
        let mut state = MotionState::at(Vec2::ZERO);
        state.retarget(Vec2::new(3.0, 0.0), linear);
        state.advance(linear, 0.15);
        assert!(
            (state.position.x - 1.5).abs() < 1e-5,
            "halfway: {}",
            state.position.x
        );
        state.advance(linear, 0.15);
        assert_eq!(state.position, Vec2::new(3.0, 0.0));
    }

    #[test]
    fn resting_objects_stay_put() {
        let mut state = MotionState::at(Vec2::new(4.0, 2.0));
        assert!(state.at_rest());
        for (_, motion) in MotionStyle::PRESETS {
            state.advance(motion, FRAME);
            assert_eq!(state.position, Vec2::new(4.0, 2.0));
            assert!(state.at_rest());
        }
    }

    #[test]
    fn presets_cycle_by_name() {
        assert_eq!(MotionStyle::default().name(), "ease-out");
        assert_eq!(MotionStyle::default().next().name(), "linear");
        assert_eq!(
            MotionStyle::from_name("snap").unwrap().next().name(),
            "ease-out"
        );
        assert_eq!(
            MotionStyle::Tween {
                duration: 0.5,
                easing: Easing::Linear
            }
            .name(),
            "custom"
        );
    }
}
