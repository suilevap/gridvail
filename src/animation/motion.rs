//! How animated objects travel between grid cells.
//!
//! Every animation is a [`Move`]: where it starts and ends, the [`Path`]
//! between them, how progress runs over time, and [`Secondary`] motion
//! layered on top (a hop per cell). A move may end in the cell it started
//! from (a bump) or many cells away (a sliding wall, a push, a dash): it
//! takes proportionally longer, at the same speed as a one-cell step.
//! [`MotionState`] plays one move at a time and starts each new move from
//! where the object is shown, so interruptions never jump.
//!
//! Locomotion shapes each move from the object's recent history, so
//! turn-based steps read as walking: a step from rest starts slowly, a
//! straight run flows at constant speed, and stopping after a run overshoots
//! the last cell before settling back.
//!
//! [`MotionState::remaining`] tells how long the current move still runs;
//! it paces the turns (see `TurnPacing`).

use bevy::prelude::*;

/// Upper bound on one spring integration step, for stability at low FPS.
const MAX_SPRING_STEP: f32 = 1.0 / 240.0;
/// A coasting object closer than this to rest lands exactly on its cell.
const REST_DISTANCE: f32 = 0.001;
const REST_SPEED: f32 = 0.01;
/// A step arriving within this multiple of the last move's time continues a
/// run.
const RUN_GAP: f32 = 1.6;
/// How far toward what blocked it a bumping object pushes, in cells.
const BUMP_CELLS: f32 = 0.2;

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
    /// Momentum-driven steps that read as walking and running.
    ///
    /// Each move lasts `step` seconds per cell and ends at `momentum` times
    /// the running speed (one cell per `step`). Momentum starts at `start`
    /// after a pause, gains `ramp` per straight step up to 1, and keeps
    /// `turn_keep` of itself on a turn. When the moves stop, the leftover
    /// speed coasts on a spring with `stop_frequency` (Hz); its damping
    /// ratio eases from 1 at no momentum to `stop_damping` at full momentum,
    /// so only a run overshoots. Every cell crossed also hops up to `bob`
    /// cells above the ground.
    Locomotion {
        step: f32,
        start: f32,
        ramp: f32,
        turn_keep: f32,
        stop_frequency: f32,
        stop_damping: f32,
        bob: f32,
    },
}

/// Per-entity motion style, overriding the `MotionStyle` resource (for a
/// sliding wall that should not walk like the player, say).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ObjectMotion(pub MotionStyle);

/// Per-entity path for moves to a new cell, replacing the style's default
/// (for a box that is thrown in an arc, say).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct MovePath(pub Path);

impl Default for MotionStyle {
    fn default() -> Self {
        Self::PRESETS[0].1
    }
}

impl MotionStyle {
    /// Named styles, in the order the runtime toggle cycles through them.
    pub const PRESETS: [(&'static str, Self); 6] = [
        (
            "locomotion",
            Self::Locomotion {
                step: 0.12,
                start: 0.15,
                ramp: 0.35,
                turn_keep: 0.4,
                stop_frequency: 3.5,
                stop_damping: 0.45,
                bob: 0.15,
            },
        ),
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

    /// Duration and easing of a one-cell move in this style.
    fn timing(&self) -> (f32, Easing) {
        match *self {
            Self::Snap => (0.0, Easing::Linear),
            Self::Tween { duration, easing } => (duration, easing),
            Self::Locomotion { step, .. } => (step, Easing::Linear),
        }
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
    /// Curved to the left of the direction of travel by up to `bulge` cells
    /// (negative curves right).
    Arc { bulge: f32 },
    /// Cubic Hermite curve leaving with tangent `m0` and arriving with
    /// tangent `m1` (cells per whole move), so consecutive moves join
    /// without a visible stop.
    Hermite { m0: Vec2, m1: Vec2 },
}

impl Path {
    /// Point at progress `s` (0 to 1) of a move from `from` to `to`.
    pub fn sample(self, from: Vec2, to: Vec2, s: f32) -> Vec2 {
        let base = from.lerp(to, s);
        let arch = (s * std::f32::consts::PI).sin();
        match self {
            Self::Straight => base,
            Self::Excursion { toward } => base + toward * arch,
            Self::Arc { bulge } => base + (to - from).normalize_or_zero().perp() * bulge * arch,
            Self::Hermite { m0, m1 } => {
                let (s2, s3) = (s * s, s * s * s);
                (2.0 * s3 - 3.0 * s2 + 1.0) * from
                    + (s3 - 2.0 * s2 + s) * m0
                    + (-2.0 * s3 + 3.0 * s2) * to
                    + (s3 - s2) * m1
            }
        }
    }

    /// Tangent at the end of the move, in cells per whole move.
    fn end_tangent(self) -> Vec2 {
        match self {
            Self::Hermite { m1, .. } => m1,
            _ => Vec2::ZERO,
        }
    }
}

/// Motion layered on top of a move's path.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Secondary {
    /// Height of each hop above the ground, in cells.
    pub hop: f32,
    /// Hops over the whole move (one per cell crossed).
    pub hops: f32,
}

impl Secondary {
    /// Height above the ground at progress `s`.
    fn lift(self, s: f32) -> f32 {
        self.hop * (s * self.hops * std::f32::consts::PI).sin().abs()
    }
}

/// Spring that carries a move's leftover speed past its end and back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coast {
    /// In Hz.
    pub frequency: f32,
    /// Damping ratio: 1.0 arrives without overshoot, lower values bounce.
    pub damping: f32,
}

/// One animated move, fixed when it starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub from: Vec2,
    pub to: Vec2,
    pub path: Path,
    pub duration: f32,
    pub easing: Easing,
    pub secondary: Secondary,
    /// Whether leftover speed coasts on after the move (locomotion).
    pub coast: Option<Coast>,
}

/// What locomotion remembers between moves.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Gait {
    since_step: f32,
    direction: Vec2,
    momentum: f32,
    last_duration: f32,
}

/// Plays the current [`Move`] of one object, in fractional grid cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionState {
    /// Where the object is shown.
    pub position: Vec2,
    /// Height above the ground in cells.
    pub lift: f32,
    velocity: Vec2,
    current: Move,
    elapsed: f32,
    /// Lift when the current move began, faded out so hops never jump.
    start_lift: f32,
    gait: Gait,
}

impl MotionState {
    pub fn at(cell: Vec2) -> Self {
        Self {
            position: cell,
            lift: 0.0,
            velocity: Vec2::ZERO,
            current: Move {
                from: cell,
                to: cell,
                path: Path::Straight,
                duration: 0.0,
                easing: Easing::Linear,
                secondary: Secondary::default(),
                coast: None,
            },
            elapsed: 0.0,
            start_lift: 0.0,
            gait: Gait {
                since_step: f32::INFINITY,
                direction: Vec2::ZERO,
                momentum: 0.0,
                last_duration: 0.0,
            },
        }
    }

    /// The cell the current move ends in.
    pub fn target(&self) -> Vec2 {
        self.current.to
    }

    /// Whether nothing is moving: resting objects cost almost nothing.
    pub fn at_rest(&self) -> bool {
        self.elapsed >= self.current.duration
            && self.position == self.current.to
            && self.velocity == Vec2::ZERO
            && self.lift == 0.0
    }

    /// Plays `next` from now on. Moves start where the object is shown, so
    /// a move that interrupts another continues without a jump.
    pub fn play(&mut self, next: Move) {
        self.current = Move {
            from: self.position,
            ..next
        };
        self.elapsed = 0.0;
        self.start_lift = self.lift;
    }

    /// Moves to `cell`, any number of cells away, along the style's default
    /// path. Longer moves take proportionally longer, at the same speed.
    pub fn move_to(&mut self, cell: Vec2, style: MotionStyle) {
        self.move_along(cell, None, style);
    }

    /// Moves to `cell` along `path`, or the style's default path if `None`.
    pub fn move_along(&mut self, cell: Vec2, path: Option<Path>, style: MotionStyle) {
        let cells = (cell - self.target()).length().max(1.0);
        let (step, easing) = style.timing();
        let duration = step * cells;
        let mut next = Move {
            from: self.position,
            to: cell,
            path: path.unwrap_or(Path::Straight),
            duration,
            easing,
            secondary: Secondary::default(),
            coast: None,
        };
        if let MotionStyle::Locomotion {
            step,
            start,
            ramp,
            turn_keep,
            stop_frequency,
            stop_damping,
            bob,
        } = style
        {
            let direction = (cell - self.target()).normalize_or_zero();
            let momentum = self.stride(direction, step, start, ramp, turn_keep, duration);
            // Running speed is one cell per `step`, whatever the distance.
            let end_velocity = direction * momentum / step.max(0.01);
            next.path = path.unwrap_or(Path::Hermite {
                m0: self.velocity * duration,
                m1: end_velocity * duration,
            });
            next.secondary = Secondary {
                hop: bob,
                hops: cells.round().max(1.0),
            };
            next.coast = Some(Coast {
                frequency: stop_frequency,
                damping: 1.0.lerp(stop_damping, momentum),
            });
        }
        self.play(next);
    }

    /// Pushes toward `direction` and back to the current cell, for an action
    /// that did not move the object (such as walking into a wall).
    pub fn bump(&mut self, direction: Vec2, style: MotionStyle) {
        let (duration, easing) = style.timing();
        self.velocity = Vec2::ZERO;
        self.play(Move {
            from: self.position,
            to: self.target(),
            path: Path::Excursion {
                toward: direction.normalize_or_zero() * BUMP_CELLS,
            },
            duration,
            easing,
            secondary: Secondary::default(),
            coast: None,
        });
    }

    /// Updates locomotion momentum for a new move and returns it.
    fn stride(
        &mut self,
        direction: Vec2,
        step: f32,
        start: f32,
        ramp: f32,
        turn_keep: f32,
        duration: f32,
    ) -> f32 {
        let gait = &mut self.gait;
        // Measured against the previous move, however long it was.
        let running = gait.since_step <= gait.last_duration.max(step) * RUN_GAP;
        gait.momentum = if !running {
            start
        } else if direction == gait.direction {
            (gait.momentum + ramp).min(1.0)
        } else {
            (gait.momentum * turn_keep).max(start)
        };
        gait.direction = direction;
        gait.since_step = 0.0;
        gait.last_duration = duration;
        gait.momentum
    }

    pub fn advance(&mut self, dt: f32) {
        self.gait.since_step += dt;
        if self.at_rest() {
            return;
        }
        let Move {
            from,
            to,
            path,
            duration,
            easing,
            secondary,
            coast,
        } = self.current;
        if self.elapsed < duration {
            self.elapsed += dt;
            let s = (self.elapsed / duration).min(1.0);
            let next = path.sample(from, to, easing.apply(s));
            if dt > 0.0 {
                self.velocity = (next - self.position) / dt;
            }
            self.position = next;
            self.lift = self.start_lift * (1.0 - s) + secondary.lift(s);
            if self.elapsed >= duration {
                // The move is over; only a coast carries its speed on.
                self.position = to;
                self.lift = 0.0;
                self.velocity = match coast {
                    Some(_) => path.end_tangent() / duration,
                    None => Vec2::ZERO,
                };
                if let Some(coast) = coast {
                    self.coast(coast, self.elapsed - duration);
                }
            }
        } else {
            self.lift = 0.0;
            match coast {
                Some(coast) => self.coast(coast, dt),
                None => {
                    self.position = to;
                    self.velocity = Vec2::ZERO;
                }
            }
        }
    }

    /// Seconds until the current move ends. Coasting after it does not
    /// count.
    pub fn remaining(&self) -> f32 {
        (self.current.duration - self.elapsed).max(0.0)
    }

    fn coast(&mut self, coast: Coast, dt: f32) {
        let omega = std::f32::consts::TAU * coast.frequency.max(0.01);
        let to = self.current.to;
        let mut remaining = dt;
        while remaining > 0.0 {
            let step = remaining.min(MAX_SPRING_STEP);
            let acceleration =
                omega * omega * (to - self.position) - 2.0 * coast.damping * omega * self.velocity;
            self.velocity += acceleration * step;
            self.position += self.velocity * step;
            remaining -= step;
        }
        if self.position.distance(to) < REST_DISTANCE && self.velocity.length() < REST_SPEED {
            self.position = to;
            self.velocity = Vec2::ZERO;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;
    /// The locomotion preset's step time.
    const STEP: f32 = 0.12;

    fn locomotion() -> MotionStyle {
        MotionStyle::from_name("locomotion").unwrap()
    }

    fn ease_out() -> MotionStyle {
        MotionStyle::from_name("ease-out").unwrap()
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

    fn straight(steps: i32) -> Vec<Vec2> {
        (1..=steps).map(|x| Vec2::new(x as f32, 0.0)).collect()
    }

    fn peak(xs: &[f32]) -> f32 {
        xs.iter().copied().fold(f32::MIN, f32::max)
    }

    #[test]
    fn every_preset_arrives_exactly() {
        for (name, motion) in MotionStyle::PRESETS {
            let path = walk(motion, &[Vec2::X], 1, 180);
            assert_eq!(*path.last().unwrap(), 1.0, "{name} did not settle");
        }
    }

    #[test]
    fn only_overshooting_styles_pass_a_single_step() {
        let peak_of = |name| {
            peak(&walk(
                MotionStyle::from_name(name).unwrap(),
                &[Vec2::X],
                1,
                180,
            ))
        };
        for name in ["ease-out", "linear", "ease-in-out", "snap"] {
            assert!(peak_of(name) <= 1.0, "{name} overshot");
        }
        assert!(peak_of("overshoot") > 1.05);
    }

    #[test]
    fn a_new_step_continues_from_the_shown_position() {
        for style in [ease_out(), locomotion()] {
            let mut state = MotionState::at(Vec2::ZERO);
            state.move_to(Vec2::X, style);
            state.advance(FRAME);
            let before = state.position;
            state.move_to(Vec2::new(2.0, 0.0), style);
            state.advance(0.0);
            assert_eq!(state.position, before, "new move jumped");
            state.advance(FRAME);
            assert!(state.position.x > before.x && state.position.x < 2.0);
        }
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

        // Locomotion keeps its running speed and hops once per cell.
        let mut state = MotionState::at(Vec2::ZERO);
        state.move_to(Vec2::new(0.0, 3.0), locomotion());
        assert!((state.remaining() - 3.0 * STEP).abs() < 1e-6);
        let mut hops = 0;
        let mut rising = false;
        let mut last = 0.0;
        for _ in 0..30 {
            state.advance(FRAME);
            if rising && state.lift < last {
                rising = false;
            } else if !rising && state.lift > last {
                rising = true;
                hops += 1;
            }
            last = state.lift;
        }
        assert_eq!(hops, 3, "one hop per cell");
        assert!(state.position.y > 2.9);
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
    fn remaining_time_counts_down_each_move() {
        let linear = MotionStyle::from_name("linear").unwrap();
        let mut state = MotionState::at(Vec2::ZERO);
        assert_eq!(state.remaining(), 0.0);
        state.move_to(Vec2::X, linear);
        state.advance(0.04);
        assert!((state.remaining() - 0.06).abs() < 1e-6);
        state.advance(0.1);
        assert_eq!(state.remaining(), 0.0);

        // Coasting after a locomotion move does not hold anything back.
        let mut state = MotionState::at(Vec2::ZERO);
        state.move_to(Vec2::X, locomotion());
        assert_eq!(state.remaining(), STEP);
        for _ in 0..8 {
            state.advance(FRAME);
        }
        assert_eq!(state.remaining(), 0.0);
    }

    #[test]
    fn bumps_go_out_and_back_to_the_same_cell() {
        for style in [ease_out(), locomotion()] {
            let mut state = MotionState::at(Vec2::ZERO);
            state.bump(Vec2::new(3.0, 0.0), style);
            assert_eq!(state.target(), Vec2::ZERO);
            let mut peak = 0.0_f32;
            for _ in 0..8 {
                state.advance(FRAME);
                peak = peak.max(state.position.x);
            }
            assert!(peak > 0.1 && peak <= BUMP_CELLS, "bump peak {peak}");
            assert_eq!(state.position, Vec2::ZERO);
            assert!(state.at_rest());
        }
    }

    #[test]
    fn a_bump_mid_move_continues_from_the_shown_position() {
        let mut state = MotionState::at(Vec2::ZERO);
        state.move_to(Vec2::X, ease_out());
        state.advance(FRAME);
        let before = state.position;
        state.bump(Vec2::Y, ease_out());
        state.advance(0.0);
        assert_eq!(state.position, before, "bump jumped");
        for _ in 0..7 {
            state.advance(FRAME);
        }
        assert_eq!(state.position, Vec2::X, "still ends in its cell");
    }

    #[test]
    fn arcs_curve_to_the_side_and_land_on_the_cell() {
        let linear = MotionStyle::from_name("linear").unwrap();
        let mut state = MotionState::at(Vec2::ZERO);
        state.move_along(Vec2::new(2.0, 0.0), Some(Path::Arc { bulge: 0.5 }), linear);
        state.advance(0.1);
        assert!(
            (state.position - Vec2::new(1.0, 0.5)).length() < 1e-5,
            "{}",
            state.position
        );
        state.advance(0.1);
        assert_eq!(state.position, Vec2::new(2.0, 0.0));
    }

    #[test]
    fn locomotion_starts_from_rest_with_friction() {
        // Two frames into the first step a linear glide covers a third of a
        // cell; the start of a walk lags well behind that.
        let xs = walk(locomotion(), &[Vec2::X], 7, 0);
        let linear = 2.0 * FRAME / STEP;
        assert!(
            xs[1] < 0.8 * linear,
            "first step started too fast: {}",
            xs[1]
        );
    }

    #[test]
    fn locomotion_overshoots_only_after_a_run() {
        let single = peak(&walk(locomotion(), &[Vec2::X], 7, 180)) - 1.0;
        let run = walk(locomotion(), &straight(8), 7, 180);
        let overshoot = peak(&run) - 8.0;
        assert!(overshoot > 0.1, "run did not overshoot: {overshoot}");
        assert!(single < overshoot / 3.0, "single step overshot {single}");
        assert_eq!(*run.last().unwrap(), 8.0);
    }

    #[test]
    fn locomotion_runs_at_the_turn_cadence() {
        // Turns follow the 0.12 s step, starting the next about a frame
        // before it ends: seven 1/60 s frames per turn. Once up to speed,
        // every frame should advance about 1/7 of a cell.
        let xs = walk(locomotion(), &straight(10), 7, 0);
        let steady: Vec<f32> = xs[42..].windows(2).map(|w| w[1] - w[0]).collect();
        for delta in steady {
            assert!(
                (delta - 1.0 / 7.0).abs() < 0.04,
                "uneven running step {delta}"
            );
        }
    }

    #[test]
    fn locomotion_turns_without_stopping() {
        let mut state = MotionState::at(Vec2::ZERO);
        for cell in straight(4) {
            state.move_to(cell, locomotion());
            for _ in 0..7 {
                state.advance(FRAME);
            }
        }
        state.move_to(Vec2::new(4.0, 1.0), locomotion());
        state.advance(FRAME);
        assert!(
            state.velocity.x > 0.0 && state.velocity.y > 0.0,
            "corner was not rounded"
        );
    }

    #[test]
    fn locomotion_restarts_after_a_pause() {
        let mut state = MotionState::at(Vec2::ZERO);
        for cell in straight(4) {
            state.move_to(cell, locomotion());
            for _ in 0..7 {
                state.advance(FRAME);
            }
        }
        for _ in 0..60 {
            state.advance(FRAME);
        }
        state.move_to(Vec2::new(5.0, 0.0), locomotion());
        assert_eq!(state.gait.momentum, 0.15);
    }

    #[test]
    fn locomotion_hops_once_per_step() {
        let mut state = MotionState::at(Vec2::ZERO);
        let mut lifts = Vec::new();
        for cell in straight(3) {
            state.move_to(cell, locomotion());
            for _ in 0..8 {
                state.advance(FRAME);
                lifts.push(state.lift);
            }
        }
        // Each 8-frame turn covers one 0.12 s step: it rises and lands.
        for step in lifts.chunks(8) {
            assert!(step[3] > 0.12, "no hop mid-step: {step:?}");
            assert!(step[7] < 0.01, "step did not land: {step:?}");
        }
        for _ in 0..60 {
            state.advance(FRAME);
        }
        assert_eq!(state.lift, 0.0);
    }

    #[test]
    fn interrupted_hop_does_not_jump() {
        let mut state = MotionState::at(Vec2::ZERO);
        state.move_to(Vec2::X, locomotion());
        for _ in 0..3 {
            state.advance(FRAME);
        }
        let before = state.lift;
        state.move_to(Vec2::new(2.0, 0.0), locomotion());
        state.advance(FRAME);
        assert!(
            (state.lift - before).abs() < 0.06,
            "{before} -> {}",
            state.lift
        );
    }

    #[test]
    fn presets_cycle_by_name() {
        assert_eq!(MotionStyle::default().name(), "locomotion");
        assert_eq!(MotionStyle::default().next().name(), "ease-out");
        assert_eq!(
            MotionStyle::from_name("snap").unwrap().next().name(),
            "locomotion"
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
