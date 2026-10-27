//! How a map comes together from what was there before ([`Arrival`]): walls
//! shown first (the game's intro builds these, see `intro`), or the walls of
//! the map before when another is loaded. It knows nothing of where they
//! came from.
//!
//! It is an ordinary room switch: a room covering the map, whose first
//! layout is the arriving walls and whose second is the map's own, played
//! as a [`Cutscene`] in up to three steps ([`TransitionPhase`]):
//!
//! 1. Shown walls rise out of the floor and stand for [`SHOW_SECONDS`].
//!    Only their area is in sight (a [`RevealArea`] that hides the player's
//!    own sight), and the camera looks at it from as far out as it zooms.
//! 2. Over [`FOCUS_SECONDS`] the area narrows into a small circle around
//!    the player, while the camera glides back to them.
//! 3. The room switches: its walls slide, rise and sink like any room's,
//!    with whatever room planner is the default. The player's own sight is
//!    back, blocked by the new walls, and what they saw before through
//!    where walls now stand is forgotten; the circle around them closes to
//!    nothing over [`CLOSE_SECONDS`]. Once every wall has settled and the
//!    circle has closed, the room goes and play goes on. Walls of the map
//!    before start here.

use std::collections::HashSet;
use std::time::Duration;

use bevy::prelude::*;

use crate::camera::{CameraOperator, CameraTarget, ZOOM_RANGE};
use crate::content::map::{SpawnCell, SpawnKind};
use crate::model::*;
use crate::schedule::GamePhase;
use crate::simulation::wall_bundle;

/// How long shown walls stand before the map comes together, rising
/// included.
pub const SHOW_SECONDS: f32 = 1.5;
/// How long their area takes to narrow onto the player, while the camera
/// glides back to them...
pub const FOCUS_SECONDS: f32 = 0.8;
/// ...into a circle this many cells across around them (where a map loaded
/// in play starts)...
const FOCUS_RADIUS: f32 = 6.0;
/// ...which, once the map's walls stand and the player's own sight is back,
/// closes to nothing over this long.
pub const CLOSE_SECONDS: f32 = 0.8;

/// What a map comes together from.
#[derive(Clone, Debug, PartialEq)]
pub enum Arrival {
    /// Walls shown first, in `area` (which hides the player's own sight),
    /// before the map comes together from them: rising out of the floor,
    /// then standing for a moment.
    Shown { walls: Vec<IVec2>, area: RevealArea },
    /// The map before: its plain walls, kept as they stood, with their cells.
    Walls(Vec<(Entity, IVec2)>),
}

/// Where a transition is.
#[derive(Clone, Debug)]
pub enum TransitionPhase {
    /// Shown walls stand; the timer counts their time.
    Show(Timer),
    /// The oval in sight closes onto the player, from `from`, into a small
    /// circle around them.
    Focus { from: RevealArea, timer: Timer },
    /// The room has switched and its walls are on their way; the player's
    /// own sight is back and the circle around them closes, from `from`.
    /// `forgotten` is whether what they saw before the map's walls stood
    /// (through where they now are) has been forgotten yet.
    Forming {
        from: RevealArea,
        timer: Timer,
        forgotten: bool,
    },
}

/// The room a transition switches, where it is, and the camera's zoom
/// before it, to go back to.
#[derive(Component, Clone, Debug)]
pub struct TransitionRoom {
    pub phase: TransitionPhase,
    zoom: f32,
}

pub(super) struct TransitionPlugin;

impl Plugin for TransitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (frame_shown, run_transition)
                .chain()
                .in_set(GamePhase::Simulation),
        )
        .add_systems(Update, forget_unseen.in_set(GamePhase::Animation))
        .add_systems(Update, end_transition.in_set(GamePhase::Finalize));
    }
}

/// Sets a transition up in place of the map's plain walls (`map_walls`,
/// not spawned): spawns the walls it comes from (keeping off the cells
/// `cells` spawn anything else on, and off the map's edge), the room that
/// switches them to the map's, the cutscene and the area in sight. Portal
/// walls and everything else are already in place.
pub(super) fn begin_transition(
    commands: &mut Commands,
    grid: &mut MapGrid,
    arrival: Arrival,
    cells: &[SpawnCell],
    map_walls: Vec<IVec2>,
) {
    let occupied: HashSet<IVec2> = cells
        .iter()
        .filter(|cell| cell.kind != SpawnKind::Wall)
        .map(|cell| cell.pos)
        .collect();
    let size = IVec2::new(grid.width, grid.height);
    let blocked =
        |p: IVec2| p.cmplt(IVec2::ZERO).any() || p.cmpge(size).any() || occupied.contains(&p);
    let player = cells
        .iter()
        .find(|cell| cell.kind == SpawnKind::Player)
        .map_or(IVec2::new(grid.width, grid.height) / 2, |cell| cell.pos);
    let (walls, area, phase) = match arrival {
        Arrival::Shown { mut walls, area } => {
            walls.retain(|&p| !blocked(p));
            // They rise out of the floor; `Rising` goes in with the wall, as
            // the animation step reads it as the wall is placed.
            for &p in &walls {
                let wall = commands.spawn((wall_bundle(p), Rising { from: p })).id();
                grid.set(p, wall);
            }
            let show = Timer::from_seconds(SHOW_SECONDS, TimerMode::Once);
            (walls, area, TransitionPhase::Show(show))
        }
        Arrival::Walls(kept) => {
            // The old walls stay as they were (shape, animation, meshes),
            // but not off the map or where the new one has something else.
            let mut walls = Vec::with_capacity(kept.len());
            for (wall, p) in kept {
                if blocked(p) {
                    commands.entity(wall).despawn();
                } else {
                    grid.set(p, wall);
                    walls.push(p);
                }
            }
            let area = RevealArea {
                centre: player.as_vec2(),
                radii: Vec2::splat(FOCUS_RADIUS),
                hides_sight: false,
            };
            let phase = TransitionPhase::Forming {
                from: area,
                timer: Timer::from_seconds(CLOSE_SECONDS, TimerMode::Once),
                forgotten: false,
            };
            (walls, area, phase)
        }
    };
    let mut room = Room::new(
        IVec2::ZERO,
        IVec2::new(grid.width, grid.height),
        vec![walls, map_walls],
    );
    if matches!(phase, TransitionPhase::Forming { .. }) {
        room.target = 1;
    }
    commands.spawn((room, TransitionRoom { phase, zoom: 1.0 }));
    commands.insert_resource(Cutscene);
    commands.insert_resource(area);
}

/// When shown walls start standing, points the camera at their area from
/// as far out as it zooms.
fn frame_shown(
    camera: Option<Res<ViewCamera>>,
    operator: Option<ResMut<CameraOperator>>,
    area: Option<Res<RevealArea>>,
    mut rooms: Query<&mut TransitionRoom, Added<TransitionRoom>>,
) {
    let (Some(camera), Some(mut operator), Some(area)) = (camera, operator, area) else {
        return;
    };
    for mut transition in &mut rooms {
        if !matches!(transition.phase, TransitionPhase::Show(_)) {
            continue;
        }
        transition.zoom = operator.target_zoom();
        operator.look_at(CameraTarget::Point(area.centre), &camera);
        operator.zoom_to(ZOOM_RANGE.0);
        operator.snap();
    }
}

/// Steps a transition on: once shown walls have stood, the area in sight closes
/// onto the player (where they are shown) and the camera goes back to them;
/// once it has, the room switches to the map's layout, the player's own
/// sight comes back and the circle left around them closes.
#[allow(clippy::too_many_arguments)]
fn run_transition(
    mut commands: Commands,
    time: Res<Time>,
    camera: Option<Res<ViewCamera>>,
    mut operator: Option<ResMut<CameraOperator>>,
    mut area: Option<ResMut<RevealArea>>,
    players: Query<(&Pos, Option<&AnimatedPos>), With<Player>>,
    mut rooms: Query<(&mut Room, &mut TransitionRoom)>,
) {
    let fallback = area.as_deref().map_or(Vec2::ZERO, |area| area.centre);
    let player = players.single().map_or(fallback, |(pos, shown)| {
        shown.map_or(pos.0.as_vec2(), |shown| shown.position)
    });
    let timer = |seconds| Timer::from_seconds(seconds, TimerMode::Once);
    for (mut room, mut transition) in &mut rooms {
        let zoom = transition.zoom;
        let Some(area) = area.as_deref_mut() else {
            // Nothing to close: straight to the map.
            if let TransitionPhase::Show(_) | TransitionPhase::Focus { .. } = transition.phase {
                room.target = 1;
                let none = RevealArea {
                    centre: player,
                    radii: Vec2::ZERO,
                    hides_sight: false,
                };
                transition.phase = TransitionPhase::Forming {
                    from: none,
                    timer: timer(0.0),
                    forgotten: false,
                };
            }
            continue;
        };
        match &mut transition.phase {
            TransitionPhase::Show(hold) => {
                if !hold.tick(time.delta()).just_finished() {
                    continue;
                }
                if let (Some(camera), Some(operator)) = (camera.as_deref(), operator.as_deref_mut())
                {
                    let transition = operator.transition;
                    operator.transition = FOCUS_SECONDS;
                    operator.look_at(CameraTarget::Player, camera);
                    operator.zoom_to(zoom);
                    operator.transition = transition;
                }
                transition.phase = TransitionPhase::Focus {
                    from: *area,
                    timer: timer(FOCUS_SECONDS),
                };
            }
            TransitionPhase::Focus { from, timer: focus } => {
                focus.tick(time.delta());
                let t = EaseFunction::SmoothStep.sample_clamped(focus.fraction());
                area.centre = from.centre.lerp(player, t);
                area.radii = from.radii.lerp(Vec2::splat(FOCUS_RADIUS), t);
                if focus.is_finished() {
                    area.hides_sight = false;
                    room.target = 1;
                    transition.phase = TransitionPhase::Forming {
                        from: *area,
                        timer: timer(CLOSE_SECONDS),
                        forgotten: false,
                    };
                }
            }
            TransitionPhase::Forming {
                from,
                timer: closing,
                ..
            } => {
                closing.tick(time.delta());
                let t = EaseFunction::SmoothStep.sample_clamped(closing.fraction());
                area.centre = player;
                area.radii = from.radii * (1.0 - t);
                if closing.is_finished() {
                    commands.remove_resource::<RevealArea>();
                }
            }
        }
    }
}

/// Once the room has switched and sight has been worked out again with the
/// map's walls standing, forgets every explored cell the player does not see
/// now: before, they saw across the open map, through where walls now are.
fn forget_unseen(
    mut rooms: Query<(&Room, &mut TransitionRoom)>,
    mut players: Query<&mut VisibilityMap, With<Player>>,
) {
    for (room, mut transition) in &mut rooms {
        let TransitionPhase::Forming { forgotten, .. } = &mut transition.phase else {
            continue;
        };
        if *forgotten || room.current != 1 {
            continue;
        }
        for mut visibility in &mut players {
            for cell in &mut visibility.data {
                if !cell.contains(Vis::VISIBLE) {
                    cell.remove(Vis::KNOWN);
                }
            }
        }
        *forgotten = true;
    }
}

/// Ends the intro once its room has switched, nothing still animates and the
/// circle in sight has closed: the room goes (no plate switches it back) and
/// the cutscene ends.
fn end_transition(
    mut commands: Commands,
    pacing: Res<TurnPacing>,
    area: Option<Res<RevealArea>>,
    rooms: Query<(Entity, &Room, &TransitionRoom)>,
) {
    for (entity, room, transition) in &rooms {
        let animating = pacing
            .animation()
            .is_some_and(|remaining| remaining > Duration::ZERO);
        let forming = matches!(transition.phase, TransitionPhase::Forming { .. });
        if !forming || room.current != 1 || animating || area.is_some() {
            continue;
        }
        commands.entity(entity).despawn();
        commands.remove_resource::<Cutscene>();
    }
}
