//! How animated objects travel between grid cells.
//!
//! A move can span any number of cells (a sliding wall, a push, a dash):
//! it takes proportionally longer, at the same speed as a one-cell step.
//! Each move tweens from wherever the object is shown, so a move that
//! arrives mid-glide continues without a jump.
//!
//! [`MotionState::remaining`] tells how long the current step still runs;
//! the player's value paces the turns (see `TurnPacing`).

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

/// The shape of a move between its start and end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Path {
    /// A straight line.
    Straight,
    /// Out toward `toward` (an offset in cells) and back: a move that ends
    /// where it started, such as bumping into a wall.
    Excursion { toward: Vec2 },
}

impl Path {
    /// Point at progress `s` (0 to 1) of a move from `from` to `to`.
    pub fn sample(self, from: Vec2, to: Vec2, s: f32) -> Vec2 {
        let base = from.lerp(to, s);
        match self {
            Self::Straight => base,
            Self::Excursion { toward } => base + toward * (s * std::f32::consts::PI).sin(),
        }
    }
}

/// One animated move: where it starts and ends, the path between, and how
/// progress runs over time. It is fixed when the move starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub from: Vec2,
    pub to: Vec2,
    pub path: Path,
    pub duration: f32,
    pub easing: Easing,
}

/// How far toward what blocked it a bumping object pushes, in cells.
const BUMP_CELLS: f32 = 0.2;

/// Plays the current [`Move`] of one object, in fractional grid cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionState {
    /// Where the object is shown.
    pub position: Vec2,
    current: Move,
    elapsed: f32,
}

impl MotionState {
    pub fn at(cell: Vec2) -> Self {
        Self {
            position: cell,
            current: Move {
                from: cell,
                to: cell,
                path: Path::Straight,
                duration: 0.0,
                easing: Easing::Linear,
            },
            elapsed: 0.0,
        }
    }

    /// The cell the current move ends in.
    pub fn target(&self) -> Vec2 {
        self.current.to
    }

    /// Whether nothing is moving: resting objects cost almost nothing.
    pub fn at_rest(&self) -> bool {
        self.elapsed >= self.current.duration
    }

    /// Plays `next` from now on. Moves start where the object is shown, so
    /// a move that interrupts another continues without a jump.
    pub fn play(&mut self, next: Move) {
        self.current = Move {
            from: self.position,
            ..next
        };
        self.elapsed = 0.0;
        self.advance(0.0);
    }

    /// Moves to `cell`, any number of cells away. Longer moves take
    /// proportionally longer, at the same speed.
    pub fn move_to(&mut self, cell: Vec2, style: MotionStyle) {
        let cells = (cell - self.target()).length().max(1.0);
        self.play(self.planned(cell, Path::Straight, style, cells));
    }

    /// Pushes toward `direction` and back to the current cell, for an action
    /// that did not move the object (such as walking into a wall).
    pub fn bump(&mut self, direction: Vec2, style: MotionStyle) {
        let toward = direction.normalize_or_zero() * BUMP_CELLS;
        self.play(self.planned(self.target(), Path::Excursion { toward }, style, 1.0));
    }

    fn planned(&self, to: Vec2, path: Path, style: MotionStyle, cells: f32) -> Move {
        let (duration, easing) = match style {
            MotionStyle::Snap => (0.0, Easing::Linear),
            MotionStyle::Tween { duration, easing } => (duration * cells, easing),
        };
        Move {
            from: self.position,
            to,
            path,
            duration,
            easing,
        }
    }

    pub fn advance(&mut self, dt: f32) {
        let Move {
            from,
            to,
            path,
            duration,
            easing,
        } = self.current;
        if self.at_rest() {
            self.position = to;
            return;
        }
        self.elapsed += dt;
        self.position = if self.elapsed >= duration {
            to
        } else {
            path.sample(from, to, easing.apply(self.elapsed / duration))
        };
    }

    /// Seconds until the current move ends.
    pub fn remaining(&self) -> f32 {
        (self.current.duration - self.elapsed).max(0.0)
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
            state.move_to(*cell, motion);
            for _ in 0..every {
                state.advance(FRAME);
                xs.push(state.position.x);
            }
        }
        for _ in 0..rest {
            state.advance(FRAME);
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
        state.move_to(Vec2::X, ease_out());
        state.advance(FRAME);
        let before = state.position;
        state.move_to(Vec2::new(2.0, 0.0), ease_out());
        assert_eq!(state.position, before, "retarget jumped");
        state.advance(FRAME);
        assert!(state.position.x > before.x && state.position.x < 2.0);
    }

    #[test]
    fn multi_cell_moves_glide_at_the_same_speed() {
        let linear = MotionStyle::from_name("linear").unwrap();
        let mut state = MotionState::at(Vec2::ZERO);
        state.move_to(Vec2::new(3.0, 0.0), linear);
        assert!(
            (state.remaining() - 0.3).abs() < 1e-6,
            "3 cells, 0.1 s each"
        );
        state.advance(0.15);
        assert!(
            (state.position.x - 1.5).abs() < 1e-5,
            "halfway: {}",
            state.position.x
        );
        state.advance(0.15);
        assert_eq!(state.position, Vec2::new(3.0, 0.0));
    }

    #[test]
    fn resting_objects_stay_put() {
        let mut state = MotionState::at(Vec2::new(4.0, 2.0));
        assert!(state.at_rest());
        for _ in 0..10 {
            state.advance(FRAME);
            assert_eq!(state.position, Vec2::new(4.0, 2.0));
            assert!(state.at_rest());
        }
    }

    #[test]
    fn remaining_time_counts_down_each_step() {
        let linear = MotionStyle::from_name("linear").unwrap();
        let mut state = MotionState::at(Vec2::ZERO);
        assert_eq!(state.remaining(), 0.0);
        state.move_to(Vec2::X, linear);
        state.advance(0.04);
        assert!((state.remaining() - 0.06).abs() < 1e-6);
        state.advance(0.1);
        assert_eq!(state.remaining(), 0.0);
        assert_eq!(state.remaining(), 0.0);
    }

    #[test]
    fn bumps_go_out_and_back_to_the_same_cell() {
        let mut state = MotionState::at(Vec2::ZERO);
        state.bump(Vec2::new(3.0, 0.0), ease_out());
        assert_eq!(state.target(), Vec2::ZERO);
        assert!((state.remaining() - 0.1).abs() < 1e-6, "one step long");
        let mut peak = 0.0_f32;
        for _ in 0..7 {
            state.advance(FRAME);
            peak = peak.max(state.position.x);
        }
        assert!(peak > 0.1 && peak <= BUMP_CELLS, "bump peak {peak}");
        assert_eq!(state.position, Vec2::ZERO);
        assert!(state.at_rest());
    }

    #[test]
    fn a_bump_mid_move_continues_from_the_shown_position() {
        let mut state = MotionState::at(Vec2::ZERO);
        state.move_to(Vec2::X, ease_out());
        state.advance(FRAME);
        let before = state.position;
        state.bump(Vec2::Y, ease_out());
        assert_eq!(state.position, before, "bump jumped");
        for _ in 0..7 {
            state.advance(FRAME);
        }
        assert_eq!(state.position, Vec2::X, "still ends in its cell");
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
