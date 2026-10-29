//! The actor roster, `assets/rules/actors.ron`: kinds of actor by map glyph,
//! the weapons they strike with and the moves they make. Plain data, read
//! with the generic `data::read`; what to spawn from it is the game's
//! business.

use std::collections::BTreeMap;

use bevy::math::IVec2;
use serde::Deserialize;

use super::art;

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Roster {
    pub weapons: BTreeMap<String, WeaponSpec>,
    pub moves: BTreeMap<String, MovesSpec>,
    /// By the map glyph that places them.
    pub actors: BTreeMap<char, ActorSpec>,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WeaponSpec {
    pub damage: i32,
    /// Turns between aiming and the strike.
    pub windup: u8,
    /// The cells struck, drawn with the wielder `@` facing up and each
    /// struck cell `x`.
    pub art: String,
}

impl WeaponSpec {
    /// The struck cells in the wielder's frame: `x` ahead, `y` to its side.
    pub fn cells(&self) -> Result<Vec<IVec2>, String> {
        // Up the art is ahead; right of the wielder is its side.
        let cells = art::cells(&self.art, '@', 'x')?;
        Ok(cells.into_iter().map(|c| IVec2::new(-c.y, c.x)).collect())
    }
}

/// One move per way the second direction relates to the first.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MovesSpec {
    pub forward: MoveSpec,
    pub side: MoveSpec,
    pub back: MoveSpec,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MoveSpec {
    pub name: String,
    /// The hit's damage (0: no hit).
    #[serde(default)]
    pub hit: i32,
    /// How far the target is knocked, in cells, the way of the second
    /// direction (0: not at all).
    #[serde(default)]
    pub knock: i32,
    /// The mover steps into the cell the target leaves.
    #[serde(default)]
    pub follow: bool,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::data;

    const ROSTER: &str = r#"(
        weapons: {
            "sweep": (damage: 1, windup: 2, art: "
                xxx
                .@.
            "),
        },
        moves: {
            "blade": (
                forward: (name: "lunge", hit: 1, knock: 1, follow: true),
                side: (name: "push", hit: 1, knock: 1),
                back: (name: "yank", knock: 1, follow: true),
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
        assert_eq!((blade.back.hit, blade.back.knock), (0, 1));
        assert!(blade.forward.follow && !blade.side.follow);
        let guard = &roster.actors[&'e'];
        assert_eq!(guard.tree, Some(Tree::Enemy));
        assert_eq!(guard.weapon.as_deref(), Some("sweep"));
        assert_eq!(roster.actors[&'p'].moves.as_deref(), Some("blade"));
    }
}
