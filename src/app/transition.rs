//! How a map comes together: from the game's title (or noise) when the game
//! starts, or from the walls of the map before when another is loaded.
//!
//! It is an ordinary room switch: a room covering the map, whose first
//! layout is what it comes from ([`Arrival`]) and whose second is the map's
//! own walls, played as a [`Cutscene`] in up to three steps
//! ([`TransitionPhase`]):
//!
//! 1. The title rises out of the floor and stands for [`HOLD_SECONDS`].
//!    Only an oval around it is in sight (a [`RevealArea`] that hides the
//!    player's own sight), and the camera looks at it from as far out as it
//!    zooms. (The intro only.)
//! 2. Over [`FOCUS_SECONDS`] the oval narrows into a small circle around the
//!    player, while the camera glides back to them. (The intro only.)
//! 3. The room switches: its walls slide, rise and sink like any room's,
//!    with whatever room planner is the default. The player's own sight is
//!    back, blocked by the new walls, and what they saw before through
//!    where walls now stand is forgotten; the circle around them closes to
//!    nothing over [`CLOSE_SECONDS`]. Once every wall has settled and the
//!    circle has closed, the room goes and play goes on. A map loaded in
//!    play starts here, from the walls of the map before.

use std::collections::HashSet;
use std::f32::consts::SQRT_2;
use std::time::Duration;

use bevy::prelude::*;
use rand::{RngExt, SeedableRng};

use crate::camera::{CameraOperator, CameraTarget, ZOOM_RANGE};
use crate::content::map::{SpawnCell, SpawnKind};
use crate::model::*;
use crate::schedule::GamePhase;
use crate::simulation::wall_bundle;

/// The title, as walls (`X`; anything else is floor).
const TITLE: &str = include_str!("../../assets/intro.txt");
/// Cells of map in sight around the title's oval, beyond its corners.
const TITLE_MARGIN: f32 = 1.5;
/// How far from the player noise reaches, in cells (also its oval).
const NOISE_RADIUS: f32 = 10.0;
/// Of the cells noise may use, the share that become walls.
const NOISE_DENSITY: f64 = 0.3;
/// Noise is the same every time.
const NOISE_SEED: u64 = 7;
/// How long the title stands before the map comes together, rising
/// included.
pub const HOLD_SECONDS: f32 = 1.5;
/// How long the title's oval takes to narrow onto the player, while the
/// camera glides back to them...
pub const FOCUS_SECONDS: f32 = 0.8;
/// ...into a circle this many cells across around them (where a map loaded
/// in play starts)...
const FOCUS_RADIUS: f32 = 6.0;
/// ...which, once the map's walls stand and the player's own sight is back,
/// closes to nothing over this long.
pub const CLOSE_SECONDS: f32 = 0.8;

/// What the game's first map comes together from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroLayout {
    /// The game's title (`assets/intro.txt`), centred on the player; noise
    /// on a map too small for it.
    Title,
    /// Walls scattered at random around the player.
    Noise,
}

impl IntroLayout {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "title" => Some(Self::Title),
            "noise" => Some(Self::Noise),
            _ => None,
        }
    }
}

/// Plays the intro when the game starts. Insert it before startup; without
/// it the first map appears at once (maps loaded later still come together
/// from the one before).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intro(pub IntroLayout);

/// What a map comes together from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arrival {
    /// The intro: the title (or noise) around the player's start.
    Intro(IntroLayout),
    /// The map before: its plain walls, kept as they stood, with their cells.
    Walls(Vec<(Entity, IVec2)>),
}

/// What the intro shows: its walls, and the oval of the map in sight
/// around them.
#[derive(Clone, Debug, PartialEq)]
pub struct IntroScene {
    pub walls: Vec<IVec2>,
    pub area: RevealArea,
}

/// Where a transition is.
#[derive(Clone, Debug)]
pub enum TransitionPhase {
    /// The title stands; the timer counts its time.
    Title(Timer),
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
            (frame_title, run_transition)
                .chain()
                .in_set(GamePhase::Simulation),
        )
        .add_systems(Update, forget_unseen.in_set(GamePhase::Animation))
        .add_systems(Update, end_transition.in_set(GamePhase::Finalize));
    }
}

/// The intro on a `width` x `height` map: the title centred on `player`
/// and kept inside the map (noise if it does not fit), or noise around
/// `player`; its walls never on a cell `blocked` says something else spawns
/// on.
pub fn intro_scene(
    layout: IntroLayout,
    width: i32,
    height: i32,
    player: IVec2,
    blocked: impl Fn(IVec2) -> bool,
) -> IntroScene {
    let scene = match layout {
        IntroLayout::Title => title_scene(TITLE, width, height, player),
        IntroLayout::Noise => None,
    };
    let mut scene = scene.unwrap_or_else(|| noise_scene(width, height, player));
    scene.walls.retain(|&p| !blocked(p));
    scene
}

/// `art`'s walls centred on `centre`, moved to lie inside the map, and the
/// oval through its corners (plus a margin); `None` when it is larger than
/// the map.
fn title_scene(art: &str, width: i32, height: i32, centre: IVec2) -> Option<IntroScene> {
    let rows: Vec<&str> = art.lines().collect();
    let size = IVec2::new(
        rows.iter()
            .map(|row| row.chars().count())
            .max()
            .unwrap_or(0) as i32,
        rows.len() as i32,
    );
    if size.x > width || size.y > height {
        return None;
    }
    let corner = (centre - size / 2).clamp(IVec2::ZERO, IVec2::new(width, height) - size);
    let walls = rows
        .iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.chars().enumerate().filter_map(move |(x, c)| {
                matches!(c, 'X' | 'x').then_some(corner + IVec2::new(x as i32, y as i32))
            })
        })
        .collect();
    // An oval through a rectangle's corners is √2 times its half size.
    let area = RevealArea {
        centre: corner.as_vec2() + (size - 1).as_vec2() / 2.0,
        radii: size.as_vec2() / 2.0 * SQRT_2 + TITLE_MARGIN,
        hides_sight: true,
    };
    Some(IntroScene { walls, area })
}

/// Walls scattered within [`NOISE_RADIUS`] of `centre`, and that circle.
fn noise_scene(width: i32, height: i32, centre: IVec2) -> IntroScene {
    let area = RevealArea {
        centre: centre.as_vec2(),
        radii: Vec2::splat(NOISE_RADIUS),
        hides_sight: true,
    };
    let mut rng = rand::rngs::StdRng::seed_from_u64(NOISE_SEED);
    let walls = (0..height)
        .flat_map(|y| (0..width).map(move |x| IVec2::new(x, y)))
        .filter(|&p| area.contains(p))
        .filter(|_| rng.random_bool(NOISE_DENSITY))
        .collect();
    IntroScene { walls, area }
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
        Arrival::Intro(layout) => {
            let scene = intro_scene(layout, grid.width, grid.height, player, blocked);
            // The title rises out of the floor; `Rising` goes in with the
            // wall, as the animation step reads it as the wall is placed.
            for &p in &scene.walls {
                let wall = commands.spawn((wall_bundle(p), Rising { from: p })).id();
                grid.set(p, wall);
            }
            let hold = Timer::from_seconds(HOLD_SECONDS, TimerMode::Once);
            (scene.walls, scene.area, TransitionPhase::Title(hold))
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

/// When a title starts standing, points the camera at it from as far out
/// as it zooms.
fn frame_title(
    camera: Option<Res<ViewCamera>>,
    operator: Option<ResMut<CameraOperator>>,
    area: Option<Res<RevealArea>>,
    mut rooms: Query<&mut TransitionRoom, Added<TransitionRoom>>,
) {
    let (Some(camera), Some(mut operator), Some(area)) = (camera, operator, area) else {
        return;
    };
    for mut transition in &mut rooms {
        if !matches!(transition.phase, TransitionPhase::Title(_)) {
            continue;
        }
        transition.zoom = operator.target_zoom();
        operator.look_at(CameraTarget::Point(area.centre), &camera);
        operator.zoom_to(ZOOM_RANGE.0);
        operator.snap();
    }
}

/// Steps a transition on: once the title has stood, the oval in sight closes
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
            if let TransitionPhase::Title(_) | TransitionPhase::Focus { .. } = transition.phase {
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
            TransitionPhase::Title(hold) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_is_centred_on_the_player_and_kept_inside_the_map() {
        let art = "X.X\n.X.";
        let walls = |centre| title_scene(art, 10, 6, centre).unwrap().walls;
        assert_eq!(
            walls(IVec2::new(5, 3)),
            [IVec2::new(4, 2), IVec2::new(6, 2), IVec2::new(5, 3)]
        );
        // Near a corner it moves inside the map.
        assert_eq!(
            walls(IVec2::ZERO),
            [IVec2::new(0, 0), IVec2::new(2, 0), IVec2::new(1, 1)]
        );
        assert!(title_scene(art, 2, 6, IVec2::ZERO).is_none(), "too small");
    }

    #[test]
    fn the_title_is_in_sight_and_little_else() {
        let scene = title_scene(TITLE, 80, 24, IVec2::new(40, 12)).unwrap();
        assert!(scene.walls.iter().all(|&p| scene.area.contains(p)));
        let shown = (0..24)
            .flat_map(|y| (0..80).map(move |x| IVec2::new(x, y)))
            .filter(|&p| scene.area.contains(p))
            .count();
        assert!(shown < 80 * 24 / 2, "less than half the map: {shown}");
    }

    #[test]
    fn a_map_too_small_for_the_title_gets_noise_around_the_player() {
        let player = IVec2::new(10, 5);
        let scene = intro_scene(IntroLayout::Title, 30, 10, player, |p| p == player);
        assert!(!scene.walls.is_empty());
        assert!(!scene.walls.contains(&player), "nothing lands on spawns");
        assert!(scene.walls.iter().all(|&p| scene.area.contains(p)));
        assert_eq!(scene.area.centre, player.as_vec2());
        assert_eq!(
            scene,
            intro_scene(IntroLayout::Noise, 30, 10, player, |p| p == player)
        );
    }
}
