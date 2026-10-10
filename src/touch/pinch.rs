//! Two fingers move the camera: pinching zooms, and twisting past an eighth
//! of a turn turns the view a quarter turn, like Q and E.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use bevy::prelude::*;

use crate::camera::CameraOperator;

/// The two fingers of a pinch and where the gesture stands.
#[derive(Resource, Default, Debug)]
pub struct Pinch {
    fingers: Option<PinchStart>,
}

#[derive(Debug)]
struct PinchStart {
    ids: (u64, u64),
    /// Distance between the fingers, and the zoom, when they landed.
    distance: f32,
    zoom: f32,
    /// The fingers' angle the next quarter turn is measured from.
    angle: f32,
}

pub fn read_pinch(
    touches: Res<Touches>,
    mut pinch: ResMut<Pinch>,
    operator: Option<ResMut<CameraOperator>>,
) {
    let Some(mut operator) = operator else {
        return;
    };
    let mut pressed = touches.iter();
    let (Some(a), Some(b)) = (pressed.next(), pressed.next()) else {
        pinch.fingers = None;
        return;
    };
    let (a, b) = if a.id() < b.id() { (a, b) } else { (b, a) };
    let span = b.position() - a.position();
    let distance = span.length().max(1.0);
    // Window coordinates grow downward, so the angle grows clockwise.
    let angle = span.y.atan2(span.x);
    let start = match &mut pinch.fingers {
        Some(start) if start.ids == (a.id(), b.id()) => start,
        fingers => fingers.insert(PinchStart {
            ids: (a.id(), b.id()),
            distance,
            zoom: operator.target_zoom(),
            angle,
        }),
    };
    let zoom = start.zoom * distance / start.distance;
    if (zoom - operator.target_zoom()).abs() > 1e-3 {
        operator.zoom_to(zoom);
    }
    let twist = (angle - start.angle + PI).rem_euclid(TAU) - PI;
    if twist.abs() > FRAC_PI_4 {
        // The view turns the way the fingers did; its own angle grows
        // counter-clockwise.
        let quarters = -twist.signum() as i32;
        operator.turn_by_quarters(quarters);
        start.angle -= quarters as f32 * FRAC_PI_2;
    }
}
