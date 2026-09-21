use bevy::prelude::*;

use crate::content::tile_rules::{DirectionTileRule, TileRule};
use crate::model::*;

#[derive(Resource, Debug)]
pub struct Rules {
    pub wall: TileRule,
    pub triangle: DirectionTileRule,
    pub v: DirectionTileRule,
}

/// Startup-only wall autotiling. Its temporary collections are deliberately
/// outside the steady gameplay path.
pub fn tile_system(
    mut commands: Commands,
    grid: Res<MapGrid>,
    rules: Res<Rules>,
    tiles: Query<(Entity, &Pos, &Tile)>,
) {
    let present: std::collections::HashSet<IVec2> = tiles.iter().map(|(_, pos, _)| pos.0).collect();
    let mut masks = Vec::new();
    for (entity, pos, tile) in tiles.iter() {
        if tile.rule != "wall_rule" {
            continue;
        }
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
        masks.push((entity, mask));
    }
    for (entity, mask) in masks {
        commands.entity(entity).insert(Glyph {
            ch: rules.wall.symbol(mask),
            depth: 1,
            color: crate::lighting::GRAY,
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
