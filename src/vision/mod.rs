//! Vision systems. Mirrors `LightSourceSystems`, `FieldOfViewSystem`, and
//! `PlayerFieldOfViewSystem`.
//!
//! Differences from the original, all intentional and recorded:
//! - FOV caches key off the grid's obstacle revision in addition to
//!   position/radius: the original cache ignores changed static obstacles.
//!   Actors with Speed do not cast shadows in either implementation.
//! - `LightSourceSystems`' player-sensor overwrite of the light radius is
//!   kept (same values in practice).

#![allow(clippy::type_complexity)]

mod plugin;

pub use plugin::*;

use bevy::prelude::*;

use crate::foundation::fov::{FovComputer, FovSample, PortalFovComputer};
use crate::model::*;

/// Shared FOV computer (mirrors the single `FieldOfViewComputationInt2`
/// instance owned by the system; its occlusion set is cleared per compute).
#[derive(Resource, Debug)]
pub struct FovShared {
    computer: FovComputer,
    scratch: Vec<FovSample>,
}

impl Default for FovShared {
    fn default() -> Self {
        Self {
            computer: FovComputer::default(),
            // Radius 16 contains 33x33 square-ring samples. It is the largest
            // bundled source and avoids growth during the normal game.
            scratch: Vec::with_capacity(33 * 33),
        }
    }
}

/// The player's field of view through portals (its storage is reused).
#[derive(Resource, Debug, Default)]
pub struct PortalFovShared(PortalFovComputer);

/// Computes what the player sees through portals: the cells within the
/// sensor's (round) radius and the map cells they show. Lights keep the
/// plain field of view in `compute_fov`.
pub fn compute_player_view(
    grid: Res<MapGrid>,
    mut shared: ResMut<PortalFovShared>,
    mut players: Query<(&Pos, &VisualSensor, &mut PlayerView), With<Player>>,
) {
    for (pos, sensor, mut view) in &mut players {
        let radius = sensor.radius;
        if view.pos == pos.0
            && view.radius == radius
            && view.obstacle_revision == grid.blocker_revision
            && view.portal_revision == grid.portal_revision
        {
            continue;
        }
        let is_obstacle = |p: IVec2| !grid.is_valid(p) || grid.blocks_vision(p);
        let PlayerView { samples, .. } = &mut *view;
        shared
            .0
            .compute(pos.0, radius, is_obstacle, |p| grid.portal_at(p), samples);
        // Like the plain field of view: round, and within the map.
        let radius_sq = radius * radius;
        samples.retain(|sample| {
            sample.delta.length_squared() <= radius_sq && grid.is_valid(sample.world)
        });
        view.revision = view.revision.wrapping_add(1);
        view.pos = pos.0;
        view.radius = radius;
        view.obstacle_revision = grid.blocker_revision;
        view.portal_revision = grid.portal_revision;
    }
}

/// Allocate per-source output storage when a light or sensor is created.
/// This is a spawn/configuration cost; normal recomputes mutate it in place.
pub fn ensure_fov_storage(
    mut commands: Commands,
    grid: Res<MapGrid>,
    sources: Query<
        Entity,
        (
            Or<(With<LightSource>, With<VisualSensor>)>,
            Without<FovResult>,
        ),
    >,
    players: Query<Entity, (With<Player>, Without<VisibilityMap>)>,
    viewers: Query<(Entity, &VisualSensor), (With<Player>, Without<PlayerView>)>,
) {
    for (e, sensor) in viewers.iter() {
        commands
            .entity(e)
            .insert(PlayerView::with_radius(sensor.radius));
    }
    let n = (grid.width * grid.height) as usize;
    for e in sources.iter() {
        commands.entity(e).insert(FovResult {
            revision: 0,
            obstacle_revision: u64::MAX,
            pos: IVec2::splat(i32::MIN),
            radius: -1,
            data: vec![0.0; n],
        });
    }
    for e in players.iter() {
        commands.entity(e).insert(VisibilityMap {
            revision: u64::MAX,
            data: vec![Vis::empty(); n],
        });
    }
}

/// Mirrors `FieldOfViewSystem`: recompute on moved observer, changed radius,
/// or changed obstacles; accumulate fractional visibility into the result
/// field (toroidally wrapped, radius-filtered, like the original).
pub fn compute_fov(
    grid: Res<MapGrid>,
    mut shared: ResMut<FovShared>,
    mut sources: Query<(
        &Pos,
        Option<&LightSource>,
        Option<&VisualSensor>,
        &mut FovResult,
    )>,
) {
    for (pos, light, sensor, mut result) in sources.iter_mut() {
        let radius = sensor
            .map(|sensor| sensor.radius)
            .or_else(|| light.map(|light| light.radius))
            .unwrap_or(0);
        if result.pos == pos.0
            && result.radius == radius
            && result.obstacle_revision == grid.blocker_revision
        {
            continue;
        }
        let w = grid.width;
        let h = grid.height;
        if result.data.len() != (w * h) as usize {
            result.data.resize((w * h) as usize, 0.0);
        }
        result.data.fill(0.0);
        // Obstacle = out of bounds, or an occupant that cannot move
        // (mirrors `HasObstacle`: no `Speed` pool entry on the occupant).
        let is_obstacle = |origin: IVec2, delta: IVec2| {
            let p = origin + delta;
            if !grid.is_valid(p) {
                return true;
            }
            grid.blocks_vision(grid.safe_pos(p))
        };
        let FovShared { computer, scratch } = &mut *shared;
        computer.compute(pos.0, radius, is_obstacle, scratch);
        let radius_sq = radius * radius;
        for sample in scratch.iter() {
            let p = pos.0 + sample.delta;
            if !grid.is_valid(p) {
                continue;
            }
            let d = sample.delta;
            if d.x * d.x + d.y * d.y > radius_sq {
                continue;
            }
            let wrapped = grid.safe_pos(p);
            let idx = (wrapped.y * w + wrapped.x) as usize;
            if idx < result.data.len() {
                result.data[idx] += sample.value;
            }
        }
        result.revision = result.revision.wrapping_add(1);
        result.obstacle_revision = grid.blocker_revision;
        result.pos = pos.0;
        result.radius = radius;
    }
}

/// Mirrors `PlayerFieldOfViewSystem`: the visibility layer persists across
/// recomputes. A map cell seen (directly or through a portal) above 0.1 is
/// marked Visible+Known; every other cell only loses Visible, so explored
/// cells stay Known.
pub fn player_visibility(
    grid: Res<MapGrid>,
    mut players: Query<(&PlayerView, &mut VisibilityMap), With<Player>>,
) {
    for (view, mut visibility) in players.iter_mut() {
        if visibility.revision == view.revision {
            continue;
        }
        let n = (grid.width * grid.height) as usize;
        if visibility.data.len() != n {
            visibility.data.resize(n, Vis::empty());
        }
        for cell in &mut visibility.data {
            cell.remove(Vis::VISIBLE);
        }
        for sample in &view.samples {
            if sample.value <= crate::model::VISIBILITY_THRESHOLD {
                continue;
            }
            if let Some(i) = grid.idx(sample.world) {
                visibility.data[i] |= Vis::VISIBLE | Vis::KNOWN;
            }
        }
        visibility.revision = view.revision;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::test_app;

    fn spawn_player(app: &mut bevy::prelude::App, pos: IVec2) -> Entity {
        app.world_mut()
            .spawn((
                Active,
                Player(0),
                Pos(pos),
                Speed::default(),
                VisualSensor { radius: 4 },
            ))
            .id()
    }

    #[test]
    fn sensor_request_flows_into_visibility() {
        let mut app = test_app::headless();
        app.init_resource::<FovShared>();
        let p = spawn_player(&mut app, IVec2::new(4, 4));
        app.update();
        // Result storage is created once, then visibility is derived.
        let result = app.world().get::<FovResult>(p).expect("fov result");
        assert_eq!(result.radius, 4);
        let vis = app.world().get::<VisibilityMap>(p).expect("visibility");
        let idx = 36; // (4, 4) on the 8-wide grid
        assert!(vis.data[idx].contains(Vis::VISIBLE));
        assert!(vis.data[idx].contains(Vis::KNOWN));
    }

    #[test]
    fn wall_blocks_vision_and_known_persists() {
        let mut app = test_app::headless();
        app.init_resource::<FovShared>();
        let p = spawn_player(&mut app, IVec2::new(1, 1));
        // Static wall directly east: collider without Speed.
        let wall = app
            .world_mut()
            .spawn((Active, Collider, Pos(IVec2::new(2, 1))))
            .id();
        app.world_mut()
            .resource_mut::<MapGrid>()
            .set(IVec2::new(2, 1), wall);
        app.update();
        let vis = app.world().get::<VisibilityMap>(p).expect("visibility");
        // Cell behind the wall is neither visible nor known...
        let behind = 11; // (3, 1)
        assert_eq!(vis.data[behind], Vis::empty());
        // ...while the wall face itself is seen.
        let face = 10; // (2, 1)
        assert!(vis.data[face].contains(Vis::VISIBLE));

        // Move the player away: the far cell stays unknown, but a cell
        // the player once saw keeps Known without Visible.
        let seen = 9; // (1, 1)
        assert!(vis.data[seen].contains(Vis::KNOWN));
        app.world_mut().get_mut::<Pos>(p).unwrap().0 = IVec2::new(6, 6);
        app.update();
        app.update();
        let vis2 = app.world().get::<VisibilityMap>(p).expect("visibility2");
        assert!(!vis2.data[seen].contains(Vis::VISIBLE));
        assert!(vis2.data[seen].contains(Vis::KNOWN));
    }

    #[test]
    fn obstacle_move_invalidates_cache() {
        let mut app = test_app::headless();
        app.init_resource::<FovShared>();
        let p = spawn_player(&mut app, IVec2::new(1, 1));
        app.update();
        let rev1 = app.world().get::<FovResult>(p).unwrap().revision;
        // No changes: no recompute.
        app.update();
        let rev2 = app.world().get::<FovResult>(p).unwrap().revision;
        assert_eq!(rev1, rev2);
        // Occupancy change bumps the grid revision -> recompute.
        let wall = app
            .world_mut()
            .spawn((Active, Collider, Pos(IVec2::new(2, 1))))
            .id();
        app.world_mut()
            .resource_mut::<MapGrid>()
            .set(IVec2::new(2, 1), wall);
        // The player's sensor observes the changed blocker revision.
        app.update();
        let rev3 = app.world().get::<FovResult>(p).unwrap().revision;
        assert!(rev3 > rev2, "stale FOV must refresh on obstacle change");
    }
}
