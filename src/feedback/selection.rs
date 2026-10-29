use bevy::prelude::*;

use crate::lighting::{WHITE, YELLOW};
use crate::model::*;

use super::{mark, mix};

/// How far a selector leans toward what it just picked, in cells.
const PICK_LEAN: f32 = 0.3;
/// How fast a picked target pulses, in pulses per second.
const PICK_PULSE: f32 = 2.0;

/// A selector that just picked something leans toward it.
pub fn lean_on_pick(
    picks: Query<(Entity, &Selected), Added<Selected>>,
    mut gestures: MessageWriter<Gesture>,
) {
    for (selector, selected) in &picks {
        gestures.write(Gesture {
            entity: selector,
            toward: selected.dir.as_vec2() * PICK_LEAN,
        });
    }
}

/// A picked target pulses between yellow and white.
pub fn mark_picks(
    time: Res<Time>,
    marks: Option<ResMut<CellMarks>>,
    selections: Query<&Selected>,
    positions: Query<&Pos>,
) {
    let Some(mut marks) = marks else {
        return;
    };
    let wave = (time.elapsed_secs() * PICK_PULSE * std::f32::consts::TAU).sin() * 0.5 + 0.5;
    let tint = mix(YELLOW, WHITE, wave);
    for selected in &selections {
        if let Ok(pos) = positions.get(selected.target) {
            marks.0.push((pos.0, mark('·', tint)));
        }
    }
}
