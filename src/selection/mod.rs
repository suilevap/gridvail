//! Picking things by moving into them, and acting on what is picked.
//!
//! A `Selector` moving into something `Selectable` picks it (`Selected`)
//! instead of bumping it, for free. Its next fresh move is a
//! `ContextAction` on the pick, its action for the turn; what that does is
//! up to whoever answers it (`combat` maneuvers, for now). A pick lasts
//! while its target stands where it was picked, until a `CancelIntent`.

#![allow(clippy::type_complexity)]

use bevy::prelude::*;

use crate::lighting::YELLOW;
use crate::model::*;
use crate::schedule::{ActionStage, GamePhase, MarkCells, PlayerCommands};

/// How a picked target is marked.
pub const SELECTION_MARK: CellMark = CellMark {
    ground: RenderCell {
        ch: '·',
        color: YELLOW,
        depth: 0,
    },
    tint: YELLOW,
};

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CancelIntent>()
            .add_message::<ContextAction>()
            .add_systems(
                Update,
                (
                    keyboard_cancel.in_set(PlayerCommands),
                    (drop_picks, act_on_pick, pick)
                        .chain()
                        .in_set(ActionStage::Decide),
                )
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                mark_picks.in_set(MarkCells).in_set(GamePhase::Visibility),
            );
    }
}

/// Backspace or `C` cancels (Esc is the settings menu).
pub fn keyboard_cancel(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut cancel: MessageWriter<CancelIntent>,
) {
    if keys.is_some_and(|keys| keys.any_just_pressed([KeyCode::Backspace, KeyCode::KeyC])) {
        cancel.write(CancelIntent);
    }
}

/// Drops every pick on a cancel, and any whose target is gone or no longer
/// where it was picked.
pub fn drop_picks(
    mut commands: Commands,
    grid: Res<MapGrid>,
    mut cancels: MessageReader<CancelIntent>,
    selectors: Query<(Entity, &Pos, &Selected)>,
    targets: Query<&Pos, (With<Selectable>, Without<DestroyRequested>)>,
) {
    let cancelled = cancels.read().count() > 0;
    for (selector, pos, selected) in &selectors {
        let in_place = targets
            .get(selected.target)
            .is_ok_and(|target| target.0 == grid.safe_pos(pos.0 + selected.dir));
        if cancelled || !in_place {
            commands.entity(selector).remove::<Selected>();
        }
    }
}

/// A fresh move by a selector with a pick acts on it: a `ContextAction`,
/// which takes the turn (the move becomes a wait). The move that picked it,
/// still held on, does nothing.
pub fn act_on_pick(
    mut commands: Commands,
    turn: Res<TurnState>,
    mut actions: MessageWriter<ContextAction>,
    mut selectors: Query<(Entity, &Selected, &mut MoveCommand, &Tokens), Without<DestroyRequested>>,
) {
    for (selector, selected, mut command, tokens) in &mut selectors {
        if !ready(&command, tokens, &turn) {
            continue;
        }
        if command.repeat && command.target == selected.dir {
            command.active = false;
            continue;
        }
        actions.write(ContextAction {
            actor: selector,
            target: selected.target,
            picked: selected.dir,
            dir: command.target,
        });
        command.target = IVec2::ZERO;
        commands.entity(selector).remove::<Selected>();
    }
}

/// A selector's move into something selectable picks it instead, for free.
pub fn pick(
    mut commands: Commands,
    grid: Res<MapGrid>,
    turn: Res<TurnState>,
    mut selectors: Query<
        (Entity, &Pos, &mut MoveCommand, &Tokens),
        (With<Selector>, Without<Selected>, Without<DestroyRequested>),
    >,
    selectable: Query<(), (With<Selectable>, Without<DestroyRequested>)>,
) {
    for (selector, pos, mut command, tokens) in &mut selectors {
        if !ready(&command, tokens, &turn) {
            continue;
        }
        let dir = command.target;
        let Some(target) = grid
            .get(grid.safe_pos(pos.0 + dir))
            .filter(|&target| selectable.contains(target))
        else {
            continue;
        };
        commands.entity(selector).insert(Selected { target, dir });
        command.active = false;
    }
}

/// A move about to be carried out this frame.
fn ready(command: &MoveCommand, tokens: &Tokens, turn: &TurnState) -> bool {
    command.active
        && command.relative
        && command.target != IVec2::ZERO
        && tokens.count > 0
        && !turn.simulation
}

/// Marks every picked target.
pub fn mark_picks(
    marks: Option<ResMut<CellMarks>>,
    selections: Query<&Selected>,
    positions: Query<&Pos>,
) {
    let Some(mut marks) = marks else {
        return;
    };
    for selected in &selections {
        if let Ok(pos) = positions.get(selected.target) {
            marks.0.push((pos.0, SELECTION_MARK));
        }
    }
}

#[cfg(test)]
mod tests;
