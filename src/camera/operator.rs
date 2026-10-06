use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::animation::{MotionState, MotionStyle, Tween};
use crate::model::*;

/// The smallest and largest zoom the operator allows.
pub const ZOOM_RANGE: (f32, f32) = (0.5, 2.0);

/// What the camera looks at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraTarget {
    /// The player, wherever they are.
    Player,
    Entity(Entity),
    /// A fixed map point, in fractional cells.
    Point(Vec2),
}

/// How the camera keeps up with a moving target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraFollow {
    /// Exactly on the target's shown (animated) position: the target stays
    /// still on screen, as smooth as its own animation.
    Locked,
    /// The camera makes its own moves to the target's cell in this style,
    /// so it trails the target and settles after it.
    Trailing(MotionStyle),
}

/// Directs the `ViewCamera`: follows a target and eases every change.
///
/// Changes go through methods so that each one becomes a transition of
/// `transition` seconds with `easing` instead of a jump.
#[derive(Resource, Debug)]
pub struct CameraOperator {
    pub follow: CameraFollow,
    /// Seconds a change of target, rotation, zoom or offset takes.
    pub transition: f32,
    pub easing: EaseFunction,
    target: CameraTarget,
    rotation: Tween<f32>,
    zoom: Tween<f32>,
    offset: Tween<Vec2>,
    /// Blends from where the camera was to the new target after a change of
    /// target, from 0 (old place) to 1 (on the target).
    handover: Tween<f32>,
    handover_from: Vec2,
    /// The camera's own motion when trailing; created on first use.
    trail: Option<MotionState>,
}

impl Default for CameraOperator {
    fn default() -> Self {
        Self::new(CameraTarget::Player)
    }
}

impl CameraOperator {
    pub fn new(target: CameraTarget) -> Self {
        Self {
            follow: CameraFollow::Locked,
            transition: 0.3,
            easing: EaseFunction::SmoothStep,
            target,
            rotation: Tween::at(0.0),
            zoom: Tween::at(1.0),
            offset: Tween::at(Vec2::ZERO),
            handover: Tween::at(1.0),
            handover_from: Vec2::ZERO,
            trail: None,
        }
    }

    pub fn target(&self) -> CameraTarget {
        self.target
    }

    /// Looks at something else, gliding there from the current view.
    pub fn look_at(&mut self, target: CameraTarget, camera: &ViewCamera) {
        if target == self.target {
            return;
        }
        self.target = target;
        self.trail = None;
        self.handover_from = camera.position;
        self.handover.snap(0.0);
        self.handover.ease_to(1.0, self.transition, self.easing);
    }

    /// The rotation the camera is turning to, in radians.
    pub fn target_rotation(&self) -> f32 {
        self.rotation.target()
    }

    /// Turns the view to `angle` radians (counter-clockwise on screen).
    pub fn turn_to(&mut self, angle: f32) {
        self.rotation.ease_to(angle, self.transition, self.easing);
    }

    /// Turns the view by quarter turns from where it is turning to,
    /// counter-clockwise for positive `quarters`. Repeated turns add up.
    pub fn turn_by_quarters(&mut self, quarters: i32) {
        self.turn_to(self.rotation.target() + quarters as f32 * FRAC_PI_2);
    }

    pub fn target_zoom(&self) -> f32 {
        self.zoom.target()
    }

    /// Zooms to `zoom`, kept within `ZOOM_RANGE`.
    pub fn zoom_to(&mut self, zoom: f32) {
        let zoom = zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
        self.zoom.ease_to(zoom, self.transition, self.easing);
    }

    /// Shows the target at `offset` view cells from the screen centre.
    pub fn offset_to(&mut self, offset: Vec2) {
        self.offset.ease_to(offset, self.transition, self.easing);
    }

    /// Finishes every transition now.
    pub fn snap(&mut self) {
        self.rotation.snap(self.rotation.target());
        self.zoom.snap(self.zoom.target());
        self.offset.snap(self.offset.target());
        self.handover.snap(1.0);
    }

    fn advance(&mut self, dt: f32) {
        self.rotation.advance(dt);
        self.zoom.advance(dt);
        self.offset.advance(dt);
        self.handover.advance(dt);
    }

    /// Where the camera should centre: the target, reached the way `follow`
    /// says. `shown` is the target's shown position, `cell` its logical one.
    fn follow_target(&mut self, shown: Vec2, cell: Vec2, grid: &MapGrid, dt: f32) -> Vec2 {
        match self.follow {
            CameraFollow::Locked => {
                self.trail = None;
                shown
            }
            CameraFollow::Trailing(style) => {
                let trail = self.trail.get_or_insert_with(|| MotionState::at(cell));
                if trail.target() != cell {
                    // Crossing the wrapping map edge is a cut, not a pan
                    // across the whole map.
                    let delta = (cell - trail.target()).abs();
                    if delta.x > grid.width as f32 / 2.0 || delta.y > grid.height as f32 / 2.0 {
                        *trail = MotionState::at(cell);
                    } else {
                        trail.move_to(cell, style);
                    }
                }
                trail.advance(dt);
                trail.position
            }
        }
    }
}

/// Moves the `ViewCamera` the way the operator directs.
pub fn operate_camera(
    time: Res<Time>,
    grid: Res<MapGrid>,
    players: Query<Entity, With<Player>>,
    objects: Query<(&Pos, Option<&AnimatedPos>)>,
    mut operator: ResMut<CameraOperator>,
    mut camera: ResMut<ViewCamera>,
) {
    let dt = time.delta_secs();
    operator.advance(dt);

    let entity = match operator.target {
        CameraTarget::Player => players.single().ok(),
        CameraTarget::Entity(entity) => Some(entity),
        CameraTarget::Point(_) => None,
    };
    let (shown, cell) = match (operator.target, entity.and_then(|e| objects.get(e).ok())) {
        (CameraTarget::Point(point), _) => (point, point),
        (_, Some((pos, animated))) => (
            animated.map_or(pos.0.as_vec2(), |animated| animated.position),
            pos.0.as_vec2(),
        ),
        // A target that is gone (or not spawned yet) leaves the view where
        // it is.
        (_, None) => (camera.position, camera.position),
    };
    let on_target = operator.follow_target(shown, cell, &grid, dt);
    let blend = operator.handover.value();
    let position = operator.handover_from.lerp(on_target, blend);

    let next = ViewCamera {
        position,
        rotation: operator.rotation.value(),
        zoom: operator.zoom.value(),
        offset: operator.offset.value(),
    };
    if *camera != next {
        *camera = next;
    }
}

/// Q and E turn the view a quarter turn; Z and X zoom out and in.
pub fn camera_controls(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut operator: ResMut<CameraOperator>,
) {
    let Some(keys) = keys else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyQ) {
        operator.turn_by_quarters(1);
    }
    if keys.just_pressed(KeyCode::KeyE) {
        operator.turn_by_quarters(-1);
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        let zoom = operator.target_zoom() / 1.25;
        operator.zoom_to(zoom);
    }
    if keys.just_pressed(KeyCode::KeyX) {
        let zoom = operator.target_zoom() * 1.25;
        operator.zoom_to(zoom);
    }
}
