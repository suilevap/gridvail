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
use pav_ecs_game_bevy_port::camera::{CameraOperator, PortalTurn};
use pav_ecs_game_bevy_port::debug_ui::DebugPerformancePlugin;
use pav_ecs_game_bevy_port::model::{
    AnimatedPos, CompassMode, Player, Pos, RevealAll, SettingsMenu, ViewCamera, Vis, VisibilityMap,
};
use pav_ecs_game_bevy_port::rendering::{ExtrudedWallRendererPlugin, TextRendererPlugin};
use pav_ecs_game_bevy_port::schedule::{GamePhase, StartupPhase};
use pav_ecs_game_bevy_port::touch::TouchControlPlugin;

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(module = "/web/options.mjs")]
extern "C" {
    fn startup_arguments(schema: &str) -> String;
}

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
        walk.chars().all(|c| "UDLRQEZXO.".contains(c)),
        "walk steps must be U, D, L, R, Q, E, Z, X, O (Esc), or . (no key)"
    );
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "PavEcsGame Lite Bevy Port".into(),
                resolution: (1100_u32, 700_u32).into(),
                // The web build draws into the page's canvas and fills it.
                #[cfg(target_family = "wasm")]
                canvas: Some("#bevy".into()),
                #[cfg(target_family = "wasm")]
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        })
        .set(ImagePlugin::default_nearest());
    let mut app = App::new();
    if let Some(path) = options.map {
        #[cfg(not(target_family = "wasm"))]
        {
            let text = std::fs::read_to_string(&path).expect("read the --map file");
            app.insert_resource(MapText(text.leak()));
        }
        #[cfg(target_family = "wasm")]
        app.insert_resource(MapText(bundled_map(&path)));
    }
    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(options.motion)
        .insert_resource(options.compass)
        .add_plugins(plugins)
        .add_plugins(GamePlugin)
        .add_plugins((
            ObjectAnimationPlugin,
            TextRendererPlugin,
            TouchControlPlugin,
        ));
    if renderer == Renderer::Walls3d {
        app.add_plugins(ExtrudedWallRendererPlugin);
    }
    app.add_plugins(DebugPerformancePlugin);
    app.world_mut().resource_mut::<CameraOperator>().portal_turn = options.portal_turn;
    if options.reveal {
        app.insert_resource(RevealAll).add_systems(
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
    /// What the view does through turning portals.
    portal_turn: PortalTurn,
    /// When the compass shows.
    compass: CompassMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Renderer {
    #[default]
    Text,
    Walls3d,
}

// Each arm declares its URL shape alongside its CLI behavior. The browser
// receives this schema, so adding an arm never needs a JavaScript allowlist edit.
macro_rules! parse_options {
    ($source:ident, $args:ident; $(
        $name:literal: $kind:ident $(if $guard:expr)? => $body:block
    ),* $(,)?) => {
        let mut $args = $source(&[$(($name, stringify!($kind))),*]).into_iter();
        while let Some(argument) = $args.next() {
            match argument.as_str() {
                $($name $(if $guard)? => $body,)*
                _ => panic!("invalid or duplicate command-line option: {argument}"),
            }
        }
    };
}

impl Options {
    fn parse() -> Self {
        #[cfg(not(target_family = "wasm"))]
        {
            Self::parse_with(|_| std::env::args().skip(1))
        }
        #[cfg(target_family = "wasm")]
        {
            Self::parse_with(|schema| {
                let schema = serde_json::to_string(schema).expect("serialize option schema");
                serde_json::from_str::<Vec<String>>(&startup_arguments(&schema))
                    .expect("web startup arguments must be a JSON string array")
            })
        }
    }

    fn parse_with<I: IntoIterator<Item = String>>(
        source: impl FnOnce(&[(&str, &str)]) -> I,
    ) -> Self {
        let mut capture = None;
        let mut record = None;
        let mut walk = None;
        let mut remote_port = None;
        let mut renderer = None;
        let mut motion = None;
        let mut map = None;
        let mut reveal = false;
        let mut portal_turn = None;
        let mut compass = None;
        parse_options!(source, args;
            "--screenshot": native if capture.is_none() => {
                capture = Some(
                    args.next()
                        .expect("--screenshot requires an output PNG path"),
                );
            },
            "--record": native if record.is_none() => {
                record = Some(
                    args.next()
                        .expect("--record requires an output directory"),
                );
            },
            "--map": value if map.is_none() => {
                map = Some(args.next().expect("--map requires a map file"));
            },
            "--reveal": flag => {
                reveal = true;
            },
            "--compass": value if compass.is_none() => {
                let name = args
                    .next()
                    .expect("--compass requires when-turned, always or off");
                compass = Some(
                    CompassMode::from_name(&name)
                        .expect("--compass must be when-turned, always or off"),
                );
            },
            "--portal-view": value if portal_turn.is_none() => {
                let name = args.next().expect("--portal-view requires turn or north");
                portal_turn = Some(
                    PortalTurn::from_name(&name)
                        .expect("--portal-view must be turn or north"),
                );
            },
            "--walk": native if walk.is_none() => {
                walk = Some(args.next().expect("--walk requires UDLRQEZXO steps"));
            },
            "--remote": native if remote_port.is_none() => {
                remote_port = Some(DEFAULT_AGENT_PORT);
            },
            "--remote-port": native if remote_port.is_none() => {
                let port = args.next().expect("--remote-port requires a port number");
                remote_port = Some(port.parse().expect("--remote-port must be a valid u16"));
            },
            "--renderer": value if renderer.is_none() => {
                renderer = Some(match args.next().as_deref() {
                    Some("text") => Renderer::Text,
                    Some("3d-walls") => Renderer::Walls3d,
                    _ => panic!("--renderer must be text or 3d-walls"),
                });
            },
            "--motion": value if motion.is_none() => {
                let name = args.next().expect("--motion requires a style name");
                motion = Some(MotionStyle::from_name(&name).unwrap_or_else(|| {
                    let names: Vec<_> =
                        MotionStyle::PRESETS.iter().map(|(name, _)| *name).collect();
                    panic!("--motion must be one of: {}", names.join(", "))
                }));
            },
        );
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
            portal_turn: portal_turn.unwrap_or_default(),
            compass: compass.unwrap_or_default(),
        }
    }
}

// Generated from assets/maps; no per-map list to maintain.
#[cfg(any(target_family = "wasm", test))]
include!(concat!(env!("OUT_DIR"), "/bundled_maps.rs"));

#[cfg(any(target_family = "wasm", test))]
fn bundled_map(path: &str) -> &'static str {
    let name = path.strip_prefix("assets/maps/").unwrap_or(path);
    let name = name.strip_suffix(".txt").unwrap_or(name);
    BUNDLED_MAPS
        .iter()
        .find(|(map, _)| *map == name)
        .map(|(_, text)| *text)
        .unwrap_or_else(|| panic!("unknown bundled map: {path}"))
}

#[cfg(test)]
mod option_tests {
    use super::*;

    #[test]
    fn browser_arguments_use_the_cli_parser() {
        let options = Options::parse_with(|schema| {
            assert!(schema.contains(&("--motion", "value")));
            assert!(schema.contains(&("--reveal", "flag")));
            assert!(schema.contains(&("--screenshot", "native")));
            [
                "--renderer",
                "3d-walls",
                "--motion",
                "snap",
                "--reveal",
                "--portal-view",
                "north",
            ]
            .map(String::from)
        });
        assert_eq!(options.renderer, Renderer::Walls3d);
        assert_eq!(options.motion, MotionStyle::Snap);
        assert!(options.reveal);
        assert_eq!(options.portal_turn, PortalTurn::KeepNorth);
    }

    #[test]
    fn generated_maps_accept_names_filenames_and_cli_paths() {
        assert!(!BUNDLED_MAPS.is_empty());
        for &(name, text) in BUNDLED_MAPS {
            assert_eq!(bundled_map(name), text);
            assert_eq!(bundled_map(&format!("{name}.txt")), text);
            assert_eq!(bundled_map(&format!("assets/maps/{name}.txt")), text);
        }
    }

    #[test]
    #[should_panic(expected = "unknown bundled map")]
    fn unknown_maps_are_rejected() {
        bundled_map("../outside-the-bundle.txt");
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
        'O' => Some(KeyCode::Escape),
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
fn hold_recorded_input(
    recording: Res<Recording>,
    menu: Option<Res<SettingsMenu>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
) {
    keys.reset_all();
    let Some(frame) = recording.frames.checked_sub(30) else {
        return;
    };
    let step = recording.walk.get((frame / CAPTURE_STEP_FRAMES) as usize);
    // Camera keys, Esc and every key in the settings menu act on each
    // press, so they are tapped, not held.
    let menu_open = menu.is_some_and(|menu| menu.open);
    let tapped = menu_open || step.is_some_and(|step| "QEZXO".contains(*step));
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
