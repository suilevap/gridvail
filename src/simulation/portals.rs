use bevy::prelude::*;

use crate::foundation::portal::{CellTransform, PortalFace};
use crate::model::*;

/// Keeps the grid's portal faces in step with `Portal` components, which
/// may be added, removed, moved or retargeted at any time. Any change
/// rebuilds the (few) faces.
pub fn sync_portals(
    mut grid: ResMut<MapGrid>,
    changed: Query<(), Or<(Changed<Portal>, (With<Portal>, Changed<Pos>))>>,
    mut removed: RemovedComponents<Portal>,
    portals: Query<(&Pos, &Portal)>,
) {
    let removed = removed.read().count() > 0;
    if changed.is_empty() && !removed {
        return;
    }
    grid.clear_portals();
    for (pos, portal) in &portals {
        let Ok((exit_pos, exit)) = portals.get(portal.exit) else {
            continue;
        };
        let face = PortalFace {
            side: portal.side,
            through: CellTransform::between_faces(pos.0, portal.side, exit_pos.0, exit.side),
        };
        grid.set_portal(pos.0, Some(face));
    }
}
