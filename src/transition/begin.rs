use std::collections::HashSet;

use bevy::prelude::*;

use super::{timer, Arrival, Forming, NextArrival, Showing, TransitionRoom};
use super::{CLOSE_SECONDS, FOCUS_RADIUS, SHOW_SECONDS};
use crate::camera::{CameraOperator, CameraTarget, ZOOM_RANGE};
use crate::foundation::oval::Oval;
use crate::model::*;
use crate::simulation::wall_bundle;

/// Starts a transition for the map just loaded, if it arrives from anything:
/// what [`NextArrival`] asks for, else the walls of the map before. The
/// map's plain walls give way to the arriving ones (not off the map, nor
/// where anything else stands), and a room covering the map is set to
/// switch back to them.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn begin_transition(
    mut commands: Commands,
    mut loaded: MessageReader<MapLoaded>,
    next: Option<Res<NextArrival>>,
    mut grid: ResMut<MapGrid>,
    camera: Option<Res<ViewCamera>>,
    mut operator: Option<ResMut<CameraOperator>>,
    map_walls: Query<(Entity, &Pos), (With<Wall>, Without<Portal>)>,
    placed: Query<&Pos, Without<Wall>>,
    players: Query<&Pos, With<Player>>,
) {
    let Some(loaded) = loaded.read().last() else {
        return;
    };
    let arrival = match next {
        Some(next) => {
            commands.remove_resource::<NextArrival>();
            next.0.clone()
        }
        None if loaded.previous_walls.is_empty() => return,
        None => Arrival::Standing(loaded.previous_walls.clone()),
    };
    let size = IVec2::new(grid.width, grid.height);
    let occupied: HashSet<IVec2> = placed.iter().map(|pos| pos.0).collect();
    let free =
        |p: IVec2| p.cmpge(IVec2::ZERO).all() && p.cmplt(size).all() && !occupied.contains(&p);
    let player = players.single().map_or(size / 2, |pos| pos.0);

    let mut target = Vec::with_capacity(map_walls.iter().len());
    for (wall, pos) in &map_walls {
        grid.clear(pos.0);
        commands.entity(wall).despawn();
        target.push(pos.0);
    }
    let zoom = operator
        .as_ref()
        .map_or(1.0, |operator| operator.target_zoom());
    match arrival {
        Arrival::Shown { walls, area } => {
            if let (Some(camera), Some(operator)) = (camera.as_deref(), operator.as_deref_mut()) {
                operator.look_at(CameraTarget::Point(area.oval.centre), camera);
                operator.zoom_to(ZOOM_RANGE.0);
                operator.snap();
            }
            let start = spawn_rising(&mut commands, &mut grid, walls, free);
            commands.spawn((
                TransitionRoom { zoom },
                Room::new(IVec2::ZERO, size, vec![start, target]),
                Showing(timer(SHOW_SECONDS)),
            ));
            commands.insert_resource(area);
        }
        Arrival::Standing(walls) => {
            let area = RevealArea {
                oval: Oval {
                    centre: player.as_vec2(),
                    radii: Vec2::splat(FOCUS_RADIUS),
                },
                hides_sight: false,
            };
            let start = spawn_standing(&mut commands, &mut grid, walls, free);
            commands.spawn((
                TransitionRoom { zoom },
                Room::new(IVec2::ZERO, size, vec![start, target]),
                Forming {
                    from: area,
                    timer: timer(CLOSE_SECONDS),
                    forgotten: false,
                },
            ));
            commands.insert_resource(area);
        }
    }
    commands.insert_resource(Cutscene);
}

/// Shown walls rise out of the floor where they stand; `Rising` goes in with
/// the wall, as the animation step reads it as the wall is placed.
fn spawn_rising(
    commands: &mut Commands,
    grid: &mut MapGrid,
    walls: Vec<IVec2>,
    free: impl Fn(IVec2) -> bool,
) -> Vec<IVec2> {
    let walls: Vec<IVec2> = walls.into_iter().filter(|&p| free(p)).collect();
    for &p in &walls {
        let wall = commands.spawn((wall_bundle(p), Rising { from: p })).id();
        grid.set(p, wall);
    }
    walls
}

/// The walls before stand again as they were drawn, so they keep their
/// shapes as they go.
fn spawn_standing(
    commands: &mut Commands,
    grid: &mut MapGrid,
    walls: Vec<(IVec2, Glyph)>,
    free: impl Fn(IVec2) -> bool,
) -> Vec<IVec2> {
    let mut placed = Vec::with_capacity(walls.len());
    for (p, glyph) in walls.into_iter().filter(|&(p, _)| free(p)) {
        let wall = commands
            .spawn(wall_bundle(p))
            .insert(glyph)
            .remove::<Tile>()
            .id();
        grid.set(p, wall);
        placed.push(p);
    }
    placed
}
