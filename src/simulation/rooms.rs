use bevy::prelude::*;

use super::room_plans::{MoveKind, RoomPlanners, SwitchContext, SwitchPlan};
use super::wall_bundle;
use crate::model::*;

/// Switches a room to the layout asked for, all in one pass. A room waits
/// while anything stands where a new wall goes. The walls around every
/// changed cell are autotiled again (`tile_system`).
///
/// How the walls get there is the room's planner's choice
/// ([`RoomPlanners`]); a plan that would not give the new layout is
/// replaced by rising and sinking. This carries the plan out; every wall
/// slides and is marked [`Sliding`]:
///
/// - a wall that moves keeps its entity and only its `Pos` changes;
/// - a new wall is spawned at its cell, [`Rising`] from where its move
///   starts;
/// - a wall that goes stops being a wall at once, its `Pos` becomes where
///   its move ends, and it is left [`Sinking`] for the animation step to
///   show going there and down, and despawn; without one (no
///   `AnimatedPos`) it is despawned here.
pub fn transform_rooms(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    planners: Res<RoomPlanners>,
    mut rooms: Query<&mut Room>,
    walls: Query<Has<AnimatedPos>, With<Wall>>,
    rails: Query<(&Pos, &Rail)>,
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
            let rails: std::collections::HashMap<IVec2, Rail> =
                rails.iter().map(|(pos, rail)| (pos.0, *rail)).collect();
            let rail = |p: IVec2, step: IVec2| rails.get(&p).is_some_and(|rail| rail.along(step));
            let switch = SwitchContext::new(room, from, to, &free, &wall).with_rails(&rail);
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
    let leaving: Vec<_> = plan
        .moves
        .iter()
        .filter_map(|&step| {
            let wall = grid.get(step.source()?)?;
            Some((step, wall, walls.get(wall).ok()?))
        })
        .collect();
    for source in plan.sources() {
        grid.clear(source);
    }
    for &(step, wall, animated) in &leaving {
        match step.kind {
            MoveKind::Slide => {
                grid.set(step.to, wall);
                commands
                    .entity(wall)
                    .insert((Pos(step.to), Sliding, Tile::wall()));
            }
            MoveKind::Sink if animated => {
                commands
                    .entity(wall)
                    .remove::<(Wall, Collider, Active, Tile)>()
                    .insert((Pos(step.to), Sliding, Sinking));
            }
            MoveKind::Sink => commands.entity(wall).despawn(),
            MoveKind::Rise => {}
        }
    }
    for step in &plan.moves {
        if step.kind == MoveKind::Rise {
            let rising = Rising { from: step.from };
            let wall = commands.spawn((wall_bundle(step.to), rising, Sliding)).id();
            grid.set(step.to, wall);
        }
    }
}
