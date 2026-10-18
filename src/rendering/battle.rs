//! Compact battle status, nearest threat, and separate decision/wait meters.
//! Shared by the top-down and perspective backends.

use crate::model::*;
use bevy::prelude::*;
use std::fmt::Write;

#[derive(Component)]
pub(super) struct BattlePanel;
#[derive(Component)]
pub(super) enum BattleLabel {
    Status,
    Threat,
    Wait,
}
#[derive(Component)]
pub(super) struct WaitFill;
#[derive(Component)]
pub(super) struct TurnFill;
#[derive(Component)]
pub(super) struct RestartBattle;

pub(super) fn spawn_battle_ui(commands: &mut Commands, camera: Entity) {
    commands
        .spawn((
            BattlePanel,
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(8.0),
                bottom: Val::Px(8.0),
                width: Val::Percent(94.0),
                max_width: Val::Px(360.0),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.025, 0.04, 0.94)),
            ZIndex(100),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node {
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((BattleLabel::Status, Text::default(), battle_font()));
                    row.spawn((
                        RestartBattle,
                        Button,
                        Node {
                            min_height: Val::Px(32.0),
                            padding: UiRect::horizontal(Val::Px(8.0)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.16, 0.21, 0.25)),
                    ))
                    .with_children(|button| {
                        button.spawn((Text::new("Reset"), battle_font()));
                    });
                });
            panel
                .spawn((
                    Node {
                        height: Val::Px(3.0),
                        width: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.12, 0.17, 0.21)),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        TurnFill,
                        Node {
                            height: Val::Percent(100.0),
                            width: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(1.0, 0.7, 0.2)),
                    ));
                });
            panel.spawn((BattleLabel::Threat, Text::default(), battle_font()));
            panel
                .spawn(Node {
                    height: Val::Px(20.0),
                    flex_shrink: 0.0,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        BattleLabel::Wait,
                        Text::default(),
                        TextLayout::no_wrap(),
                        battle_font(),
                        Node {
                            min_width: Val::Px(176.0),
                            height: Val::Px(20.0),
                            flex_shrink: 0.0,
                            ..default()
                        },
                    ));
                    row.spawn((
                        Node {
                            height: Val::Px(6.0),
                            flex_grow: 1.0,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.12, 0.17, 0.21)),
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            WaitFill,
                            Node {
                                height: Val::Percent(100.0),
                                width: Val::Percent(0.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.25, 0.75, 0.95)),
                        ));
                    });
                });
        });
}

fn battle_font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(14.0),
        ..default()
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn draw_battle_ui(
    outcome: Option<Res<EncounterOutcome>>,
    restart: Option<ResMut<RestartRequest>>,
    wait: Option<Res<WaitHold>>,
    turn: Res<TurnState>,
    timer: Res<TokenTimer>,
    menu: Option<Res<SettingsMenu>>,
    scheme: Option<Res<ControlScheme>>,
    grid: Res<MapGrid>,
    frame: Res<RenderBuffers>,
    player: Query<(&Pos, &Health, &Tokens, Option<&Brace>), With<Player>>,
    enemies: Query<(&Pos, &Health, &AttackCycle, &AttackPresentation), (With<Enemy>, With<Active>)>,
    buttons: Query<&Interaction, (With<RestartBattle>, Changed<Interaction>)>,
    mut labels: Query<(&BattleLabel, &mut Text)>,
    mut panels: Query<&mut Visibility, With<BattlePanel>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), (With<WaitFill>, Without<TurnFill>)>,
    mut clocks: Query<(&mut Node, &mut BackgroundColor), With<TurnFill>>,
) {
    let (Some(outcome), Some(mut restart), Some(wait)) = (outcome, restart, wait) else {
        return;
    };
    let defeated = *outcome == EncounterOutcome::Defeat;
    let won = *outcome == EncounterOutcome::Victory;
    let open = SettingsMenu::is_open(menu.as_deref());
    for mut visibility in &mut panels {
        *visibility = if open {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    if !open
        && buttons
            .iter()
            .any(|interaction| *interaction == Interaction::Pressed)
    {
        restart.0 = true;
    }
    let Ok((pos, health, tokens, brace)) = player.single() else {
        return;
    };
    let nearest = enemies
        .iter()
        .filter(|(enemy_pos, health, _, _)| {
            health.current > 0
                && grid
                    .idx(enemy_pos.0)
                    .is_some_and(|index| frame.seen_first[index] != RenderBuffers::NO_CELL)
        })
        .min_by_key(|(enemy_pos, _, _, _)| (enemy_pos.0 - pos.0).abs().element_sum());
    for (label, mut text) in &mut labels {
        text.0.clear();
        match label {
            BattleLabel::Status => {
                write!(text.0, "HP {}/{}", health.current, health.maximum).unwrap();
                if defeated {
                    write!(text.0, " | Defeated").unwrap();
                } else if won {
                    write!(text.0, " | Victory").unwrap();
                } else if turn.simulation || tokens.count <= 0 {
                    write!(text.0, " | Acting").unwrap();
                } else {
                    write!(text.0, " | ACT {:.1}s", timer.0.remaining_secs()).unwrap();
                }
            }
            BattleLabel::Threat if !defeated && !won => {
                if let Some((_, enemy_health, cycle, display)) = nearest {
                    write!(
                        text.0,
                        "{} {}/{} | ",
                        display.name, enemy_health.current, enemy_health.maximum
                    )
                    .unwrap();
                    match cycle.phase {
                        AttackPhase::Recovering(left) => {
                            write!(text.0, "OPEN {} turns | {}", left, display.opening_hint)
                                .unwrap()
                        }
                        AttackPhase::Windup(left) => write!(
                            text.0,
                            "{} {} turns | {}",
                            display.attack,
                            left + 1,
                            display.avoid_hint
                        )
                        .unwrap(),
                        AttackPhase::Ready => write!(text.0, "{}", display.ready_hint).unwrap(),
                        AttackPhase::Idle => write!(text.0, "{}", display.idle_hint).unwrap(),
                    }
                }
            }
            BattleLabel::Wait if !defeated && !won => {
                if wait.latched && wait.progress() == 1.0 {
                    write!(text.0, "WAIT COUNTED | release").unwrap();
                } else if wait.elapsed > 0.0 {
                    write!(
                        text.0,
                        "WAIT {:.1}s | hold",
                        (WaitHold::SECONDS - wait.elapsed).max(0.0)
                    )
                    .unwrap();
                } else if wait.latched {
                    write!(text.0, "Release to brace again").unwrap();
                } else {
                    write!(
                        text.0,
                        "{} {:.1}s | {}",
                        if scheme.as_deref() == Some(&ControlScheme::Touch) {
                            "Hold"
                        } else {
                            "Space"
                        },
                        WaitHold::SECONDS,
                        if brace.is_some_and(|brace| brace.charges > 0) {
                            "BRACED"
                        } else {
                            "brace"
                        }
                    )
                    .unwrap();
                }
            }
            _ => {}
        }
    }
    for (mut node, mut color) in &mut fills {
        node.width = Val::Percent(wait.progress() * 100.0);
        color.0 = if wait.latched && wait.progress() == 1.0 {
            Color::srgb(0.25, 0.95, 0.55)
        } else {
            Color::srgb(0.25, 0.75, 0.95)
        };
    }
    for (mut node, mut color) in &mut clocks {
        node.width = Val::Percent(if defeated || won {
            0.0
        } else {
            timer.0.fraction_remaining() * 100.0
        });
        color.0 = if timer.0.fraction_remaining() <= 0.25 {
            Color::srgb(1.0, 0.25, 0.25)
        } else {
            Color::srgb(1.0, 0.7, 0.2)
        };
    }
}
