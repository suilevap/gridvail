use bevy::prelude::*;
use rand::RngExt;

use crate::lighting::{DARK_RED, DARK_YELLOW, RED, YELLOW};
use crate::model::*;
use crate::navigation::{NavCell, NavMap};

/// Tries at picking a stroll cell per turn.
const STROLL_TRIES: usize = 8;

/// Fill each enemy's blackboard for this frame's tick.
///
/// Written through `bypass_change_detection`: the blackboard is rewritten
/// every frame, and nothing downstream filters on it changing.
pub fn perceive(
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    nav: Res<NavMap>,
    mut rng: ResMut<SharedRng>,
    players: Query<&Pos, (With<Player>, With<Active>, Without<DestroyRequested>)>,
    mut enemies: Query<
        (
            &Pos,
            &Tokens,
            Has<DestroyRequested>,
            Option<&FovResult>,
            Option<(&Destination, &PathFollow)>,
            &mut EnemyMind,
        ),
        (With<Enemy>, With<Active>),
    >,
) {
    let player = players.iter().next().map(|pos| pos.0);
    for (pos, tokens, doomed, sight, walking, mut mind) in enemies.iter_mut() {
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
        mind.walk = walking.and_then(|(destination, follow)| {
            destination.goal().map(|goal| Walk {
                goal,
                status: follow.status,
            })
        });
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

/// Turn each enemy's act into movement: a single step as a move command, or
/// a walk as a `Destination` for locomotion, which plans and takes its steps.
///
/// A walk is requested again only when its goal changes, or as soon as a step
/// of it failed (another actor in the way), so locomotion replans around
/// whoever stands there now instead of bumping into them; otherwise it keeps
/// following its plan.
pub fn carry_out(
    mut enemies: Query<(
        &EnemyAct,
        &EnemyMind,
        &mut MoveCommand,
        Option<(&mut Destination, &PathFollow)>,
    )>,
) {
    for (act, mind, mut command, walking) in enemies.iter_mut() {
        if !mind.has_turn {
            continue;
        }
        let walk_to = match act.movement() {
            Movement::Step(step) => {
                command.target = step;
                command.relative = true;
                command.active = true;
                None
            }
            Movement::WalkTo(cell) => Some(cell),
        };
        let Some((mut destination, follow)) = walking else {
            continue;
        };
        match walk_to {
            Some(goal) => {
                // The last step was ordered from here, was carried out (no
                // command still waiting for a token), and here we still are.
                let stuck = (follow.ordered_from == Some(mind.pos) && !command.active)
                    || follow.status == WalkStatus::Blocked;
                if destination.goal() != Some(goal) || stuck {
                    destination.go_to(goal);
                }
            }
            None => {
                if destination.goal().is_some() {
                    destination.stop();
                }
            }
        }
    }
}

/// An enemy whose walk took no step this turn (arrived, or no way) waits,
/// spending its token, so turns never stall on it.
pub fn wait_if_idle(mut enemies: Query<(&EnemyMind, &mut MoveCommand), With<EnemyAct>>) {
    for (mind, mut command) in enemies.iter_mut() {
        if mind.has_turn && !command.active {
            command.target = IVec2::ZERO;
            command.relative = true;
            command.active = true;
        }
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
