use bevy::prelude::*;

use crate::model::*;

pub fn has_destroy_requests(doomed: Query<(), With<DestroyRequested>>) -> bool {
    !doomed.is_empty()
}

/// Destruction is exceptional and may allocate in Bevy's command queue. The
/// application guards it with [`has_destroy_requests`].
pub fn destroy_entities(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    doomed: Query<(Entity, Option<&Pos>), With<DestroyRequested>>,
) {
    for (entity, pos) in doomed.iter() {
        if let Some(pos) = pos {
            if grid.get(pos.0) == Some(entity) {
                grid.clear(pos.0);
            }
        }
        commands.entity(entity).despawn();
    }
}
