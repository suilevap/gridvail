use bevy::prelude::*;

use super::room_plans::{RoomPlanners, SwitchContext, SwitchPlan, WallChange};
use super::wall_bundle;
use crate::model::*;

/// Switches a room to the layout asked for, all in one pass. A room waits
/// while anything stands where a new wall goes. The walls around every
/// changed cell are autotiled again (`tile_system`).
///
/// How the walls get there is the room's planner's choice
/// ([`RoomPlanners`]); a plan that would not give the new layout is
/// replaced by rising and sinking. This carries the plan out:
///
/// - a sliding wall keeps its entity, is marked [`Sliding`] and only its
///   `Pos` changes;
/// - a rising wall is spawned [`Rising`];
/// - a sinking wall stops being a wall at once and is left [`Sinking`] for
///   the animation step to show going down and despawn; without one (no
///   `AnimatedPos`) it is despawned here.
pub fn transform_rooms(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    planners: Res<RoomPlanners>,
    mut rooms: Query<&mut Room>,
    walls: Query<Has<AnimatedPos>, With<Wall>>,
) {
    let wall_at = |grid: &MapGrid, p: IVec2| grid.get(p).filter(|&e| walls.contains(e));
    for mut room in &mut rooms {
        if room.current == room.target {
            continue;
        }
        let plan = {
            let room = &*room;
            let (from, to) = (&room.layouts[room.current], &room.layouts[room.target]);
            if to
                .iter()
                .any(|p| !from.contains(p) && grid.get(*p).is_some())
            {
                continue;
            }
            let free = |p: IVec2| grid.get(p).is_none();
            let wall = |p: IVec2| wall_at(&grid, p).is_some();
            let switch = SwitchContext::new(room, from, to, &free, &wall);
            let plan = planners.for_room(room).plan(&switch);
            match plan.check(&switch) {
                Ok(()) => plan,
                Err(problem) => {
                    warn!(
                        "room at {}: its plan does not give the new layout ({problem}); \
                         walls rise and sink",
                        room.origin
                    );
                    SwitchPlan::rise_and_sink(&switch)
                }
            }
        };
        carry_out(&mut commands, &mut grid, &walls, &plan);
        for p in plan.sources().chain(plan.targets()) {
            for side in [IVec2::X, IVec2::Y, IVec2::NEG_X, IVec2::NEG_Y] {
                if let Some(wall) = wall_at(&grid, grid.safe_pos(p + side)) {
                    commands.entity(wall).insert(Tile::wall());
                }
            }
        }
        room.current = room.target;
    }
}

/// Carries out a checked plan: every wall leaves its cell before any
/// arrives, so walls can slide into each other's places.
fn carry_out(
    commands: &mut Commands,
    grid: &mut MapGrid,
    walls: &Query<Has<AnimatedPos>, With<Wall>>,
    plan: &SwitchPlan,
) {
    let leaving: Vec<(WallChange, Entity, bool)> = plan
        .changes
        .iter()
        .filter_map(|&change| {
            let source = match change {
                WallChange::Slide { from, .. } => from,
                WallChange::Sink { at } => at,
                WallChange::Rise { .. } => return None,
            };
            let wall = grid.get(source)?;
            Some((change, wall, walls.get(wall).ok()?))
        })
        .collect();
    for source in plan.sources() {
        grid.clear(source);
    }
    for &(change, wall, animated) in &leaving {
        match change {
            WallChange::Slide { to, .. } => {
                grid.set(to, wall);
                commands
                    .entity(wall)
                    .insert((Pos(to), Sliding, Tile::wall()));
            }
            WallChange::Sink { .. } if animated => {
                commands
                    .entity(wall)
                    .remove::<(Wall, Collider, Active, Tile)>()
                    .insert(Sinking);
            }
            WallChange::Sink { .. } => commands.entity(wall).despawn(),
            WallChange::Rise { .. } => {}
        }
    }
    for change in &plan.changes {
        if let WallChange::Rise { at } = *change {
            let wall = commands.spawn((wall_bundle(at), Rising)).id();
            grid.set(at, wall);
        }
    }
}
