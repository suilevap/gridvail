use bevy::prelude::*;

use crate::model::*;

/// What can be attacked: anything with `Health` still there.
type Target = (With<Health>, Without<DestroyRequested>);

/// An actor with moves, about to move.
type Mover = (
    Entity,
    &'static Pos,
    &'static Facing,
    &'static Maneuvers,
    &'static mut MoveCommand,
    &'static Tokens,
);

/// A move about to be carried out this frame.
fn ready(command: &MoveCommand, tokens: &Tokens, turn: &TurnState) -> bool {
    command.active
        && command.relative
        && command.target != IVec2::ZERO
        && tokens.count > 0
        && !turn.simulation
}

/// A move by an actor with `Maneuvers` becomes its maneuver for that way
/// from where it faces, when one of the maneuver's cells holds a target
/// (anything with `Health` but itself): an `Attack`, its action for the
/// turn, and the move becomes a wait. A move that meets nothing is left to
/// walk.
pub fn choose_attacks(
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    mut attacks: MessageWriter<Attack>,
    mut actors: Query<Mover, Without<DestroyRequested>>,
    targets: Query<(), Target>,
) {
    for (actor, pos, facing, maneuvers, mut command, tokens) in &mut actors {
        if !ready(&command, tokens, &turn) {
            continue;
        }
        let relation = Relation::of(facing.0, command.target);
        let meets = maneuvers
            .get(relation)
            .area
            .cells(pos.0, facing.0)
            .filter_map(|cell| grid.get(grid.safe_pos(cell)))
            .any(|target| target != actor && targets.contains(target));
        if !meets {
            continue;
        }
        attacks.write(Attack {
            actor,
            relation,
            facing: facing.0,
            dir: command.target,
        });
        command.target = IVec2::ZERO;
    }
}

/// Carries out each `Attack` as its maneuver says: it hits each target in
/// its cells in order (only the first, if it stops there), knocks each
/// `Movable` one the way the swing goes through its cell, plus the way the
/// actor steps, if it does (the blow carries its step), leaves the actor
/// where its stance says, and turns it to face the move if it turns. It
/// only asks; hits, movement and turning are carried out elsewhere.
pub fn perform_attacks(
    grid: Res<MapGrid>,
    mut attacks: MessageReader<Attack>,
    (mut hits, mut knocks, mut follows, mut turns): (
        MessageWriter<Hit>,
        MessageWriter<Knock>,
        MessageWriter<Follow>,
        MessageWriter<Turn>,
    ),
    actors: Query<(&Pos, &Maneuvers)>,
    (targets, movable): (Query<(), Target>, Query<(), With<Movable>>),
) {
    for attack in attacks.read() {
        let Ok((pos, maneuvers)) = actors.get(attack.actor) else {
            continue;
        };
        let maneuver = maneuvers.get(attack.relation);
        let mut first_knocked = None;
        let carried = match maneuver.stance {
            Stance::Stay => IVec2::ZERO,
            Stance::Follow | Stance::Step => attack.dir,
        };
        for (cell, travel) in maneuver.area.sweep(pos.0, attack.facing) {
            let cell = grid.safe_pos(cell);
            let Some(target) = grid
                .get(cell)
                .filter(|&target| target != attack.actor && targets.contains(target))
            else {
                continue;
            };
            if maneuver.hit > 0 {
                hits.write(Hit {
                    source: attack.actor,
                    target,
                    damage: maneuver.hit,
                });
            }
            if maneuver.knock > 0 && movable.contains(target) {
                knocks.write(Knock {
                    entity: target,
                    velocity: travel * maneuver.knock + carried,
                });
                first_knocked.get_or_insert(cell);
            }
            if maneuver.stop {
                break;
            }
        }
        let into = match maneuver.stance {
            Stance::Stay => None,
            Stance::Follow => first_knocked,
            Stance::Step => Some(grid.safe_pos(pos.0 + attack.dir)),
        };
        if let Some(into) = into {
            follows.write(Follow {
                follower: attack.actor,
                into,
            });
        }
        if maneuver.turn {
            turns.write(Turn {
                entity: attack.actor,
                dir: attack.dir,
            });
        }
    }
}
