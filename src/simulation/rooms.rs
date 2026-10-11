use bevy::prelude::*;

use super::wall_bundle;
use crate::model::*;

/// Switches a room to the layout asked for: walls the new layout lacks
/// leave the map and the ones it adds are placed on it, all in one pass. A
/// room waits while anything stands where a new wall goes. The walls around
/// every changed cell are autotiled again (`tile_system`).
///
/// New walls are [`Rising`]. Walls that go stop being walls at once and are
/// left [`Sinking`] for the animation step to show going down and despawn;
/// without one (no `AnimatedPos`) they are despawned here.
pub fn transform_rooms(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    mut rooms: Query<&mut Room>,
    walls: Query<Has<AnimatedPos>, With<Wall>>,
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
            let Some(wall) = grid.get(p) else { continue };
            let Ok(animated) = walls.get(wall) else {
                continue;
            };
            grid.clear(p);
            if animated {
                commands
                    .entity(wall)
                    .remove::<(Wall, Collider, Active, Tile)>()
                    .insert(Sinking);
            } else {
                commands.entity(wall).despawn();
            }
        }
        for p in added() {
            let wall = commands.spawn((wall_bundle(p), Rising)).id();
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
