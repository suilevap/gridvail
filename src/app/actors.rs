//! Kinds of actor, by map glyph, from the catalogs of actors, weapons and
//! fighting styles (`assets/actors`, `assets/weapons`, `assets/moves`):
//! what each is made of when spawned.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use flatbt_bevy::prelude::Behavior;

use crate::ai::{enemy_tree, hunter_tree};
pub use crate::content::actors::Tree;
use crate::content::actors::{ActorSpec, MoveSpec, MovesSpec, StanceSpec, SwingSpec, WeaponSpec};
use crate::content::catalog::Catalog;
use crate::model::*;

/// One kind of actor: its stats and what it fights with.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorKind {
    pub name: String,
    pub tree: Option<Tree>,
    pub hp: i32,
    pub weapon: Option<(Weapon, WeaponLook)>,
    pub maneuvers: Option<(Maneuvers, ManeuverLooks)>,
    pub bleeds: Option<EffectId>,
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
        if let Some(effect) = self.bleeds {
            actor.insert(Bleeds(effect));
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

/// Every kind of actor, by map glyph. The bundled catalogs by default;
/// change it (or insert another) before the map is built to change who is
/// who.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ActorKinds(pub HashMap<char, ActorKind>);

impl ActorKinds {
    /// The kinds the actors catalog describes, with the weapons and
    /// fighting styles they name, which must be in theirs; no two kinds
    /// share a glyph.
    pub fn from_catalogs(
        actors: &Catalog<ActorId, ActorSpec>,
        weapons: &Catalog<WeaponId, WeaponSpec>,
        styles: &Catalog<MovesId, MovesSpec>,
    ) -> Result<Self, String> {
        let move_of = |spec: &MoveSpec| -> Result<Maneuver, String> {
            let cells = spec
                .cells()
                .map_err(|error| format!("move `{}`: {error}", spec.name))?;
            Ok(Maneuver {
                name: spec.name.clone(),
                area: Area(cells),
                stop: spec.stop,
                hit: spec.hit,
                knock: spec.knock,
                stance: match spec.stance {
                    StanceSpec::Stay => Stance::Stay,
                    StanceSpec::Follow => Stance::Follow,
                    StanceSpec::Step => Stance::Step,
                },
                turn: spec.turn,
            })
        };
        let look_of = |swing: Option<SwingSpec>, streak: Option<EffectId>| SwingLook {
            swing: swing.map(SwingSpec::swing),
            streak,
        };
        let mut kinds = HashMap::default();
        for (id, actor) in actors.iter() {
            let name = id.file();
            let weapon = match actor.weapon {
                None => None,
                Some(weapon) => {
                    let spec = weapons
                        .get(weapon)
                        .ok_or_else(|| format!("{name} names no known weapon {weapon:?}"))?;
                    let cells = spec
                        .cells()
                        .map_err(|error| format!("weapon {weapon:?}: {error}"))?;
                    Some((
                        Weapon {
                            area: Area(cells),
                            damage: spec.damage,
                            windup: spec.windup,
                        },
                        // A weapon is swung, a thrust when not said.
                        WeaponLook(SwingLook {
                            swing: Some(spec.swing.map(SwingSpec::swing).unwrap_or_default()),
                            ..look_of(None, spec.streak)
                        }),
                    ))
                }
            };
            let maneuvers = match actor.moves {
                None => None,
                Some(style) => {
                    let spec = styles
                        .get(style)
                        .ok_or_else(|| format!("{name} names no known moves {style:?}"))?;
                    Some((
                        Maneuvers {
                            forward: move_of(&spec.forward)?,
                            left: move_of(&spec.left)?,
                            right: move_of(&spec.right)?,
                            back: move_of(&spec.back)?,
                        },
                        ManeuverLooks {
                            forward: look_of(spec.forward.swing, spec.forward.streak),
                            left: look_of(spec.left.swing, spec.left.streak),
                            right: look_of(spec.right.swing, spec.right.streak),
                            back: look_of(spec.back.swing, spec.back.streak),
                        },
                    ))
                }
            };
            let kind = ActorKind {
                name: name.to_string(),
                tree: actor.tree,
                hp: actor.hp,
                weapon,
                maneuvers,
                bleeds: actor.bleeds,
            };
            if let Some(other) = kinds.insert(actor.glyph, kind) {
                return Err(format!(
                    "{name} and {} share the glyph `{}`",
                    other.name, actor.glyph
                ));
            }
        }
        Ok(Self(kinds))
    }

    /// The kind spawned for `glyph`; a map naming one no kind has
    /// cannot be played.
    pub fn get(&self, glyph: char) -> &ActorKind {
        self.0
            .get(&glyph)
            .unwrap_or_else(|| panic!("map: no actor kind for `{glyph}` in assets/actors"))
    }

    /// The player's kind (`p`).
    pub fn player_mut(&mut self) -> &mut ActorKind {
        self.0
            .get_mut(&'p')
            .expect("assets/actors has the player, `p`")
    }

    /// Changes every kind but the player's.
    pub fn others_mut(&mut self, mut change: impl FnMut(&mut ActorKind)) {
        for (_, kind) in self.0.iter_mut().filter(|(glyph, _)| **glyph != 'p') {
            change(kind);
        }
    }
}

impl Default for ActorKinds {
    fn default() -> Self {
        Catalog::bundled()
            .and_then(|actors| {
                Self::from_catalogs(&actors, &Catalog::bundled()?, &Catalog::bundled()?)
            })
            .unwrap_or_else(|error| panic!("actors: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::catalog::Bundled;

    #[test]
    fn the_bundled_kinds_have_the_player_and_armed_enemies() {
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

    #[test]
    fn effects_are_named_by_their_ids_in_data() {
        let guard = ActorKinds::default().get('e').clone();
        assert_eq!(guard.bleeds, Some(EffectId::Blood));
        let (_, look) = guard.weapon.unwrap();
        assert_eq!(look.0.streak, Some(EffectId::Swoosh));
        // A name nobody wrote fails to read.
        let text =
            ActorId::TEXTS[ActorId::Guard.index()].replacen("bleeds: Blood", "bleeds: Bloood", 1);
        let error =
            Catalog::<ActorId, ActorSpec>::read([(ActorId::Guard, text.as_str())]).unwrap_err();
        assert!(error.contains("Bloood"), "{error}");
    }

    #[test]
    fn no_two_kinds_share_a_glyph() {
        let actors = Catalog::read([
            (ActorId::Guard, "(glyph: 'e', hp: 2)"),
            (ActorId::Spearman, "(glyph: 'e', hp: 2)"),
        ])
        .unwrap();
        let error = ActorKinds::from_catalogs(&actors, &Catalog::default(), &Catalog::default())
            .unwrap_err();
        assert!(error.contains("glyph"), "{error}");
    }
}
