//! The game's intro: what its first map comes together from. It only picks
//! the walls and the area to show them in (an [`Arrival::Shown`]); how they
//! become the map is the transition's job (`transition`), and the pieces
//! are general: text art (`content::art`) and scattered cells
//! (`foundation::scatter`).

use std::f32::consts::SQRT_2;

use bevy::prelude::*;

use super::transition::Arrival;
use crate::content::art::Art;
use crate::content::map::{parse_map, SpawnKind};
use crate::foundation::scatter::scatter_in_oval;
use crate::model::RevealArea;

/// The game's title, as text art.
const TITLE: &str = include_str!("../../assets/intro.txt");
/// Cells of map in sight around the title's oval, beyond its corners.
const TITLE_MARGIN: f32 = 1.5;
/// How far from the player noise reaches, in cells (also its oval).
const NOISE_RADIUS: f32 = 10.0;
/// Of the cells noise may use, the share that become walls.
const NOISE_DENSITY: f64 = 0.3;
/// Noise is the same every time.
const NOISE_SEED: u64 = 7;

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

/// What the map `text` comes together from with `layout`, around its start
/// (or its middle).
pub fn intro_arrival(layout: IntroLayout, text: &str) -> Arrival {
    let (width, height, cells) = parse_map(text);
    let bounds = IVec2::new(width, height);
    let player = cells
        .iter()
        .find(|cell| cell.kind == SpawnKind::Player)
        .map_or(bounds / 2, |cell| cell.pos);
    match layout {
        IntroLayout::Title => art_arrival(&Art::parse(TITLE), bounds, player),
        IntroLayout::Noise => None,
    }
    .unwrap_or_else(|| noise_arrival(bounds, player))
}

/// `art` centred on `centre` inside the map, shown in the oval through its
/// corners (plus a margin); `None` if it does not fit.
fn art_arrival(art: &Art, bounds: IVec2, centre: IVec2) -> Option<Arrival> {
    let corner = art.corner_near(centre, bounds)?;
    // An oval through a rectangle's corners is √2 times its half size.
    let area = RevealArea {
        centre: corner.as_vec2() + (art.size - 1).as_vec2() / 2.0,
        radii: art.size.as_vec2() / 2.0 * SQRT_2 + TITLE_MARGIN,
        hides_sight: true,
    };
    let walls = art.walls.iter().map(|&p| corner + p).collect();
    Some(Arrival::Shown { walls, area })
}

/// Walls scattered within [`NOISE_RADIUS`] of `centre`, shown in that circle.
fn noise_arrival(bounds: IVec2, centre: IVec2) -> Arrival {
    let area = RevealArea {
        centre: centre.as_vec2(),
        radii: Vec2::splat(NOISE_RADIUS),
        hides_sight: true,
    };
    let walls = scatter_in_oval(area.centre, area.radii, bounds, NOISE_DENSITY, NOISE_SEED);
    Arrival::Shown { walls, area }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(arrival: Arrival) -> (Vec<IVec2>, RevealArea) {
        match arrival {
            Arrival::Shown { walls, area } => (walls, area),
            Arrival::Walls(_) => panic!("the intro shows its walls"),
        }
    }

    #[test]
    fn the_title_is_in_sight_and_little_else() {
        let map1 = include_str!("../../assets/maps/map1.txt");
        let (walls, area) = shown(intro_arrival(IntroLayout::Title, map1));
        assert_eq!(walls.len(), Art::parse(TITLE).walls.len());
        assert!(walls.iter().all(|&p| area.contains(p)));
        let in_sight = (0..24)
            .flat_map(|y| (0..80).map(move |x| IVec2::new(x, y)))
            .filter(|&p| area.contains(p))
            .count();
        assert!(in_sight < 80 * 24 / 2, "less than half the map: {in_sight}");
    }

    #[test]
    fn a_map_too_small_for_the_title_gets_noise_around_the_player() {
        let small = "XXXXXXXXXX\nX...p....X\nXXXXXXXXXX\n";
        let (walls, area) = shown(intro_arrival(IntroLayout::Title, small));
        assert!(!walls.is_empty());
        assert_eq!(area.centre, Vec2::new(4.0, 1.0));
        assert!(walls.iter().all(|&p| area.contains(p)));
        assert_eq!(
            shown(intro_arrival(IntroLayout::Noise, small)).0,
            walls,
            "the same noise"
        );
    }
}
