//! Kinds of actor, from the catalogs of actors, weapons and fighting styles
//! (`assets/actors`, `assets/weapons`, `assets/styles`): what each is made
//! of when spawned. The player's kind is `ActorId::Player`; the others are
//! placed on maps by their glyphs.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use flatbt_bevy::prelude::Behavior;

use crate::ai::{enemy_tree, hunter_tree};
pub use crate::content::actors::Tree;
use crate::content::actors::{MoveSpec, StanceSpec, StyleSpec, SwingSpec, WeaponSpec};
use crate::content::catalog::Catalog;
use crate::content::map::{spawn_kind_of, SpawnKind};
use crate::model::*;

/// The kind maps place with `p`.
const PLAYER: ActorId = ActorId::Player;

/// One kind of actor: its stats and what it fights with.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorKind {
    pub id: ActorId,
    pub tree: Option<Tree>,
    pub hp: i32,
    pub weapon: Option<(Weapon, WeaponLook)>,
    pub maneuvers: Option<(Maneuvers, ManeuverLooks)>,
    pub bleeds: Option<EffectId>,
    pub readies: Option<f32>,
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
        if let Some(secs) = self.readies {
            actor.insert(Readies(secs));
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

/// Every kind of actor: the player's, and the others by the map glyph that
/// places them. The bundled catalogs by default; change it (or insert
/// another) before the map is built to change who is who.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ActorKinds {
    pub player: ActorKind,
    pub by_glyph: HashMap<char, ActorKind>,
}

/// A weapon as its components.
fn weapon_of(spec: &WeaponSpec) -> Result<(Weapon, WeaponLook), String> {
    Ok((
        Weapon {
            area: Area(spec.cells()?),
            damage: spec.damage,
            windup: spec.windup,
        },
        // A weapon is swung, a thrust when not said.
        WeaponLook(SwingLook {
            swing: Some(spec.swing.map(SwingSpec::swing).unwrap_or_default()),
            streak: spec.streak,
        }),
    ))
}

/// A fighting style as its components.
fn style_of(spec: &StyleSpec) -> Result<(Maneuvers, ManeuverLooks), String> {
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
    let look_of = |spec: &MoveSpec| SwingLook {
        swing: spec.swing.map(SwingSpec::swing),
        streak: spec.streak,
    };
    Ok((
        Maneuvers {
            forward: move_of(&spec.forward)?,
            left: move_of(&spec.left)?,
            right: move_of(&spec.right)?,
            back: move_of(&spec.back)?,
        },
        ManeuverLooks {
            forward: look_of(&spec.forward),
            left: look_of(&spec.left),
            right: look_of(&spec.right),
            back: look_of(&spec.back),
        },
    ))
}

/// What `id` names, which must be known.
fn named<T: Clone>(id: ActorId, what: &str, found: Option<&T>) -> Result<T, String> {
    found
        .cloned()
        .ok_or_else(|| format!("{} names no known {what}", id.path()))
}

impl ActorKinds {
    /// The kinds the actors catalog describes, with the weapons and
    /// fighting styles they name. Every weapon and style must make sense
    /// (whether or not a kind names it), the player must be there, and
    /// every other kind needs its own glyph, one maps give no other
    /// meaning.
    pub fn from_catalogs(
        actors: &Catalog<ActorId>,
        weapons: &Catalog<WeaponId>,
        styles: &Catalog<StyleId>,
    ) -> Result<Self, String> {
        let weapons: HashMap<WeaponId, _> = weapons
            .iter()
            .map(|(id, spec)| {
                Ok((
                    id,
                    weapon_of(spec).map_err(|e| format!("{}: {e}", id.path()))?,
                ))
            })
            .collect::<Result<_, String>>()?;
        let styles: HashMap<StyleId, _> = styles
            .iter()
            .map(|(id, spec)| {
                Ok((
                    id,
                    style_of(spec).map_err(|e| format!("{}: {e}", id.path()))?,
                ))
            })
            .collect::<Result<_, String>>()?;
        let (mut player, mut by_glyph) = (None, HashMap::default());
        for (id, actor) in actors.iter() {
            let kind = ActorKind {
                id,
                tree: actor.tree,
                hp: actor.hp,
                weapon: match actor.weapon {
                    Some(weapon) => Some(named(id, "weapon", weapons.get(&weapon))?),
                    None => None,
                },
                maneuvers: match actor.style {
                    Some(style) => Some(named(id, "style", styles.get(&style))?),
                    None => None,
                },
                bleeds: actor.bleeds,
                readies: actor.readies,
            };
            match (id == PLAYER, actor.glyph) {
                (true, None) => player = Some(kind),
                (true, Some(_)) => {
                    return Err(format!("{}: maps place the player with `p`", id.path()));
                }
                (false, None) => return Err(format!("{} names no glyph", id.path())),
                (false, Some(glyph)) => {
                    if spawn_kind_of(glyph) != Some(SpawnKind::Actor(glyph)) {
                        return Err(format!(
                            "{}: `{glyph}` means something else in maps",
                            id.path()
                        ));
                    }
                    if let Some(other) = by_glyph.insert(glyph, kind) {
                        return Err(format!(
                            "{} and {} share the glyph `{glyph}`",
                            id.path(),
                            other.id.path()
                        ));
                    }
                }
            }
        }
        let player = player.ok_or_else(|| format!("{} is missing", PLAYER.path()))?;
        Ok(Self { player, by_glyph })
    }

    /// The kind spawned for `glyph`; a map naming one no kind has cannot be
    /// played.
    pub fn get(&self, glyph: char) -> &ActorKind {
        self.by_glyph
            .get(&glyph)
            .unwrap_or_else(|| panic!("map: no actor kind for `{glyph}` in assets/actors"))
    }

    /// The player's kind.
    pub fn player_mut(&mut self) -> &mut ActorKind {
        &mut self.player
    }

    /// Changes every kind but the player's.
    pub fn others_mut(&mut self, change: impl FnMut(&mut ActorKind)) {
        self.by_glyph.values_mut().for_each(change);
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

    /// The bundled kinds of actor, with `files` read in place of theirs.
    fn with(files: &[(ActorId, &str)]) -> Result<ActorKinds, String> {
        let bundled = ActorId::ALL
            .iter()
            .map(|&id| (id, ActorId::TEXTS[id.index()]));
        let actors = Catalog::read(bundled.chain(files.iter().copied()))?;
        ActorKinds::from_catalogs(&actors, &Catalog::bundled()?, &Catalog::bundled()?)
    }

    #[test]
    fn the_bundled_kinds_have_the_player_and_armed_enemies() {
        let kinds = ActorKinds::default();
        assert!(kinds.player.maneuvers.is_some());
        for glyph in ['e', 'h', 's', 'r', 'g', 'f'] {
            let kind = kinds.get(glyph);
            assert!(
                kind.tree.is_some() && kind.weapon.is_some(),
                "{:?}",
                kind.id
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
        let error = with(&[(ActorId::Guard, text.as_str())]).unwrap_err();
        assert!(error.contains("Bloood"), "{error}");
    }

    #[test]
    fn glyphs_are_their_own_and_only_the_player_has_none() {
        let refused = |files: &[(ActorId, &str)], why: &str| {
            let error = with(files).unwrap_err();
            assert!(error.contains(why), "{error}");
        };
        refused(&[(ActorId::Spearman, "(glyph: 'e', hp: 2)")], "share");
        // `k` is a key on maps, `p` the player.
        refused(&[(ActorId::Guard, "(glyph: 'k', hp: 2)")], "means");
        refused(&[(ActorId::Guard, "(hp: 2)")], "no glyph");
        refused(&[(ActorId::Player, "(glyph: 'q', hp: 2)")], "`p`");
    }

    #[test]
    fn every_weapon_must_make_sense_even_unused() {
        let weapons =
            Catalog::read([(WeaponId::Jab, "(damage: 1, windup: 1, art: \"@\")")]).unwrap();
        let error = ActorKinds::from_catalogs(
            &Catalog::bundled().unwrap(),
            &weapons,
            &Catalog::bundled().unwrap(),
        )
        .unwrap_err();
        assert!(error.contains("assets/weapons/jab.ron"), "{error}");
    }
}
