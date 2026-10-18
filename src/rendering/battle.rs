//! A persistent wait meter: time remaining, an explicit commitment message,
//! health and feedback. Shared by the top-down and perspective backends.

use crate::model::*;
use bevy::prelude::*;
use std::fmt::Write;

#[derive(Component)]
pub(super) struct BattlePanel;
#[derive(Component)]
pub(super) struct BattleLabel;
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
                left: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Percent(85.0),
                max_width: Val::Px(540.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.025, 0.04, 0.94)),
            ZIndex(100),
        ))
        .with_children(|panel| {
            panel
                .spawn((
                    Node {
                        height: Val::Px(6.0),
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
            panel.spawn((
                BattleLabel,
                Text::new(String::with_capacity(512)),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
            ));
            panel
                .spawn((
                    Node {
                        height: Val::Px(12.0),
                        width: Val::Percent(100.0),
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
            panel
                .spawn((
                    RestartBattle,
                    Button,
                    Node {
                        align_self: AlignSelf::FlexStart,
                        padding: UiRect::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.16, 0.21, 0.25)),
                ))
                .with_children(|button| {
                    button.spawn((
                        Text::new("Reset encounter (R)"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                    ));
                });
        });
}

#[allow(clippy::type_complexity)]
pub(super) fn draw_battle_ui(
    battle: Option<ResMut<BattleState>>,
    wait: Option<Res<WaitHold>>,
    turn: Res<TurnState>,
    timer: Res<TokenTimer>,
    menu: Option<Res<SettingsMenu>>,
    scheme: Option<Res<ControlScheme>>,
    grid: Res<MapGrid>,
    frame: Res<RenderBuffers>,
    player: Query<(&Pos, &Health, &Tokens), With<Player>>,
    enemies: Query<(&Pos, &Health, &Fighter), (With<Enemy>, With<Active>)>,
    buttons: Query<&Interaction, (With<RestartBattle>, Changed<Interaction>)>,
    mut labels: Query<&mut Text, With<BattleLabel>>,
    mut panels: Query<&mut Visibility, With<BattlePanel>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), (With<WaitFill>, Without<TurnFill>)>,
    mut clocks: Query<(&mut Node, &mut BackgroundColor), With<TurnFill>>,
) {
    let (Some(mut battle), Some(wait)) = (battle, wait) else {
        return;
    };
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
        battle.restart_requested = true;
    }
    let Ok((pos, health, tokens)) = player.single() else {
        return;
    };
    for mut text in &mut labels {
        text.0.clear();
        if !battle.defeated && !battle.won {
            if turn.simulation || tokens.count <= 0 {
                writeln!(text.0, "TURN: resolving action").unwrap();
            } else {
                writeln!(
                    text.0,
                    "ACT IN {:.1}s | timeout: enemies act",
                    timer.0.remaining_secs()
                )
                .unwrap();
            }
        }
        writeln!(
            text.0,
            "HP {}/{}  |  Round {}  |  Enemies {}{}",
            health.current,
            health.maximum,
            battle.round,
            enemies
                .iter()
                .filter(|(_, health, _)| health.current > 0)
                .count(),
            if health.guarded { "  |  BRACED" } else { "" }
        )
        .unwrap();
        if battle.defeated {
            write!(text.0, "Defeated - reset to try again").unwrap();
        } else if battle.won {
            write!(text.0, "Battle won!").unwrap();
        } else if wait.latched && wait.progress() == 1.0 {
            write!(text.0, "WAIT COUNTED - release to wait again").unwrap();
        } else if turn.simulation || tokens.count <= 0 {
            write!(text.0, "Resolving action...").unwrap();
        } else if wait.elapsed > 0.0 {
            write!(
                text.0,
                "Hold... {:.1}s until WAIT counts",
                (WaitHold::SECONDS - wait.elapsed).max(0.0)
            )
            .unwrap();
        } else if wait.latched {
            write!(text.0, "Release wait input to brace again").unwrap();
        } else {
            write!(
                text.0,
                "{} {:.1}s to WAIT + brace",
                if scheme.as_deref() == Some(&ControlScheme::Touch) {
                    "Hold a finger still for"
                } else {
                    "Hold SPACE for"
                },
                WaitHold::SECONDS
            )
            .unwrap();
        }
        write!(
            text.0,
            "\n{}\n?: wind-up | !: next action | +: spent attack",
            battle.feedback
        )
        .unwrap();
        if let Some((_, enemy_health, fighter)) = enemies
            .iter()
            .filter(|(enemy_pos, health, _)| {
                health.current > 0
                    && grid
                        .idx(enemy_pos.0)
                        .is_some_and(|index| frame.seen_first[index] != RenderBuffers::NO_CELL)
            })
            .min_by_key(|(enemy_pos, _, _)| (enemy_pos.0 - pos.0).abs().element_sum())
        {
            write!(
                text.0,
                "\n{} HP {}/{}: ",
                match fighter.role {
                    BattleRole::Skirmisher => "Skirmisher",
                    BattleRole::Spearman => "Spear",
                    BattleRole::ShieldBearer => "Shield",
                    BattleRole::Brute => "Brute",
                },
                enemy_health.current,
                enemy_health.maximum
            )
            .unwrap();
            if fighter.intent == BattleIntent::Recover {
                write!(
                    text.0,
                    "{} - {} action{} OPEN; bump for 3 damage",
                    match fighter.role {
                        BattleRole::Spearman => "EXTENDED",
                        BattleRole::ShieldBearer => "SHIELD DOWN (any side)",
                        _ => "RECOVERING",
                    },
                    fighter.recovery_left + 1,
                    if fighter.recovery_left == 0 { "" } else { "s" }
                )
                .unwrap();
            } else {
                write!(text.0, "{}", enemy_hint(fighter)).unwrap();
            }
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
        node.width = Val::Percent(if battle.defeated || battle.won {
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

fn enemy_hint(fighter: &Fighter) -> &'static str {
    match (fighter.role, fighter.intent) {
        (_, BattleIntent::WindUp { .. }) => "SLAM IN 2 ACTIONS: leave yellow area (armoured)",
        (_, BattleIntent::Slam { .. }) => "SLAM NEXT: get outside red !; counter after",
        (BattleRole::Spearman, BattleIntent::Strike { .. }) => {
            "LUNGE NEXT: sidestep, then punish the miss"
        }
        (BattleRole::ShieldBearer, BattleIntent::Strike { .. }) => {
            "BASH NEXT: sidestep; shield drops after"
        }
        (BattleRole::Spearman, _) => "Bait its lane, sidestep, punish while EXTENDED",
        (BattleRole::ShieldBearer, _) => "Bait its bash; attack any side while shield DOWN",
        (BattleRole::Brute, _) => "Bait the slam, leave its area, then punish",
        _ => "Dodge red ! or bump to interrupt",
    }
}
