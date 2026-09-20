//! Bevy port of PavEcsLiteGame (the latest, most complete variant).
//!
//! Turn-based dungeon demo: token-gated movement, random-walk enemies,
//! interval-based field of view, CPU lightmaps, autotiled walls, and a
//! player-bound direction marker. Controls: arrows or WASD.

use bevy::prelude::*;
use pav_ecs_game_bevy_port::game::GamePlugin;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "PavEcsGame Lite Bevy Port".into(),
                        resolution: (1600.0_f32, 700.0_f32).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins(GamePlugin)
        .run();
}
