//! Map-screen HUD: top bar (turn, map mode, End Turn) and the busy overlay
//! shown while a turn resolves. The tile inspector lives in the side panel.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use crate::game::commands::GameCommand;
use crate::game::resources::TurnInfo;
use crate::map::layers::MapMode;
use crate::map::picking::PickingBlocker;
use crate::state::Screen;
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ModalStack};

#[derive(Component)]
pub struct TurnDisplay;

/// Top-bar screen tab (web header tabs: F1 Map … F5 Trade).
#[derive(Component)]
pub struct ScreenTabButton(pub Screen);

/// `(screen, label, hotkey)` for the top-bar tabs and the F-key bindings.
pub const SCREEN_TABS: [(Screen, &str, &str); 10] = [
    (Screen::Map, "Map", "F1"),
    (Screen::Transport, "Transport", "F2"),
    (Screen::Industry, "Industry", "F3"),
    (Screen::Diplomacy, "Diplomacy", "F4"),
    (Screen::Trade, "Trade", "F5"),
    (Screen::Tech, "Tech", "F6"),
    (Screen::Ledger, "Ledger", "F7"),
    (Screen::News, "News", "F8"),
    (Screen::Battles, "Battles", "F9"),
    (Screen::Legend, "Legend", "F10"),
];

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
            // Compact left block — ten screen tabs share the 1280px bar.
            bar.spawn((Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                flex_shrink: 0.0,
                ..default()
            },))
                .with_children(|left| {
                    left.spawn((
                        Text::new("Imperialism"),
                        theme.font_bold(15.0),
                        TextColor(theme::GOLD),
                    ));
                    left.spawn((
                        Text::new("1815 Q1"),
                        theme.font(13.0),
                        TextColor(theme::TEXT),
                        TurnDisplay,
                    ));
                    left.spawn((
                        Text::new("Political (M/Tab)"),
                        theme.font(11.0),
                        TextColor(Color::srgb(0.74, 0.77, 0.70)),
                        ModeDisplay,
                    ));
                });

            // Screen tabs (web header: F1 Map, F2 Transport, F3 Industry,
            // F5 Trade).
            bar.spawn((Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.0),
                ..default()
            },))
                .with_children(|tabs| {
                    for (screen, label, hotkey) in SCREEN_TABS {
                        let button = widgets::spawn_button(
                            tabs,
                            &theme,
                            ButtonProps {
                                label: format!("{label} {hotkey}"),
                                font_size: 11.5,
                                flat: true,
                                auto_label_tint: false,
                                ..default()
                            },
                        );
                        tabs.commands().entity(button).insert((
                            ScreenTabButton(screen),
                            // Tighter than the kit default so ten tabs fit.
                            Node {
                                height: Val::Px(30.0),
                                padding: UiRect::horizontal(Val::Px(6.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border_radius: BorderRadius::all(Val::Px(3.0)),
                                flex_shrink: 0.0,
                                ..default()
                            },
                        ));
                    }
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
    focus: Res<InputFocus>,
    screen: Res<State<Screen>>,
    mut next_screen: ResMut<NextState<Screen>>,
    mut commands_out: MessageWriter<GameCommand>,
) {
    // A focused text input owns the keyboard. Esc is handled by the
    // cascading cancel in `game::selection::esc_cascade` (modals first).
    if focus.0.is_none() && keys.just_pressed(KeyCode::Space) {
        // Web parity: Space on the newspaper dismisses it instead of ending
        // another turn (the proposal modal then opens if proposals pend).
        if *screen.get() == Screen::News {
            next_screen.set(Screen::Map);
        } else {
            commands_out.write(GameCommand::EndTurn);
        }
    }
}

/// Top-bar tab clicks switch the active screen.
pub fn handle_screen_tabs(
    mut activations: MessageReader<ButtonActivated>,
    buttons: Query<&ScreenTabButton>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(tab) = buttons.get(*entity) {
            next_screen.set(tab.0);
        }
    }
}

/// F1/F2/F3/F5 jump to a screen; Esc returns to the map from a non-map
/// screen once no modal is open (web `isFullScreen` Esc parity).
pub fn screen_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    screen: Res<State<Screen>>,
    modal_stack: Res<ModalStack>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    if focus.0.is_some() {
        return;
    }
    for (target, key) in [
        (Screen::Map, KeyCode::F1),
        (Screen::Transport, KeyCode::F2),
        (Screen::Industry, KeyCode::F3),
        (Screen::Diplomacy, KeyCode::F4),
        (Screen::Trade, KeyCode::F5),
        (Screen::Tech, KeyCode::F6),
        (Screen::Ledger, KeyCode::F7),
        (Screen::News, KeyCode::F8),
        (Screen::Battles, KeyCode::F9),
        (Screen::Legend, KeyCode::F10),
    ] {
        if keys.just_pressed(key) {
            next_screen.set(target);
        }
    }
    // Web parity: Esc only exits the full-screen views (Transport keeps the
    // map live; its Esc belongs to the map's cancel cascade).
    if keys.just_pressed(KeyCode::Escape) && modal_stack.is_empty() && screen.get().is_full_screen()
    {
        next_screen.set(Screen::Map);
    }
}

/// Gold-tint the active screen tab.
pub fn update_screen_tabs(
    screen: Res<State<Screen>>,
    buttons: Query<(&ScreenTabButton, &Children)>,
    mut labels: Query<&mut TextColor>,
) {
    if !screen.is_changed() {
        return;
    }
    for (tab, children) in &buttons {
        let color = if tab.0 == *screen.get() {
            theme::GOLD
        } else {
            theme::TEXT_DIM
        };
        for child in children {
            if let Ok(mut text_color) = labels.get_mut(*child)
                && text_color.0 != color
            {
                text_color.0 = color;
            }
        }
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
        **text = format!("{} (M/Tab)", mode.label());
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
