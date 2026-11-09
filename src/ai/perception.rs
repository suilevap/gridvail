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

/// How much catching the player where it stands counts against the
/// cells it might step into: more than all of them.
const CAUGHT: u32 = 8;

/// The way `area` (from `pos`) best catches the player at `player`, and
/// whether it catches the player where it stands: one whose cells hold it,
/// or failing that the most cells it could step into (`free`); none when
/// no way catches either. Ties go to the way nearest the player's.
pub fn best_strike(
    area: &Area,
    pos: IVec2,
    player: IVec2,
    grid: &MapGrid,
    free: impl Fn(IVec2) -> bool,
) -> Option<(IVec2, bool)> {
    let toward = grid.delta(pos, player);
    let steps =
        [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y].map(|step| grid.safe_pos(player + step));
    [IVec2::NEG_Y, IVec2::X, IVec2::Y, IVec2::NEG_X]
        .into_iter()
        .filter_map(|dir| {
            let (mut caught, mut next) = (false, 0);
            for cell in area.cells(pos, dir).map(|cell| grid.safe_pos(cell)) {
                caught |= cell == player;
                next += steps
                    .iter()
                    .filter(|&&step| step == cell && step != pos && free(step))
                    .count() as u32;
            }
            let score = if caught { CAUGHT } else { 0 } + next;
            (score > 0).then_some((score, dir.dot(toward), dir, caught))
        })
        .max_by_key(|&(score, near, ..)| (score, near))
        .map(|(_, _, dir, caught)| (dir, caught))
}

/// Picks the way each enemy that sees the player would use its weapon
/// (`EnemyMind::strike`, see `best_strike`): not only once the player is in
/// its reach, but when one step would take it there, unless it just struck
/// at a step the player did not take (`EnemyMind::feinted`): then it comes
/// closer first.
pub fn aim_strikes(grid: Res<MapGrid>, mut enemies: Query<(Option<&Weapon>, &mut EnemyMind)>) {
    for (weapon, mut mind) in &mut enemies {
        let mind = mind.bypass_change_detection();
        let best = match (weapon, mind.player) {
            (Some(weapon), Some(player)) => {
                best_strike(&weapon.area, mind.pos, player, &grid, |cell| {
                    grid.get(cell).is_none()
                })
            }
            _ => None,
        };
        let best = best.filter(|&(_, catches)| catches || !mind.feinted);
        mind.strike = best.map(|(dir, _)| dir);
        mind.strike_catches = best.is_some_and(|(_, catches)| catches);
    }
}

/// Turn each enemy's act into this turn's step, and an attack into using
/// its weapon (the attack's step is a wait: the weapon takes the turn).
///
/// A thinking enemy takes no step yet: it keeps its turn while its path is on
/// its way, and its tree is entered again on the next frame.
pub fn carry_out(
    nav: Res<NavMap>,
    mut weapons: MessageWriter<UseWeapon>,
    mut enemies: Query<(Entity, &EnemyAct, &mut EnemyMind, &mut MoveCommand)>,
) {
    for (enemy, act, mut mind, mut command) in enemies.iter_mut() {
        let mind = mind.bypass_change_detection();
        if !mind.has_turn {
            continue;
        }
        // A strike at a step not taken is a feint, until it acts otherwise
        // (thinking is not acting yet).
        match act {
            EnemyAct::Attack(_) => mind.feinted = !mind.strike_catches,
            EnemyAct::Think(_) => {}
            _ => mind.feinted = false,
        }
        if let EnemyAct::Attack(dir) = act {
            weapons.write(UseWeapon {
                user: enemy,
                dir: *dir,
            });
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

/// An enemy is caught off guard (`Exposed`, so a hit is critical) while it
/// has not noticed the player, and on the beat it freezes when it just has.
pub fn expose_unaware(mut enemies: Query<(&EnemyMind, Option<&EnemyAct>, &mut Exposed)>) {
    for (mind, act, mut exposed) in &mut enemies {
        let off_guard = !mind.aware() || matches!(act, Some(EnemyAct::Alert));
        exposed.set_if_neq(Exposed(off_guard));
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
