use bevy::prelude::*;

use crate::model::*;

pub fn has_destroy_requests(doomed: Query<(), With<DestroyRequested>>) -> bool {
    !doomed.is_empty()
}

/// Destruction is exceptional and may allocate in Bevy's command queue. The
/// application guards it with [`has_destroy_requests`].
pub fn destroy_entities(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    doomed: Query<(Entity, Option<&Pos>), With<DestroyRequested>>,
) {
    for (entity, pos) in doomed.iter() {
        if let Some(pos) = pos {
            if grid.get(pos.0) == Some(entity) {
                grid.clear(pos.0);
            }
        }
        commands.entity(entity).despawn();
    }
}

/// A dead body cannot consume another command, including one prepared before
/// damage was resolved. Keeping this separate also handles damage after moves.
pub fn stop_dead_actions(mut actors: Query<(&Health, &mut MoveCommand, &mut Tokens, &mut Speed)>) {
    for (health, mut command, mut tokens, mut speed) in &mut actors {
        if health.current <= 0 {
            command.active = false;
            tokens.count = 0;
            speed.0 = IVec2::ZERO;
        }
    }
}

/// Resettable bodies become inactive; other dead entities use the existing
/// destruction/item-drop pipeline. Neither decision depends on an enemy type.
pub fn deactivate_dead(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    mut actors: Query<
        (
            Entity,
            &Pos,
            &Health,
            Option<&RespawnPoint>,
            Option<&mut Tokens>,
            Option<&mut Speed>,
        ),
        With<Active>,
    >,
) {
    for (entity, pos, health, respawn, tokens, speed) in &mut actors {
        if health.current > 0 {
            continue;
        }
        if grid.get(pos.0) == Some(entity) {
            grid.clear(pos.0);
        }
        if let Some(mut tokens) = tokens {
            tokens.count = 0;
        }
        if let Some(mut speed) = speed {
            speed.0 = IVec2::ZERO;
        }
        commands.entity(entity).remove::<(Active, Collider)>();
        if respawn.is_none() {
            commands.entity(entity).insert(DestroyRequested);
        }
    }
}
