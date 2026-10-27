//! How a map comes together from what was there before, instead of
//! appearing at once.
//!
//! It reacts to [`MapLoaded`] and knows nothing of how maps are loaded or
//! where the walls it starts from come from. What a map arrives from is an
//! [`Arrival`]: walls to show first, put in [`NextArrival`] by whoever wants
//! them (the game's intro), or else the plain walls of the map before. The
//! first map, with nothing before it and nothing asked for, appears at once.
//!
//! It is an ordinary room switch: the map's plain walls are replaced by the
//! arriving ones and a room covering the map switches back to the map's,
//! played as a [`Cutscene`] (player input and new turns wait) in up to three
//! phases, each its own component on that room and its own system:
//!
//! 1. [`Showing`]: shown walls rise out of the floor and stand for
//!    [`SHOW_SECONDS`]. Only their area is in sight (a [`RevealArea`] that
//!    hides the player's own sight), and the camera looks at it from as far
//!    out as it zooms.
//! 2. [`Focusing`]: over [`FOCUS_SECONDS`] the area narrows into a small
//!    circle around the player, while the camera glides back to them.
//! 3. [`Forming`]: the room switches, so its walls slide, rise and sink like
//!    any room's, with whatever room planner is the default. The player's
//!    own sight is back, blocked by the new walls; what they saw before
//!    through where walls now stand is forgotten, and the circle around them
//!    closes to nothing over [`CLOSE_SECONDS`]. The walls of the map before
//!    start here. Once every wall has settled and the circle has closed, the
//!    room goes and play goes on.

mod begin;
mod phases;

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::GamePhase;

/// How long shown walls stand before the map comes together, rising
/// included.
pub const SHOW_SECONDS: f32 = 1.5;
/// How long their area takes to narrow onto the player, while the camera
/// glides back to them...
pub const FOCUS_SECONDS: f32 = 0.8;
/// ...into a circle this many cells across around them (where a map that
/// comes from the one before starts)...
pub const FOCUS_RADIUS: f32 = 6.0;
/// ...which, once the map's walls stand and the player's own sight is back,
/// closes to nothing over this long.
pub const CLOSE_SECONDS: f32 = 0.8;

/// What a map comes together from.
#[derive(Clone, Debug, PartialEq)]
pub enum Arrival {
    /// Walls shown first, in `area` (which hides the player's own sight),
    /// before the map comes together from them: rising out of the floor,
    /// then standing for a moment.
    Shown { walls: Vec<IVec2>, area: RevealArea },
    /// Walls standing as they were drawn: the map before's.
    Standing(Vec<(IVec2, Glyph)>),
}

/// What the next map loaded comes together from, instead of the walls of
/// the map before. Taken by the next [`MapLoaded`].
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct NextArrival(pub Arrival);

/// The room a transition switches, and the camera's zoom before it, to go
/// back to.
#[derive(Component, Clone, Copy, Debug)]
pub struct TransitionRoom {
    zoom: f32,
}

/// Phase 1: shown walls stand until the timer ends.
#[derive(Component, Clone, Debug)]
pub struct Showing(pub Timer);

/// Phase 2: the area in sight narrows from `from` onto the player.
#[derive(Component, Clone, Debug)]
pub struct Focusing {
    pub from: RevealArea,
    pub timer: Timer,
}

/// Phase 3: the room switches to the map's walls and the circle around the
/// player closes from `from`. `forgotten` is whether what they saw before
/// the walls stood has been forgotten yet.
#[derive(Component, Clone, Debug)]
pub struct Forming {
    pub from: RevealArea,
    pub timer: Timer,
    pub forgotten: bool,
}

/// Brings every map loaded together from what was there before.
pub struct TransitionPlugin;

impl Plugin for TransitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MapLoaded>()
            .add_systems(
                Update,
                begin::begin_transition
                    .run_if(on_message::<MapLoaded>)
                    .before(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                (phases::show, phases::focus, phases::form)
                    .chain()
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(Update, phases::forget_unseen.in_set(GamePhase::Animation))
            .add_systems(Update, phases::end_transition.in_set(GamePhase::Finalize));
    }
}

fn timer(seconds: f32) -> Timer {
    Timer::from_seconds(seconds, TimerMode::Once)
}
