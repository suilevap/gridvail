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

/// Starts each mover's step. A step into a portal face from its open side
/// goes to the floor in front of the exit face instead, and is recorded in
/// `PortalCrossings`; collisions there are resolved like any other step.
pub fn movement(
    grid: Res<MapGrid>,
    mut crossings: ResMut<PortalCrossings>,
    mut movers: Query<
        (Entity, &Pos, &Speed, &mut PendingPos),
        (With<Active>, Without<DestroyRequested>),
    >,
) {
    crossings.0.clear();
    for (entity, pos, speed, mut pending) in movers.iter_mut() {
        if speed.0 == IVec2::ZERO || *pending != PendingPos::None {
            continue;
        }
        let mut target = pos.0 + speed.0;
        if let Some(face) = grid.portal_at(target).filter(|face| face.side == -speed.0) {
            target = grid.safe_pos(face.through.apply(target));
            crossings.0.push(PortalCrossing {
                entity,
                through: face.through,
                arrival: target,
            });
        }
        *pending = PendingPos::MoveTo(grid.safe_pos(target));
    }
}

/// Turns the speed and facing of each mover that came out of a portal by
/// the portal's quarter turns, so it goes on the way it was going: into a
/// wall facing south, out of one facing east, the step carries on east.
/// Steps blocked at the exit turn nothing.
pub fn turn_portal_crossers(
    crossings: Res<PortalCrossings>,
    mut movers: Query<(&Pos, Option<&mut Speed>, Option<&mut Facing>)>,
) {
    for crossing in &crossings.0 {
        if crossing.through.quarters == 0 {
            continue;
        }
        let Ok((pos, speed, facing)) = movers.get_mut(crossing.entity) else {
            continue;
        };
        if pos.0 != crossing.arrival {
            continue;
        }
        if let Some(mut speed) = speed {
            speed.0 = crossing.through.turn(speed.0);
        }
        if let Some(mut facing) = facing {
            facing.0 = crossing.through.turn(facing.0);
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
