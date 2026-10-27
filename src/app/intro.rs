//! The game's intro: what its first map comes together from. It only picks
//! a layout source (`layout`: the title as text art, or noise) and asks for
//! its walls to be shown in their frame first (a [`NextArrival`]); how they
//! become the map is the transition's job (`transition`).

use bevy::prelude::*;

use super::map::MapText;
use crate::content::map::{parse_map, SpawnKind};
use crate::foundation::oval::Oval;
use crate::layout::{Art, ArtLayout, Fallback, LayoutRequest, LayoutSource, ScatterLayout};
use crate::model::RevealArea;
use crate::schedule::StartupPhase;
use crate::transition::{Arrival, NextArrival};

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

impl IntroLayout {
    /// The layout source it stands for. The title falls back to noise on a
    /// map too small for it.
    pub fn source(self) -> Box<dyn LayoutSource> {
        let noise = ScatterLayout {
            radius: NOISE_RADIUS,
            density: NOISE_DENSITY,
            seed: NOISE_SEED,
        };
        match self {
            Self::Title => Box::new(Fallback {
                first: ArtLayout::new(Art::parse(TITLE), TITLE_MARGIN),
                then: noise,
            }),
            Self::Noise => Box::new(noise),
        }
    }
}

/// Plays the [`Intro`], if there is one, when the game starts.
pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, ask_for_intro.in_set(StartupPhase::Content));
    }
}

/// Asks for the first map to come together from the intro's walls.
fn ask_for_intro(mut commands: Commands, intro: Option<Res<Intro>>, map: Res<MapText>) {
    if let Some(intro) = intro {
        commands.insert_resource(NextArrival(intro_arrival(intro.0, map.0)));
    }
}

/// What the map `text` comes together from with `layout`, around its start
/// (or its middle): its walls, shown in their frame. A layout that cannot
/// be laid out arrives as nothing to show.
pub fn intro_arrival(layout: IntroLayout, text: &str) -> Arrival {
    let (width, height, cells) = parse_map(text);
    let bounds = IVec2::new(width, height);
    let centre = cells
        .iter()
        .find(|cell| cell.kind == SpawnKind::Player)
        .map_or(bounds / 2, |cell| cell.pos);
    let request = LayoutRequest { bounds, centre };
    let (walls, frame) = layout
        .source()
        .layout(&request)
        .map_or((Vec::new(), None), |laid| (laid.walls, Some(laid.frame)));
    let oval = frame.unwrap_or(Oval {
        centre: centre.as_vec2(),
        radii: Vec2::ZERO,
    });
    let area = RevealArea {
        oval,
        hides_sight: true,
    };
    Arrival::Shown { walls, area }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(arrival: Arrival) -> (Vec<IVec2>, RevealArea) {
        match arrival {
            Arrival::Shown { walls, area } => (walls, area),
            Arrival::Standing(_) => panic!("the intro shows its walls"),
        }
    }

    #[test]
    fn the_title_is_in_sight_and_little_else() {
        let map1 = include_str!("../../assets/maps/map1.txt");
        let (walls, area) = shown(intro_arrival(IntroLayout::Title, map1));
        assert_eq!(walls.len(), crate::layout::Art::parse(TITLE).walls.len());
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
        assert_eq!(area.oval.centre, Vec2::new(4.0, 1.0));
        assert!(walls.iter().all(|&p| area.contains(p)));
        assert_eq!(
            shown(intro_arrival(IntroLayout::Noise, small)).0,
            walls,
            "the same noise"
        );
    }
}
