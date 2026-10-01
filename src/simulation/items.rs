use bevy::prelude::*;

use crate::model::*;

/// Walking into a closed door opens it when the walker carries a key of the
/// door's color. The key is kept, and the actor enters on a later move.
pub fn open_doors(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    collisions: Res<CollisionBuffer>,
    carriers: Query<&Inventory, Without<DestroyRequested>>,
    keys: Query<&Key>,
    mut doors: Query<(&Pos, &mut Door, &mut Glyph)>,
) {
    for collision in &collisions.0 {
        let Ok((pos, mut door, mut glyph)) = doors.get_mut(collision.target) else {
            continue;
        };
        let Ok(inventory) = carriers.get(collision.source) else {
            continue;
        };
        let has_key = inventory
            .0
            .iter()
            .any(|&item| keys.get(item).is_ok_and(|key| key.0 == door.color));
        if door.open || !has_key {
            continue;
        }
        door.open = true;
        glyph.ch = Door::OPEN_GLYPH;
        glyph.depth = 0;
        if grid.get(pos.0) == Some(collision.target) {
            grid.clear(pos.0);
        }
        commands.entity(collision.target).remove::<Collider>();
    }
}

/// An actor with an inventory picks up every item on the cell it enters.
pub fn pick_up_items(
    mut commands: Commands,
    mut carriers: Query<(&Pos, &mut Inventory), (Changed<Pos>, Without<DestroyRequested>)>,
    items: Query<(Entity, &Pos), (With<Item>, Without<Collider>)>,
) {
    for (pos, mut inventory) in &mut carriers {
        for (item, at) in &items {
            if at.0 == pos.0 {
                inventory.0.push(item);
                commands.entity(item).remove::<Pos>();
            }
        }
    }
}

/// An actor being destroyed drops everything it carries on its cell. Like
/// destruction itself this is exceptional and may allocate.
pub fn drop_items(
    mut commands: Commands,
    mut carriers: Query<(Option<&Pos>, &mut Inventory), With<DestroyRequested>>,
) {
    for (pos, mut inventory) in &mut carriers {
        for item in inventory.0.drain(..) {
            let Ok(mut item) = commands.get_entity(item) else {
                continue;
            };
            match pos {
                Some(pos) => {
                    item.insert(Pos(pos.0));
                }
                // Nowhere to drop it: it goes with its carrier.
                None => item.despawn(),
            }
        }
    }
}
