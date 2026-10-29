use bevy::prelude::*;

use crate::model::*;

/// Answers a `ContextAction` by an actor with `Maneuvers`: the move for how
/// the second direction relates to the first hits the target, knocks it
/// the way of the second direction and has the actor step after it, as the
/// move says. It only asks; hits and impulses are carried out elsewhere.
pub fn perform_maneuvers(
    mut actions: MessageReader<ContextAction>,
    mut hits: MessageWriter<Hit>,
    mut impulses: MessageWriter<Impulse>,
    actors: Query<&Maneuvers>,
    movable: Query<(), With<Movable>>,
) {
    for action in actions.read() {
        let Ok(maneuvers) = actors.get(action.actor) else {
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
        if maneuver.knock <= 0 || !movable.contains(action.target) {
            continue;
        }
        impulses.write(Impulse {
            entity: action.target,
            velocity: action.dir * maneuver.knock,
            forced: true,
        });
        if maneuver.follow {
            impulses.write(Impulse {
                entity: action.actor,
                velocity: action.picked,
                forced: false,
            });
        }
    }
}
