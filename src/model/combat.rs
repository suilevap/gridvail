use bevy::prelude::*;

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
    /// Damage of the hit, if it hits.
    pub hit: i32,
    /// Knocks a `Movable` target one cell the way of the second direction
    /// (forward pushes it on, sideways aside, back into the actor's cell).
    pub knock: bool,
    /// The actor moves into the cell the target was in.
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

/// Can be knocked about by maneuvers.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Movable;

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

/// Damage an `Impact` does to the mover and to what it was driven into.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImpactDamage(pub i32);

impl Default for ImpactDamage {
    fn default() -> Self {
        Self(1)
    }
}

/// Cells a strike covers, in the striker's frame: `x` counts cells ahead
/// in the direction it strikes, `y` cells to its side (a quarter turn from
/// ahead). `[(1, 0)]` is the cell in front.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrikeShape(pub Vec<IVec2>);

impl StrikeShape {
    /// The one cell in front.
    pub fn front() -> Self {
        Self(vec![IVec2::X])
    }

    /// The cells struck from `pos` striking toward `dir`.
    pub fn cells(&self, pos: IVec2, dir: IVec2) -> impl Iterator<Item = IVec2> + '_ {
        let side = IVec2::new(-dir.y, dir.x);
        self.0
            .iter()
            .map(move |offset| pos + dir * offset.x + side * offset.y)
    }
}

/// A weapon an actor strikes with: the cells it covers, the damage, and
/// how many turns it is wound up before the strike lands (0: at once).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Weapon {
    pub shape: StrikeShape,
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

    #[test]
    fn a_shape_turns_with_its_direction() {
        let shape = StrikeShape(vec![IVec2::X, IVec2::new(1, 1)]);
        let cells = |dir| shape.cells(IVec2::new(5, 5), dir).collect::<Vec<_>>();
        assert_eq!(cells(IVec2::X)[0], IVec2::new(6, 5));
        assert_eq!(cells(IVec2::NEG_X)[0], IVec2::new(4, 5));
        assert_eq!(cells(IVec2::Y)[0], IVec2::new(5, 6));
        assert_eq!(cells(IVec2::NEG_Y)[0], IVec2::new(5, 4));
        // The second cell turns with the first: diagonally ahead.
        let ahead_side = cells(IVec2::Y)[1] - IVec2::new(5, 5);
        assert_eq!(ahead_side.y, 1);
        assert_eq!(ahead_side.x.abs(), 1);
    }
}
