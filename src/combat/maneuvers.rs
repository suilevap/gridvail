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

/// An actor that readies its moves, what its input holds, the attack it
/// aims, if any, and whether a cancel withholds what is held on.
type Readier = (
    Mover,
    &'static Readies,
    &'static MoveIntent,
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

/// An actor that `Readies` aims a move that would strike instead of making
/// it, while its input is held (`MoveIntent`): `Readied`, which the turn
/// waits for (`HoldsTurn`) up to the actor's seconds. Pointing another way
/// aims that way's attack (any attack, backing off too; none if it meets
/// nothing). Letting go makes the aimed attack (it is then chosen as any
/// move is, so if nothing is there any more it walks), or nothing when
/// nothing is aimed. A move already let go (a flick, a tap of a key) is
/// made at once, as is one made `at_once`. After a cancel, a move held on
/// from before is not made until a fresh one comes.
pub fn ready_attacks(
    mut commands: Commands,
    time: Res<Time>,
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    mut actors: Query<Readier, Without<DestroyRequested>>,
    targets: Query<(), Target>,
) {
    let now = time.elapsed_secs();
    for (
        (actor, pos, facing, maneuvers, mut command, tokens),
        readies,
        intent,
        readied,
        withheld,
    ) in &mut actors
    {
        let mover = (actor, pos, facing, maneuvers);
        let held = intent.held.map(|held| held.map);
        if withheld {
            if ready(&command, tokens, &turn) && !command.repeat {
                commands.entity(actor).remove::<Withheld>();
            } else {
                command.active = false;
                continue;
            }
        }
        if let Some(readied) = readied {
            command.active = false;
            match held {
                // Let go: made as aimed.
                None => {
                    if let Some(attack) = readied.attack {
                        command.target = attack.dir;
                        command.active = true;
                        command.relative = true;
                        command.repeat = false;
                    }
                    commands.entity(actor).remove::<(Readied, HoldsTurn)>();
                }
                Some(dir) => {
                    if dir != readied.dir {
                        commands.entity(actor).insert(Readied {
                            dir,
                            attack: attack_of(mover, dir, &grid, &targets),
                            ..*readied
                        });
                    }
                    if now - readied.since >= readies.0 {
                        commands.entity(actor).remove::<HoldsTurn>();
                    }
                }
            }
            continue;
        }
        if !ready(&command, tokens, &turn) || command.at_once || held != Some(command.target) {
            continue;
        }
        let Some(attack) = attack_of(mover, command.target, &grid, &targets) else {
            continue;
        };
        if maneuvers.get(attack.relation).strikes() {
            commands.entity(actor).insert((
                Readied {
                    dir: command.target,
                    attack: Some(attack),
                    since: now,
                },
                HoldsTurn,
            ));
            command.active = false;
            command.target = IVec2::ZERO;
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
            commands
                .entity(actor)
                .remove::<(Readied, HoldsTurn)>()
                .insert(Withheld);
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
        for (cell, knock) in maneuver.knocks(pos.0, attack.facing, attack.dir) {
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
            if knock != IVec2::ZERO && movable.contains(target) {
                knocks.write(Knock {
                    entity: target,
                    velocity: knock,
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
