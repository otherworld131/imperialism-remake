//! Map-screen HUD: top bar (turn, map mode, End Turn), the turn-convenience
//! strip (viewpoint, Skip N / Skip Until, Save / Load / Restart) and the
//! busy overlay shown while a turn resolves. The tile inspector lives in
//! the side panel.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use std::sync::atomic::Ordering;

use crate::game::commands::GameCommand;
use crate::game::resources::{GameMeta, TurnInfo, ViewModels};
use crate::game::turn_runner::{ActiveSkip, BusyProgress};
use crate::map::layers::MapMode;
use crate::map::picking::PickingBlocker;
use crate::screens::saveload;
use crate::setup::jobs::ActiveSetupJob;
use crate::state::Screen;
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ButtonActivated, ButtonProps, DropdownChanged, DropdownProps, ModalStack, UiDropdown,
    UiTextInput,
};

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

/// Busy-overlay status line ("Processing 1834 Q2… (12/50)").
#[derive(Component)]
pub struct BusyText;

/// Cancel button inside the busy overlay; visible only during skip runs.
#[derive(Component)]
pub struct SkipCancelBtn;

// ── Turn-convenience strip (second top bar) ─────────────────────────────

/// Observer viewpoint dropdown (hidden in human games).
#[derive(Component)]
pub struct ViewpointDropdown;

#[derive(Component)]
pub struct ViewpointRow;

#[derive(Component)]
pub struct SkipCountInput;

#[derive(Component)]
pub struct SkipNBtn;

#[derive(Component)]
pub struct SkipUntilInput;

#[derive(Component)]
pub struct SkipUntilBtn;

/// "☰" button that opens the burger menu.
#[derive(Component)]
pub struct BurgerMenuBtn;

/// Burger-menu popover holding the save/load/skip machinery.
#[derive(Component)]
pub struct BurgerMenu;

#[derive(Component)]
pub struct SaveBtn;

#[derive(Component)]
pub struct LoadBtn;

#[derive(Component)]
pub struct RestartBtn;

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
            BackgroundColor(theme::PANEL_BG_SOLID),
            Interaction::default(),
            PickingBlocker,
            crate::screens::session::SessionHiddenChrome,
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
                    let burger = widgets::spawn_button(
                        left,
                        &theme,
                        ButtonProps {
                            label: "☰".into(),
                            font_size: 13.0,
                            width: Some(Val::Px(34.0)),
                            ..default()
                        },
                    );
                    left.commands().entity(burger).insert((
                        BurgerMenuBtn,
                        widgets::TooltipText("Menu: viewpoint, save / load, skip turns".into()),
                    ));

                    // The menu itself: a vertical popover under the button.
                    left.spawn((
                        BurgerMenu,
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(40.0),
                            left: Val::Px(6.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            padding: UiRect::all(Val::Px(10.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            display: Display::None,
                            ..default()
                        },
                        BackgroundColor(theme::PANEL_BG_SOLID),
                        BorderColor::all(theme::BORDER),
                        GlobalZIndex(20),
                    ))
                    .with_children(|menu| {
                        // Observer viewpoint dropdown (synced from the roster).
                        menu.spawn((
                            ViewpointRow,
                            Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(6.0),
                                ..default()
                            },
                        ))
                        .with_children(|row| {
                            row.spawn((
                                Text::new("View:"),
                                theme.font(11.0),
                                TextColor(theme::TEXT_DIM),
                            ));
                            let dropdown = widgets::spawn_dropdown(
                                row,
                                &theme,
                                DropdownProps {
                                    options: vec!["—".to_string()],
                                    selected: 0,
                                    width: Val::Px(160.0),
                                },
                            );
                            row.commands().entity(dropdown).insert(ViewpointDropdown);
                        });

                        menu.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|row| {
                            for (label, tooltip) in [
                                ("Save", "Save to ./saves/ (CLI-compatible)"),
                                ("Load", "Load a save from ./saves/"),
                                ("↻ Restart", "Restart this map from turn 1"),
                            ] {
                                let button = widgets::spawn_button(
                                    row,
                                    &theme,
                                    ButtonProps {
                                        label: label.into(),
                                        font_size: 11.5,
                                        ..default()
                                    },
                                );
                                let mut commands = row.commands();
                                let mut entity = commands.entity(button);
                                entity.insert(widgets::TooltipText(tooltip.into()));
                                match label {
                                    "Save" => entity.insert(SaveBtn),
                                    "Load" => entity.insert(LoadBtn),
                                    _ => entity.insert(RestartBtn),
                                };
                            }
                        });

                        // Skip machinery (dev): Skip N / Skip Until.
                        menu.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Text::new("Skip"),
                                theme.font(11.0),
                                TextColor(theme::TEXT_DIM),
                            ));
                            let count = widgets::spawn_text_input(
                                row,
                                &theme,
                                widgets::TextInputProps {
                                    width: Val::Px(46.0),
                                    max_len: 3,
                                    value: "5".into(),
                                    ..default()
                                },
                            );
                            row.commands().entity(count).insert(SkipCountInput);
                            let skip = widgets::spawn_button(
                                row,
                                &theme,
                                ButtonProps {
                                    label: "Go".into(),
                                    font_size: 11.5,
                                    ..default()
                                },
                            );
                            row.commands().entity(skip).insert((
                                SkipNBtn,
                                widgets::TooltipText(
                                    "Process N turns (1–500) with progress + cancel".into(),
                                ),
                            ));
                        });
                        menu.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Text::new("Until"),
                                theme.font(11.0),
                                TextColor(theme::TEXT_DIM),
                            ));
                            let until = widgets::spawn_text_input(
                                row,
                                &theme,
                                widgets::TextInputProps {
                                    width: Val::Px(140.0),
                                    max_len: 40,
                                    value: String::new(),
                                    ..default()
                                },
                            );
                            row.commands().entity(until).insert(SkipUntilInput);
                            let until_btn = widgets::spawn_button(
                                row,
                                &theme,
                                ButtonProps {
                                    label: "Skip Until".into(),
                                    font_size: 11.5,
                                    ..default()
                                },
                            );
                            row.commands().entity(until_btn).insert((
                        SkipUntilBtn,
                        widgets::TooltipText(
                            "Skip turns until a headline contains this text (case-insensitive)"
                                .into(),
                        ),
                    ));
                        });
                    });

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
}

/// The busy overlay lives outside the HUD so the setup flow's async world
/// generation can use it too. Spawned once at startup.
pub fn spawn_busy_overlay(mut commands: Commands, theme: Res<Theme>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(theme::OVERLAY_BG),
            GlobalZIndex(100),
            Visibility::Hidden,
            BusyOverlay,
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|overlay| {
            overlay.spawn((
                Text::new("Processing turn…"),
                theme.font_bold(26.0),
                TextColor(theme::GOLD),
                BusyText,
            ));
            let cancel = widgets::spawn_button(
                overlay,
                &theme,
                ButtonProps {
                    label: "Cancel".into(),
                    width: Some(Val::Px(120.0)),
                    ..default()
                },
            );
            overlay
                .commands()
                .entity(cancel)
                .insert((SkipCancelBtn, Visibility::Hidden));
        });
}

/// Busy-overlay status line: skip progress → world generation → plain turn.
pub fn update_busy_text(
    progress: Res<BusyProgress>,
    skip: Res<ActiveSkip>,
    job: Res<ActiveSetupJob>,
    mut texts: Query<&mut Text, With<BusyText>>,
) {
    let target = if skip.0.is_some() && !progress.0.is_empty() {
        progress.0.clone()
    } else if skip.0.is_some() {
        "Processing turns…".to_string()
    } else if job.0.is_some() {
        "Generating world…".to_string()
    } else {
        "Processing turn…".to_string()
    };
    for mut text in &mut texts {
        if **text != target {
            **text = target.clone();
        }
    }
}

/// The Cancel button shows only while a skip run is in flight, and flags
/// the run's cancel token when activated (the task stops after the turn in
/// progress).
pub fn handle_skip_cancel(
    mut activations: MessageReader<ButtonActivated>,
    buttons: Query<Entity, With<SkipCancelBtn>>,
    mut visibilities: Query<&mut Visibility, With<SkipCancelBtn>>,
    skip: Res<ActiveSkip>,
) {
    let target = if skip.0.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut visibility in &mut visibilities {
        if *visibility != target {
            *visibility = target;
        }
    }
    for ButtonActivated(entity) in activations.read() {
        if buttons.contains(*entity)
            && let Some(run) = skip.0.as_ref()
        {
            run.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// Skip / Save / Load / Restart buttons in the convenience strip.
#[allow(clippy::too_many_arguments)]
pub fn handle_convenience_buttons(
    mut activations: MessageReader<ButtonActivated>,
    mut commands: Commands,
    theme: Res<Theme>,
    mut stack: ResMut<ModalStack>,
    skip_n: Query<(), With<SkipNBtn>>,
    skip_until: Query<(), With<SkipUntilBtn>>,
    save: Query<(), With<SaveBtn>>,
    load: Query<(), With<LoadBtn>>,
    restart: Query<(), With<RestartBtn>>,
    count_inputs: Query<&UiTextInput, With<SkipCountInput>>,
    until_inputs: Query<&UiTextInput, With<SkipUntilInput>>,
    session: Res<crate::game::resources::SessionRes>,
    mut commands_out: MessageWriter<GameCommand>,
) {
    for ButtonActivated(entity) in activations.read() {
        if skip_n.contains(*entity) {
            let count = count_inputs
                .iter()
                .next()
                .and_then(|i| i.value.trim().parse::<u32>().ok())
                .unwrap_or(5)
                .clamp(1, 500);
            commands_out.write(GameCommand::SkipTurns { count });
        } else if skip_until.contains(*entity) {
            let text = until_inputs
                .iter()
                .next()
                .map(|i| i.value.clone())
                .unwrap_or_default();
            commands_out.write(GameCommand::SkipUntil { text });
        } else if save.contains(*entity) {
            let default_name = saveload::default_save_name(&session);
            saveload::open_save_modal(&mut commands, &mut stack, &theme, &default_name);
        } else if load.contains(*entity) {
            saveload::open_load_modal(&mut commands, &mut stack, &theme);
        } else if restart.contains(*entity) {
            saveload::open_restart_modal(&mut commands, &mut stack, &theme);
        }
    }
}

/// Keep the viewpoint dropdown's options in step with the Great-Power
/// roster (names change on re-roll / restart / load) and its selection on
/// the current viewpoint. Hidden entirely in non-observer games.
pub fn sync_viewpoint_dropdown(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    mut rows: Query<&mut Visibility, With<ViewpointRow>>,
    mut dropdowns: Query<&mut UiDropdown, With<ViewpointDropdown>>,
) {
    let target = if meta.observer {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut visibility in &mut rows {
        if *visibility != target {
            *visibility = target;
        }
    }
    if !meta.observer {
        return;
    }
    let gps: Vec<(u32, String)> = vms
        .nations
        .iter()
        .filter(|n| n.nation_type == "GreatPower")
        .map(|n| (n.nation_id, n.name.clone()))
        .collect();
    if gps.is_empty() {
        return;
    }
    let options: Vec<String> = gps.iter().map(|(_, name)| name.clone()).collect();
    let selected = gps
        .iter()
        .position(|(id, _)| *id == meta.player_nation)
        .unwrap_or(0);
    for mut dropdown in &mut dropdowns {
        if dropdown.options != options || dropdown.selected != selected {
            dropdown.options = options.clone();
            dropdown.selected = selected;
        }
    }
}

/// Dropdown selection → viewpoint switch command.
pub fn handle_viewpoint_dropdown(
    mut changes: MessageReader<DropdownChanged>,
    dropdowns: Query<(), With<ViewpointDropdown>>,
    mut commands_out: MessageWriter<GameCommand>,
) {
    for change in changes.read() {
        if dropdowns.contains(change.entity) {
            commands_out.write(GameCommand::SetViewpoint {
                index: change.index,
            });
        }
    }
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

/// Toggle the burger menu; triggering any action in it closes it.
pub fn handle_burger_menu(
    mut activations: MessageReader<widgets::ButtonActivated>,
    toggles: Query<(), With<BurgerMenuBtn>>,
    actions: Query<
        (),
        Or<(
            With<SkipNBtn>,
            With<SkipUntilBtn>,
            With<SaveBtn>,
            With<LoadBtn>,
            With<RestartBtn>,
        )>,
    >,
    mut menus: Query<&mut Node, With<BurgerMenu>>,
) {
    for widgets::ButtonActivated(entity) in activations.read() {
        if toggles.contains(*entity) {
            for mut node in &mut menus {
                node.display = if node.display == Display::None {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        } else if actions.contains(*entity) {
            for mut node in &mut menus {
                node.display = Display::None;
            }
        }
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

/// Restart needs the original generation parameters; loaded saves don't
/// carry them, so ↻ is disabled rather than silently regenerating a
/// different world (review finding F-004).
pub fn sync_restart_enabled(
    mut commands: Commands,
    active_config: Res<crate::setup::ActiveGameConfig>,
    buttons: Query<Entity, With<RestartBtn>>,
) {
    if !active_config.is_changed() {
        return;
    }
    let enabled = active_config.0.is_some();
    for button in &buttons {
        widgets::set_enabled(&mut commands, button, enabled);
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
