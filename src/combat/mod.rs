//! Bump attacks, locked enemy telegraphs, and explicit guarded waits.
//! The movement/collision pipeline still owns every move and action token.

#![allow(clippy::type_complexity)]

use bevy::prelude::*;

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
            .add_systems(Startup, prepare_fighters.after(StartupPhase::Content).before(StartupPhase::Derive))
            .add_systems(PreUpdate, restart_battle.after(bevy::input::InputSystems))
            .add_systems(Update,
                (hold_to_wait, begin_action, order_enemy_actions)
                    .chain()
                    .after(player_input)
                    .after(swipe_commands)
                    .before(move_commands)
                    .in_set(GamePhase::Simulation))
            .add_systems(Update,
                (resolve_attacks, downed_actors)
                    .chain()
                    .after(resolve_commit)
                    .before(verify_map)
                    .in_set(GamePhase::Simulation))
            .add_systems(Update, plan_intents.before(compose_frame).in_set(GamePhase::Presentation));
    }
}

fn prepare_fighters(
    mut commands: Commands,
    grid: Res<MapGrid>,
    mut path: ResMut<BattlePath>,
    actors: Query<(Entity, &Pos, Has<Player>, Option<&BattleRole>), Or<(With<Player>, With<Enemy>)>>,
) {
    path.0 = Vec::with_capacity((grid.width * grid.height) as usize);
    for (entity, pos, player, role) in &actors {
        commands.entity(entity).insert((Health::new(if player { 12 } else { role.copied().unwrap_or_default().health() }), BattleStart(pos.0)));
        if !player {
            commands.entity(entity).insert(Fighter {
                role: role.copied().unwrap_or_default(),
                intent: BattleIntent::Idle,
                needs_plan: true,
                recovering: false,
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
    let Ok((mut command, tokens, health)) = player.single_mut() else { return; };
    let held = !SettingsMenu::is_open(menu.as_deref())
        && (keys.is_some_and(|keys| keys.pressed(KeyCode::Space)) || wait.touch_held);
    // Movement cancels a simultaneous hold; never queue an extra turn behind it.
    if held && command.active && command.target != IVec2::ZERO {
        wait.elapsed = 0.0;
        wait.latched = true;
        return;
    }
    if wait.update(held, !turn.simulation && tokens.count > 0 && health.current > 0, time.delta_secs()) {
        *command = MoveCommand { target: IVec2::ZERO, relative: true, active: true };
    }
}

fn begin_action(
    grid: Res<MapGrid>,
    mut state: ResMut<BattleState>,
    mut player: Query<(&Pos, &MoveCommand, &Tokens, &mut Health), With<Player>>,
    mut enemies: Query<(&Pos, &Facing, &mut Health, &mut Fighter), (With<Enemy>, Without<Player>)>,
) {
    state.committed = false;
    let Ok((pos, command, tokens, mut health)) = player.single_mut() else { return; };
    if !command.active || tokens.count <= 0 || health.current == 0 { return; }
    state.committed = true;
    state.round += 1;
    health.guarded = command.target == IVec2::ZERO;
    state.feedback = if health.guarded { "Wait counted - braced for the next hit" } else { "Moved" };
    if !command.relative || command.target == IVec2::ZERO { return; }
    let Some(target) = grid.get(grid.safe_pos(pos.0 + command.target)) else { return; };
    let Ok((enemy_pos, facing, mut enemy_health, mut fighter)) = enemies.get_mut(target) else { return; };
    if enemy_health.current == 0 { return; }
    if fighter.role == BattleRole::ShieldBearer && pos.0 - enemy_pos.0 == facing.0 {
        state.feedback = "Shield blocked - attack from the side or behind";
        return;
    }
    enemy_health.hit(2);
    // A successful bump interrupts the currently advertised attack.
    fighter.intent = BattleIntent::Recover;
    fighter.recovering = false;
    state.feedback = if enemy_health.current == 0 { "Enemy defeated" } else { "Hit for 2 - enemy interrupted" };
}

fn order_enemy_actions(
    state: Res<BattleState>,
    mut enemies: Query<(&Fighter, &Health, &Tokens, &mut MoveCommand), (With<Enemy>, Without<Player>)>,
) {
    if !state.committed { return; }
    for (fighter, health, tokens, mut command) in &mut enemies {
        if tokens.count <= 0 || health.current == 0 { continue; }
        *command = MoveCommand {
            target: match fighter.intent { BattleIntent::Move(step) => step, _ => IVec2::ZERO },
            relative: true,
            active: true,
        };
    }
}

/// Resolve the advertised cells after movement, so dodging really avoids a hit.
fn resolve_attacks(
    mut state: ResMut<BattleState>,
    mut player: Query<(&Pos, &mut Health), With<Player>>,
    mut enemies: Query<(&Health, &mut Fighter), (With<Enemy>, Without<Player>)>,
) {
    if !state.committed { return; }
    let Ok((pos, mut health)) = player.single_mut() else { return; };
    for (enemy_health, mut fighter) in &mut enemies {
        if enemy_health.current == 0 { continue; }
        if let BattleIntent::Strike { damage, .. } = fighter.intent {
            if fighter.intent.cells().contains(&Some(pos.0)) {
                let guarded = health.guarded;
                let dealt = health.hit(damage);
                state.feedback = if guarded && dealt == 0 { "Braced - hit absorbed" }
                    else if guarded { "Braced - damage reduced" } else { "Hit - dodge red ! cells or interrupt the attacker" };
            }
            fighter.recovering = true;
        }
        fighter.needs_plan = true;
    }
    state.defeated = health.current == 0;
    state.won = enemies.iter().all(|(health, _)| health.current == 0);
}

/// Keep defeated actors for an allocation-free encounter reset, but remove
/// their occupancy and activity so they cannot block, move, or attack.
fn downed_actors(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    mut actors: Query<(Entity, &Pos, &Health, Has<Player>, &mut Glyph, &mut Tokens, &mut Speed), With<Active>>,
) {
    for (entity, pos, health, player, mut glyph, mut tokens, mut speed) in &mut actors {
        if health.current > 0 { continue; }
        tokens.count = 0;
        speed.0 = IVec2::ZERO;
        glyph.color = DARK_RED;
        if !player {
            if grid.get(pos.0) == Some(entity) { grid.clear(pos.0); }
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
    mut enemies: Query<(&Pos, &mut Facing, &Health, Option<&FovResult>, &mut Fighter, &mut Glyph), (With<Enemy>, Without<Player>)>,
) {
    let Ok((player, player_health)) = player.single() else { return; };
    for (pos, mut facing, health, sight, mut fighter, mut glyph) in &mut enemies {
        if health.current == 0 || !fighter.needs_plan { continue; }
        fighter.needs_plan = false;
        let intent = if state.defeated || player_health.current == 0 {
            BattleIntent::Idle
        } else if fighter.recovering {
            fighter.recovering = false;
            BattleIntent::Recover
        } else {
            let seen = grid.idx(player.0).is_some_and(|index| sight.is_some_and(|sight|
                sight.data.get(index).copied().unwrap_or(0.0) > ENEMY_SIGHT_THRESHOLD));
            let delta = player.0 - pos.0;
            let distance = delta.abs().element_sum();
            let straight = delta.x == 0 || delta.y == 0;
            if seen && straight && distance > 0 && distance <= fighter.role.reach() {
                let direction = delta.signum();
                // Stop the spear at a wall/closed door instead of threatening through it.
                let reach = (1..=fighter.role.reach())
                    .take_while(|step| grid.is_valid(pos.0 + direction * *step)
                        && !grid.blocks_vision(pos.0 + direction * *step))
                    .count() as i32;
                // A shield keeps its front until it moves; circling during its
                // recovery therefore creates a real flanking opportunity.
                if fighter.role != BattleRole::ShieldBearer || facing.0 == IVec2::ZERO {
                    facing.0 = direction;
                }
                BattleIntent::Strike { origin: pos.0, direction, reach, damage: fighter.role.damage() }
            } else if seen && planner.plan(&nav, pos.0, player.0, None, &mut path.0) {
                let step = path.0.get(1).copied().map_or(IVec2::ZERO, |next| next - pos.0);
                BattleIntent::Move(step)
            } else { BattleIntent::Idle }
        };
        fighter.intent = intent;
        glyph.ch = fighter.role.glyph();
        glyph.color = match intent {
            BattleIntent::Strike { .. } => RED,
            BattleIntent::Recover => CYAN,
            BattleIntent::Move(_) => YELLOW,
            BattleIntent::Idle => DARK_RED,
        };
    }
}

fn restart_battle(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    menu: Option<Res<SettingsMenu>>,
    mut commands: Commands,
    mut state: ResMut<BattleState>,
    mut wait: ResMut<WaitHold>,
    mut grid: ResMut<MapGrid>,
    mut actors: Query<(Entity, &BattleStart, &mut Pos, &mut Health, &mut Tokens, &mut Speed, &mut MoveCommand, &mut PendingPos, &mut Facing, &mut Glyph, Option<&mut Fighter>)>,
) {
    if SettingsMenu::is_open(menu.as_deref())
        || !(state.restart_requested || keys.is_some_and(|keys| keys.just_pressed(KeyCode::KeyR))) { return; }
    // A changed room layout may now cover an original spawn. Never overwrite
    // a wall or other object when resetting actors into it.
    if actors.iter().any(|(_, start, ..)| grid.get(start.0)
        .is_some_and(|occupant| !actors.contains(occupant))) {
        state.restart_requested = false;
        state.feedback = "Spawn blocked by a changed room - reload the map to reset";
        return;
    }
    for (entity, _, pos, ..) in &actors {
        if grid.get(pos.0) == Some(entity) { grid.clear(pos.0); }
    }
    for (entity, start, mut pos, mut health, mut tokens, mut speed, mut command, mut pending, mut facing, mut glyph, fighter) in &mut actors {
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
            fighter.recovering = false;
        } else {
            glyph.color = WHITE;
        }
        grid.set_with_blocking(pos.0, entity, false);
        commands.entity(entity).insert((Active, Collider));
    }
    *state = BattleState { feedback: "Encounter reset", ..default() };
    *wait = WaitHold::default();
}

#[cfg(test)]
mod tests;
