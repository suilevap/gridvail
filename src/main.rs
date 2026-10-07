//! Bevy port of PavEcsLiteGame (the latest, most complete variant).
//!
//! Turn-based dungeon demo: token-gated movement, wandering enemies,
//! interval-based field of view, CPU lightmaps, autotiled walls, and a
//! player-bound direction marker. Controls: arrows or WASD; M cycles the
//! motion style.

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
use pav_ecs_game_bevy_port::animation::{MotionStyle, ObjectAnimationPlugin};
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
use pav_ecs_game_bevy_port::debug_ui::DebugPerformancePlugin;
use pav_ecs_game_bevy_port::model::{AnimatedPos, Player, Pos, ViewCamera, Vis, VisibilityMap};
use pav_ecs_game_bevy_port::rendering::{ExtrudedWallRendererPlugin, TextRendererPlugin};
use pav_ecs_game_bevy_port::schedule::{GamePhase, StartupPhase};
use pav_ecs_game_bevy_port::service::ServiceMode;

const CAPTURE_STEP_FRAMES: u32 = 8;
/// Frames recorded after the walk, so the last move can settle on camera.
const RECORD_TAIL_FRAMES: u32 = 60;

fn main() -> AppExit {
    let options = Options::parse();
    let capture = options.capture;
    let record = options.record;
    let walk = options.walk;
    let renderer = options.renderer;
    assert!(
        walk.chars().all(|c| "UDLRQEZX.".contains(c)),
        "walk steps must be U, D, L, R, Q, E, Z, X, or . (no key)"
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
    // Tasks (path plans) run off the frame in play, so a slow plan costs that
    // enemy a turn at most and never a frame. Captures replay scripted input
    // and stay deterministic.
    let services = if let Some(polls) = options.plan_delay {
        ServiceMode::Deferred(polls)
    } else if capture.is_some() || record.is_some() {
        ServiceMode::Inline
    } else {
        ServiceMode::Background
    };
    let mut app = App::new();
    if let Some(path) = options.map {
        let text = std::fs::read_to_string(&path).expect("read the --map file");
        app.insert_resource(MapText(text.leak()));
    }
    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(services)
        .insert_resource(options.motion)
        .add_plugins(plugins)
        .add_plugins(GamePlugin)
        .add_plugins((ObjectAnimationPlugin, TextRendererPlugin));
    if renderer == Renderer::Walls3d {
        app.add_plugins(ExtrudedWallRendererPlugin);
    }
    app.add_plugins(DebugPerformancePlugin);
    if options.reveal {
        app.add_systems(
            Update,
            reveal_map
                .after(GamePhase::Visibility)
                .before(GamePhase::Presentation),
        );
    }
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
    if let Some(dir) = record {
        std::fs::create_dir_all(&dir).expect("create recording directory");
        let walk: Vec<char> = walk.chars().collect();
        app.insert_resource(Recording {
            frames: 0,
            total: CAPTURE_STEP_FRAMES * walk.len() as u32 + RECORD_TAIL_FRAMES,
            saved: 0,
            dir,
            walk,
            trace: String::from(
                "frame,cell_x,cell_y,shown_x,shown_y,camera_x,camera_y,rotation,zoom\n",
            ),
        })
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(Startup, setup_capture_target.after(StartupPhase::Renderer))
        .add_systems(
            PreUpdate,
            hold_recorded_input.after(bevy::input::InputSystems),
        )
        .add_systems(Update, record_frame);
    }
    app.run()
}

struct Options {
    capture: Option<String>,
    record: Option<String>,
    walk: String,
    remote_port: Option<u16>,
    renderer: Renderer,
    motion: MotionStyle,
    /// A map file to play instead of the bundled map.
    map: Option<String>,
    /// Show the whole map, not only what the player sees: for recordings of
    /// what happens out of sight.
    reveal: bool,
    /// Every task (path plans) lands only after this many polls: to see
    /// thinking that takes longer than a turn. An enemy polls each frame it
    /// keeps its turn open, a few per turn.
    plan_delay: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Renderer {
    #[default]
    Text,
    Walls3d,
}

impl Options {
    fn parse() -> Self {
        let mut capture = None;
        let mut record = None;
        let mut walk = None;
        let mut remote_port = None;
        let mut renderer = None;
        let mut motion = None;
        let mut map = None;
        let mut reveal = false;
        let mut plan_delay = None;
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--screenshot" if capture.is_none() => {
                    capture = Some(
                        args.next()
                            .expect("--screenshot requires an output PNG path"),
                    );
                }
                "--record" if record.is_none() => {
                    record = Some(
                        args.next()
                            .expect("--record requires an output directory"),
                    );
                }
                "--map" if map.is_none() => {
                    map = Some(args.next().expect("--map requires a map file"));
                }
                "--reveal" => reveal = true,
                "--plan-delay" if plan_delay.is_none() => {
                    let polls = args.next().expect("--plan-delay requires a poll count");
                    plan_delay = Some(polls.parse().expect("--plan-delay must be a poll count"));
                }
                "--walk" if walk.is_none() => {
                    walk = Some(args.next().expect("--walk requires UDLRQEZX steps"));
                }
                "--remote" if remote_port.is_none() => remote_port = Some(DEFAULT_AGENT_PORT),
                "--remote-port" if remote_port.is_none() => {
                    let port = args.next().expect("--remote-port requires a port number");
                    remote_port = Some(port.parse().expect("--remote-port must be a valid u16"));
                }
                "--renderer" if renderer.is_none() => {
                    renderer = Some(match args.next().as_deref() {
                        Some("text") => Renderer::Text,
                        Some("3d-walls") => Renderer::Walls3d,
                        _ => panic!("--renderer must be text or 3d-walls"),
                    });
                }
                "--motion" if motion.is_none() => {
                    let name = args.next().expect("--motion requires a style name");
                    motion = Some(MotionStyle::from_name(&name).unwrap_or_else(|| {
                        let names: Vec<_> =
                            MotionStyle::PRESETS.iter().map(|(name, _)| *name).collect();
                        panic!("--motion must be one of: {}", names.join(", "))
                    }));
                }
                _ => panic!(
                    "usage: pav_ecs_game_bevy_port [--renderer text|3d-walls] [--motion STYLE] [--map FILE] [--reveal] [--plan-delay POLLS] [--remote | --remote-port PORT] [--screenshot OUTPUT.png | --record DIR] [--walk UDLRQEZX.]"
                ),
            }
        }
        let walk = walk.unwrap_or_default();
        assert!(
            capture.is_none() || record.is_none(),
            "--screenshot and --record are exclusive"
        );
        assert!(
            capture.is_some() || record.is_some() || walk.is_empty(),
            "--walk requires --screenshot or --record"
        );
        Self {
            capture,
            record,
            walk,
            remote_port,
            renderer: renderer.unwrap_or_default(),
            motion: motion.unwrap_or_default(),
            map,
            reveal,
            plan_delay,
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

type RenderCameraFilter = Or<(With<Camera2d>, With<Camera3d>)>;

fn setup_capture_target(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut targets: Query<&mut RenderTarget, RenderCameraFilter>,
) {
    let image = images.add(Image::new_target_texture(
        1100,
        700,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    for mut target in &mut targets {
        *target = RenderTarget::Image(image.clone().into());
    }
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
            if let Some(key) = walk_key(*step) {
                keys.press(key);
            }
        }
    }
}

fn walk_key(step: char) -> Option<KeyCode> {
    match step {
        'U' => Some(KeyCode::ArrowUp),
        'D' => Some(KeyCode::ArrowDown),
        'L' => Some(KeyCode::ArrowLeft),
        'R' => Some(KeyCode::ArrowRight),
        'Q' => Some(KeyCode::KeyQ),
        'E' => Some(KeyCode::KeyE),
        'Z' => Some(KeyCode::KeyZ),
        'X' => Some(KeyCode::KeyX),
        _ => None,
    }
}

/// Everything is in sight, for `--reveal`.
fn reveal_map(mut players: Query<&mut VisibilityMap, With<Player>>) {
    for mut visibility in &mut players {
        visibility.data.fill(Vis::VISIBLE | Vis::KNOWN);
    }
}

/// Frames recorded to `dir` as numbered PNGs while a walk plays.
#[derive(Resource)]
struct Recording {
    dir: String,
    frames: u32,
    total: u32,
    saved: u32,
    walk: Vec<char>,
    /// `frame,cell_x,cell_y,shown_x,shown_y,camera_x,camera_y,rotation,zoom`
    /// per recorded frame.
    trace: String,
}

/// Recorded walks hold each key for its whole step, like a player holding
/// an arrow key, so turns follow the animation pacing.
fn hold_recorded_input(recording: Res<Recording>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    keys.reset_all();
    let Some(frame) = recording.frames.checked_sub(30) else {
        return;
    };
    let step = recording.walk.get((frame / CAPTURE_STEP_FRAMES) as usize);
    // Camera keys act on each press, so they are tapped, not held.
    let tapped = step.is_some_and(|step| "QEZX".contains(*step));
    if tapped && !frame.is_multiple_of(CAPTURE_STEP_FRAMES) {
        return;
    }
    if let Some(key) = step.and_then(|step| walk_key(*step)) {
        keys.press(key);
    }
}

/// Saves every frame after warm-up; exits once all of them are on disk.
fn record_frame(
    mut commands: Commands,
    mut recording: ResMut<Recording>,
    target: Res<CaptureTarget>,
    player: Single<(&Pos, &AnimatedPos), With<Player>>,
    camera: Res<ViewCamera>,
) {
    recording.frames += 1;
    let Some(index) = recording.frames.checked_sub(31) else {
        return;
    };
    if index >= recording.total {
        return;
    }
    let (cell, shown) = *player;
    let line = format!(
        "{index},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}\n",
        cell.0.x,
        cell.0.y,
        shown.position.x,
        shown.position.y,
        camera.position.x,
        camera.position.y,
        camera.rotation,
        camera.zoom,
    );
    recording.trace.push_str(&line);
    let path = format!("{}/frame_{index:05}.png", recording.dir);
    commands.spawn(Screenshot::image(target.0.clone())).observe(
        move |trigger: On<ScreenshotCaptured>,
              mut recording: ResMut<Recording>,
              mut exit: MessageWriter<AppExit>| {
            let image = trigger
                .image
                .clone()
                .try_into_dynamic()
                .expect("readable frame");
            image.to_rgb8().save(&path).expect("save frame");
            recording.saved += 1;
            if recording.saved == recording.total {
                let trace = format!("{}/trace.csv", recording.dir);
                std::fs::write(&trace, &recording.trace).expect("write trace");
                println!("Recorded {} frames to {}", recording.total, recording.dir);
                exit.write(AppExit::Success);
            }
        },
    );
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
