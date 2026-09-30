use bevy::prelude::*;

use crate::model::*;
use crate::schedule::GamePhase;

use super::{MotionState, MotionStyle, ObjectMotion};

/// Animation state of one positioned object.
#[derive(Component, Clone, Copy, Debug)]
pub struct ObjectAnimation {
    pub state: MotionState,
}

/// Animates every positioned object (actors, walls, decor) between the
/// cells the simulation moves it through.
pub struct ObjectAnimationPlugin;

impl Plugin for ObjectAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MotionStyle>()
            .add_observer(start_animation)
            .add_systems(Update, animate_objects.in_set(GamePhase::Animation));
    }
}

/// Objects get their animation state when they are first placed, so steady
/// frames never queue commands.
fn start_animation(add: On<Add, Pos>, objects: Query<&Pos>, mut commands: Commands) {
    let Ok(pos) = objects.get(add.entity) else {
        return;
    };
    commands.entity(add.entity).insert((
        ObjectAnimation {
            state: MotionState::at(pos.0.as_vec2()),
        },
        AnimatedPos::at(pos.0),
    ));
}

/// Whether moving from `from` to `to` crossed the map edge: the map wraps,
/// so that move is a teleport, not a glide across the whole map.
fn wrapped(from: Vec2, to: Vec2, grid: &MapGrid) -> bool {
    let delta = (to - from).abs();
    delta.x > grid.width as f32 / 2.0 || delta.y > grid.height as f32 / 2.0
}

pub fn animate_objects(
    time: Res<Time>,
    style: Res<MotionStyle>,
    grid: Res<MapGrid>,
    mut objects: Query<(
        &Pos,
        Option<&ObjectMotion>,
        &mut ObjectAnimation,
        &mut AnimatedPos,
    )>,
) {
    let dt = time.delta_secs();
    for (pos, own_style, mut animation, mut shown) in &mut objects {
        let motion = own_style.map_or(*style, |own| own.0);
        let target = pos.0.as_vec2();
        let from = animation.state.target();
        if from != target {
            if wrapped(from, target, &grid) {
                animation.state = MotionState::at(target);
            } else {
                animation.state.retarget(target, motion);
            }
        }
        animation.state.advance(motion, dt);

        let next = AnimatedPos {
            position: animation.state.position,
        };
        if *shown != next {
            *shown = next;
        }
    }
}
