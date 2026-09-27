use bevy::prelude::*;
use rand::RngExt;

use crate::foundation::line::line_clear;
use crate::lighting::{DARK_RED, DARK_YELLOW, RED, YELLOW};
use crate::model::*;

/// Fill each enemy's blackboard for this frame's tick.
///
/// Written through `bypass_change_detection`: the blackboard is rewritten
/// every frame, and nothing downstream filters on it changing.
pub fn perceive(
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    mut rng: ResMut<SharedRng>,
    players: Query<(Entity, &Pos), (With<Player>, With<Active>, Without<DestroyRequested>)>,
    mut enemies: Query<
        (&Pos, &Tokens, Has<DestroyRequested>, &mut EnemyMind),
        (With<Enemy>, With<Active>),
    >,
) {
    let (player_entity, player) = players
        .iter()
        .next()
        .map_or((None, None), |(entity, pos)| (Some(entity), Some(pos.0)));
    for (pos, tokens, doomed, mut mind) in enemies.iter_mut() {
        let mind = mind.bypass_change_detection();
        mind.pos = pos.0;
        mind.has_turn = !turn.simulation && !doomed && tokens.count > 0;
        if !mind.has_turn {
            continue;
        }
        mind.player = player.filter(|&target| {
            (target - pos.0).length_squared() <= ENEMY_SIGHT_RADIUS * ENEMY_SIGHT_RADIUS
                && line_clear(pos.0, target, |p| grid.blocks_vision(p))
        });
        if mind.player.is_some() {
            mind.last_seen = mind.player;
        } else if mind.last_seen == Some(pos.0) {
            mind.last_seen = None;
        }
        // Walls and other actors block a step; the player does not, since
        // stepping into it is the attack bump. Without this, an enemy queues
        // behind an ally forever instead of stepping around it.
        for (open, step) in mind.open.iter_mut().zip(STEPS) {
            let at = pos.0 + step;
            *open = grid.is_valid(at)
                && grid
                    .get(at)
                    .is_none_or(|occupant| Some(occupant) == player_entity);
        }
        mind.seed = rng.0.random();
    }
}

/// Turn each enemy's act into a move command for the resolve step.
pub fn carry_out(mut enemies: Query<(&EnemyAct, &EnemyMind, &mut MoveCommand)>) {
    for (act, mind, mut command) in enemies.iter_mut() {
        if !mind.has_turn {
            continue;
        }
        command.target = act.step();
        command.relative = true;
        command.active = true;
    }
}

/// Show what each enemy is up to: color by mood, `!` on the alert beat.
///
/// Runs after `direction_tiles`, which rewrites the glyph from facing.
pub fn show_mood(mut enemies: Query<(&EnemyAct, &mut Glyph)>) {
    for (act, mut glyph) in enemies.iter_mut() {
        let color = match act {
            EnemyAct::Alert => YELLOW,
            EnemyAct::Hunt(_) | EnemyAct::Attack(_) | EnemyAct::Hold => RED,
            EnemyAct::Search(_) => DARK_YELLOW,
            EnemyAct::Patrol(_) | EnemyAct::Rest => DARK_RED,
        };
        let mut shown = *glyph;
        shown.color = color;
        if *act == EnemyAct::Alert {
            shown.ch = '!';
        }
        glyph.set_if_neq(shown);
    }
}
