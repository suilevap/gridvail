use bevy::prelude::*;

use crate::lighting::{RED, YELLOW};
use crate::model::*;

/// A cell a strike is about to land on.
pub const STRIKE_MARK: CellMark = CellMark {
    ground: RenderCell {
        ch: '×',
        color: RED,
        depth: 0,
    },
    tint: RED,
};

/// A picked target.
pub const SELECTION_MARK: CellMark = CellMark {
    ground: RenderCell {
        ch: '·',
        color: YELLOW,
        depth: 0,
    },
    tint: YELLOW,
};

/// Lands each wound-up strike on its striker's next turn, before anything
/// else acts in it: whoever stands on the cells it covers then is hurt,
/// friend or foe. Landing it is the striker's action for the turn.
#[allow(clippy::type_complexity)]
pub fn land_strikes(
    mut commands: Commands,
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    mut pacing: ResMut<TurnPacing>,
    mut hurt: MessageWriter<Hurt>,
    mut strikers: Query<
        (Entity, &Pos, &mut Tokens, &Telegraph, &Striker),
        (With<Active>, Without<DestroyRequested>),
    >,
    health: Query<(), With<Health>>,
) {
    if turn.simulation {
        return;
    }
    for (striker, pos, mut tokens, telegraph, strike) in &mut strikers {
        if tokens.count <= 0 {
            continue;
        }
        for cell in strike.shape.cells(pos.0, telegraph.dir) {
            let Some(hit) = grid.get(grid.safe_pos(cell)) else {
                continue;
            };
            if hit != striker && health.contains(hit) {
                hurt.write(Hurt {
                    target: hit,
                    amount: strike.damage,
                    source: Some(striker),
                });
            }
        }
        tokens.count -= 1;
        pacing.action_committed();
        commands.entity(striker).remove::<Telegraph>();
    }
}

/// Takes damage off health. Anything that runs out is destroyed, except the
/// player, who is `Defeated` instead, for the game to decide what follows.
pub fn take_damage(
    mut commands: Commands,
    mut hurt: MessageReader<Hurt>,
    mut defeated: MessageWriter<Defeated>,
    mut bodies: Query<(&mut Health, Has<Player>), Without<DestroyRequested>>,
) {
    for hit in hurt.read() {
        let Ok((mut health, player)) = bodies.get_mut(hit.target) else {
            continue;
        };
        if health.is_down() {
            continue;
        }
        health.hp -= hit.amount;
        if !health.is_down() {
            continue;
        }
        if player {
            defeated.write(Defeated(hit.target));
        } else {
            commands.entity(hit.target).insert(DestroyRequested);
        }
    }
}

/// Gives a player without health the rules' hit points.
pub fn give_player_health(
    mut commands: Commands,
    rules: Res<CombatRules>,
    players: Query<Entity, (With<Player>, Without<Health>)>,
) {
    for player in &players {
        commands
            .entity(player)
            .insert((Health::full(rules.player_hp), Engager));
    }
}

/// Marks every cell a wound-up strike will land on, and every picked
/// target.
pub fn mark_cells(
    grid: Res<MapGrid>,
    mut marks: ResMut<CellMarks>,
    strikers: Query<(&Pos, &Telegraph, &Striker), Without<DestroyRequested>>,
    selections: Query<&Selection>,
    positions: Query<&Pos>,
) {
    marks.0.clear();
    for selection in &selections {
        if let Ok(pos) = positions.get(selection.target) {
            marks.0.push((pos.0, SELECTION_MARK));
        }
    }
    // After the selection, so danger shows over it.
    for (pos, telegraph, strike) in &strikers {
        for cell in strike.shape.cells(pos.0, telegraph.dir) {
            marks.0.push((grid.safe_pos(cell), STRIKE_MARK));
        }
    }
}
