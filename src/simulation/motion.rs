use bevy::prelude::*;

use crate::model::*;

pub fn update_direction(
    mut facings: Query<(&Speed, &mut Facing), (With<DirectionBasedOnSpeed>, With<Active>)>,
) {
    for (speed, mut facing) in facings.iter_mut() {
        if speed.0 != IVec2::ZERO {
            facing.0 = speed.0;
        }
    }
}

pub fn movement(
    grid: Res<MapGrid>,
    mut movers: Query<(&Pos, &Speed, &mut PendingPos), (With<Active>, Without<DestroyRequested>)>,
) {
    for (pos, speed, mut pending) in movers.iter_mut() {
        if speed.0 != IVec2::ZERO && *pending == PendingPos::None {
            *pending = PendingPos::MoveTo(grid.safe_pos(pos.0 + speed.0));
        }
    }
}

pub fn relative_position(
    mut children: Query<
        (&BoundTo, &mut Pos, &mut PrevPos, &mut Facing),
        (Without<Collider>, Without<DestroyRequested>),
    >,
    parents: Query<(&Pos, &Facing), With<Collider>>,
) {
    for (bound, mut pos, mut previous, mut facing) in children.iter_mut() {
        let Ok((parent_pos, parent_facing)) = parents.get(bound.parent) else {
            continue;
        };
        let new_pos = parent_pos.0 + rotate(bound.offset, parent_facing.0);
        if pos.0 != new_pos {
            previous.0 = pos.0;
            pos.0 = new_pos;
        }
        let new_dir = rotate(bound.offset_dir, parent_facing.0);
        if facing.0 != new_dir {
            facing.0 = new_dir;
        }
    }
}

fn decay_axis(value: i32, friction: i32) -> i32 {
    if value.abs() > friction {
        value - value.signum() * friction
    } else {
        0
    }
}

pub fn friction(mut movers: Query<(&mut Speed, &Friction), With<Active>>) {
    for (mut speed, friction) in movers.iter_mut() {
        if speed.0 == IVec2::ZERO || friction.0 == 0 {
            continue;
        }
        speed.0.x = decay_axis(speed.0.x, friction.0);
        speed.0.y = decay_axis(speed.0.y, friction.0);
    }
}
