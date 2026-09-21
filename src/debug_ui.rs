//! Optional runtime diagnostics presented independently of the game renderer.

use std::fmt::Write;

use bevy::{
    diagnostic::{
        DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
        SystemInformationDiagnosticsPlugin,
    },
    prelude::*,
};

use crate::{
    model::{RenderBuffers, TurnPacing, TurnState},
    rendering::TextRenderStats,
    schedule::{GamePhase, StartupPhase},
};

#[derive(Component)]
struct PerformancePanel;

#[derive(Resource)]
struct PerformancePanelState {
    visible: bool,
    refresh: Timer,
}

impl Default for PerformancePanelState {
    fn default() -> Self {
        Self {
            visible: true,
            refresh: Timer::from_seconds(0.25, TimerMode::Repeating),
        }
    }
}

/// Adds an F3-toggleable performance panel without coupling it to gameplay.
pub struct DebugPerformancePlugin;

impl Plugin for DebugPerformancePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PerformancePanelState>()
            .add_plugins((
                FrameTimeDiagnosticsPlugin::default(),
                EntityCountDiagnosticsPlugin::default(),
                SystemInformationDiagnosticsPlugin,
            ))
            .add_systems(Startup, setup.after(StartupPhase::Renderer))
            .add_systems(
                Update,
                (toggle_panel, update_panel)
                    .chain()
                    .after(GamePhase::Output),
            );
    }
}

fn setup(mut commands: Commands, camera: Single<Entity, With<Camera2d>>) {
    let mut initial = String::with_capacity(512);
    initial.push_str("DEBUG PERFORMANCE [F3]\ncollecting diagnostics...");
    commands.spawn((
        PerformancePanel,
        UiTargetCamera(*camera),
        Text::new(initial),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::WHITE),
        BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.88)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        ZIndex(100),
    ));
}

fn toggle_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<PerformancePanelState>,
    mut panel: Single<&mut Visibility, With<PerformancePanel>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        state.visible = !state.visible;
        **panel = if state.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn update_panel(
    time: Res<Time>,
    mut state: ResMut<PerformancePanelState>,
    diagnostics: Res<DiagnosticsStore>,
    turn: Res<TurnState>,
    pacing: Res<TurnPacing>,
    buffers: Res<RenderBuffers>,
    render_stats: Res<TextRenderStats>,
    mut panel: Single<&mut Text, With<PerformancePanel>>,
) {
    state.refresh.tick(time.delta());
    if !state.visible || !state.refresh.just_finished() {
        return;
    }

    let fps = smoothed(&diagnostics, &FrameTimeDiagnosticsPlugin::FPS);
    let frame_ms = smoothed(&diagnostics, &FrameTimeDiagnosticsPlugin::FRAME_TIME);
    let entities = latest(&diagnostics, &EntityCountDiagnosticsPlugin::ENTITY_COUNT);
    let process_cpu = smoothed(
        &diagnostics,
        &SystemInformationDiagnosticsPlugin::PROCESS_CPU_USAGE,
    );
    let process_memory = smoothed(
        &diagnostics,
        &SystemInformationDiagnosticsPlugin::PROCESS_MEM_USAGE,
    );
    let system_cpu = smoothed(
        &diagnostics,
        &SystemInformationDiagnosticsPlugin::SYSTEM_CPU_USAGE,
    );
    let system_memory = smoothed(
        &diagnostics,
        &SystemInformationDiagnosticsPlugin::SYSTEM_MEM_USAGE,
    );

    panel.0.clear();
    write!(
        panel.0,
        "DEBUG PERFORMANCE [F3]\n\
         FPS {fps:>6.1} | frame {frame_ms:>6.2} ms\n\
         CPU process {process_cpu:>5.1}% | system {system_cpu:>5.1}%\n\
         RAM process {process_memory:>5.2} GiB | system {system_memory:>5.1}%\n\
         Entities {:>5.0} | text cells {:>4}/{}\n\
         Turn {} {} | minimum {} ms",
        entities,
        render_stats.changed_cells,
        buffers.current.len(),
        turn.tick,
        turn.phase_name(),
        pacing.minimum_interval.as_millis(),
    )
    .expect("writing to String cannot fail");
}

fn smoothed(diagnostics: &DiagnosticsStore, path: &bevy::diagnostic::DiagnosticPath) -> f64 {
    diagnostics
        .get(path)
        .and_then(|diagnostic| diagnostic.smoothed())
        .unwrap_or_default()
}

fn latest(diagnostics: &DiagnosticsStore, path: &bevy::diagnostic::DiagnosticPath) -> f64 {
    diagnostics
        .get(path)
        .and_then(|diagnostic| diagnostic.value())
        .unwrap_or_default()
}
