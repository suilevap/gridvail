use std::fmt::Write;

use bevy::prelude::*;

use crate::model::*;

pub fn flush_cells(
    buffers: Res<RenderBuffers>,
    mut cells: Query<(&MapCell, &mut Text2d, &mut TextColor)>,
) {
    for (cell, mut text, mut color) in cells.iter_mut() {
        let Some(index) = buffers.idx(cell.0) else {
            continue;
        };
        if buffers.current[index] != buffers.previous[index] {
            let cell = buffers.current[index];
            text.0.clear();
            text.0.push(if cell.ch == '\0' { ' ' } else { cell.ch });
            color.0 = crate::lighting::palette_color(cell.color);
        }
    }
}

pub fn update_hud(
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    players: Query<(&Pos, &Tokens), With<Player>>,
    enemies: Query<Entity, (With<Enemy>, Without<DestroyRequested>)>,
    collisions: Res<CollisionBuffer>,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let (position, tokens) = players
        .iter()
        .next()
        .map(|(pos, tokens)| (pos.0, tokens.count))
        .unwrap_or((IVec2::ZERO, 0));
    for mut text in hud.iter_mut() {
        text.0.clear();
        write!(
            text.0,
            "PavEcsGame Lite Bevy port | arrows/WASD\nTick {} | {} | map {}x{} | player ({},{}) | tokens {} | enemies {} | bumps {}",
            turn.tick,
            turn.phase_name(),
            grid.width,
            grid.height,
            position.x,
            position.y,
            tokens,
            enemies.iter().count(),
            collisions.0.len(),
        )
        .expect("writing to String cannot fail");
    }
}
