use bevy::prelude::*;

use crate::content::tile_rules::{DirectionTileRule, TileRule};
use crate::model::*;

#[derive(Resource, Debug)]
pub struct Rules {
    pub wall: TileRule,
    pub triangle: DirectionTileRule,
    pub v: DirectionTileRule,
}

/// A wall as the map spawns it; `tile_system` gives it its glyph.
pub fn wall_bundle(pos: IVec2) -> impl Bundle {
    (
        Active,
        Collider,
        Wall,
        Pos(pos),
        Glyph::new('#', 1, crate::lighting::GRAY),
        Tile::wall(),
    )
}

/// A rail as the map spawns it: flat, and `tile_system` gives it its glyph.
pub fn rail_bundle(pos: IVec2) -> impl Bundle {
    (
        Active,
        Rail,
        Pos(pos),
        Glyph::new('=', 0, RAIL_COLOR),
        Stature(0.0),
        Tile::rail(),
    )
}

/// Rails are drawn dimmer than walls.
const RAIL_COLOR: Rgb = crate::lighting::DARK_GRAY;

/// Wall autotiling: every wall marked with a `Tile` gets the glyph that
/// joins it to its neighbours, at startup and when walls change (a room
/// switching layouts); rails get the same shapes, joined to rails, flat and
/// dimmed. Its temporary collections are deliberately outside the steady
/// gameplay path; it runs only while something is marked.
pub fn tile_system(
    mut commands: Commands,
    grid: Res<MapGrid>,
    rules: Res<Rules>,
    tiles: Query<(Entity, &Pos, &Tile)>,
    walls: Query<&Pos, With<Wall>>,
    doors: Query<&Pos, With<Door>>,
    rails: Query<&Pos, With<Rail>>,
) {
    // Walls connect to doors set into them.
    let walls: std::collections::HashSet<IVec2> =
        walls.iter().chain(doors.iter()).map(|pos| pos.0).collect();
    let rails: std::collections::HashSet<IVec2> = rails.iter().map(|pos| pos.0).collect();
    let mut masks = Vec::new();
    for (entity, pos, tile) in tiles.iter() {
        let (present, depth, color) = match tile.rule.as_str() {
            "wall_rule" => (&walls, 1, crate::lighting::GRAY),
            "rail_rule" => (&rails, 0, RAIL_COLOR),
            _ => continue,
        };
        let at = |delta| present.contains(&grid.safe_pos(pos.0 + delta));
        let mut mask = 0;
        for (bit, delta) in [IVec2::X, IVec2::Y, IVec2::NEG_X, IVec2::NEG_Y]
            .into_iter()
            .enumerate()
        {
            if at(delta) {
                mask |= 1 << bit;
            }
        }
        masks.push((entity, mask, depth, color));
    }
    for (entity, mask, depth, color) in masks {
        commands.entity(entity).insert(Glyph {
            ch: rules.wall.symbol(mask),
            depth,
            color,
        });
        commands.entity(entity).remove::<Tile>();
    }
}

pub fn direction_tiles(
    rules: Res<Rules>,
    mut glyphs: Query<(&Facing, &DirectionTile, &mut Glyph)>,
) {
    for (facing, tile, mut glyph) in glyphs.iter_mut() {
        let rule = match tile.rule.as_str() {
            "direction_triangle_rule" => &rules.triangle,
            "direction_v_rule" => &rules.v,
            _ => continue,
        };
        glyph.ch = rule.symbol(Direction::of(facing.0));
    }
}
