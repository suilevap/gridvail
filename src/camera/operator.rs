use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::animation::{MotionState, MotionStyle, Tween};
use crate::foundation::portal::CellTransform;
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

/// What the view does when its target steps through a portal that turns
/// (one whose two faces point different ways).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PortalTurn {
    /// The view turns with the target: the picture on screen stays as it
    /// was and arrow keys keep their screen directions, but the map's north
    /// is no longer up.
    #[default]
    WithTarget,
    /// The view turns with the target, then eases back to the way it was
    /// turned before (north up, unless Q/E turned it), so the map keeps its
    /// orientation on screen. While it eases back, arrow keys switch to the
    /// restored directions halfway through.
    KeepNorth,
}

impl PortalTurn {
    pub fn name(self) -> &'static str {
        match self {
            Self::WithTarget => "turn",
            Self::KeepNorth => "north",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::WithTarget, Self::KeepNorth]
            .into_iter()
            .find(|turn| turn.name() == name)
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::WithTarget => Self::KeepNorth,
            Self::KeepNorth => Self::WithTarget,
        }
    }
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
    /// What the view does when the target goes through a turning portal.
    pub portal_turn: PortalTurn,
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
            portal_turn: PortalTurn::default(),
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

    /// Carries the view through a portal its target stepped through: the
    /// camera goes on from the exit, turned by the portal's quarter turns
    /// the other way, so the picture on screen stays exactly as it was. A
    /// turn under way carries on from there. With `PortalTurn::KeepNorth`
    /// the view then eases back to the turn it was heading for.
    pub fn carry(&mut self, through: &CellTransform) {
        let heading = self.rotation.target();
        // The shorter way round, so going back through restores the view.
        let quarters = match through.quarters {
            3 => -1,
            quarters => quarters as i32,
        };
        let turn = quarters as f32 * FRAC_PI_2;
        self.rotation.carry(|angle| angle - turn);
        if self.portal_turn == PortalTurn::KeepNorth && quarters != 0 {
            self.turn_to(heading);
        }
        self.handover_from = through.apply_point(self.handover_from);
        if let Some(trail) = self.trail.as_mut() {
            trail.carry(through);
        }
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
    crossings: Option<Res<PortalCrossings>>,
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
    // A target that stepped through a portal takes the view with it, like
    // its animation.
    if let (Some(entity), Some(crossings)) = (entity, crossings.as_ref()) {
        if let Some(through) = crossings.arrived(entity, cell.as_ivec2()) {
            operator.carry(&through);
        }
    }
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

/// Q and E turn the view a quarter turn; Z and X zoom out and in; N
/// switches what the view does through turning portals (`PortalTurn`).
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
    if keys.just_pressed(KeyCode::KeyN) {
        operator.portal_turn = operator.portal_turn.toggled();
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
