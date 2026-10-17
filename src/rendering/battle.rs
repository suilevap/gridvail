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
    menu: Option<Res<SettingsMenu>>,
    scheme: Option<Res<ControlScheme>>,
    player: Query<(&Health, &Tokens), With<Player>>,
    enemies: Query<&Health, (With<Enemy>, With<Active>)>,
    buttons: Query<&Interaction, (With<RestartBattle>, Changed<Interaction>)>,
    mut labels: Query<&mut Text, With<BattleLabel>>,
    mut panels: Query<&mut Visibility, With<BattlePanel>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<WaitFill>>,
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
    let Ok((health, tokens)) = player.single() else {
        return;
    };
    for mut text in &mut labels {
        text.0.clear();
        writeln!(
            text.0,
            "HP {}/{}  |  Round {}  |  Enemies {}{}",
            health.current,
            health.maximum,
            battle.round,
            enemies.iter().filter(|health| health.current > 0).count(),
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
            "\n{}\nRed !: next attack | Blue enemy: recovering",
            battle.feedback
        )
        .unwrap();
    }
    for (mut node, mut color) in &mut fills {
        node.width = Val::Percent(wait.progress() * 100.0);
        color.0 = if wait.latched && wait.progress() == 1.0 {
            Color::srgb(0.25, 0.95, 0.55)
        } else {
            Color::srgb(0.25, 0.75, 0.95)
        };
    }
}
