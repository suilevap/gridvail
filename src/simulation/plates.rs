use bevy::prelude::*;

use crate::model::*;

/// A stand-in trigger for trying rooms out, until a trigger/action system
/// replaces it: a plate presses when an actor steps onto it, and each press
/// runs the one room action there is, [`Room::advance`], on every room.
/// Standing on it does not press it again.
pub fn press_plates(
    grid: Res<MapGrid>,
    actors: Query<(), With<Speed>>,
    mut plates: Query<(&Pos, &mut PressurePlate)>,
    mut rooms: Query<&mut Room>,
) {
    for (pos, mut plate) in &mut plates {
        let pressed = grid
            .get(pos.0)
            .is_some_and(|occupant| actors.contains(occupant));
        if pressed && !plate.pressed {
            for mut room in &mut rooms {
                room.advance();
            }
        }
        plate.set_if_neq(PressurePlate { pressed });
    }
}
