use bevy::prelude::*;
use rand::RngExt;

use crate::lighting::{DARK_RED, DARK_YELLOW, MAGENTA, RED, YELLOW};
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

/// Hunters are ordered to the player's cell. The one source of orders for
/// now; anything else that commands an agent writes its `Order` the same way.
pub fn order_hunters(
    players: Query<&Pos, (With<Player>, With<Active>, Without<DestroyRequested>)>,
    mut hunters: Query<&mut Order, With<Hunter>>,
) {
    let target = players.iter().next().map(|pos| pos.0);
    for mut order in hunters.iter_mut() {
        order.set_if_neq(Order { target });
    }
}

/// What goal-driven agents read on top of `perceive`: their order and keys.
pub fn perceive_objectives(
    carried_keys: Query<(), With<Key>>,
    keys_on_map: Query<(Entity, &Pos), (With<Key>, With<Item>)>,
    mut agents: Query<(&Order, Option<&Inventory>, &mut EnemyMind)>,
) {
    for (order, inventory, mut mind) in agents.iter_mut() {
        let mind = mind.bypass_change_detection();
        if !mind.has_turn {
            continue;
        }
        mind.order = order.target;
        mind.has_key =
            inventory.is_some_and(|items| items.0.iter().any(|&item| carried_keys.contains(item)));
        let pos = mind.pos;
        // Nearest first, kept without sorting a list: few keys, few slots.
        mind.keys = [None; KNOWN_KEYS];
        for key in keys_on_map.iter().map(|(entity, cell)| (entity, cell.0)) {
            let distance = |(_, cell): (Entity, IVec2)| (cell - pos).abs().element_sum();
            let mut candidate = Some(key);
            for slot in &mut mind.keys {
                match (*slot, candidate) {
                    (_, None) => break,
                    (None, _) => {
                        *slot = candidate;
                        break;
                    }
                    (Some(held), Some(new)) if distance(new) < distance(held) => {
                        *slot = Some(new);
                        candidate = Some(held);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Turn each enemy's act into this turn's step.
///
/// A thinking enemy takes no step yet: it keeps its turn while its path is on
/// its way, and its tree is entered again on the next frame.
pub fn carry_out(nav: Res<NavMap>, mut enemies: Query<(&EnemyAct, &EnemyMind, &mut MoveCommand)>) {
    for (act, mind, mut command) in enemies.iter_mut() {
        if !mind.has_turn {
            continue;
        }
        if let Some(step) = act.step(mind.pos, |from, to| nav.step_toward(from, to)) {
            command.target = step;
            command.relative = true;
            command.active = true;
        }
    }
}

/// An enemy that decided on no step this turn waits, spending its token, so
/// turns never stall on it. A thinking enemy keeps its token for a few frames
/// ([`THINK_FRAMES`]), to act this turn if its path lands by then; after
/// that it waits out the turn too, and goes on thinking on its next one.
pub fn wait_if_idle(mut enemies: Query<(&EnemyAct, &mut EnemyMind, &mut MoveCommand)>) {
    for (act, mut mind, mut command) in enemies.iter_mut() {
        let mind = mind.bypass_change_detection();
        if !mind.has_turn {
            mind.think_frames = 0;
            continue;
        }
        if matches!(act, EnemyAct::Think(_)) && mind.think_frames < THINK_FRAMES {
            mind.think_frames += 1;
            continue;
        }
        if !command.active {
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
                Mood::Order | Mood::Fetch => MAGENTA,
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
