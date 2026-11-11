//! Parries: a swing that crosses a wound-up strike turns it. Where an
//! attack's swing goes through a cell an enemy's strike is aimed at, and
//! the two blades go across each other there (each one's way through the
//! cell: along its line for a thrust, across its front for a sweep), the
//! strike is turned a quarter turn, to the side the swing came at it from,
//! and lands there instead: on nothing, a wall, or another enemy. Blades
//! going along each other (a thrust up the line of a thrust, a lunge along
//! a sweeping blade) turn nothing: a strike is parried across its blade.
//! Only strikes of enemies the attack does not reach are turned: one it
//! hits is moved by the blow instead (its strike moves with it), so close
//! in a fight displaces, and from a step off it parries.

use bevy::prelude::*;

use crate::model::*;

/// A wound-up strike that could be turned: its wielder, where it stands,
/// its weapon and the way it is aimed.
pub type Striker = (Entity, &'static Pos, &'static Weapon, &'static Telegraph);

/// The way a strike aimed `strike`, whose blade goes `blade` through a
/// cell, is turned by a swing going `swing` through it: a quarter turn, to
/// the side the swing came at the blade from; none when they go along each
/// other.
pub fn deflection(strike: IVec2, blade: IVec2, swing: IVec2) -> Option<IVec2> {
    match blade.perp_dot(swing).signum() {
        0 => None,
        sign => Some(strike.perp() * sign),
    }
}

/// Each strike `maneuver` (made from `pos` facing `facing`) would turn:
/// its wielder, the cell where they meet and the way it would go then. A
/// maneuver that neither hits nor knocks turns nothing, and the strike of
/// a wielder in its own cells is not turned (it is struck instead).
pub fn parries<'a>(
    maneuver: &'a Maneuver,
    pos: IVec2,
    facing: IVec2,
    grid: &'a MapGrid,
    strikers: impl Iterator<Item = (Entity, &'a Pos, &'a Weapon, &'a Telegraph)> + Clone + 'a,
) -> impl Iterator<Item = (Entity, IVec2, IVec2)> + 'a {
    maneuver
        .area
        .sweep(pos, facing)
        .filter(move |_| maneuver.strikes())
        .flat_map(move |(cell, travel)| {
            let cell = grid.safe_pos(cell);
            strikers
                .clone()
                .filter(move |(_, at, ..)| {
                    !maneuver
                        .area
                        .cells(pos, facing)
                        .any(|reached| grid.safe_pos(reached) == at.0)
                })
                .filter_map(move |(wielder, at, weapon, telegraph)| {
                    weapon
                        .area
                        .sweep(at.0, telegraph.dir)
                        .find(|&(struck, _)| grid.safe_pos(struck) == cell)
                        .and_then(|(_, blade)| deflection(telegraph.dir, blade, travel))
                        .map(|turned| (wielder, cell, turned))
                })
        })
}

/// Each `Attack` turns the strikes its swing crosses (`Deflect`).
pub fn parry_strikes(
    grid: Res<MapGrid>,
    mut attacks: MessageReader<Attack>,
    mut deflects: MessageWriter<Deflect>,
    actors: Query<(&Pos, &Maneuvers)>,
    strikers: Query<Striker>,
) {
    for attack in attacks.read() {
        let Ok((pos, maneuvers)) = actors.get(attack.actor) else {
            continue;
        };
        let maneuver = maneuvers.get(attack.relation);
        for (wielder, _, dir) in parries(maneuver, pos.0, attack.facing, &grid, strikers.iter()) {
            deflects.write(Deflect { wielder, dir });
        }
    }
}

/// A `Deflect` re-aims its wielder's wound-up strike: the mark, and the
/// strike that will land.
pub fn deflect_strikes(
    mut deflects: MessageReader<Deflect>,
    mut strikers: Query<(&mut Telegraph, Option<&mut Scheduled<Strike>>)>,
) {
    for deflect in deflects.read() {
        let Ok((mut telegraph, scheduled)) = strikers.get_mut(deflect.wielder) else {
            continue;
        };
        telegraph.dir = deflect.dir;
        if let Some(mut scheduled) = scheduled {
            scheduled.0.dir = deflect.dir;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swing_across_a_blade_turns_its_strike_to_the_side_it_came_from() {
        // A jab west (its blade goes west), swung through going south: it
        // turns south; going north, north.
        let west = IVec2::NEG_X;
        assert_eq!(deflection(west, west, IVec2::Y), Some(IVec2::Y));
        assert_eq!(deflection(west, west, IVec2::NEG_Y), Some(IVec2::NEG_Y));
        // Across on a slant counts.
        assert_eq!(deflection(west, west, IVec2::new(1, 1)), Some(IVec2::Y));
    }

    #[test]
    fn a_swing_along_a_blade_turns_nothing() {
        let west = IVec2::NEG_X;
        // Thrust against thrust, or with it.
        assert_eq!(deflection(west, west, IVec2::X), None);
        assert_eq!(deflection(west, west, west), None);
        // A sweep west sweeps its blade south along its front: a lunge north
        // goes along it.
        assert_eq!(deflection(west, IVec2::Y, IVec2::NEG_Y), None);
        assert!(deflection(west, IVec2::Y, IVec2::X).is_some());
    }
}
