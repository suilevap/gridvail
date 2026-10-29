//! Kinds of actor, by map glyph, from the roster in
//! `assets/rules/actors.ron`: what each is made of when spawned.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use flatbt_bevy::prelude::Behavior;

use crate::ai::{enemy_tree, hunter_tree};
pub use crate::content::actors::Tree;
use crate::content::actors::{MoveSpec, Roster};
use crate::content::data;
use crate::model::*;

const ROSTER_TEXT: &str = include_str!("../../assets/rules/actors.ron");

/// One kind of actor: its stats and what it fights with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorKind {
    pub name: String,
    pub tree: Option<Tree>,
    pub hp: i32,
    pub weapon: Option<Weapon>,
    pub maneuvers: Option<Maneuvers>,
}

impl ActorKind {
    /// Gives `actor` what this kind is made of beyond its body: health, a
    /// weapon, moves, a behaviour.
    pub fn equip(&self, actor: &mut EntityCommands) {
        actor.insert(Health::full(self.hp));
        if let Some(weapon) = &self.weapon {
            actor.insert(weapon.clone());
        }
        if let Some(maneuvers) = &self.maneuvers {
            actor.insert(maneuvers.clone());
        }
        match self.tree {
            Some(Tree::Enemy) => {
                actor.insert((EnemyMind::default(), Behavior::for_tree(enemy_tree)));
            }
            Some(Tree::Hunter) => {
                actor.insert((
                    Hunter,
                    Order::default(),
                    EnemyMind::default(),
                    Behavior::for_tree(hunter_tree),
                ));
            }
            None => {}
        }
    }
}

/// Every kind of actor, by map glyph. The bundled roster by default; change
/// it (or insert another) before the map is built to change who is who.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ActorKinds(pub HashMap<char, ActorKind>);

impl ActorKinds {
    /// The kinds a roster describes; every weapon and move set an actor
    /// names must be in it.
    pub fn from_roster(roster: &Roster) -> Result<Self, String> {
        let move_of = |spec: &MoveSpec| Maneuver {
            name: spec.name.clone(),
            hit: spec.hit,
            knock: spec.knock,
            follow: spec.follow,
            swap: spec.swap,
        };
        let mut kinds = HashMap::default();
        for (&glyph, actor) in &roster.actors {
            let weapon = match actor.weapon.as_deref() {
                None => None,
                Some(name) => {
                    let spec = roster
                        .weapons
                        .get(name)
                        .ok_or_else(|| format!("{} names no known weapon `{name}`", actor.name))?;
                    let cells = spec
                        .cells()
                        .map_err(|error| format!("weapon `{name}`: {error}"))?;
                    Some(Weapon {
                        area: Area(cells),
                        damage: spec.damage,
                        windup: spec.windup,
                    })
                }
            };
            let maneuvers = match actor.moves.as_deref() {
                None => None,
                Some(name) => {
                    let spec = roster
                        .moves
                        .get(name)
                        .ok_or_else(|| format!("{} names no known moves `{name}`", actor.name))?;
                    Some(Maneuvers {
                        forward: move_of(&spec.forward),
                        side: move_of(&spec.side),
                        back: move_of(&spec.back),
                    })
                }
            };
            kinds.insert(
                glyph,
                ActorKind {
                    name: actor.name.clone(),
                    tree: actor.tree,
                    hp: actor.hp,
                    weapon,
                    maneuvers,
                },
            );
        }
        Ok(Self(kinds))
    }

    /// The kind spawned for `glyph`; a map naming one the roster lacks
    /// cannot be played.
    pub fn get(&self, glyph: char) -> &ActorKind {
        self.0
            .get(&glyph)
            .unwrap_or_else(|| panic!("map: no actor kind for `{glyph}` in the roster"))
    }

    /// The player's kind (`p`).
    pub fn player_mut(&mut self) -> &mut ActorKind {
        self.0
            .get_mut(&'p')
            .expect("the roster has the player, `p`")
    }
}

impl Default for ActorKinds {
    fn default() -> Self {
        data::read::<Roster>("actors.ron", ROSTER_TEXT)
            .and_then(|roster| Self::from_roster(&roster))
            .unwrap_or_else(|error| panic!("actors: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_roster_has_the_player_and_armed_enemies() {
        let mut kinds = ActorKinds::default();
        assert!(kinds.player_mut().maneuvers.is_some());
        for glyph in ['e', 'h', 's'] {
            let kind = kinds.get(glyph);
            assert!(
                kind.tree.is_some() && kind.weapon.is_some(),
                "{}",
                kind.name
            );
        }
    }
}
