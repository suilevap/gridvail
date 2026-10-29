use bevy::prelude::*;

use super::Area;

/// How a second direction relates to the direction a target was picked in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Relation {
    /// The same direction again: into the target.
    Forward,
    /// Either side.
    Side,
    /// Back, away from the target.
    Back,
}

impl Relation {
    /// How `second` relates to `picked`, both unit grid directions.
    pub fn of(picked: IVec2, second: IVec2) -> Self {
        if second == picked {
            Self::Forward
        } else if second == -picked {
            Self::Back
        } else {
            Self::Side
        }
    }
}

/// One move of a maneuver set: what acting on a picked target does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Maneuver {
    pub name: String,
    /// Damage of the hit (0: no hit).
    pub hit: i32,
    /// How many cells a `Movable` target is knocked the way of the second
    /// direction (forward on, sideways aside, back into the actor's cell);
    /// 0: not at all.
    pub knock: i32,
    /// The actor steps into the cell the target leaves.
    pub follow: bool,
}

/// The moves an actor makes on a picked target, one for each `Relation` of
/// the second direction to the first (a fighting style, a weapon's moves).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Maneuvers {
    pub forward: Maneuver,
    pub side: Maneuver,
    pub back: Maneuver,
}

impl Maneuvers {
    pub fn get(&self, relation: Relation) -> &Maneuver {
        match relation {
            Relation::Forward => &self.forward,
            Relation::Side => &self.side,
            Relation::Back => &self.back,
        }
    }
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

/// Damage a forced `Collision` does to the mover and to what it ran into,
/// per cell of speed it had left.
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

/// `wielder`'s weapon strikes where it is aimed, now. Sent at once by a
/// weapon with no wind-up, else `Scheduled` until its wind-up is paid.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strike {
    pub wielder: Entity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relations_follow_the_picked_direction() {
        assert_eq!(Relation::of(IVec2::X, IVec2::X), Relation::Forward);
        assert_eq!(Relation::of(IVec2::X, IVec2::NEG_X), Relation::Back);
        assert_eq!(Relation::of(IVec2::X, IVec2::Y), Relation::Side);
        assert_eq!(Relation::of(IVec2::X, IVec2::NEG_Y), Relation::Side);
    }
}
