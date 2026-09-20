//! Vision systems. Mirrors `LightSourceSystems`, `FieldOfViewSystem`, and
//! `PlayerFieldOfViewSystem`.
//!
//! Differences from the original, all intentional and recorded:
//! - FOV caches key off the grid's obstacle revision in addition to
//!   position/radius: the original cache ignores obstacle changes, so a
//!   moved actor would leave a stale shadow.
//! - `LightSourceSystems`' player-sensor overwrite of the light radius is
//!   kept (same values in practice).

use bevy::prelude::*;

use crate::components::*;
use crate::fov::{FovComputer, FovSample};

/// Shared FOV computer (mirrors the single `FieldOfViewComputationInt2`
/// instance owned by the system; its occlusion set is cleared per compute).
#[derive(Resource, Debug, Default)]
pub struct FovShared {
    computer: FovComputer,
    scratch: Vec<FovSample>,
}

/// Mirrors `LightSourceSystems`: lights request their radius (keeping the
/// max), then player sensors overwrite with their own radius.
pub fn ensure_fov_requests(
    mut commands: Commands,
    lights: Query<(Entity, &LightSource, Option<&FovRequest>)>,
    players: Query<(Entity, &VisualSensor, Option<&FovRequest>)>,
) {
    for (e, light, existing) in lights.iter() {
        let radius = existing.map(|r| r.radius.max(light.radius)).unwrap_or(light.radius);
        commands.entity(e).insert(FovRequest { radius });
    }
    for (e, sensor, _) in players.iter() {
        commands.entity(e).insert(FovRequest {
            radius: sensor.radius,
        });
    }
}

/// Mirrors `FieldOfViewSystem`: recompute on moved observer, changed radius,
/// or changed obstacles; accumulate fractional visibility into the result
/// field (toroidally wrapped, radius-filtered, like the original).
pub fn compute_fov(
    mut commands: Commands,
    grid: Res<MapGrid>,
    mut shared: ResMut<FovShared>,
    mut sources: Query<(Entity, &Pos, &FovRequest, Option<&mut FovResult>)>,
    blockers: Query<(), (With<Collider>, Without<Speed>)>,
) {
    for (e, pos, request, result) in sources.iter_mut() {
        let cached = result.as_ref().map(|r| {
            r.pos == pos.0 && r.radius == request.radius && r.obstacle_revision == grid.revision
        });
        if cached == Some(true) {
            commands.entity(e).remove::<FovRequest>();
            continue;
        }
        let w = grid.width;
        let h = grid.height;
        let mut data = result
            .as_ref()
            .map(|r| r.data.clone())
            .unwrap_or_else(|| vec![0.0; (w * h) as usize]);
        if data.len() != (w * h) as usize {
            data.resize((w * h) as usize, 0.0);
        }
        data.fill(0.0);
        let revision = result.as_ref().map(|r| r.revision + 1).unwrap_or(0);
        // Obstacle = out of bounds, or an occupant that cannot move
        // (mirrors `HasObstacle`: no `Speed` pool entry on the occupant).
        let is_obstacle = |origin: IVec2, delta: IVec2| {
            let p = origin + delta;
            if !grid.is_valid(p) {
                return true;
            }
            match grid.get(grid.safe_pos(p)) {
                None => false,
                Some(o) => blockers.get(o).is_ok(),
            }
        };
        let FovShared { computer, scratch } = &mut *shared;
        computer.compute(pos.0, request.radius, is_obstacle, scratch);
        let radius_sq = request.radius * request.radius;
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
            if idx < data.len() {
                data[idx] += sample.value;
            }
        }
        commands.entity(e).insert(FovResult {
            revision,
            obstacle_revision: grid.revision,
            pos: pos.0,
            radius: request.radius,
            data,
        });
        commands.entity(e).remove::<FovRequest>();
    }
}

/// Mirrors `PlayerFieldOfViewSystem`: the visibility layer persists across
/// recomputes. FOV above 0.1 marks Visible+Known, otherwise only Visible is
/// cleared, so explored cells stay Known.
pub fn player_visibility(
    mut commands: Commands,
    grid: Res<MapGrid>,
    players: Query<(Entity, &FovResult, Option<&VisibilityMap>), With<Player>>,
) {
    for (e, fov, existing) in players.iter() {
        if existing.as_ref().map(|v| v.revision) == Some(fov.revision) {
            continue;
        }
        let n = (grid.width * grid.height) as usize;
        let mut data = existing
            .map(|v| v.data.clone())
            .unwrap_or_else(|| vec![Vis::empty(); n]);
        if data.len() != n {
            data.resize(n, Vis::empty());
        }
        for (i, v) in fov.data.iter().enumerate() {
            if i >= data.len() {
                break;
            }
            if *v > crate::components::VISIBILITY_THRESHOLD {
                data[i] |= Vis::VISIBLE | Vis::KNOWN;
            } else {
                data[i] &= !Vis::VISIBLE;
            }
        }
        commands.entity(e).insert(VisibilityMap {
            revision: fov.revision,
            data,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::test_app;

    fn spawn_player(app: &mut bevy::prelude::App, pos: IVec2) -> Entity {
        app.world_mut()
            .spawn((Active, Player(0), Pos(pos), Speed::default(), VisualSensor { radius: 4 }))
            .id()
    }

    #[test]
    fn sensor_request_flows_into_visibility() {
        let mut app = test_app::headless();
        app.init_resource::<FovShared>();
        let p = spawn_player(&mut app, IVec2::new(4, 4));
        app.update();
        // Request consumed, result cached, visibility derived.
        assert!(app.world().get::<FovRequest>(p).is_none());
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
        app.world_mut().resource_mut::<MapGrid>().set(IVec2::new(2, 1), wall);
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
        app.world_mut().resource_mut::<MapGrid>().set(IVec2::new(2, 1), wall);
        // ensure_fov_requests only fires for lights/players-with-sensor...
        // the player has a sensor, so a fresh request appears.
        app.update();
        let rev3 = app.world().get::<FovResult>(p).unwrap().revision;
        assert!(rev3 > rev2, "stale FOV must refresh on obstacle change");
    }
}
