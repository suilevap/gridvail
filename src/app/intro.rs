//! The intro: the map's walls come together from the game's title (or
//! noise) when a map is first loaded.
//!
//! It is an ordinary room switch: a room covering the whole map whose first
//! layout is the title and whose second is the map's own walls. The title
//! rises out of the floor and stands for [`HOLD_SECONDS`]; then the room
//! switches, so its walls slide, rise and sink like any room's, with
//! whatever room planner is the default. While it plays it is a
//! [`Cutscene`], the whole map is in sight ([`RevealAll`]) and the camera
//! looks at the title from as far out as it zooms; once every wall has
//! settled, the room goes, the camera glides back to the player and the
//! game starts.

use std::collections::HashSet;
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
/// Of the cells noise may use, the share that become walls.
const NOISE_DENSITY: f64 = 0.3;
/// Noise is the same every time.
const NOISE_SEED: u64 = 7;
/// How long the title stands before the map comes together, rising
/// included.
pub const HOLD_SECONDS: f32 = 1.5;
/// How long the camera takes to glide back to the player afterwards.
const RETURN_SECONDS: f32 = 0.8;

/// What the map's walls come together from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroLayout {
    /// The game's title (`assets/intro.txt`), centred on the player; noise
    /// on a map too small for it.
    Title,
    /// Walls scattered at random.
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

/// The room the intro switches. `revealed` is whether the intro put the
/// whole map in sight, to take it away again; `hold` is how long the title
/// still stands; `centre` is the title's middle, which the camera looks at,
/// and `zoom` the camera's zoom before, to go back to.
#[derive(Component, Clone, Debug)]
pub struct IntroRoom {
    revealed: bool,
    hold: Timer,
    centre: Vec2,
    zoom: f32,
}

pub(super) struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, frame_intro.in_set(StartupPhase::Derive))
            .add_systems(Update, switch_intro.in_set(GamePhase::Simulation))
            .add_systems(Update, end_intro.in_set(GamePhase::Finalize));
    }
}

/// The intro's walls on a `width` x `height` map: the title centred on
/// `player` and kept inside the map (noise if it does not fit), or noise,
/// never on a cell `blocked` says something else spawns on.
pub fn intro_walls(
    layout: IntroLayout,
    width: i32,
    height: i32,
    player: IVec2,
    blocked: impl Fn(IVec2) -> bool,
) -> Vec<IVec2> {
    let walls = match layout {
        IntroLayout::Title => title_walls(TITLE, width, height, player),
        IntroLayout::Noise => None,
    };
    let walls = walls.unwrap_or_else(|| noise_walls(width, height));
    walls.into_iter().filter(|&p| !blocked(p)).collect()
}

/// `art`'s walls centred on `centre`, moved to lie inside the map; `None`
/// when it is larger than the map.
fn title_walls(art: &str, width: i32, height: i32, centre: IVec2) -> Option<Vec<IVec2>> {
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
    Some(
        rows.iter()
            .enumerate()
            .flat_map(|(y, row)| {
                row.chars().enumerate().filter_map(move |(x, c)| {
                    matches!(c, 'X' | 'x').then_some(corner + IVec2::new(x as i32, y as i32))
                })
            })
            .collect(),
    )
}

fn noise_walls(width: i32, height: i32) -> Vec<IVec2> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(NOISE_SEED);
    (0..height)
        .flat_map(|y| (0..width).map(move |x| IVec2::new(x, y)))
        .filter(|_| rng.random_bool(NOISE_DENSITY))
        .collect()
}

/// Sets the intro up in place of the map's plain walls (`map_walls`, not
/// spawned): spawns the intro's walls, the room that switches them to the
/// map's, and the cutscene. Portal walls and everything else are already
/// in place; the intro keeps off their cells.
pub(super) fn begin_intro(
    commands: &mut Commands,
    grid: &mut MapGrid,
    layout: IntroLayout,
    cells: &[SpawnCell],
    map_walls: Vec<IVec2>,
    already_revealed: bool,
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
    let walls = intro_walls(layout, grid.width, grid.height, player, |p| {
        occupied.contains(&p)
    });
    for &p in &walls {
        let wall = commands.spawn((wall_bundle(p), Rising { from: p })).id();
        grid.set(p, wall);
    }
    let centre = match (
        walls.iter().copied().reduce(IVec2::min),
        walls.iter().copied().reduce(IVec2::max),
    ) {
        (Some(min), Some(max)) => (min + max).as_vec2() / 2.0,
        _ => player.as_vec2(),
    };
    let room = Room::new(
        IVec2::ZERO,
        IVec2::new(grid.width, grid.height),
        vec![walls, map_walls],
    );
    commands.spawn((
        room,
        IntroRoom {
            revealed: !already_revealed,
            hold: Timer::from_seconds(HOLD_SECONDS, TimerMode::Once),
            centre,
            zoom: 1.0,
        },
    ));
    commands.insert_resource(Cutscene);
    if !already_revealed {
        commands.insert_resource(RevealAll);
    }
}

/// Points the camera at the title, from as far out as it zooms.
fn frame_intro(
    camera: Option<Res<ViewCamera>>,
    operator: Option<ResMut<CameraOperator>>,
    mut rooms: Query<&mut IntroRoom>,
) {
    let (Some(camera), Some(mut operator)) = (camera, operator) else {
        return;
    };
    for mut intro in &mut rooms {
        intro.zoom = operator.target_zoom();
        operator.look_at(CameraTarget::Point(intro.centre), &camera);
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
/// the room goes (no plate switches it back), the cutscene and the intro's
/// view of the whole map end, and the camera goes back to the player.
fn end_intro(
    mut commands: Commands,
    pacing: Res<TurnPacing>,
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
        if intro.revealed {
            commands.remove_resource::<RevealAll>();
        }
        if let (Some(camera), Some(operator)) = (camera.as_deref(), operator.as_deref_mut()) {
            let transition = operator.transition;
            operator.transition = RETURN_SECONDS;
            operator.look_at(CameraTarget::Player, camera);
            operator.zoom_to(intro.zoom);
            operator.transition = transition;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_is_centred_on_the_player_and_kept_inside_the_map() {
        let art = "X.X\n.X.";
        let walls = |centre| title_walls(art, 10, 6, centre).unwrap();
        assert_eq!(
            walls(IVec2::new(5, 3)),
            [IVec2::new(4, 2), IVec2::new(6, 2), IVec2::new(5, 3)]
        );
        // Near a corner it moves inside the map.
        assert_eq!(
            walls(IVec2::ZERO),
            [IVec2::new(0, 0), IVec2::new(2, 0), IVec2::new(1, 1)]
        );
        assert!(title_walls(art, 2, 6, IVec2::ZERO).is_none(), "too small");
    }

    #[test]
    fn a_map_too_small_for_the_title_gets_noise_and_nothing_lands_on_spawns() {
        let player = IVec2::new(2, 2);
        let walls = intro_walls(IntroLayout::Title, 10, 5, player, |p| p == player);
        assert!(!walls.is_empty());
        assert!(!walls.contains(&player));
        assert_eq!(
            walls,
            intro_walls(IntroLayout::Noise, 10, 5, player, |p| p == player)
        );
    }

    #[test]
    fn the_bundled_title_fits_the_bundled_maps() {
        assert!(title_walls(TITLE, 80, 24, IVec2::new(40, 12)).is_some());
    }
}
