use bevy::prelude::*;

use super::RenderCell;

/// Hit points. Anything with health can be hurt by a maneuver, a slam or a
/// strike; at zero it is down (see `combat`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    pub hp: i32,
    pub max: i32,
}

impl Health {
    pub const fn full(max: i32) -> Self {
        Self { hp: max, max }
    }

    pub fn is_down(&self) -> bool {
        self.hp <= 0
    }
}

/// Taken by surprise: a hit does critical damage. Whoever knows what the
/// actor is aware of writes it (the AI, for enemies); combat only reads it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Exposed(pub bool);

/// Something that can be picked as the target of a maneuver: moving into it
/// selects it instead of bumping it. Its `Maneuvers`, if it has its own,
/// say what each second direction does to it; otherwise the attacker's
/// `CombatRules::maneuvers`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Targetable;

/// Can be moved by a maneuver. Without it a target stays where it is and
/// takes the hit as if it were slammed into something.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Movable;

/// Picks targets and carries out maneuvers on them (the player).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Engager;

/// The target an engager has picked, and the direction it was picked in
/// (from the engager to it). Picking costs no turn; it stays picked across
/// turns while the target is next to the engager, until cancelled or used.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub target: Entity,
    pub dir: IVec2,
}

/// How the second direction relates to the direction the target was picked
/// in.
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

/// What a maneuver does. The target is always moved one cell the way of the
/// second direction (so forward pushes it on, sideways pushes it aside and
/// back pulls it into the engager's cell); `follow` moves the engager into
/// the cell the target leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Maneuver {
    pub name: &'static str,
    pub follow: bool,
    pub damage: i32,
}

/// A maneuver for each relation of the second direction to the first.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Maneuvers {
    pub forward: Maneuver,
    pub side: Maneuver,
    pub back: Maneuver,
}

impl Maneuvers {
    /// The MVP's attack: every direction hits; only where the engager and
    /// the target end up differs.
    pub const ATTACK: Self = Self {
        forward: Maneuver {
            name: "lunge",
            follow: true,
            damage: 1,
        },
        side: Maneuver {
            name: "push",
            follow: false,
            damage: 1,
        },
        back: Maneuver {
            name: "yank",
            follow: true,
            damage: 1,
        },
    };

    pub fn get(&self, relation: Relation) -> Maneuver {
        match relation {
            Relation::Forward => self.forward,
            Relation::Side => self.side,
            Relation::Back => self.back,
        }
    }
}

/// The numbers combat runs on. Insert another before adding `CombatPlugin`
/// to tune them.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CombatRules {
    /// The player's hit points.
    pub player_hp: i32,
    /// Damage of a hit on an `Exposed` target, in place of the maneuver's.
    pub critical_damage: i32,
    /// Extra damage a moved target takes when its way is blocked, and that
    /// whatever blocked it takes too.
    pub slam_damage: i32,
    /// What a target without its own `Maneuvers` answers to.
    pub maneuvers: Maneuvers,
}

impl Default for CombatRules {
    fn default() -> Self {
        Self {
            player_hp: 2,
            critical_damage: 2,
            slam_damage: 1,
            maneuvers: Maneuvers::ATTACK,
        }
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

/// How an actor strikes: the shape it covers and the damage it does. An
/// actor without it bumps instead.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Striker {
    pub shape: StrikeShape,
    pub damage: i32,
}

impl Default for Striker {
    fn default() -> Self {
        Self {
            shape: StrikeShape::front(),
            damage: 1,
        }
    }
}

/// A strike wound up: it lands on the striker's next turn on the cells its
/// shape covers then, counted from where the striker stands by then (moving
/// it moves the strike), hitting whoever is there.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Telegraph {
    pub dir: IVec2,
}

/// How a marked cell is drawn: whatever stands there in `tint`, or, when
/// nothing does, the floor as `ground`. Both keep their own colours (light
/// does not change them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellMark {
    pub ground: RenderCell,
    pub tint: super::Rgb,
}

/// Cells marked for the player to see (where a strike lands, what is
/// picked), drawn where the player sees them. Rebuilt every frame by
/// whoever marks cells; a later mark of a cell wins.
#[derive(Resource, Debug, Default)]
pub struct CellMarks(pub Vec<(IVec2, CellMark)>);

/// Something took damage: `amount` from `source` (none for a slam into a
/// wall).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hurt {
    pub target: Entity,
    pub amount: i32,
    pub source: Option<Entity>,
}

/// An actor that is not removed when its health runs out (the player) went
/// down; what happens next is up to the game.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Defeated(pub Entity);

/// The player asked to drop whatever they were composing (a selection).
#[derive(Message, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CancelIntent;

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
