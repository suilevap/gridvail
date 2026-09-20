//! Bevy port of PavEcsLiteGame (the latest, most complete variant).
//!
//! Turn-based dungeon demo: token-gated movement, random-walk enemies,
//! interval-based field of view, CPU lightmaps, autotiled walls, and a
//! player-bound direction marker. Controls: arrows or WASD.

use bevy::prelude::*;
use bevy::{
    app::AppExit,
    render::{
        pipelined_rendering::PipelinedRenderingPlugin,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
};
use pav_ecs_game_bevy_port::game::GamePlugin;

fn main() -> AppExit {
    let mut args = std::env::args().skip(1);
    let capture = match args.next().as_deref() {
        Some("--screenshot") => Some(
            args.next()
                .expect("--screenshot requires an output PNG path"),
        ),
        None => None,
        _ => panic!("usage: pav_ecs_game_bevy_port [--screenshot output.png [--walk UDLR...]]"),
    };
    let walk = match args.next().as_deref() {
        Some("--walk") if capture.is_some() => args.next().expect("--walk requires UDLR steps"),
        None => String::new(),
        _ => panic!("--walk is only supported after --screenshot output.png"),
    };
    assert!(
        walk.chars().all(|c| "UDLR".contains(c)),
        "walk steps must be U, D, L, or R"
    );
    assert!(args.next().is_none(), "unexpected argument");
    let mut plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "PavEcsGame Lite Bevy Port".into(),
                resolution: (1100.0_f32, 700.0_f32).into(),
                ..default()
            }),
            ..default()
        })
        .set(ImagePlugin::default_nearest());
    if capture.is_some() {
        // Bevy 0.16's RenderAppChannels destructor can wait indefinitely on
        // the render thread after screenshot readback on macOS. Capture mode
        // needs deterministic completion, so run its renderer synchronously.
        plugins = plugins.disable::<PipelinedRenderingPlugin>();
    }
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::BLACK))
        .add_plugins(plugins)
        .add_plugins(GamePlugin);
    if let Some(path) = capture {
        app.insert_resource(Capture {
            path,
            frames: 0,
            walk: walk.chars().collect(),
        })
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(
            PreUpdate,
            replay_capture_input.after(bevy::input::InputSystem),
        )
        .add_systems(Update, capture_frame);
    }
    app.run()
}

#[derive(Resource)]
struct Capture {
    path: String,
    frames: u32,
    walk: Vec<char>,
}

/// Optional reproducible walk uses the same keyboard system as live play.
fn replay_capture_input(capture: Res<Capture>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    keys.reset_all();
    if capture.frames >= 30 && (capture.frames - 30).is_multiple_of(2) {
        if let Some(step) = capture.walk.get(((capture.frames - 30) / 2) as usize) {
            keys.press(match step {
                'U' => KeyCode::ArrowUp,
                'D' => KeyCode::ArrowDown,
                'L' => KeyCode::ArrowLeft,
                'R' => KeyCode::ArrowRight,
                _ => unreachable!(),
            });
        }
    }
}

/// Capture the real GPU-rendered window after startup/font layout settles.
/// Exit only when readback and saving have completed.
fn capture_frame(mut commands: Commands, mut capture: ResMut<Capture>) {
    capture.frames += 1;
    if capture.frames == 32 + 2 * capture.walk.len() as u32 {
        if let Some(parent) = std::path::Path::new(&capture.path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).expect("create screenshot directory");
            }
        }
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_capture);
    }
}

fn save_capture(
    trigger: Trigger<ScreenshotCaptured>,
    capture: Res<Capture>,
    mut exit: EventWriter<AppExit>,
) {
    let saved = trigger
        .event()
        .0
        .clone()
        .try_into_dynamic()
        .map_err(|e| e.to_string())
        .and_then(|image| {
            image
                .to_rgb8()
                .save(&capture.path)
                .map_err(|e| e.to_string())
        });
    match saved {
        Ok(()) => {
            println!("Screenshot saved to {}", capture.path);
            exit.write(AppExit::Success);
        }
        Err(error) => {
            eprintln!("Could not save screenshot: {error}");
            exit.write(AppExit::error());
        }
    }
}
