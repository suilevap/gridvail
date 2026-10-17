//! Bump attacks, locked enemy telegraphs, and explicit guarded waits.
//! The movement/collision pipeline still owns every move and action token.

#![allow(clippy::type_complexity)]

use bevy::prelude::*;

use crate::animation::{MotionStyle, Move, ObjectAnimation, Path, Secondary};
use crate::lighting::{CYAN, DARK_RED, RED, WHITE, YELLOW};
use crate::model::*;
use crate::navigation::{NavMap, PathPlanner};
use crate::presentation::compose_frame;
use crate::schedule::{GamePhase, StartupPhase};
use crate::simulation::{move_commands, player_input, resolve_commit, verify_map};
use crate::touch::swipe_commands;

#[derive(Resource, Default)]
struct BattlePath(Vec<IVec2>);

pub struct BattlePlugin;

impl Plugin for BattlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BattleState>()
            .init_resource::<WaitHold>()
            .init_resource::<BattlePath>()
            .add_systems(
                Startup,
                prepare_fighters
                    .after(StartupPhase::Content)
                    .before(StartupPhase::Derive),
            )
            .add_systems(PreUpdate, restart_battle.after(bevy::input::InputSystems))
            .add_systems(
                Update,
                (hold_to_wait, begin_action, order_enemy_actions)
                    .chain()
                    .after(player_input)
                    .after(swipe_commands)
                    .before(move_commands)
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                (resolve_attacks, downed_actors)
                    .chain()
                    .after(resolve_commit)
                    .before(verify_map)
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                (plan_intents, show_guard)
                    .chain()
                    .before(compose_frame)
                    .in_set(GamePhase::Presentation),
            );
    }
}

fn prepare_fighters(
    mut commands: Commands,
    grid: Res<MapGrid>,
    mut path: ResMut<BattlePath>,
    actors: Query<
        (Entity, &Pos, Has<Player>, Option<&BattleRole>),
        Or<(With<Player>, With<Enemy>)>,
    >,
) {
    path.0 = Vec::with_capacity((grid.width * grid.height) as usize);
    for (entity, pos, player, role) in &actors {
        commands.entity(entity).insert((
            Health::new(if player {
                12
            } else {
                role.copied().unwrap_or_default().health()
            }),
            BattleStart(pos.0),
        ));
        if !player {
            // Battle roles own their glyph; the legacy directional enemy
            // tile would replace it on the next idle frame.
            commands.entity(entity).remove::<DirectionTile>();
            if role == Some(&BattleRole::ShieldBearer) {
                commands.spawn((
                    Active,
                    BoundTo {
                        parent: entity,
                        offset: IVec2::X,
                        offset_dir: IVec2::X,
                    },
                    Pos(pos.0 + IVec2::X),
                    PrevPos(pos.0 + IVec2::X),
                    Facing::default(),
                    DirectionTile {
                        rule: "direction_v_rule".into(),
                    },
                    Glyph::new('>', 0, CYAN),
                ));
            }
            commands.entity(entity).insert(Fighter {
                role: role.copied().unwrap_or_default(),
                intent: BattleIntent::Idle,
                needs_plan: true,
                recovery_left: 0,
                last_attack: BattleIntent::Idle,
            });
        }
    }
}

fn hold_to_wait(
    time: Res<Time>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    menu: Option<Res<SettingsMenu>>,
    turn: Res<TurnState>,
    mut wait: ResMut<WaitHold>,
    mut player: Query<(&mut MoveCommand, &Tokens, &Health), With<Player>>,
) {
    let Ok((mut command, tokens, health)) = player.single_mut() else {
        return;
    };
    let held = !SettingsMenu::is_open(menu.as_deref())
        && (keys.is_some_and(|keys| keys.pressed(KeyCode::Space)) || wait.touch_held);
    // Movement cancels a simultaneous hold; never queue an extra turn behind it.
    if held && command.active && command.target != IVec2::ZERO {
        wait.elapsed = 0.0;
        wait.latched = true;
        return;
    }
    if wait.update(
        held,
        !turn.simulation && tokens.count > 0 && health.current > 0,
        time.delta_secs(),
    ) {
        *command = MoveCommand {
            target: IVec2::ZERO,
            relative: true,
            active: true,
        };
    }
}

fn begin_action(
    grid: Res<MapGrid>,
    mut state: ResMut<BattleState>,
    mut player: Query<(&Pos, &MoveCommand, &Tokens, &mut Health), With<Player>>,
    mut enemies: Query<(&Pos, &Facing, &mut Health, &mut Fighter), (With<Enemy>, Without<Player>)>,
) {
    state.committed = false;
    let Ok((pos, command, tokens, mut health)) = player.single_mut() else {
        return;
    };
    if !command.active || tokens.count <= 0 || health.current == 0 {
        return;
    }
    state.committed = true;
    state.round += 1;
    health.guarded = command.target == IVec2::ZERO;
    state.feedback = if health.guarded {
        "Wait counted - braced for the next hit"
    } else {
        "Moved"
    };
    if !command.relative || command.target == IVec2::ZERO {
        return;
    }
    let Some(target) = grid.get(grid.safe_pos(pos.0 + command.target)) else {
        return;
    };
    let Ok((enemy_pos, facing, mut enemy_health, mut fighter)) = enemies.get_mut(target) else {
        return;
    };
    if enemy_health.current == 0 {
        return;
    }
    let open = fighter.intent == BattleIntent::Recover;
    if fighter.role == BattleRole::ShieldBearer && !open && pos.0 - enemy_pos.0 == facing.0 {
        state.feedback = "BLOCKED: bait a bash; hit when shield DOWN";
        return;
    }
    let armoured = matches!(
        fighter.intent,
        BattleIntent::WindUp { .. } | BattleIntent::Slam { .. }
    );
    let damage = if armoured {
        1
    } else if open {
        3
    } else {
        2
    };
    enemy_health.hit(damage);
    // A successful bump interrupts the currently advertised attack.
    // The brute's wind-up is armoured: bait the slam instead of stunlocking it.
    if !open && !armoured {
        fighter.intent = BattleIntent::Recover;
        fighter.recovery_left = 1;
        fighter.last_attack = BattleIntent::Idle;
    }
    state.feedback = if enemy_health.current == 0 {
        "Enemy defeated"
    } else if armoured {
        "Armoured: 1 damage; slam still coming"
    } else if open {
        "Opening punished for 3 damage"
    } else {
        "Hit for 2 - enemy interrupted"
    };
}

fn order_enemy_actions(
    state: Res<BattleState>,
    mut enemies: Query<
        (&Fighter, &Health, &Tokens, &mut MoveCommand),
        (With<Enemy>, Without<Player>),
    >,
) {
    if !state.committed {
        return;
    }
    for (fighter, health, tokens, mut command) in &mut enemies {
        if tokens.count <= 0 || health.current == 0 {
            continue;
        }
        *command = MoveCommand {
            target: match fighter.intent {
                BattleIntent::Move(step) => step,
                BattleIntent::Strike { direction, .. } if fighter.role == BattleRole::Spearman => {
                    direction
                }
                _ => IVec2::ZERO,
            },
            relative: true,
            active: true,
        };
    }
}

/// Resolve the advertised cells after movement, so dodging really avoids a hit.
fn resolve_attacks(
    mut state: ResMut<BattleState>,
    motion: Option<Res<MotionStyle>>,
    mut player: Query<(&Pos, &mut Health), With<Player>>,
    mut enemies: Query<
        (
            &Pos,
            &Health,
            &mut Fighter,
            Option<&mut ObjectAnimation>,
            &mut Glyph,
        ),
        (With<Enemy>, Without<Player>),
    >,
) {
    if !state.committed {
        return;
    }
    let Ok((pos, mut health)) = player.single_mut() else {
        return;
    };
    let mut hit = false;
    for (enemy_pos, enemy_health, mut fighter, animation, mut glyph) in &mut enemies {
        if enemy_health.current == 0 {
            continue;
        }
        if let BattleIntent::WindUp { origin, damage } = fighter.intent {
            fighter.intent = BattleIntent::Slam { origin, damage };
            glyph.color = RED;
            // Keep the original centre even if the player has left sight.
            fighter.needs_plan = false;
            continue;
        }
        if let BattleIntent::Strike { damage, .. } | BattleIntent::Slam { damage, .. } =
            fighter.intent
        {
            let attack = fighter.intent;
            if fighter.intent.cells().contains(&Some(pos.0)) {
                hit = true;
                let guarded = health.guarded;
                let dealt = health.hit(damage);
                state.feedback = if guarded && dealt == 0 {
                    "Braced - hit absorbed"
                } else if guarded {
                    "Braced - damage reduced"
                } else {
                    "Hit - dodge red ! cells or interrupt the attacker"
                };
            } else if !hit {
                state.feedback = match fighter.role {
                    BattleRole::Spearman => "Spear MISSED - extended: punish!",
                    BattleRole::ShieldBearer => "Bash MISSED - shield DOWN: punish!",
                    BattleRole::Brute => "Slam MISSED - brute OPEN: punish!",
                    BattleRole::Skirmisher => "Attack missed - counter now",
                };
            }
            // Movement animates the spear's lunge. Bashes and slams also play
            // on a miss, using the existing renderer-neutral motion player.
            if motion.as_deref() != Some(&MotionStyle::Snap) {
                if let Some(mut animation) = animation {
                    if matches!(attack, BattleIntent::Slam { .. }) {
                        animation.state.play(Move {
                            from: enemy_pos.0.as_vec2(),
                            to: enemy_pos.0.as_vec2(),
                            path: Path::Straight,
                            duration: 0.35,
                            easing: EaseFunction::CubicIn,
                            secondary: Secondary {
                                hop: 0.6,
                                hops: 1.0,
                            },
                            coast: None,
                        });
                    } else if let BattleIntent::Strike { direction, .. } = attack {
                        if fighter.role == BattleRole::ShieldBearer {
                            animation.state.bump(
                                direction.as_vec2(),
                                MotionStyle::Tween {
                                    duration: 0.3,
                                    easing: EaseFunction::CubicIn,
                                },
                            );
                        }
                    }
                }
            }
            fighter.last_attack = attack;
            fighter.recovery_left = fighter.role.recovery_actions();
        }
        fighter.needs_plan = true;
    }
    state.defeated = health.current == 0;
    state.won = enemies.iter().all(|(_, health, ..)| health.current == 0);
}

/// Keep defeated actors for an allocation-free encounter reset, but remove
/// their occupancy and activity so they cannot block, move, or attack.
fn downed_actors(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    mut actors: Query<
        (
            Entity,
            &Pos,
            &Health,
            Has<Player>,
            &mut Glyph,
            &mut Tokens,
            &mut Speed,
        ),
        With<Active>,
    >,
) {
    for (entity, pos, health, player, mut glyph, mut tokens, mut speed) in &mut actors {
        if health.current > 0 {
            continue;
        }
        tokens.count = 0;
        speed.0 = IVec2::ZERO;
        glyph.color = DARK_RED;
        if !player {
            if grid.get(pos.0) == Some(entity) {
                grid.clear(pos.0);
            }
            glyph.ch = ' ';
            commands.entity(entity).remove::<(Active, Collider)>();
        }
    }
}

fn plan_intents(
    state: Res<BattleState>,
    grid: Res<MapGrid>,
    nav: Res<NavMap>,
    mut planner: ResMut<PathPlanner>,
    mut path: ResMut<BattlePath>,
    player: Query<(&Pos, &Health), With<Player>>,
    mut enemies: Query<
        (
            &Pos,
            &mut Facing,
            &Health,
            Option<&FovResult>,
            &mut Fighter,
            &mut Glyph,
        ),
        (With<Enemy>, Without<Player>),
    >,
) {
    let Ok((player, player_health)) = player.single() else {
        return;
    };
    for (pos, mut facing, health, sight, mut fighter, mut glyph) in &mut enemies {
        if health.current == 0 || !fighter.needs_plan {
            continue;
        }
        fighter.needs_plan = false;
        let intent = if state.defeated || player_health.current == 0 {
            BattleIntent::Idle
        } else if fighter.recovery_left > 0 {
            fighter.recovery_left -= 1;
            BattleIntent::Recover
        } else {
            let seen = grid.idx(player.0).is_some_and(|index| {
                sight.is_some_and(|sight| {
                    sight.data.get(index).copied().unwrap_or(0.0) > ENEMY_SIGHT_THRESHOLD
                })
            });
            let delta = player.0 - pos.0;
            let distance = delta.abs().element_sum();
            let straight = delta.x == 0 || delta.y == 0;
            if seen && fighter.role == BattleRole::Brute && delta.abs().max_element() == 1 {
                BattleIntent::WindUp {
                    origin: pos.0,
                    damage: fighter.role.damage(),
                }
            } else if seen && straight && distance > 0 && distance <= fighter.role.reach() {
                let direction = delta.signum();
                // Stop the spear at a wall/closed door instead of threatening through it.
                let reach = (1..=fighter.role.reach())
                    .take_while(|step| {
                        grid.is_valid(pos.0 + direction * *step)
                            && !grid.blocks_vision(pos.0 + direction * *step)
                    })
                    .count() as i32;
                // A shield keeps its front until it moves; circling during its
                // recovery therefore creates a real flanking opportunity.
                if fighter.role != BattleRole::ShieldBearer || facing.0 == IVec2::ZERO {
                    facing.0 = direction;
                }
                BattleIntent::Strike {
                    origin: pos.0,
                    direction,
                    reach,
                    damage: fighter.role.damage(),
                }
            } else if seen && planner.plan(&nav, pos.0, player.0, None, &mut path.0) {
                let step = path
                    .0
                    .get(1)
                    .copied()
                    .map_or(IVec2::ZERO, |next| next - pos.0);
                BattleIntent::Move(step)
            } else {
                BattleIntent::Idle
            }
        };
        fighter.intent = intent;
        glyph.ch = if intent == BattleIntent::Recover {
            fighter.role.glyph().to_ascii_lowercase()
        } else {
            fighter.role.glyph()
        };
        glyph.color = match intent {
            BattleIntent::Strike { .. } | BattleIntent::Slam { .. } => RED,
            BattleIntent::Recover => CYAN,
            BattleIntent::Move(_) | BattleIntent::WindUp { .. } => YELLOW,
            BattleIntent::Idle => DARK_RED,
        };
    }
}

/// The shield is lowered during recovery; don't show a protected front then.
fn show_guard(fighters: Query<(&Facing, &Fighter)>, mut markers: Query<(&BoundTo, &mut Glyph)>) {
    for (bound, mut glyph) in &mut markers {
        if let Ok((facing, fighter)) = fighters.get(bound.parent) {
            if fighter.role == BattleRole::ShieldBearer
                && (facing.0 == IVec2::ZERO || fighter.intent == BattleIntent::Recover)
            {
                glyph.ch = ' ';
            }
        }
    }
}

fn restart_battle(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    menu: Option<Res<SettingsMenu>>,
    mut commands: Commands,
    mut state: ResMut<BattleState>,
    mut wait: ResMut<WaitHold>,
    mut grid: ResMut<MapGrid>,
    mut actors: Query<(
        Entity,
        &BattleStart,
        &mut Pos,
        &mut Health,
        &mut Tokens,
        &mut Speed,
        &mut MoveCommand,
        &mut PendingPos,
        &mut Facing,
        &mut Glyph,
        Option<&mut Fighter>,
    )>,
) {
    if SettingsMenu::is_open(menu.as_deref())
        || !(state.restart_requested || keys.is_some_and(|keys| keys.just_pressed(KeyCode::KeyR)))
    {
        return;
    }
    // A changed room layout may now cover an original spawn. Never overwrite
    // a wall or other object when resetting actors into it.
    if actors.iter().any(|(_, start, ..)| {
        grid.get(start.0)
            .is_some_and(|occupant| !actors.contains(occupant))
    }) {
        state.restart_requested = false;
        state.feedback = "Spawn blocked by a changed room - reload the map to reset";
        return;
    }
    for (entity, _, pos, ..) in &actors {
        if grid.get(pos.0) == Some(entity) {
            grid.clear(pos.0);
        }
    }
    for (
        entity,
        start,
        mut pos,
        mut health,
        mut tokens,
        mut speed,
        mut command,
        mut pending,
        mut facing,
        mut glyph,
        fighter,
    ) in &mut actors
    {
        pos.0 = start.0;
        *health = Health::new(health.maximum);
        tokens.count = tokens.recharge;
        speed.0 = IVec2::ZERO;
        *command = MoveCommand::default();
        *pending = PendingPos::None;
        facing.0 = IVec2::ZERO;
        if let Some(mut fighter) = fighter {
            fighter.intent = BattleIntent::Idle;
            fighter.needs_plan = true;
            fighter.recovery_left = 0;
            fighter.last_attack = BattleIntent::Idle;
        } else {
            glyph.color = WHITE;
        }
        grid.set_with_blocking(pos.0, entity, false);
        commands.entity(entity).insert((Active, Collider));
    }
    *state = BattleState {
        feedback: "Encounter reset",
        ..default()
    };
    *wait = WaitHold::default();
}

#[cfg(test)]
mod tests;
