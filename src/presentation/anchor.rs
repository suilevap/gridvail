use bevy::prelude::*;

use crate::model::{AnimatedPos, Player, Pos, ViewAnchor};

/// Centres the view on the player where they are shown: their animated
/// position, or their cell when nothing animates them.
pub fn anchor_view(
    players: Query<(&Pos, Option<&AnimatedPos>), With<Player>>,
    mut anchor: ResMut<ViewAnchor>,
) {
    let Ok((pos, shown)) = players.single() else {
        return;
    };
    let position = shown.map_or(pos.0.as_vec2(), |shown| shown.position);
    if anchor.position != position {
        anchor.position = position;
    }
}
