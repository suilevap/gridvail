//! Bevy port of PavEcsLiteGame (the latest, most complete variant).
//!
//! Turn-based dungeon demo: token-gated movement, random-walk enemies,
//! interval-based field of view, CPU lightmaps, autotiled walls, and a
//! player-bound direction marker. Controls: arrows or WASD.

use bevy::prelude::*;
use bevy::{
    app::AppExit,
    camera::RenderTarget,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
};
use pav_ecs_game_bevy_port::agent_api::{AgentApiPlugin, DEFAULT_AGENT_PORT};
use pav_ecs_game_bevy_port::app::GamePlugin;
use pav_ecs_game_bevy_port::rendering::TextRendererPlugin;
use pav_ecs_game_bevy_port::schedule::StartupPhase;

const CAPTURE_STEP_FRAMES: u32 = 8;

fn main() -> AppExit {
    let options = Options::parse();
    let capture = options.capture;
    let walk = options.walk;
    assert!(
        walk.chars().all(|c| "UDLR".contains(c)),
        "walk steps must be U, D, L, or R"
    );
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "PavEcsGame Lite Bevy Port".into(),
                resolution: (1100_u32, 700_u32).into(),
                ..default()
            }),
            ..default()
        })
        .set(ImagePlugin::default_nearest());
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::BLACK))
        .add_plugins(plugins)
        .add_plugins((GamePlugin, TextRendererPlugin));
    if let Some(port) = options.remote_port {
        println!("Bevy Remote agent API: http://127.0.0.1:{port}");
        app.add_plugins(AgentApiPlugin::new(port));
    }
    if let Some(path) = capture {
        app.insert_resource(Capture {
            path,
            frames: 0,
            walk: walk.chars().collect(),
        })
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(Startup, setup_capture_target.after(StartupPhase::Renderer))
        .add_systems(
            PreUpdate,
            replay_capture_input.after(bevy::input::InputSystems),
        )
        .add_systems(Update, capture_frame);
    }
    app.run()
}

struct Options {
    capture: Option<String>,
    walk: String,
    remote_port: Option<u16>,
}

impl Options {
    fn parse() -> Self {
        let mut capture = None;
        let mut walk = None;
        let mut remote_port = None;
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--screenshot" if capture.is_none() => {
                    capture = Some(
                        args.next()
                            .expect("--screenshot requires an output PNG path"),
                    );
                }
                "--walk" if walk.is_none() => {
                    walk = Some(args.next().expect("--walk requires UDLR steps"));
                }
                "--remote" if remote_port.is_none() => remote_port = Some(DEFAULT_AGENT_PORT),
                "--remote-port" if remote_port.is_none() => {
                    let port = args.next().expect("--remote-port requires a port number");
                    remote_port = Some(port.parse().expect("--remote-port must be a valid u16"));
                }
                _ => panic!(
                    "usage: pav_ecs_game_bevy_port [--remote | --remote-port PORT] [--screenshot OUTPUT.png [--walk UDLR...]]"
                ),
            }
        }
        let walk = walk.unwrap_or_default();
        assert!(
            capture.is_some() || walk.is_empty(),
            "--walk requires --screenshot"
        );
        Self {
            capture,
            walk,
            remote_port,
        }
    }
}

#[derive(Resource)]
struct Capture {
    path: String,
    frames: u32,
    walk: Vec<char>,
}

#[derive(Resource)]
struct CaptureTarget(Handle<Image>);

fn setup_capture_target(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut target: Single<&mut RenderTarget, With<Camera2d>>,
) {
    let image = images.add(Image::new_target_texture(
        1100,
        700,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    **target = RenderTarget::Image(image.clone().into());
    commands.insert_resource(CaptureTarget(image));
}

/// Optional reproducible walk uses the same keyboard system as live play.
fn replay_capture_input(capture: Res<Capture>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    keys.reset_all();
    if capture.frames >= 30 && (capture.frames - 30).is_multiple_of(CAPTURE_STEP_FRAMES) {
        if let Some(step) = capture
            .walk
            .get(((capture.frames - 30) / CAPTURE_STEP_FRAMES) as usize)
        {
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

/// Capture a GPU-rendered image after startup, font layout, and pipelines settle.
/// Exit only when readback and saving have completed.
fn capture_frame(mut commands: Commands, mut capture: ResMut<Capture>, target: Res<CaptureTarget>) {
    capture.frames += 1;
    if capture.frames == 120 + CAPTURE_STEP_FRAMES * capture.walk.len() as u32 {
        if let Some(parent) = std::path::Path::new(&capture.path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).expect("create screenshot directory");
            }
        }
        commands
            .spawn(Screenshot::image(target.0.clone()))
            .observe(save_capture);
    }
}

fn save_capture(
    trigger: On<ScreenshotCaptured>,
    capture: Res<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    let saved = trigger
        .image
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
