//! Who is who and what they fight with: kinds of actor (`assets/actors`),
//! weapons (`assets/weapons`) and fighting styles (`assets/styles`), one
//! file each, read as catalogs (`content::catalog`) and naming each other
//! by id. Plain data; what to spawn from it is the game's business.

use bevy::math::IVec2;
use serde::Deserialize;

use super::art;
use crate::model::{CharacterId, EffectId, StyleId, Swing, WeaponId};

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
    /// (an effect, by its `EffectId`; none when not given).
    #[serde(default)]
    pub swing: Option<SwingSpec>,
    #[serde(default)]
    pub streak: Option<EffectId>,
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
pub struct StyleSpec {
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
    pub streak: Option<EffectId>,
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

/// A kind of actor (its name is its file's).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct ActorSpec {
    /// Optional static sprite look; absent keeps the text representation.
    #[serde(default)]
    pub sprite: Option<CharacterId>,
    /// The map glyph that places it: a lowercase letter without a meaning
    /// of its own in maps. The player has none (maps place it with `p`).
    #[serde(default)]
    pub glyph: Option<char>,
    /// The behaviour it runs, if any.
    #[serde(default)]
    pub tree: Option<Tree>,
    pub hp: i32,
    /// Its weapon or its fighting style, if any.
    #[serde(default)]
    pub weapon: Option<WeaponId>,
    #[serde(default)]
    pub style: Option<StyleId>,
    /// What a blow knocks out of it (an effect, by its `EffectId`; nothing
    /// when not given).
    #[serde(default)]
    pub bleeds: Option<EffectId>,
    /// Seconds a move of its that would attack is held, shown as it would
    /// be made, before it is made (a cancel drops it); made at once when
    /// not given.
    #[serde(default)]
    pub readies: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::data;

    #[test]
    fn weapons_styles_and_kinds_are_read_by_the_generic_reader() {
        let sweep: WeaponSpec = data::read(
            "sweep",
            "(damage: 1, windup: 2, art: \"\n    123\n    .@.\n\")",
        )
        .unwrap();
        assert_eq!((sweep.damage, sweep.windup), (1, 2));
        assert_eq!(
            sweep.cells().unwrap(),
            vec![IVec2::new(1, -1), IVec2::new(1, 0), IVec2::new(1, 1)]
        );
        let back: MoveSpec = data::read(
            "back off",
            "(name: \"back off\", stance: Step, art: \"x\n@\")",
        )
        .unwrap();
        assert_eq!(
            (back.hit, back.knock, back.stance),
            (0, 0, StanceSpec::Step)
        );
        let left: MoveSpec = data::read(
            "sweep left",
            "(name: \"sweep left\", hit: 1, turn: true, art: \"21\n3@\")",
        )
        .unwrap();
        assert!(left.turn);
        assert_eq!(
            left.cells().unwrap(),
            vec![IVec2::X, IVec2::new(1, -1), IVec2::new(0, -1)],
            "ahead, ahead-left, left"
        );
        let guard: ActorSpec = data::read(
            "guard",
            "(glyph: 'e', tree: Enemy, hp: 3, weapon: Sweep, bleeds: Blood)",
        )
        .unwrap();
        assert_eq!(guard.tree, Some(Tree::Enemy));
        assert_eq!(guard.weapon, Some(WeaponId::Sweep));
        assert_eq!(guard.bleeds, Some(EffectId::Blood));
    }
}
