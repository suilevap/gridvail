//! The actor roster, `assets/rules/actors.ron`: kinds of actor by map glyph,
//! the weapons they strike with and the moves they make. Plain data, read
//! with the generic `data::read`; what to spawn from it is the game's
//! business.

use std::collections::BTreeMap;

use bevy::math::IVec2;
use serde::Deserialize;

use super::art;
use crate::model::Swing;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Roster {
    pub weapons: BTreeMap<String, WeaponSpec>,
    pub moves: BTreeMap<String, MovesSpec>,
    /// By the map glyph that places them.
    pub actors: BTreeMap<char, ActorSpec>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct WeaponSpec {
    pub damage: i32,
    /// Turns between aiming and the strike.
    pub windup: u8,
    /// The cells struck, drawn with the wielder `@` facing up and each
    /// struck cell a digit in the order the swing goes through them (or
    /// all `x`).
    pub art: String,
    /// How it is swung (a thrust when not given), and the streak it leaves
    /// (an effect in `assets/effects`; none when not given).
    #[serde(default)]
    pub swing: Option<SwingSpec>,
    #[serde(default)]
    pub streak: Option<String>,
}

impl WeaponSpec {
    /// The struck cells in the wielder's frame, in order.
    pub fn cells(&self) -> Result<Vec<IVec2>, String> {
        frame_cells(&self.art)
    }
}

/// The cells of art drawn with `@` facing up, in order, in the frame of a
/// facing: `x` ahead, `y` to its right side.
fn frame_cells(drawn: &str) -> Result<Vec<IVec2>, String> {
    // Up the art is ahead; right of the wielder is its side.
    let cells = art::ordered(drawn, '@')?;
    Ok(cells.into_iter().map(|c| IVec2::new(-c.y, c.x)).collect())
}

/// One move for each way a swipe can go from where the mover faces.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct MovesSpec {
    pub forward: MoveSpec,
    pub left: MoveSpec,
    pub right: MoveSpec,
    pub back: MoveSpec,
}

/// Where a move leaves the mover.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StanceSpec {
    /// Where it was.
    #[default]
    Stay,
    /// Into the cell the first target it knocks leaves.
    Follow,
    /// One step the way of the swipe.
    Step,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct MoveSpec {
    pub name: String,
    /// The hit's damage (0: no hit).
    #[serde(default)]
    pub hit: i32,
    /// The cells it plays on, drawn as for weapons: it plays only when one
    /// holds a target, and hits them in order.
    pub art: String,
    /// It stops at the first target it hits, instead of going through them
    /// all.
    #[serde(default)]
    pub stop: bool,
    /// How far each target is knocked, in cells, the way the swing goes
    /// through its cell (0: not at all).
    #[serde(default)]
    pub knock: i32,
    /// Where it leaves the mover, and whether the mover turns to face the
    /// way of the swipe.
    #[serde(default)]
    pub stance: StanceSpec,
    #[serde(default)]
    pub turn: bool,
    /// How it is swung (a thrust when not given), and the streak it leaves
    /// (as for weapons).
    #[serde(default)]
    pub swing: Option<SwingSpec>,
    #[serde(default)]
    pub streak: Option<String>,
}

impl MoveSpec {
    /// The cells it plays on in the mover's frame, in order.
    pub fn cells(&self) -> Result<Vec<IVec2>, String> {
        frame_cells(&self.art)
    }
}

/// How a weapon or a move is swung, `reach` cells out: `Thrust` straight
/// at the target, `Sweep` in an arc across the front, `Hook` out and
/// pulled back.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum SwingSpec {
    Thrust { reach: f32 },
    Sweep { reach: f32 },
    Hook { reach: f32 },
}

impl SwingSpec {
    pub fn swing(self) -> Swing {
        match self {
            Self::Thrust { reach } => Swing::Thrust { reach },
            Self::Sweep { reach } => Swing::Sweep { reach },
            Self::Hook { reach } => Swing::Hook { reach },
        }
    }
}

/// The behaviour an actor runs.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tree {
    /// Watches, hunts, wanders.
    Enemy,
    /// Follows its order, fetching keys.
    Hunter,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ActorSpec {
    pub name: String,
    /// The behaviour it runs, if any.
    #[serde(default)]
    pub tree: Option<Tree>,
    pub hp: i32,
    #[serde(default)]
    pub weapon: Option<String>,
    #[serde(default)]
    pub moves: Option<String>,
    /// What a blow knocks out of it (an effect in `assets/effects`;
    /// nothing when not given).
    #[serde(default)]
    pub bleeds: Option<String>,
    /// The figure it is drawn as where height shows
    /// (`assets/rules/figures.txt`); its glyph if none.
    #[serde(default)]
    pub figure: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::data;

    const ROSTER: &str = r#"(
        weapons: {
            "sweep": (damage: 1, windup: 2, art: "
                123
                .@.
            "),
        },
        moves: {
            "blade": (
                forward: (name: "lunge", hit: 1, knock: 1, stance: Follow, art: "
                    1
                    @
                "),
                left: (name: "sweep left", hit: 1, knock: 1, turn: true, art: "
                    21
                    3@
                "),
                right: (name: "sweep right", hit: 1, knock: 1, turn: true, art: "
                    12
                    @3
                "),
                back: (name: "back off", stance: Step, art: "
                    x
                    @
                "),
            ),
        },
        actors: {
            'e': (name: "guard", tree: Enemy, hp: 3, weapon: "sweep"),
            'p': (name: "player", hp: 2, moves: "blade"),
        },
    )"#;

    #[test]
    fn a_roster_is_read_by_the_generic_reader() {
        let roster: Roster = data::read("roster", ROSTER).unwrap();
        let sweep = &roster.weapons["sweep"];
        assert_eq!((sweep.damage, sweep.windup), (1, 2));
        assert_eq!(
            sweep.cells().unwrap(),
            vec![IVec2::new(1, -1), IVec2::new(1, 0), IVec2::new(1, 1)]
        );
        let blade = &roster.moves["blade"];
        assert_eq!((blade.back.hit, blade.back.knock), (0, 0));
        assert_eq!(blade.back.stance, StanceSpec::Step);
        assert_eq!(blade.forward.stance, StanceSpec::Follow);
        assert!(blade.left.turn && !blade.forward.turn);
        assert_eq!(
            blade.left.cells().unwrap(),
            vec![IVec2::X, IVec2::new(1, -1), IVec2::new(0, -1)],
            "ahead, ahead-left, left"
        );
        let guard = &roster.actors[&'e'];
        assert_eq!(guard.tree, Some(Tree::Enemy));
        assert_eq!(guard.weapon.as_deref(), Some("sweep"));
        assert_eq!(roster.actors[&'p'].moves.as_deref(), Some("blade"));
    }
}
