//! Map-screen HUD: top bar (turn, map mode, End Turn) and the busy overlay
//! shown while a turn resolves. The tile inspector lives in the side panel.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use crate::game::commands::GameCommand;
use crate::game::resources::TurnInfo;
use crate::map::layers::MapMode;
use crate::map::picking::PickingBlocker;
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ModalStack};

#[derive(Component)]
pub struct TurnDisplay;

#[derive(Component)]
pub struct ModeDisplay;

#[derive(Component)]
pub struct EndTurnButton;

#[derive(Component)]
pub struct BusyOverlay;

pub fn setup_hud(mut commands: Commands, theme: Res<Theme>) {
    // Top bar.
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(44.0),
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG),
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|bar| {
            bar.spawn((Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(18.0),
                ..default()
            },))
                .with_children(|left| {
                    left.spawn((
                        Text::new("Imperialism Remake"),
                        theme.font_bold(17.0),
                        TextColor(theme::GOLD),
                    ));
                    left.spawn((
                        Text::new("1815 Q1"),
                        theme.font(15.0),
                        TextColor(theme::TEXT),
                        TurnDisplay,
                    ));
                    left.spawn((
                        Text::new("Political map (M/Tab)"),
                        theme.font(13.0),
                        TextColor(Color::srgb(0.74, 0.77, 0.70)),
                        ModeDisplay,
                    ));
                });

            let end_turn = widgets::spawn_button(
                bar,
                &theme,
                ButtonProps {
                    label: "End Turn".into(),
                    width: Some(Val::Px(110.0)),
                    ..default()
                },
            );
            bar.commands().entity(end_turn).insert((
                EndTurnButton,
                widgets::TooltipText("Resolve the turn (Space)".into()),
            ));
        });

    // Busy overlay, shown only while a turn resolves.
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme::OVERLAY_BG),
            GlobalZIndex(100),
            Visibility::Hidden,
            BusyOverlay,
        ))
        .with_children(|overlay| {
            overlay.spawn((
                Text::new("Processing turn…"),
                theme.font_bold(26.0),
                TextColor(theme::GOLD),
            ));
        });
}

/// End-turn button (kit widget): sends [`GameCommand::EndTurn`] on
/// activation. Visual feedback comes from the widget kit.
pub fn end_turn_button(
    mut activations: MessageReader<ButtonActivated>,
    buttons: Query<(), With<EndTurnButton>>,
    mut commands_out: MessageWriter<GameCommand>,
) {
    for ButtonActivated(entity) in activations.read() {
        if buttons.contains(*entity) {
            commands_out.write(GameCommand::EndTurn);
        }
    }
}

pub fn keyboard_commands(
    keys: Res<ButtonInput<KeyCode>>,
    modals: Res<ModalStack>,
    focus: Res<InputFocus>,
    mut commands_out: MessageWriter<GameCommand>,
    mut exit: MessageWriter<AppExit>,
) {
    // A focused text input owns the keyboard; modals own Esc.
    if focus.0.is_none() && keys.just_pressed(KeyCode::Space) {
        commands_out.write(GameCommand::EndTurn);
    }
    if modals.is_empty() && keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

/// While a turn resolves the End Turn button is disabled (and re-enabled on
/// exit); the busy overlay communicates why.
pub fn disable_end_turn(mut commands: Commands, buttons: Query<Entity, With<EndTurnButton>>) {
    for button in &buttons {
        widgets::set_enabled(&mut commands, button, false);
    }
}

pub fn enable_end_turn(mut commands: Commands, buttons: Query<Entity, With<EndTurnButton>>) {
    for button in &buttons {
        widgets::set_enabled(&mut commands, button, true);
    }
}

pub fn update_turn_display(
    turn_info: Res<TurnInfo>,
    mut texts: Query<&mut Text, With<TurnDisplay>>,
) {
    if !turn_info.is_changed() {
        return;
    }
    for mut text in &mut texts {
        **text = turn_info.label.clone();
    }
}

pub fn update_mode_display(mode: Res<MapMode>, mut texts: Query<&mut Text, With<ModeDisplay>>) {
    if !mode.is_changed() {
        return;
    }
    for mut text in &mut texts {
        **text = format!("{} map (M/Tab)", mode.label());
    }
}

pub fn show_busy_overlay(mut overlays: Query<&mut Visibility, With<BusyOverlay>>) {
    for mut visibility in &mut overlays {
        *visibility = Visibility::Visible;
    }
}

pub fn hide_busy_overlay(mut overlays: Query<&mut Visibility, With<BusyOverlay>>) {
    for mut visibility in &mut overlays {
        *visibility = Visibility::Hidden;
    }
}
