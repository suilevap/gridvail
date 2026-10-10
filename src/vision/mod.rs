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

use crate::foundation::fov::{PortalFovComputer, ViewSample};
use crate::model::*;

/// Shared FOV computer for lights and sensors (mirrors the single
/// `FieldOfViewComputationInt2` instance owned by the system; its occlusion
/// sets are cleared per compute). It sees through portals.
#[derive(Resource, Debug, Default)]
pub struct FovShared {
    computer: PortalFovComputer,
}

/// The player's field of view through portals (its storage is reused).
#[derive(Resource, Debug, Default)]
pub struct PortalFovShared(PortalFovComputer);

/// Computes what the player sees through portals: the cells within the
/// sensor's (round) radius and the map cells they show.
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
        commands.entity(e).insert(FovResult::empty(n));
    }
    for e in players.iter() {
        commands.entity(e).insert(VisibilityMap {
            revision: u64::MAX,
            data: vec![Vis::empty(); n],
        });
    }
}

/// Mirrors `FieldOfViewSystem`: recompute on moved observer, changed radius,
/// or changed obstacles or portals. Lights and sensors see through portals
/// as the player does: the samples keep the cells around the source and the
/// map cells they show, and `data` keeps per map cell the best visibility
/// over every way it is seen (radius-filtered, like the original). Without
/// portals it equals the original's field.
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
            && result.portal_revision == grid.portal_revision
        {
            continue;
        }
        let n = (grid.width * grid.height) as usize;
        let FovResult { data, samples, .. } = &mut *result;
        if data.len() != n {
            data.resize(n, 0.0);
        }
        data.fill(0.0);
        // Obstacle = out of bounds, or an occupant that cannot move
        // (mirrors `HasObstacle`: no `Speed` pool entry on the occupant).
        let is_obstacle = |p: IVec2| !grid.is_valid(p) || grid.blocks_vision(p);
        shared
            .computer
            .compute(pos.0, radius, is_obstacle, |p| grid.portal_at(p), samples);
        let radius_sq = radius * radius;
        samples.retain(|sample: &ViewSample| {
            sample.delta.length_squared() <= radius_sq && grid.is_valid(sample.world)
        });
        for sample in samples.iter() {
            if let Some(i) = grid.idx(sample.world) {
                data[i] = data[i].max(sample.value);
            }
        }
        result.revision = result.revision.wrapping_add(1);
        result.obstacle_revision = grid.blocker_revision;
        result.portal_revision = grid.portal_revision;
        result.pos = pos.0;
        result.radius = radius;
    }
}

/// Mirrors `PlayerFieldOfViewSystem`: the visibility layer persists across
/// recomputes. A map cell seen directly above 0.1 is marked Visible+Known;
/// one seen only through a portal is Visible while in sight but not
/// remembered. Every other cell only loses Visible, so explored cells stay
/// Known.
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
                visibility.data[i] |= if sample.transform.is_identity() {
                    Vis::VISIBLE | Vis::KNOWN
                } else {
                    Vis::VISIBLE
                };
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
    fn cells_seen_through_a_portal_are_visible_but_not_remembered() {
        use crate::foundation::portal::{CellTransform, PortalFace};

        let mut app = test_app::headless();
        app.init_resource::<FovShared>();
        // A wall at (3, 1) open to the west, leading out of a wall at
        // (4, 6) open to the east: (3, 1) shows (5, 6).
        let wall = app
            .world_mut()
            .spawn((Active, Collider, Pos(IVec2::new(3, 1))))
            .id();
        {
            let mut grid = app.world_mut().resource_mut::<MapGrid>();
            grid.set(IVec2::new(3, 1), wall);
            grid.set_portal(
                IVec2::new(3, 1),
                Some(PortalFace {
                    side: IVec2::NEG_X,
                    through: CellTransform::between_faces(
                        IVec2::new(3, 1),
                        IVec2::NEG_X,
                        IVec2::new(4, 6),
                        IVec2::X,
                    ),
                }),
            );
        }
        let p = spawn_player(&mut app, IVec2::new(1, 1));
        app.update();
        let vis = app.world().get::<VisibilityMap>(p).expect("visibility");
        let at = |x: i32, y: i32| vis.data[(y * 8 + x) as usize];
        assert_eq!(at(2, 1), Vis::VISIBLE | Vis::KNOWN, "seen directly");
        assert_eq!(at(5, 6), Vis::VISIBLE, "seen through the portal only");

        // Once out of sight, it is forgotten; the direct view is not.
        app.world_mut().get_mut::<Pos>(p).unwrap().0 = IVec2::new(1, 5);
        app.update();
        let vis = app.world().get::<VisibilityMap>(p).expect("visibility");
        assert!(!vis.data[6 * 8 + 5].contains(Vis::KNOWN));
        assert!(vis.data[8 + 2].contains(Vis::KNOWN));
    }

    /// A wall at (3, 1) open to the west leading out of a wall at (4, 6)
    /// open to the east, on the 8x8 test grid: from the west, (3, 1) shows
    /// (5, 6).
    fn portal_grid(app: &mut bevy::prelude::App) {
        use crate::foundation::portal::{CellTransform, PortalFace};

        let wall = app
            .world_mut()
            .spawn((Active, Collider, Pos(IVec2::new(3, 1))))
            .id();
        let mut grid = app.world_mut().resource_mut::<MapGrid>();
        grid.set(IVec2::new(3, 1), wall);
        grid.set_portal(
            IVec2::new(3, 1),
            Some(PortalFace {
                side: IVec2::NEG_X,
                through: CellTransform::between_faces(
                    IVec2::new(3, 1),
                    IVec2::NEG_X,
                    IVec2::new(4, 6),
                    IVec2::X,
                ),
            }),
        );
    }

    #[test]
    fn lights_shine_through_portals_and_fade_by_the_way_they_came() {
        use crate::presentation::{render_light_layers, DynamicLight};
        use bevy::ecs::system::RunSystemOnce;

        let mut app = test_app::headless();
        app.insert_resource(StaticLight::new())
            .insert_resource(DynamicLight::sized(64));
        portal_grid(&mut app);
        let lamp = app
            .world_mut()
            .spawn((
                Pos(IVec2::new(1, 1)),
                LightSource {
                    radius: 4,
                    kind: LightKind::Fire,
                    value: 100,
                },
            ))
            .id();
        app.update();
        let fov = app.world().get::<FovResult>(lamp).expect("fov result");
        // (5, 6) is 2 cells away through the portal, but too far for the
        // lamp in a straight line (about 6.4 > 4).
        let through = (6 * 8 + 5) as usize;
        assert_eq!(fov.data[through], 1.0, "seen through the portal");
        app.world_mut()
            .run_system_once(render_light_layers)
            .unwrap();
        let light = app.world().resource::<DynamicLight>().data[through];
        // Faded as 2 cells away: 100 * (1 - 4 / 25).
        assert_eq!((light.value, light.kind), (84, LightKind::Fire));

        // Without the portal the wall stops it, and (5, 6) stays dark.
        app.world_mut()
            .resource_mut::<MapGrid>()
            .set_portal(IVec2::new(3, 1), None);
        app.update();
        let fov = app.world().get::<FovResult>(lamp).expect("fov result");
        assert_eq!(fov.data[through], 0.0);
        app.world_mut()
            .run_system_once(render_light_layers)
            .unwrap();
        assert_eq!(
            app.world().resource::<DynamicLight>().data[through].kind,
            LightKind::None
        );
    }

    #[test]
    fn without_portals_the_field_is_the_plain_one() {
        use crate::foundation::fov::{FovComputer, FovSample};
        use rand::{RngExt, SeedableRng};

        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        for _ in 0..20 {
            let mut app = test_app::headless();
            let mut walls = Vec::new();
            for _ in 0..12 {
                let p = IVec2::new(rng.random_range(0..8), rng.random_range(0..8));
                if p != IVec2::new(4, 4) && !walls.contains(&p) {
                    let wall = app.world_mut().spawn((Active, Collider, Pos(p))).id();
                    app.world_mut().resource_mut::<MapGrid>().set(p, wall);
                    walls.push(p);
                }
            }
            let sensor = app
                .world_mut()
                .spawn((Pos(IVec2::new(4, 4)), VisualSensor { radius: 5 }))
                .id();
            app.update();
            let fov = app.world().get::<FovResult>(sensor).expect("fov result");

            // The original field: accumulated, radius-filtered, in the map.
            let mut plain = vec![0.0_f32; 64];
            let mut samples: Vec<FovSample> = Vec::new();
            let is_wall =
                |p: IVec2| !(0..8).contains(&p.x) || !(0..8).contains(&p.y) || walls.contains(&p);
            FovComputer::new().compute(
                IVec2::new(4, 4),
                5,
                |origin, delta| is_wall(origin + delta),
                &mut samples,
            );
            for sample in &samples {
                let p = IVec2::new(4, 4) + sample.delta;
                if sample.delta.length_squared() <= 25 && !is_wall_bounds(p) {
                    plain[(p.y * 8 + p.x) as usize] += sample.value;
                }
            }
            assert_eq!(fov.data, plain, "walls {walls:?}");
        }

        fn is_wall_bounds(p: IVec2) -> bool {
            !(0..8).contains(&p.x) || !(0..8).contains(&p.y)
        }
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
