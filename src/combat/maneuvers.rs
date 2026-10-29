use bevy::prelude::*;

use crate::model::*;

/// Answers a `ContextAction` by an actor with `Maneuvers`: the move for how
/// the second direction relates to the first hits the target, and knocks a
/// `Movable` target the way of the second direction (the actor following
/// into the cell it leaves) or trades places with it, as the move says. It
/// only asks; hits and movement are carried out elsewhere.
pub fn perform_maneuvers(
    mut actions: MessageReader<ContextAction>,
    mut hits: MessageWriter<Hit>,
    mut knocks: MessageWriter<Knock>,
    mut follows: MessageWriter<Follow>,
    mut swaps: MessageWriter<Swap>,
    actors: Query<&Maneuvers>,
    targets: Query<&Pos, With<Movable>>,
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
        let Ok(target_pos) = targets.get(action.target) else {
            continue;
        };
        if maneuver.swap {
            swaps.write(Swap {
                a: action.actor,
                b: action.target,
            });
        }
        if maneuver.knock > 0 {
            knocks.write(Knock {
                entity: action.target,
                velocity: action.dir * maneuver.knock,
            });
            if maneuver.follow {
                follows.write(Follow {
                    follower: action.actor,
                    into: target_pos.0,
                });
            }
        }
    }
}
