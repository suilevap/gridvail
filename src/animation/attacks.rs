//! Optional visual responses to an attack execution. Attack/damage systems
//! only publish data and never manipulate the motion player or glyphs.
use super::{MotionStyle, Move, ObjectAnimation, Path, Secondary};
use crate::model::{AttackAnimation, AttackExecuted, Pos};
use bevy::prelude::*;

pub(super) fn animate_attacks(
    style: Res<MotionStyle>,
    mut actors: Query<(
        &Pos,
        &AttackExecuted,
        &AttackAnimation,
        &mut ObjectAnimation,
    )>,
) {
    if *style == MotionStyle::Snap {
        return;
    }
    for (pos, execution, cue, mut animation) in &mut actors {
        if !execution.fired {
            continue;
        }
        match cue {
            AttackAnimation::Bump { seconds } => animation.state.bump(
                execution.direction.as_vec2(),
                MotionStyle::Tween {
                    duration: *seconds,
                    easing: EaseFunction::CubicIn,
                },
            ),
            AttackAnimation::Hop { seconds, height } => animation.state.play(Move {
                from: pos.0.as_vec2(),
                to: pos.0.as_vec2(),
                path: Path::Straight,
                duration: *seconds,
                easing: EaseFunction::CubicIn,
                secondary: Secondary {
                    hop: *height,
                    hops: 1.0,
                },
                coast: None,
            }),
        }
    }
}
