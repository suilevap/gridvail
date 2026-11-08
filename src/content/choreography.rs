//! How fights are timed on screen, `assets/rules/choreography.ron`: plain
//! data, read with the generic `data::read`.

use serde::Deserialize;

use crate::model::EffectId;

/// Seconds and cells for one exchange of blows (see `choreography`).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ChoreographySpec {
    /// A whole swing, from the wind-up to settling back.
    pub swing_secs: f32,
    /// After the contact, before an attacker's own move (a lunge's step, a
    /// swap) starts.
    pub follow_gap: f32,
    /// A knocked body's slide, a cell.
    pub fling_secs: f32,
    /// Taking a blow: how long it shakes, and how far.
    pub shake_secs: f32,
    pub shake_reach: f32,
    /// An aimed weapon's wind-up (then held until it strikes).
    pub aim_secs: f32,
    /// Easing back from a wind-up that was dropped.
    pub unwind_secs: f32,
    /// How much longer than its strike a swing's streak takes to draw.
    pub streak_stretch: f32,
    /// How much more a killing blow knocks out of a body than its damage,
    /// and how much a knocked body running into something knocks out.
    pub kill_strength: i32,
    pub impact_strength: i32,
    /// After the blow lands, when a knocked body's slam into something
    /// plays, and the effect it plays there (none when not given).
    pub impact_gap: f32,
    #[serde(default)]
    pub impact_effect: Option<EffectId>,
}
