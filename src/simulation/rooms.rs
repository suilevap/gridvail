use bevy::prelude::*;

use super::wall_bundle;
use crate::model::*;

/// A plate presses when an actor steps onto it; each press asks every room
/// for its next layout. Standing on it does not press it again.
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

/// Switches a room to the layout asked for, all at once: walls the new
/// layout lacks are despawned and the ones it adds are spawned. A room
/// waits while anything stands where a new wall goes. The walls around
/// every changed cell are autotiled again (`tile_system`).
pub fn transform_rooms(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    mut rooms: Query<&mut Room>,
    walls: Query<(), With<Wall>>,
) {
    for mut room in &mut rooms {
        if room.current == room.target {
            continue;
        }
        let Room {
            layouts,
            current,
            target,
            ..
        } = &*room;
        let (from, to) = (&layouts[*current], &layouts[*target]);
        let added = || to.iter().copied().filter(|p| !from.contains(p));
        if added().any(|p| grid.get(p).is_some()) {
            continue;
        }
        let removed = from.iter().copied().filter(|p| !to.contains(p));
        for p in removed.clone() {
            if let Some(wall) = grid.get(p).filter(|&e| walls.contains(e)) {
                grid.clear(p);
                commands.entity(wall).despawn();
            }
        }
        for p in added() {
            let wall = commands.spawn(wall_bundle(p)).id();
            grid.set(p, wall);
        }
        for p in removed.chain(added()) {
            for side in [IVec2::X, IVec2::Y, IVec2::NEG_X, IVec2::NEG_Y] {
                let next = grid.safe_pos(p + side);
                if let Some(wall) = grid.get(next).filter(|&e| walls.contains(e)) {
                    commands.entity(wall).insert(Tile::wall());
                }
            }
        }
        room.current = room.target;
    }
}
