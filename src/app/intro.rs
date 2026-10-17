//! The intro: the map's walls come together from the game's title (or
//! noise) when a map is first loaded.
//!
//! It is an ordinary room switch: a room covering the whole map whose first
//! layout is the title and whose second is the map's own walls. The title
//! rises out of the floor and stands for [`HOLD_SECONDS`]; then the room
//! switches, so its walls slide, rise and sink like any room's, with
//! whatever room planner is the default. While it plays it is a
//! [`Cutscene`]; an oval around the title is in sight (a [`RevealArea`],
//! nothing else of the map) and the camera looks at it from as far out as
//! it zooms. Once every wall has settled the room goes and play starts:
//! over [`CLOSE_SECONDS`] the oval closes onto the player, leaving what
//! they see, while the camera glides back to them.

use std::collections::HashSet;
use std::f32::consts::SQRT_2;
use std::time::Duration;

use bevy::prelude::*;
use rand::{RngExt, SeedableRng};

use crate::camera::{CameraOperator, CameraTarget, ZOOM_RANGE};
use crate::content::map::{SpawnCell, SpawnKind};
use crate::model::*;
use crate::schedule::{GamePhase, StartupPhase};
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
/// How long the oval takes to close onto the player afterwards, while the
/// camera glides back to them.
pub const CLOSE_SECONDS: f32 = 0.8;

/// What the map's walls come together from.
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

/// Plays the intro when the map is constructed. Insert it before startup;
/// without it the map appears at once.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intro(pub IntroLayout);

/// What the intro shows: its walls, and the oval of the map in sight
/// around them.
#[derive(Clone, Debug, PartialEq)]
pub struct IntroScene {
    pub walls: Vec<IVec2>,
    pub area: RevealArea,
}

/// The room the intro switches. `hold` is how long the title still stands;
/// `zoom` is the camera's zoom before, to go back to.
#[derive(Component, Clone, Debug)]
pub struct IntroRoom {
    hold: Timer,
    zoom: f32,
}

/// The oval closing onto the player once the intro is over: where it
/// started and how far it has got.
#[derive(Resource, Clone, Debug)]
pub struct IntroClosing {
    from: RevealArea,
    timer: Timer,
}

pub(super) struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, frame_intro.in_set(StartupPhase::Derive))
            .add_systems(Update, switch_intro.in_set(GamePhase::Simulation))
            .add_systems(
                Update,
                close_intro
                    .run_if(resource_exists::<IntroClosing>)
                    .in_set(GamePhase::Animation),
            )
            .add_systems(Update, end_intro.in_set(GamePhase::Finalize));
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
    };
    Some(IntroScene { walls, area })
}

/// Walls scattered within [`NOISE_RADIUS`] of `centre`, and that circle.
fn noise_scene(width: i32, height: i32, centre: IVec2) -> IntroScene {
    let area = RevealArea {
        centre: centre.as_vec2(),
        radii: Vec2::splat(NOISE_RADIUS),
    };
    let mut rng = rand::rngs::StdRng::seed_from_u64(NOISE_SEED);
    let walls = (0..height)
        .flat_map(|y| (0..width).map(move |x| IVec2::new(x, y)))
        .filter(|&p| area.contains(p))
        .filter(|_| rng.random_bool(NOISE_DENSITY))
        .collect();
    IntroScene { walls, area }
}

/// Sets the intro up in place of the map's plain walls (`map_walls`, not
/// spawned): spawns the intro's walls, the room that switches them to the
/// map's, the cutscene and the oval in sight. Portal walls and everything
/// else are already in place; the intro keeps off their cells.
pub(super) fn begin_intro(
    commands: &mut Commands,
    grid: &mut MapGrid,
    layout: IntroLayout,
    cells: &[SpawnCell],
    map_walls: Vec<IVec2>,
) {
    let occupied: HashSet<IVec2> = cells
        .iter()
        .filter(|cell| cell.kind != SpawnKind::Wall)
        .map(|cell| cell.pos)
        .collect();
    let player = cells
        .iter()
        .find(|cell| cell.kind == SpawnKind::Player)
        .map_or(IVec2::new(grid.width, grid.height) / 2, |cell| cell.pos);
    let scene = intro_scene(layout, grid.width, grid.height, player, |p| {
        occupied.contains(&p)
    });
    for &p in &scene.walls {
        let wall = commands.spawn((wall_bundle(p), Rising { from: p })).id();
        grid.set(p, wall);
    }
    let room = Room::new(
        IVec2::ZERO,
        IVec2::new(grid.width, grid.height),
        vec![scene.walls, map_walls],
    );
    commands.spawn((
        room,
        IntroRoom {
            hold: Timer::from_seconds(HOLD_SECONDS, TimerMode::Once),
            zoom: 1.0,
        },
    ));
    commands.insert_resource(Cutscene);
    commands.insert_resource(scene.area);
}

/// Points the camera at the oval in sight, from as far out as it zooms.
fn frame_intro(
    camera: Option<Res<ViewCamera>>,
    operator: Option<ResMut<CameraOperator>>,
    area: Option<Res<RevealArea>>,
    mut rooms: Query<&mut IntroRoom>,
) {
    let (Some(camera), Some(mut operator), Some(area)) = (camera, operator, area) else {
        return;
    };
    for mut intro in &mut rooms {
        intro.zoom = operator.target_zoom();
        operator.look_at(CameraTarget::Point(area.centre), &camera);
        operator.zoom_to(ZOOM_RANGE.0);
        operator.snap();
    }
}

/// Asks for the map's layout once the title has stood long enough.
fn switch_intro(time: Res<Time>, mut rooms: Query<(&mut Room, &mut IntroRoom)>) {
    for (mut room, mut intro) in &mut rooms {
        if intro.hold.tick(time.delta()).just_finished() {
            room.target = 1;
        }
    }
}

/// Ends the intro once its room has switched and nothing still animates:
/// the room goes (no plate switches it back), the cutscene ends, the oval
/// in sight starts closing and the camera goes back to the player.
fn end_intro(
    mut commands: Commands,
    pacing: Res<TurnPacing>,
    area: Option<Res<RevealArea>>,
    camera: Option<Res<ViewCamera>>,
    mut operator: Option<ResMut<CameraOperator>>,
    rooms: Query<(Entity, &Room, &IntroRoom)>,
) {
    for (entity, room, intro) in &rooms {
        let animating = pacing
            .animation()
            .is_some_and(|remaining| remaining > Duration::ZERO);
        if room.current != 1 || room.target != 1 || animating {
            continue;
        }
        commands.entity(entity).despawn();
        commands.remove_resource::<Cutscene>();
        if let Some(area) = area.as_deref() {
            commands.insert_resource(IntroClosing {
                from: *area,
                timer: Timer::from_seconds(CLOSE_SECONDS, TimerMode::Once),
            });
        }
        if let (Some(camera), Some(operator)) = (camera.as_deref(), operator.as_deref_mut()) {
            let transition = operator.transition;
            operator.transition = CLOSE_SECONDS;
            operator.look_at(CameraTarget::Player, camera);
            operator.zoom_to(intro.zoom);
            operator.transition = transition;
        }
    }
}

/// Closes the oval in sight onto the player (where they are shown, as they
/// may already be walking) until nothing of it is left but what they see.
fn close_intro(
    mut commands: Commands,
    time: Res<Time>,
    mut closing: ResMut<IntroClosing>,
    area: Option<ResMut<RevealArea>>,
    players: Query<(&Pos, Option<&AnimatedPos>), With<Player>>,
) {
    closing.timer.tick(time.delta());
    if closing.timer.is_finished() {
        commands.remove_resource::<RevealArea>();
        commands.remove_resource::<IntroClosing>();
        return;
    }
    let from = closing.from;
    let player = players.single().map_or(from.centre, |(pos, shown)| {
        shown.map_or(pos.0.as_vec2(), |shown| shown.position)
    });
    let t = EaseFunction::SmoothStep.sample_clamped(closing.timer.fraction());
    if let Some(mut area) = area {
        *area = RevealArea {
            centre: from.centre.lerp(player, t),
            radii: from.radii * (1.0 - t),
        };
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
