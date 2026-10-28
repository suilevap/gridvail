//! The actor roster: kinds of actor by map glyph, the weapons they strike
//! with and the moves they make, read from `assets/rules/actors.txt`.
//!
//! ```text
//! # A weapon: damage, turns wound up before it strikes, and the cells it
//! # covers drawn with the wielder `@` facing up and each struck cell `x`.
//! @weapon sweep damage 1 windup 1
//! xxx
//! .@.
//!
//! # Moves on a picked target, one per relation of the second direction to
//! # the first: a name, the hit's damage, and whether it knocks the target
//! # and the mover follows.
//! @moves blade
//! forward lunge hit 1 knock follow
//! side push hit 1 knock
//! back yank hit 1 knock follow
//!
//! # An actor: map glyph, name, then its properties.
//! @actor e guard tree enemy hp 2 weapon sweep
//! ```
//!
//! Plain data: what to spawn from it is the game's business.

use bevy::math::IVec2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponSpec {
    pub name: String,
    pub damage: i32,
    pub windup: u8,
    /// Struck cells: `x` ahead of the wielder, `y` to its side.
    pub cells: Vec<IVec2>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveSpec {
    pub name: String,
    pub hit: i32,
    pub knock: bool,
    pub follow: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovesSpec {
    pub name: String,
    pub forward: MoveSpec,
    pub side: MoveSpec,
    pub back: MoveSpec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorSpec {
    pub glyph: char,
    pub name: String,
    /// The behaviour it runs, if any.
    pub tree: Option<String>,
    pub hp: i32,
    pub weapon: Option<String>,
    pub moves: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roster {
    pub weapons: Vec<WeaponSpec>,
    pub moves: Vec<MovesSpec>,
    pub actors: Vec<ActorSpec>,
}

impl Roster {
    pub fn weapon(&self, name: &str) -> Option<&WeaponSpec> {
        self.weapons.iter().find(|weapon| weapon.name == name)
    }

    pub fn moves(&self, name: &str) -> Option<&MovesSpec> {
        self.moves.iter().find(|moves| moves.name == name)
    }

    pub fn actor(&self, glyph: char) -> Option<&ActorSpec> {
        self.actors.iter().find(|actor| actor.glyph == glyph)
    }
}

/// Reads a roster; every weapon and move set an actor names must be in it.
pub fn parse_roster(text: &str) -> Result<Roster, String> {
    let mut roster = Roster::default();
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.trim_end()))
        .filter(|(_, line)| !line.trim_start().starts_with('#'))
        .peekable();
    while let Some((number, line)) = lines.next() {
        let mut words = line.split_whitespace();
        let fail = |error: String| format!("actors line {number}: {error}");
        match words.next() {
            None => {}
            Some("@weapon") => {
                let name = words
                    .next()
                    .ok_or_else(|| fail("a weapon needs a name".into()))?;
                let props = Props::read(words);
                let mut art = Vec::new();
                while let Some(&(_, row)) = lines.peek() {
                    if row.trim().is_empty() || is_directive(row) {
                        break;
                    }
                    art.push(row);
                    lines.next();
                }
                roster.weapons.push(WeaponSpec {
                    name: name.to_string(),
                    damage: props.number("damage").map_err(fail)?,
                    windup: props.number("windup").map_err(fail)?,
                    cells: struck_cells(&art).map_err(fail)?,
                });
            }
            Some("@moves") => {
                let name = words
                    .next()
                    .ok_or_else(|| fail("moves need a name".into()))?;
                let mut take = |relation: &str| {
                    let (number, line) = lines
                        .next()
                        .ok_or_else(|| fail(format!("`{name}` has no `{relation}` move")))?;
                    let fail = |error: String| format!("actors line {number}: {error}");
                    let mut words = line.split_whitespace();
                    if words.next() != Some(relation) {
                        return Err(fail(format!("expected the `{relation}` move")));
                    }
                    let move_name = words
                        .next()
                        .ok_or_else(|| fail("a move needs a name".into()))?;
                    let props = Props::read(words);
                    Ok(MoveSpec {
                        name: move_name.to_string(),
                        hit: props.number("hit").map_err(fail)?,
                        knock: props.flag("knock"),
                        follow: props.flag("follow"),
                    })
                };
                let (forward, side, back) = (take("forward")?, take("side")?, take("back")?);
                roster.moves.push(MovesSpec {
                    name: name.to_string(),
                    forward,
                    side,
                    back,
                });
            }
            Some("@actor") => {
                let glyph = words
                    .next()
                    .and_then(|glyph| glyph.chars().next())
                    .ok_or_else(|| fail("an actor needs a map glyph".into()))?;
                let name = words
                    .next()
                    .ok_or_else(|| fail("an actor needs a name".into()))?;
                let props = Props::read(words);
                roster.actors.push(ActorSpec {
                    glyph,
                    name: name.to_string(),
                    tree: props.text("tree"),
                    hp: props.number("hp").map_err(fail)?,
                    weapon: props.text("weapon"),
                    moves: props.text("moves"),
                });
            }
            Some(other) => return Err(fail(format!("unexpected `{other}`"))),
        }
    }
    for actor in &roster.actors {
        if let Some(weapon) = actor
            .weapon
            .as_deref()
            .filter(|w| roster.weapon(w).is_none())
        {
            return Err(format!("{} names no known weapon `{weapon}`", actor.name));
        }
        if let Some(moves) = actor.moves.as_deref().filter(|m| roster.moves(m).is_none()) {
            return Err(format!("{} names no known moves `{moves}`", actor.name));
        }
    }
    Ok(roster)
}

/// A line opening an entry: `@` and a word (a lone `@` is a wielder).
fn is_directive(line: &str) -> bool {
    line.strip_prefix('@')
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_alphabetic()))
}

/// The cells marked `x` in `art`, from the wielder `@` facing up.
fn struck_cells(art: &[&str]) -> Result<Vec<IVec2>, String> {
    let at = |ch: char| {
        art.iter().enumerate().flat_map(move |(row, line)| {
            line.chars()
                .enumerate()
                .filter(move |&(_, c)| c == ch)
                .map(move |(col, _)| IVec2::new(col as i32, row as i32))
        })
    };
    let mut wielders = at('@');
    let wielder = wielders
        .next()
        .ok_or("a weapon's art needs its wielder `@`")?;
    if wielders.next().is_some() {
        return Err("a weapon's art has more than one `@`".into());
    }
    // Up the art is ahead; right of the wielder is its side.
    let cells: Vec<_> = at('x')
        .map(|cell| IVec2::new(wielder.y - cell.y, cell.x - wielder.x))
        .collect();
    if cells.is_empty() {
        return Err("a weapon's art strikes no `x`".into());
    }
    Ok(cells)
}

/// `key value` pairs, and bare flags.
struct Props<'a>(Vec<&'a str>);

impl<'a> Props<'a> {
    fn read(words: impl Iterator<Item = &'a str>) -> Self {
        Self(words.collect())
    }

    fn value(&self, key: &str) -> Option<&'a str> {
        self.0
            .iter()
            .position(|&word| word == key)
            .and_then(|i| self.0.get(i + 1).copied())
    }

    fn text(&self, key: &str) -> Option<String> {
        self.value(key).map(str::to_string)
    }

    fn number<T: std::str::FromStr>(&self, key: &str) -> Result<T, String> {
        let value = self.value(key).ok_or_else(|| format!("missing `{key}`"))?;
        value
            .parse()
            .map_err(|_| format!("`{key}` must be a number, not `{value}`"))
    }

    fn flag(&self, key: &str) -> bool {
        self.0.contains(&key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROSTER: &str = "\
# a comment
@weapon sweep damage 1 windup 2
xxx
.@.

@weapon spear damage 2 windup 0
x
x
@
@moves blade
forward lunge hit 1 knock follow
side push hit 1 knock
back yank hit 0 knock follow

@actor e guard tree enemy hp 3 weapon sweep
@actor p player hp 2 moves blade
";

    #[test]
    fn reads_weapons_with_their_struck_cells() {
        let roster = parse_roster(ROSTER).unwrap();
        let sweep = roster.weapon("sweep").unwrap();
        assert_eq!((sweep.damage, sweep.windup), (1, 2));
        assert_eq!(
            sweep.cells,
            vec![IVec2::new(1, -1), IVec2::new(1, 0), IVec2::new(1, 1)]
        );
        let spear = roster.weapon("spear").unwrap();
        assert_eq!(spear.cells, vec![IVec2::new(2, 0), IVec2::new(1, 0)]);
    }

    #[test]
    fn reads_moves_and_actors() {
        let roster = parse_roster(ROSTER).unwrap();
        let blade = roster.moves("blade").unwrap();
        assert_eq!(blade.forward.name, "lunge");
        assert!(blade.forward.knock && blade.forward.follow);
        assert!(blade.side.knock && !blade.side.follow);
        assert_eq!(blade.back.hit, 0);
        let guard = roster.actor('e').unwrap();
        assert_eq!(guard.tree.as_deref(), Some("enemy"));
        assert_eq!((guard.hp, guard.weapon.as_deref()), (3, Some("sweep")));
        assert_eq!(roster.actor('p').unwrap().moves.as_deref(), Some("blade"));
    }

    #[test]
    fn an_actor_naming_an_unknown_weapon_is_refused() {
        let error = parse_roster("@actor e guard hp 1 weapon club\n").unwrap_err();
        assert!(error.contains("club"), "{error}");
    }

    #[test]
    fn a_weapon_without_its_wielder_is_refused() {
        let error = parse_roster("@weapon odd damage 1 windup 1\nx\n").unwrap_err();
        assert!(error.contains('@'), "{error}");
    }
}
