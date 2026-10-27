use std::time::Duration;

use bevy::prelude::*;

use super::{timer, Focusing, Forming, Showing, TransitionRoom};
use super::{CLOSE_SECONDS, FOCUS_RADIUS, FOCUS_SECONDS};
use crate::camera::{CameraOperator, CameraTarget};
use crate::model::*;

/// Where the player is shown, or `fallback` without one.
fn player_shown(
    players: &Query<(&Pos, Option<&AnimatedPos>), With<Player>>,
    fallback: Vec2,
) -> Vec2 {
    players.single().map_or(fallback, |(pos, shown)| {
        shown.map_or(pos.0.as_vec2(), |shown| shown.position)
    })
}

fn eased(timer: &Timer) -> f32 {
    EaseFunction::SmoothStep.sample_clamped(timer.fraction())
}

/// Phase 1: once shown walls have stood, focus on the player, and the camera
/// glides back to them at the zoom it had.
pub(super) fn show(
    mut commands: Commands,
    time: Res<Time>,
    area: Option<Res<RevealArea>>,
    camera: Option<Res<ViewCamera>>,
    mut operator: Option<ResMut<CameraOperator>>,
    mut rooms: Query<(Entity, &TransitionRoom, &mut Showing)>,
) {
    for (entity, transition, mut showing) in &mut rooms {
        if !showing.0.tick(time.delta()).just_finished() {
            continue;
        }
        if let (Some(camera), Some(operator)) = (camera.as_deref(), operator.as_deref_mut()) {
            let before = operator.transition;
            operator.transition = FOCUS_SECONDS;
            operator.look_at(CameraTarget::Player, camera);
            operator.zoom_to(transition.zoom);
            operator.transition = before;
        }
        let Some(&from) = area.as_deref() else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<Showing>()
            .insert(Focusing {
                from,
                timer: timer(FOCUS_SECONDS),
            });
    }
}

/// Phase 2: the area narrows onto the player; once it has, the player's own
/// sight is back and the room switches.
pub(super) fn focus(
    mut commands: Commands,
    time: Res<Time>,
    area: Option<ResMut<RevealArea>>,
    players: Query<(&Pos, Option<&AnimatedPos>), With<Player>>,
    mut rooms: Query<(Entity, &mut Focusing)>,
) {
    let Some(mut area) = area else {
        return;
    };
    for (entity, mut focusing) in &mut rooms {
        focusing.timer.tick(time.delta());
        let t = eased(&focusing.timer);
        let from = focusing.from.oval;
        area.oval.centre = from.centre.lerp(player_shown(&players, from.centre), t);
        area.oval.radii = from.radii.lerp(Vec2::splat(FOCUS_RADIUS), t);
        if focusing.timer.is_finished() {
            area.hides_sight = false;
            commands
                .entity(entity)
                .remove::<Focusing>()
                .insert(Forming {
                    from: *area,
                    timer: timer(CLOSE_SECONDS),
                    forgotten: false,
                });
        }
    }
}

/// Phase 3: the room switches to the map's walls, and the circle around the
/// player closes; once it has, nothing extra is in sight.
pub(super) fn form(
    mut commands: Commands,
    time: Res<Time>,
    area: Option<ResMut<RevealArea>>,
    players: Query<(&Pos, Option<&AnimatedPos>), With<Player>>,
    mut rooms: Query<(&mut Room, &mut Forming)>,
) {
    let Some(mut area) = area else {
        return;
    };
    for (mut room, mut forming) in &mut rooms {
        room.target = 1;
        forming.timer.tick(time.delta());
        area.oval.centre = player_shown(&players, area.oval.centre);
        area.oval.radii = forming.from.oval.radii * (1.0 - eased(&forming.timer));
        if forming.timer.is_finished() {
            commands.remove_resource::<RevealArea>();
        }
    }
}

/// Once the room has switched and sight has been worked out again with the
/// map's walls standing, forgets every explored cell the player does not see
/// now: before, they saw through where walls now are.
pub(super) fn forget_unseen(
    mut rooms: Query<(&Room, &mut Forming)>,
    mut players: Query<&mut VisibilityMap, With<Player>>,
) {
    for (room, mut forming) in &mut rooms {
        if forming.forgotten || room.current != 1 {
            continue;
        }
        for mut visibility in &mut players {
            for cell in &mut visibility.data {
                if !cell.contains(Vis::VISIBLE) {
                    cell.remove(Vis::KNOWN);
                }
            }
        }
        forming.forgotten = true;
    }
}

/// Ends a transition once its room has switched, nothing still animates and
/// the circle in sight has closed: the room goes (no plate switches it
/// back) and the cutscene ends.
pub(super) fn end_transition(
    mut commands: Commands,
    pacing: Res<TurnPacing>,
    area: Option<Res<RevealArea>>,
    rooms: Query<(Entity, &Room), With<Forming>>,
) {
    let animating = pacing
        .animation()
        .is_some_and(|remaining| remaining > Duration::ZERO);
    for (entity, room) in &rooms {
        if room.current != 1 || animating || area.is_some() {
            continue;
        }
        commands.entity(entity).despawn();
        commands.remove_resource::<Cutscene>();
    }
}
