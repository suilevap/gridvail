//! Composition and restart policy for the playable encounter. Domain plugins
//! do not import this module or disable themselves when it is installed.
#![allow(clippy::type_complexity)]
use crate::animation::{MotionState, ObjectAnimation, Tween};
use crate::combat::CombatPlugin;
use crate::model::*;
use crate::schedule::GamePhase;
use crate::simulation::{deactivate_dead, request_restart, verify_map};
use bevy::prelude::*;

#[derive(Resource)]
pub(super) struct TacticalRules;

pub struct BattlePlugin;
impl Plugin for BattlePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TacticalRules)
            .insert_resource(TurnMode::LeaderDriven)
            .init_resource::<WaitHold>()
            .init_resource::<EncounterOutcome>()
            .init_resource::<RestartRequest>()
            .add_plugins(CombatPlugin)
            .add_observer(prepare_encounter)
            .add_observer(reset_map_turn)
            .add_systems(
                PreUpdate,
                (
                    validate_restart,
                    restore_positions,
                    restore_health,
                    restore_appearance,
                    restore_attacks,
                    restore_animations,
                    restore_turn,
                    finish_restart,
                )
                    .chain()
                    .after(request_restart)
                    .after(super::map::load_map),
            )
            .add_systems(
                Update,
                evaluate_encounter
                    .after(deactivate_dead)
                    .before(verify_map)
                    .in_set(GamePhase::Simulation),
            );
    }
}

// Initial health insertion and map placement are exceptional lifecycle
// events. Ordinary Pos changes never change the encounter's restart anchors.
fn prepare_encounter(
    added: On<Add, (Health, Pos)>,
    mut commands: Commands,
    actors: Query<
        (
            &Pos,
            &Glyph,
            Option<&AttackPresentation>,
            Option<&RespawnGlyph>,
        ),
        (With<Health>, Or<(With<Player>, With<Enemy>)>),
    >,
) {
    let Ok((pos, glyph, display, original)) = actors.get(added.entity) else {
        return;
    };
    commands.entity(added.entity).insert((
        RespawnPoint(pos.0),
        original.copied().unwrap_or(RespawnGlyph(*glyph)),
    ));
    if display.is_some() {
        commands.entity(added.entity).remove::<DirectionTile>();
    }
}

fn reset_map_turn(
    placed: On<Add, Pos>,
    leaders: Query<(), With<TurnLeader>>,
    mut turn: ResMut<TurnState>,
    mut timer: ResMut<TokenTimer>,
    mut wait: ResMut<WaitHold>,
    mut pacing: ResMut<TurnPacing>,
    mut outcome: ResMut<EncounterOutcome>,
    mut deadline: ResMut<TurnDeadline>,
) {
    if !leaders.contains(placed.entity) {
        return;
    }
    *turn = TurnState::default();
    timer.0.reset();
    *wait = WaitHold::default();
    *pacing = TurnPacing::new(pacing.animation_lead);
    *outcome = EncounterOutcome::Ongoing;
    deadline.enabled = true;
}

fn evaluate_encounter(
    mut outcome: ResMut<EncounterOutcome>,
    mut deadline: ResMut<TurnDeadline>,
    player: Query<&Health, With<Player>>,
    enemies: Query<&Health, With<Enemy>>,
) {
    *outcome = if player.single().is_ok_and(|health| health.current <= 0) {
        EncounterOutcome::Defeat
    } else if enemies.iter().all(|health| health.current <= 0) {
        EncounterOutcome::Victory
    } else {
        EncounterOutcome::Ongoing
    };
    deadline.enabled = *outcome == EncounterOutcome::Ongoing;
}

fn validate_restart(
    mut request: ResMut<RestartRequest>,
    menu: Option<Res<SettingsMenu>>,
    grid: Res<MapGrid>,
    actors: Query<&RespawnPoint>,
) {
    if request.0
        && (SettingsMenu::is_open(menu.as_deref())
            || actors.iter().any(|start| {
                grid.get(start.0)
                    .is_some_and(|occupant| !actors.contains(occupant))
            }))
    {
        request.0 = false;
    }
}

fn restore_positions(
    request: Res<RestartRequest>,
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    mut actors: Query<(
        Entity,
        &RespawnPoint,
        &mut Pos,
        &mut PrevPos,
        &mut Facing,
        &mut MoveCommand,
        &mut PendingPos,
        &mut Speed,
        &mut Tokens,
        &mut ActionTaken,
        Option<&mut MoveIntent>,
        Option<&mut Destination>,
        Option<&mut PathFollow>,
    )>,
) {
    if !request.0 {
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
        mut previous,
        mut facing,
        mut command,
        mut pending,
        mut speed,
        mut tokens,
        mut action,
        intent,
        destination,
        follow,
    ) in &mut actors
    {
        pos.0 = start.0;
        previous.0 = start.0;
        facing.0 = IVec2::ZERO;
        *command = MoveCommand::default();
        *pending = PendingPos::None;
        speed.0 = IVec2::ZERO;
        tokens.count = tokens.recharge;
        *action = ActionTaken::default();
        if let Some(mut intent) = intent {
            *intent = MoveIntent::default();
        }
        if let Some(mut destination) = destination {
            destination.stop();
        }
        if let Some(mut follow) = follow {
            follow.finish(WalkStatus::Idle);
            follow.planned_for = None;
        }
        grid.set_with_blocking(pos.0, entity, false);
        commands.entity(entity).insert((Active, Collider));
    }
}
fn restore_health(
    request: Res<RestartRequest>,
    mut actors: Query<(&mut Health, Option<&mut Brace>), With<RespawnPoint>>,
) {
    if !request.0 {
        return;
    }
    for (mut health, brace) in &mut actors {
        health.current = health.maximum;
        if let Some(mut brace) = brace {
            brace.charges = 0;
        }
    }
}
fn restore_appearance(
    request: Res<RestartRequest>,
    mut actors: Query<(&RespawnGlyph, &mut Glyph)>,
) {
    if !request.0 {
        return;
    }
    for (start, mut glyph) in &mut actors {
        *glyph = start.0;
    }
}

fn restore_attacks(
    request: Res<RestartRequest>,
    mut actors: Query<(&mut AttackCycle, &mut AttackPoints, &mut AttackExecuted)>,
) {
    if !request.0 {
        return;
    }
    for (mut cycle, mut points, mut executed) in &mut actors {
        *cycle = AttackCycle::default();
        *points = AttackPoints::default();
        *executed = AttackExecuted::default();
    }
}
fn restore_animations(
    request: Res<RestartRequest>,
    mut actors: Query<(&RespawnPoint, &mut ObjectAnimation, &mut AnimatedPos)>,
) {
    if !request.0 {
        return;
    }
    for (start, mut animation, mut shown) in &mut actors {
        animation.state = MotionState::at(start.0.as_vec2());
        animation.height = Tween::at(1.0);
        *shown = AnimatedPos::at(start.0);
    }
}
fn restore_turn(
    request: Res<RestartRequest>,
    mut turn: ResMut<TurnState>,
    mut timer: ResMut<TokenTimer>,
    mut wait: ResMut<WaitHold>,
) {
    if !request.0 {
        return;
    }
    *turn = TurnState::default();
    timer.0.reset();
    *wait = WaitHold::default();
}
fn finish_restart(
    mut request: ResMut<RestartRequest>,
    mut outcome: ResMut<EncounterOutcome>,
    mut deadline: ResMut<TurnDeadline>,
) {
    if !request.0 {
        return;
    }
    request.0 = false;
    *outcome = EncounterOutcome::Ongoing;
    deadline.enabled = true;
}
