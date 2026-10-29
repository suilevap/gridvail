use bevy::prelude::*;

use crate::lighting::RED;
use crate::model::*;

use super::mark;

/// How far an attacker swings toward what it attacks, in cells.
const SWING: f32 = 0.45;

/// How a cell about to be struck is marked.
pub const STRIKE_MARK: CellMark = mark('×', RED);

/// An actor acting on what it picked swings at it.
pub fn swing_on_actions(
    mut actions: MessageReader<ContextAction>,
    mut gestures: MessageWriter<Gesture>,
) {
    for action in actions.read() {
        gestures.write(Gesture {
            entity: action.actor,
            toward: action.picked.as_vec2() * SWING,
        });
    }
}

/// A weapon striking swings its wielder the way it strikes.
pub fn swing_on_strikes(mut strikes: MessageReader<Strike>, mut gestures: MessageWriter<Gesture>) {
    for strike in strikes.read() {
        gestures.write(Gesture {
            entity: strike.wielder,
            toward: strike.dir.as_vec2() * SWING,
        });
    }
}

/// Marks every cell an aimed weapon will strike.
pub fn mark_strikes(
    grid: Res<MapGrid>,
    marks: Option<ResMut<CellMarks>>,
    wielders: Query<(&Pos, &Weapon, &Telegraph), Without<DestroyRequested>>,
) {
    let Some(mut marks) = marks else {
        return;
    };
    for (pos, weapon, telegraph) in &wielders {
        for cell in weapon.area.cells(pos.0, telegraph.dir) {
            marks.0.push((grid.safe_pos(cell), STRIKE_MARK));
        }
    }
}
