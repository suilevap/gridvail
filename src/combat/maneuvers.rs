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

/// An actor that readies its moves, with the move it holds, if any, and
/// whether a cancel withholds what is held on.
type Readier = (
    Mover,
    &'static Readies,
    Option<&'static Readied>,
    Has<Withheld>,
);

/// A move about to be carried out this frame.
fn ready(command: &MoveCommand, tokens: &Tokens, turn: &TurnState) -> bool {
    command.active
        && command.relative
        && command.target != IVec2::ZERO
        && tokens.count > 0
        && !turn.simulation
}

/// The attack a move `dir` by `actor` (at `pos`, facing `facing`) makes:
/// its maneuver for that way from where it faces, when one of the
/// maneuver's cells holds a target (anything with `Health` but itself).
fn attack_of(
    (actor, pos, facing, maneuvers): (Entity, &Pos, &Facing, &Maneuvers),
    dir: IVec2,
    grid: &MapGrid,
    targets: &Query<(), Target>,
) -> Option<Attack> {
    let relation = Relation::of(facing.0, dir);
    maneuvers
        .get(relation)
        .area
        .cells(pos.0, facing.0)
        .filter_map(|cell| grid.get(grid.safe_pos(cell)))
        .any(|target| target != actor && targets.contains(target))
        .then_some(Attack {
            actor,
            relation,
            facing: facing.0,
            dir,
        })
}

/// A move by an actor with `Maneuvers` that meets a target becomes an
/// `Attack`, its action for the turn, and the move becomes a wait. A move
/// that meets nothing is left to walk.
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
        let mover = (actor, pos, facing, maneuvers);
        if let Some(attack) = attack_of(mover, command.target, &grid, &targets) {
            attacks.write(attack);
            command.target = IVec2::ZERO;
        }
    }
}

/// An actor that `Readies` holds a move that would strike (an attack that
/// hits or knocks) as `Readied` for its seconds instead of making it, and
/// makes it when they are up, or at once when the same move comes again
/// (it is then chosen as any move is, so if nothing is there any more it
/// walks). A move made `at_once` is never held. While one is held, a move
/// held on from before waits, and a fresh move in another way takes its
/// place: readied too if it strikes, else made as it is. After a cancel,
/// a move held on from before is not made until a fresh one comes.
pub fn ready_attacks(
    mut commands: Commands,
    time: Res<Time>,
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    mut actors: Query<Readier, Without<DestroyRequested>>,
    targets: Query<(), Target>,
) {
    let now = time.elapsed_secs();
    for ((actor, pos, facing, maneuvers, mut command, tokens), readies, readied, withheld) in
        &mut actors
    {
        let fresh = ready(&command, tokens, &turn) && !command.repeat;
        if withheld {
            if !fresh {
                command.active = false;
                continue;
            }
            commands.entity(actor).remove::<Withheld>();
        }
        if let Some(readied) = readied {
            if (fresh && command.target == readied.attack.dir) || now >= readied.until {
                // Made now: the move goes on to be chosen.
                command.target = readied.attack.dir;
                command.active = true;
                command.relative = true;
                command.repeat = false;
                commands.entity(actor).remove::<Readied>();
                continue;
            }
            if !fresh {
                command.active = false;
                continue;
            }
        }
        if !ready(&command, tokens, &turn) {
            continue;
        }
        let mover = (actor, pos, facing, maneuvers);
        match attack_of(mover, command.target, &grid, &targets) {
            Some(attack) if !command.at_once && maneuvers.get(attack.relation).strikes() => {
                commands.entity(actor).insert(Readied {
                    attack,
                    until: now + readies.0,
                });
                command.active = false;
                command.target = IVec2::ZERO;
            }
            _ if readied.is_some() => {
                commands.entity(actor).remove::<Readied>();
            }
            _ => {}
        }
    }
}

/// A `Cancel` drops the move its actor readied, and withholds what is held
/// on from it (`Withheld`).
pub fn cancel_readied(
    mut commands: Commands,
    mut cancels: MessageReader<Cancel>,
    readied: Query<(), With<Readied>>,
) {
    for &Cancel(actor) in cancels.read() {
        if readied.contains(actor) {
            commands.entity(actor).remove::<Readied>().insert(Withheld);
        }
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
