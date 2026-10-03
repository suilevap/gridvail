use bevy::prelude::*;
use rand::RngExt;

use crate::foundation::line::line_clear;
use crate::lighting::{CYAN, DARK_RED, DARK_YELLOW, MAGENTA, RED, YELLOW};
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

/// What goal-driven agents read on top of `perceive`: their order, the
/// navigation service's last answer, keys, and doors next to them.
///
/// A route answer that blamed a door which has opened since is dropped, so
/// the tree does not go for a key it no longer needs.
pub fn perceive_objectives(
    grid: Res<MapGrid>,
    doors: Query<&Door>,
    carried_keys: Query<(), With<Key>>,
    keys_on_map: Query<&Pos, (With<Key>, With<Item>)>,
    mut agents: Query<(&Order, &Route, Option<&Inventory>, &mut EnemyMind)>,
) {
    let closed_door = |at: IVec2| {
        grid.get(at)
            .and_then(|occupant| doors.get(occupant).ok())
            .is_some_and(|door| !door.open)
    };
    for (order, route, inventory, mut mind) in agents.iter_mut() {
        let mind = mind.bypass_change_detection();
        if !mind.has_turn {
            continue;
        }
        mind.order = order.target;
        mind.route = route.answer.filter(|answer| match answer.status {
            RouteStatus::Blocked(door) => closed_door(door),
            _ => true,
        });
        mind.has_key =
            inventory.is_some_and(|items| items.0.iter().any(|&item| carried_keys.contains(item)));
        let pos = mind.pos;
        mind.nearest_key = keys_on_map
            .iter()
            .map(|key| key.0)
            .min_by_key(|&key| (key - pos).abs().element_sum());
        for (closed, step) in mind.closed_doors.iter_mut().zip(STEPS) {
            *closed = closed_door(pos + step);
        }
    }
}

/// Turn each enemy's act into a move command for the resolve step. A `GoTo`
/// spends the step the navigation service chose for it.
pub fn carry_out(mut enemies: Query<(&EnemyAct, &EnemyMind, Option<&Route>, &mut MoveCommand)>) {
    for (act, mind, route, mut command) in enemies.iter_mut() {
        if !mind.has_turn {
            continue;
        }
        command.target = act
            .step()
            .unwrap_or_else(|| route.map_or(IVec2::ZERO, |route| route.step));
        command.relative = true;
        command.active = true;
    }
}

/// Show what each enemy is up to: color by mood, `!` on the alert beat, `?`
/// while waiting for a plan.
///
/// Runs after `direction_tiles`, which rewrites the glyph from facing.
pub fn show_mood(mut enemies: Query<(&EnemyAct, Option<&Route>, &mut Glyph)>) {
    for (act, route, mut glyph) in enemies.iter_mut() {
        let thinking = matches!(act, EnemyAct::GoTo(_))
            && route
                .and_then(|route| route.answer)
                .map(|answer| answer.status)
                == Some(RouteStatus::Pending);
        let color = match act {
            EnemyAct::Alert => YELLOW,
            EnemyAct::Hunt(_) | EnemyAct::Attack(_) | EnemyAct::Hold => RED,
            EnemyAct::Search(_) => DARK_YELLOW,
            EnemyAct::Patrol(_) | EnemyAct::Rest => DARK_RED,
            EnemyAct::GoTo(_) => MAGENTA,
            EnemyAct::Open(_) => CYAN,
        };
        let mut shown = *glyph;
        shown.color = color;
        if *act == EnemyAct::Alert {
            shown.ch = '!';
        } else if thinking {
            shown.ch = '?';
        }
        glyph.set_if_neq(shown);
    }
}
