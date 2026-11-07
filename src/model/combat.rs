use bevy::prelude::*;

use super::Area;

/// Which way a move goes from where its mover faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Relation {
    /// The way it faces.
    Forward,
    /// A quarter turn to its left.
    Left,
    /// A quarter turn to its right (the side of an `Area`).
    Right,
    /// Away from the way it faces.
    Back,
}

impl Relation {
    /// How a move `dir` relates to facing `facing`, both unit grid
    /// directions.
    pub fn of(facing: IVec2, dir: IVec2) -> Self {
        if dir == facing {
            Self::Forward
        } else if dir == -facing {
            Self::Back
        } else if dir == IVec2::new(-facing.y, facing.x) {
            Self::Right
        } else {
            Self::Left
        }
    }
}

/// How a weapon or a move is swung: the gesture its user makes, for
/// showing it. Each reaches the target at the middle of the gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Swing {
    /// Straight at the target and back, `reach` cells out.
    Thrust { reach: f32 },
    /// In an arc across the front, `reach` cells out.
    Sweep { reach: f32 },
    /// Out at the target, then pulled back past where it started.
    Hook { reach: f32 },
}

impl Default for Swing {
    fn default() -> Self {
        Self::Thrust { reach: 0.55 }
    }
}

/// How a weapon or a move looks when swung: the gesture, and the streak it
/// leaves (an effect's name). Only shown; combat never reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SwingLook {
    /// No swing: the move makes no gesture (a step back).
    pub swing: Option<Swing>,
    pub streak: Option<String>,
}

/// How an actor's `Weapon` looks swung.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct WeaponLook(pub SwingLook);

/// How each of an actor's `Maneuvers` looks swung, by `Relation`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ManeuverLooks {
    pub forward: SwingLook,
    pub left: SwingLook,
    pub right: SwingLook,
    pub back: SwingLook,
}

impl ManeuverLooks {
    pub fn get(&self, relation: Relation) -> &SwingLook {
        match relation {
            Relation::Forward => &self.forward,
            Relation::Left => &self.left,
            Relation::Right => &self.right,
            Relation::Back => &self.back,
        }
    }
}

/// Where a move leaves its mover.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stance {
    /// Where it was.
    #[default]
    Stay,
    /// Into the cell the first target it knocks leaves.
    Follow,
    /// One step the way of the move.
    Step,
}

/// One move of a fighting style: what a move made a given way from where
/// the mover faces does, when one of its cells holds a target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Maneuver {
    pub name: String,
    /// The cells it plays on, in the mover's frame and in the order its
    /// swing goes through them.
    pub area: Area,
    /// It stops at the first target it hits, instead of going through all.
    pub stop: bool,
    /// Damage of each hit (0: no hit).
    pub hit: i32,
    /// How many cells a `Movable` target is knocked the way the swing goes
    /// through its cell; 0: not at all.
    pub knock: i32,
    /// Where it leaves the mover, and whether the mover turns to face the
    /// way of the move.
    pub stance: Stance,
    pub turn: bool,
}

/// The moves an actor makes when one of them meets a target, one for each
/// `Relation` of the move to where it faces (a fighting style, a weapon's
/// moves). A move that meets none is a plain step.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Maneuvers {
    pub forward: Maneuver,
    pub left: Maneuver,
    pub right: Maneuver,
    pub back: Maneuver,
}

impl Maneuvers {
    pub fn get(&self, relation: Relation) -> &Maneuver {
        match relation {
            Relation::Forward => &self.forward,
            Relation::Left => &self.left,
            Relation::Right => &self.right,
            Relation::Back => &self.back,
        }
    }
}

/// `actor` makes its maneuver for `relation`: a move `dir` while facing
/// `facing`, which met a target. Its action for the turn.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attack {
    pub actor: Entity,
    pub relation: Relation,
    pub facing: IVec2,
    pub dir: IVec2,
}

/// An attack landed on `target`: a maneuver's hit or a weapon's strike.
/// What it costs the target is up to whoever reads it (base damage,
/// critical hits).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub source: Entity,
    pub target: Entity,
    pub damage: i32,
}

/// Caught off guard: hits on it are critical. Whoever knows what the actor
/// is aware of writes it (the AI, for enemies).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Exposed(pub bool);

/// Extra damage of a hit on an `Exposed` target.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CriticalHit {
    pub bonus: i32,
}

impl Default for CriticalHit {
    fn default() -> Self {
        Self { bonus: 1 }
    }
}

/// Damage a `Collision` does to the knocked body and to what it ran into.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImpactDamage(pub i32);

impl Default for ImpactDamage {
    fn default() -> Self {
        Self(1)
    }
}

/// A weapon an actor strikes with: the cells it covers (facing where it
/// is aimed), the damage, and how many turns it is wound up before the
/// strike lands (0: at once).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Weapon {
    pub area: Area,
    pub damage: i32,
    pub windup: u8,
}

/// Where a wound-up weapon will strike: a direction from its wielder, so
/// moving the wielder moves the strike.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Telegraph {
    pub dir: IVec2,
}

/// `user` uses its weapon toward `dir`; the weapon decides how (at once,
/// or wound up first).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UseWeapon {
    pub user: Entity,
    pub dir: IVec2,
}

/// `wielder`'s weapon strikes toward `dir`, now. Sent at once by a weapon
/// with no wind-up, else `Scheduled` until its wind-up is paid.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strike {
    pub wielder: Entity,
    pub dir: IVec2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relations_follow_the_facing() {
        assert_eq!(Relation::of(IVec2::X, IVec2::X), Relation::Forward);
        assert_eq!(Relation::of(IVec2::X, IVec2::NEG_X), Relation::Back);
        // Facing east, south (map y grows down) is to the right.
        assert_eq!(Relation::of(IVec2::X, IVec2::Y), Relation::Right);
        assert_eq!(Relation::of(IVec2::X, IVec2::NEG_Y), Relation::Left);
    }
}
