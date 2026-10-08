use bevy::prelude::*;
use rand::RngExt;

use crate::lighting::{DARK_RED, DARK_YELLOW, RED, YELLOW};
use crate::model::*;
use crate::navigation::{NavCell, NavMap};
use crate::service::Services;

/// Tries at picking a stroll cell per turn.
const STROLL_TRIES: usize = 8;

/// Fill each enemy's blackboard for this frame's tick. The services handle is
/// handed over once.
///
/// Written through `bypass_change_detection`: the blackboard is rewritten
/// every frame, and nothing downstream filters on it changing.
pub fn perceive(
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    nav: Res<NavMap>,
    services: Res<Services>,
    mut rng: ResMut<SharedRng>,
    players: Query<&Pos, (With<Player>, With<Active>, Without<DestroyRequested>)>,
    mut enemies: Query<
        (
            &Pos,
            &Tokens,
            Has<DestroyRequested>,
            Option<&FovResult>,
            Option<&TraversalPrefs>,
            &mut EnemyMind,
        ),
        (With<Enemy>, With<Active>),
    >,
) {
    let player = players.iter().next().map(|pos| pos.0);
    for (pos, tokens, doomed, sight, prefs, mut mind) in enemies.iter_mut() {
        let mind = mind.bypass_change_detection();
        mind.pos = pos.0;
        mind.has_turn = !turn.simulation && !doomed && tokens.count > 0;
        if !mind.has_turn {
            continue;
        }
        // Seen as the player sees: through the enemy's field of view, which
        // walls shade, so the threshold decides how much of a cell is enough.
        mind.player = player.filter(|&target| {
            let cell = grid.safe_pos(target);
            let index = (cell.y * grid.width + cell.x) as usize;
            sight.is_some_and(|fov| {
                fov.pos == pos.0
                    && fov.data.get(index).copied().unwrap_or(0.0) > ENEMY_SIGHT_THRESHOLD
            })
        });
        if mind.player.is_some() {
            mind.last_seen = mind.player;
        } else if mind.last_seen == Some(pos.0) {
            mind.last_seen = None;
        }
        mind.prefs = prefs.copied().unwrap_or_default();
        if mind.services.is_none() {
            mind.services = Some(services.clone());
        }
        mind.seed = rng.0.random();
        mind.stroll = (0..STROLL_TRIES)
            .map(|_| {
                let offset = IVec2::new(
                    rng.0.random_range(-STROLL_RADIUS..=STROLL_RADIUS),
                    rng.0.random_range(-STROLL_RADIUS..=STROLL_RADIUS),
                );
                pos.0 + offset
            })
            .find(|&cell| cell != pos.0 && nav.at(cell) == NavCell::Floor);
    }
}

/// Turn each enemy's act into this turn's step.
///
/// A thinking enemy takes no step yet: it keeps its turn while its path is on
/// its way, and its tree is entered again on the next frame.
pub fn carry_out(mut enemies: Query<(&EnemyAct, &EnemyMind, &mut MoveCommand)>) {
    for (act, mind, mut command) in enemies.iter_mut() {
        if !mind.has_turn {
            continue;
        }
        if let Some(step) = act.step(mind.pos) {
            command.target = step;
            command.relative = true;
            command.active = true;
        }
    }
}

/// An enemy that decided on no step this turn (a thinking one included)
/// waits, spending its token, so turns never stall on it.
pub fn wait_if_idle(mut enemies: Query<(&EnemyMind, &mut MoveCommand), With<EnemyAct>>) {
    for (mind, mut command) in enemies.iter_mut() {
        if mind.has_turn && !command.active {
            command.target = IVec2::ZERO;
            command.relative = true;
            command.active = true;
        }
    }
}

/// Show what each enemy is up to: color by mood, `!` on the alert beat, `?`
/// while it waits for a path.
///
/// Runs after `direction_tiles`, which rewrites the glyph from facing.
pub fn show_mood(mut enemies: Query<(&EnemyAct, &mut Glyph)>) {
    for (act, mut glyph) in enemies.iter_mut() {
        let color = match act {
            EnemyAct::Alert => YELLOW,
            EnemyAct::Attack(_) | EnemyAct::Hold => RED,
            EnemyAct::Rest => DARK_RED,
            EnemyAct::Think(mood) | EnemyAct::Move(mood, _) => match mood {
                Mood::Hunt => RED,
                Mood::Search => DARK_YELLOW,
                Mood::Patrol => DARK_RED,
            },
        };
        let mut shown = *glyph;
        shown.color = color;
        match act {
            EnemyAct::Alert => shown.ch = '!',
            EnemyAct::Think(_) => shown.ch = '?',
            _ => {}
        }
        glyph.set_if_neq(shown);
    }
}
