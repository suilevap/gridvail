//! The in-game settings menu (Esc) and the compass.
//!
//! The menu lists the settings a player may want to choose between: how the
//! view behaves through portals that turn, when the compass shows, and the
//! motion style. Up and down pick a line, left, right and Enter change it.
//! While it is open the player and the camera ignore the keys.
//!
//! The compass shows which way the map's north, east, south and west lie on
//! screen. Its letters stay upright and move round as the view turns.

use bevy::prelude::*;

use crate::animation::MotionStyle;
use crate::camera::{CameraOperator, PortalTurn};
use crate::model::*;

/// The menu's lines, in order.
const LINES: usize = 3;
/// The compass's size and how far its letters sit from its centre, in px.
const COMPASS_SIZE: f32 = 72.0;
const COMPASS_RADIUS: f32 = 26.0;
const LETTER_SIZE: f32 = 14.0;

#[derive(Component)]
pub(super) struct SettingsText;

#[derive(Component)]
pub(super) struct Compass;

/// A compass letter and the map direction it marks.
#[derive(Component, Clone, Copy)]
pub(super) struct CompassLetter(Vec2);

/// Spawns the (hidden) menu and compass on the UI of `camera`.
pub(super) fn spawn_settings_ui(commands: &mut Commands, camera: Entity) {
    commands.spawn((
        SettingsText,
        UiTargetCamera(camera),
        Text::new(String::with_capacity(256)),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(35.0),
            left: Val::Percent(30.0),
            padding: UiRect::all(Val::Px(12.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.95)),
        BorderColor::all(Color::srgb(0.5, 0.5, 0.5)),
        ZIndex(200),
        Visibility::Hidden,
    ));
    commands
        .spawn((
            Compass,
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                bottom: Val::Px(16.0),
                width: Val::Px(COMPASS_SIZE),
                height: Val::Px(COMPASS_SIZE),
                border_radius: BorderRadius::MAX,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.8)),
            BorderColor::all(Color::srgb(0.4, 0.4, 0.4)),
            Visibility::Hidden,
        ))
        .with_children(|compass| {
            for (letter, direction, color) in [
                ("N", Vec2::NEG_Y, Color::srgb(1.0, 0.3, 0.3)),
                ("E", Vec2::X, Color::WHITE),
                ("S", Vec2::Y, Color::WHITE),
                ("W", Vec2::NEG_X, Color::WHITE),
            ] {
                compass.spawn((
                    CompassLetter(direction),
                    Text::new(letter),
                    TextFont {
                        font_size: FontSize::Px(LETTER_SIZE),
                        ..default()
                    },
                    TextColor(color),
                    Node {
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                ));
            }
        });
}

/// Esc opens and closes the menu; while open, up and down pick a line and
/// left, right or Enter change it.
pub(super) fn settings_input(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut menu: ResMut<SettingsMenu>,
    mut compass: ResMut<CompassMode>,
    operator: Option<ResMut<CameraOperator>>,
    motion: Option<ResMut<MotionStyle>>,
) {
    let Some(keys) = keys else {
        return;
    };
    if keys.just_pressed(KeyCode::Escape) {
        menu.open = !menu.open;
    }
    if !menu.open {
        return;
    }
    let pressed = |codes: [KeyCode; 2]| keys.any_just_pressed(codes);
    if pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        menu.selected = (menu.selected + LINES - 1) % LINES;
    }
    if pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        menu.selected = (menu.selected + 1) % LINES;
    }
    let step = if pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
        -1
    } else if pressed([KeyCode::ArrowRight, KeyCode::KeyD])
        || pressed([KeyCode::Enter, KeyCode::Space])
    {
        1
    } else {
        return;
    };
    match menu.selected {
        0 => {
            if let Some(mut operator) = operator {
                operator.portal_turn = operator.portal_turn.toggled();
            }
        }
        1 => *compass = compass.step(step),
        _ => {
            if let Some(mut motion) = motion {
                let presets = MotionStyle::PRESETS.len() as i32;
                for _ in 0..step.rem_euclid(presets) {
                    *motion = motion.next();
                }
            }
        }
    }
}

/// What each setting's value reads as in the menu.
fn portal_turn_label(turn: PortalTurn) -> &'static str {
    match turn {
        PortalTurn::WithTarget => "turn with the player",
        PortalTurn::KeepNorth => "keep north up",
    }
}

fn compass_label(mode: CompassMode) -> &'static str {
    match mode {
        CompassMode::WhenTurned => "when the view is turned",
        CompassMode::Always => "always",
        CompassMode::Off => "off",
    }
}

/// Shows the menu while it is open.
pub(super) fn draw_settings(
    menu: Res<SettingsMenu>,
    compass: Res<CompassMode>,
    operator: Option<Res<CameraOperator>>,
    motion: Option<Res<MotionStyle>>,
    mut panels: Query<(&mut Text, &mut Visibility), With<SettingsText>>,
) {
    for (mut text, mut visibility) in &mut panels {
        let desired = if menu.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(desired);
        if !menu.open {
            continue;
        }
        let lines = [
            (
                "Portal view",
                operator
                    .as_ref()
                    .map_or("-", |operator| portal_turn_label(operator.portal_turn)),
            ),
            ("Compass", compass_label(*compass)),
            (
                "Motion",
                motion.as_ref().map_or("-", |motion| motion.name()),
            ),
        ];
        let mut drawn = String::from("Settings (Esc closes)\n");
        for (index, (name, value)) in lines.iter().enumerate() {
            let cursor = if index == menu.selected { '>' } else { ' ' };
            drawn.push_str(&format!("\n{cursor} {name}: < {value} >"));
        }
        drawn.push_str("\n\nUp/Down choose, Left/Right change");
        if text.0 != drawn {
            text.0 = drawn;
        }
    }
}

/// Turns the compass with the view and shows it as `CompassMode` says.
pub(super) fn draw_compass(
    camera: Res<ViewCamera>,
    mode: Res<CompassMode>,
    mut compasses: Query<&mut Visibility, With<Compass>>,
    mut letters: Query<(&CompassLetter, &mut Node)>,
) {
    let shown = mode.shown(camera.rotation);
    for mut visibility in &mut compasses {
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    if !shown {
        return;
    }
    let centre = Vec2::splat(COMPASS_SIZE / 2.0);
    for (letter, mut node) in &mut letters {
        // Where that map direction points on screen (y down).
        let at = centre + camera.turn() * letter.0 * COMPASS_RADIUS - LETTER_SIZE / 2.0;
        let (left, top) = (Val::Px(at.x), Val::Px(at.y));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::animation::ObjectAnimationPlugin;
    use crate::app::GamePlugin;
    use crate::rendering::TextRendererPlugin;

    fn boot() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins((GamePlugin, ObjectAnimationPlugin, TextRendererPlugin));
        for _ in 0..64 {
            app.update();
        }
        app
    }

    /// Presses `key` for one frame.
    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        app.update();
    }

    fn player(app: &mut App) -> IVec2 {
        let world = app.world_mut();
        world
            .query_filtered::<&Pos, With<Player>>()
            .single(world)
            .unwrap()
            .0
    }

    #[test]
    fn the_menu_changes_settings_and_holds_the_player_and_camera() {
        let mut app = boot();
        tap(&mut app, KeyCode::Escape);
        assert!(app.world().resource::<SettingsMenu>().open);

        // Arrow keys move the cursor, not the player.
        let start = player(&mut app);
        tap(&mut app, KeyCode::ArrowRight);
        assert_eq!(
            app.world().resource::<CameraOperator>().portal_turn,
            PortalTurn::KeepNorth
        );
        tap(&mut app, KeyCode::ArrowDown);
        tap(&mut app, KeyCode::ArrowRight);
        assert_eq!(*app.world().resource::<CompassMode>(), CompassMode::Always);
        tap(&mut app, KeyCode::ArrowLeft);
        tap(&mut app, KeyCode::ArrowLeft);
        assert_eq!(*app.world().resource::<CompassMode>(), CompassMode::Off);
        tap(&mut app, KeyCode::ArrowDown);
        let motion = *app.world().resource::<MotionStyle>();
        tap(&mut app, KeyCode::ArrowRight);
        assert_eq!(*app.world().resource::<MotionStyle>(), motion.next());
        // Camera keys wait too.
        tap(&mut app, KeyCode::KeyQ);
        assert_eq!(
            app.world().resource::<CameraOperator>().target_rotation(),
            0.0
        );
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(player(&mut app), start, "the player waited");

        // The menu shows what it set.
        let world = app.world_mut();
        let (text, visibility) = world
            .query_filtered::<(&Text, &Visibility), With<SettingsText>>()
            .single(world)
            .unwrap();
        assert_eq!(*visibility, Visibility::Inherited);
        assert!(text.0.contains("keep north up"), "{}", text.0);
        assert!(text.0.contains("Compass: < off >"), "{}", text.0);

        tap(&mut app, KeyCode::Escape);
        assert!(!app.world().resource::<SettingsMenu>().open);
        tap(&mut app, KeyCode::KeyQ);
        assert!(app.world().resource::<CameraOperator>().target_rotation() > 0.0);
    }

    #[test]
    fn the_compass_shows_where_north_is_when_the_view_is_turned() {
        let mut app = boot();
        let shown = |app: &mut App| {
            let world = app.world_mut();
            *world
                .query_filtered::<&Visibility, With<Compass>>()
                .single(world)
                .unwrap()
                != Visibility::Hidden
        };
        assert!(!shown(&mut app), "hidden while north is up");
        app.world_mut()
            .resource_mut::<CameraOperator>()
            .turn_by_quarters(1);
        for _ in 0..40 {
            app.update();
        }
        assert!(shown(&mut app));
        // Turned a quarter turn counter-clockwise, north is to the left.
        let world = app.world_mut();
        let north = world
            .query::<(&CompassLetter, &Node)>()
            .iter(world)
            .find(|(letter, _)| letter.0 == Vec2::NEG_Y)
            .map(|(_, node)| (node.left, node.top))
            .unwrap();
        let (Val::Px(left), Val::Px(top)) = north else {
            panic!("placed in pixels");
        };
        let centre = COMPASS_SIZE / 2.0 - LETTER_SIZE / 2.0;
        assert!((left - (centre - COMPASS_RADIUS)).abs() < 1e-3, "{left}");
        assert!((top - centre).abs() < 1e-3, "{top}");

        // Always and off override the view.
        app.insert_resource(CompassMode::Off);
        app.update();
        assert!(!shown(&mut app));
    }
}
