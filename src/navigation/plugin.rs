use bevy::prelude::*;

use crate::model::*;
use crate::schedule::GamePhase;

use super::NavMap;

/// Keeps [`NavMap`] in step with the map's walls and doors.
pub struct NavigationPlugin;

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavMap>()
            .add_systems(Update, update_nav_map.in_set(GamePhase::Navigation));
    }
}

/// Rebuilds the snapshot when a wall or closed door appears, moves, or goes
/// (an opened door stops blocking sight, which bumps the blocker revision).
pub fn update_nav_map(
    grid: Res<MapGrid>,
    mut nav: ResMut<NavMap>,
    walls: Query<&Pos, With<Wall>>,
    doors: Query<(&Pos, &Door)>,
) {
    if !nav.is_stale(&grid) {
        return;
    }
    nav.rebuild(
        &grid,
        walls.iter().map(|pos| pos.0),
        doors
            .iter()
            .filter(|(_, door)| !door.open)
            .map(|(pos, _)| pos.0),
    );
}
