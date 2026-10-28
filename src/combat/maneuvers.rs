use bevy::prelude::*;

use crate::model::*;

/// Answers a `ContextAction` by an actor with `Maneuvers`: the move for how
/// the second direction relates to the first hits the target, knocks it
/// the way of the second direction and has the actor follow, as the move
/// says. It only says what should happen; hits and displacements are
/// carried out by their own systems.
pub fn perform_maneuvers(
    mut actions: MessageReader<ContextAction>,
    mut hits: MessageWriter<Hit>,
    mut displacements: MessageWriter<Displace>,
    actors: Query<&Maneuvers>,
    targets: Query<(&Pos, Has<Movable>)>,
) {
    for action in actions.read() {
        let (Ok(maneuvers), Ok((target_pos, movable))) =
            (actors.get(action.actor), targets.get(action.target))
        else {
            continue;
        };
        let maneuver = maneuvers.get(Relation::of(action.picked, action.dir));
        if maneuver.hit > 0 {
            hits.write(Hit {
                source: action.actor,
                target: action.target,
                damage: maneuver.hit,
            });
        }
        if !(maneuver.knock && movable) {
            continue;
        }
        displacements.write(Displace {
            entity: action.target,
            to: target_pos.0 + action.dir,
            forced: true,
        });
        if maneuver.follow {
            displacements.write(Displace {
                entity: action.actor,
                to: target_pos.0,
                forced: false,
            });
        }
    }
}
