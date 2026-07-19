//! Map-screen HUD: top bar (turn, map mode, End Turn), the turn-convenience
//! strip (viewpoint, Skip N / Skip Until, Save / Load / Restart) and the
//! busy overlay shown while a turn resolves. The tile inspector lives in
//! the side panel.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use std::sync::atomic::Ordering;

use crate::game::commands::GameCommand;
use crate::game::resources::{GameMeta, NewsDebugSettings, RenderSettings, TurnInfo, ViewModels};
use crate::game::turn_runner::{ActiveSkip, BusyProgress};
use crate::map::picking::PickingBlocker;
use crate::screens::saveload;
use crate::screens::side_panel;
use crate::setup::jobs::ActiveSetupJob;
use crate::state::{AppState, Screen};
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ButtonActivated, ButtonProps, DropdownChanged, DropdownProps, ModalProps, ModalStack,
    ScrollProps, UiDropdown, UiTextInput, open_modal,
};

/// Chrome spawned on entering `AppState::InGame` (top bar, side panel, map
/// tooltip); despawned again on leaving the state (Quit to Title).
#[derive(Component)]
pub struct InGameChrome;

#[derive(Component)]
pub struct TurnDisplay;

/// Top-bar screen tab (web header tabs: F1 Map … F5 Trade).
#[derive(Component)]
pub struct ScreenTabButton(pub Screen);

/// Pictogram slot inside a screen tab; the `ImageNode` is filled in by
/// [`populate_screen_tab_icons`] once `IconAssets` finished loading (the
/// HUD can spawn before `Startup` when a debug shortcut boots straight
/// into `InGame`).
#[derive(Component)]
pub struct ScreenTabIcon(pub &'static str);

/// Text label inside a screen tab; visible only while its tab is active
/// (Industry inner-tab behavior).
#[derive(Component)]
pub struct ScreenTabLabel(pub Screen);

/// `(screen, label, hotkey, ui-icon name)` for the top-bar tabs and the
/// F-key bindings.
pub const SCREEN_TABS: [(Screen, &str, &str, &str); 10] = [
    (Screen::Map, "Map", "F1", "Map"),
    (Screen::Transport, "Transport", "F2", "FreightCar"),
    (Screen::Industry, "Industry", "F3", "Factory"),
    (Screen::Diplomacy, "Diplomacy", "F4", "Diplomacy"),
    (Screen::Trade, "Trade", "F5", "Trade"),
    (Screen::Tech, "Tech", "F6", "Science"),
    (Screen::Ledger, "Ledger", "F7", "Ledger"),
    (Screen::News, "News", "F8", "News"),
    (Screen::Battles, "Battles", "F9", "Swords"),
    (Screen::Legend, "Legend", "F10", "Legend"),
];

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

/// "Quit to Title" burger-menu entry: opens the confirm modal.
#[derive(Component)]
pub struct QuitToTitleBtn;

/// Confirm button inside the quit modal; leaves the game for the title
/// screen.
#[derive(Component)]
pub struct QuitConfirmBtn {
    pub modal: Entity,
}

pub fn setup_hud(
    mut commands: Commands,
    theme: Res<Theme>,
    settings: Res<RenderSettings>,
    news_debug: Res<NewsDebugSettings>,
    ui_scale: Res<bevy::ui::UiScale>,
    debug_expanded: Res<side_panel::DebugPanelExpanded>,
) {
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
            InGameChrome,
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
                        widgets::TooltipText(
                            "Menu: save / load, display settings, quit to title".into(),
                        ),
                    ));

                    // The menu itself: a vertical popover under the button.
                    // Capped to the viewport and scrollable so it never
                    // overflows small windows at large UI scales.
                    let mut scroll_content = Entity::PLACEHOLDER;
                    left.spawn((
                        BurgerMenu,
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(40.0),
                            left: Val::Px(6.0),
                            width: Val::Px(280.0),
                            max_height: Val::Vh(82.0),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(10.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            display: Display::None,
                            ..default()
                        },
                        BackgroundColor(theme::PANEL_BG_SOLID),
                        BorderColor::all(theme::BORDER),
                        GlobalZIndex(20),
                        // The popover spills below the top bar over the map;
                        // tag it so map picking and wheel-zoom treat it as UI
                        // (Trello #543).
                        Interaction::default(),
                        PickingBlocker,
                    ))
                    .with_children(|menu| {
                        let scroll = widgets::spawn_scroll_area(
                            menu,
                            &theme,
                            ScrollProps {
                                flex_grow: 1.0,
                                ..default()
                            },
                        );
                        scroll_content = scroll.content;
                    });
                    let mut commands = left.commands();
                    commands.entity(scroll_content).insert(Node {
                        flex_grow: 1.0,
                        min_width: Val::Px(0.0),
                        min_height: Val::Px(0.0),
                        height: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    });
                    commands.entity(scroll_content).with_children(|menu| {
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

                        // Leave the game for the title screen (confirmed) —
                        // grouped with the other game actions above the
                        // settings sections.
                        let quit = widgets::spawn_button(
                            menu,
                            &theme,
                            ButtonProps {
                                label: "Quit to Title".into(),
                                font_size: 11.5,
                                ..default()
                            },
                        );
                        menu.commands().entity(quit).insert((
                            QuitToTitleBtn,
                            widgets::TooltipText(
                                "Leave this game and return to the title screen".into(),
                            ),
                        ));

                        // Display settings + Debug toggles (moved here from
                        // the map side panel).
                        side_panel::spawn_display_and_debug(
                            menu,
                            &theme,
                            &settings,
                            &news_debug,
                            ui_scale.0,
                            debug_expanded.0,
                        );

                        // Skip machinery (dev): Skip N / Skip Until.
                        menu_section_title(menu, &theme, "Turns");
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
                        theme.font_bold(14.0),
                        TextColor(theme::GOLD),
                    ));
                    left.spawn((
                        Text::new("1815 Q1"),
                        theme.font(13.0),
                        TextColor(theme::TEXT),
                        TurnDisplay,
                    ));
                });

            // Screen tabs: pixel-art pictograms (Industry inner-tab
            // behavior — unlabeled unless active, tooltip names the screen
            // and its F-key). Icons are filled in by
            // [`populate_screen_tab_icons`].
            bar.spawn((Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.0),
                flex_shrink: 0.0,
                ..default()
            },))
                .with_children(|tabs| {
                    let active = Screen::default();
                    for (screen, label, hotkey, icon) in SCREEN_TABS {
                        let button = widgets::spawn_button(
                            tabs,
                            &theme,
                            ButtonProps {
                                label: String::new(),
                                flat: true,
                                auto_label_tint: false,
                                ..default()
                            },
                        );
                        let is_active = screen == active;
                        let mut commands = tabs.commands();
                        let mut entity = commands.entity(button);
                        entity.insert((
                            ScreenTabButton(screen),
                            // Tighter than the kit default so ten tabs fit.
                            Node {
                                height: Val::Px(32.0),
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(4.0),
                                padding: UiRect::axes(Val::Px(5.0), Val::Px(2.0)),
                                border: UiRect::bottom(Val::Px(2.0)),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            BorderColor::all(if is_active { theme::GOLD } else { Color::NONE }),
                            widgets::TooltipText(format!("{label} ({hotkey})")),
                        ));
                        entity.with_children(|tab| {
                            tab.spawn((
                                ScreenTabIcon(icon),
                                Node {
                                    width: Val::Px(20.0),
                                    height: Val::Px(20.0),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                            ));
                            tab.spawn((
                                ScreenTabLabel(screen),
                                Text::new(label),
                                theme.font_bold(12.5),
                                TextColor(theme::GOLD),
                                Node {
                                    display: if is_active {
                                        Display::Flex
                                    } else {
                                        Display::None
                                    },
                                    ..default()
                                },
                            ));
                        });
                    }
                });

            let end_turn = widgets::spawn_button(
                bar,
                &theme,
                ButtonProps {
                    label: "End Turn".into(),
                    width: Some(Val::Px(100.0)),
                    ..default()
                },
            );
            bar.commands().entity(end_turn).insert((
                EndTurnButton,
                widgets::TooltipText("Resolve the turn (Space)".into()),
            ));
        });
}

/// Small gold heading inside the burger menu.
fn menu_section_title(parent: &mut ChildSpawnerCommands, theme: &Theme, title: &str) {
    parent.spawn((
        Text::new(title.to_string()),
        theme.font_bold(12.5),
        TextColor(theme::GOLD),
        Node {
            margin: UiRect::top(Val::Px(4.0)),
            ..default()
        },
    ));
}

/// Fill each screen tab's icon slot once `IconAssets` is available (the
/// HUD may spawn before `Startup` finishes when a debug shortcut boots
/// straight into `InGame`).
pub fn populate_screen_tab_icons(
    mut commands: Commands,
    icons: Option<Res<crate::map::icons::IconAssets>>,
    slots: Query<(Entity, &ScreenTabIcon), Without<ImageNode>>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (entity, slot) in &slots {
        if let Some(image) = icons.get("ui", slot.0) {
            commands.entity(entity).insert(ImageNode::new(image));
        } else {
            // Insert a blank ImageNode so the slot leaves the query and the
            // warning fires once, not every frame.
            warn!(
                "screen-tab icon 'ui/{}' missing — tab renders blank",
                slot.0
            );
            commands.entity(entity).insert(ImageNode::default());
        }
    }
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
    screen: Res<State<Screen>>,
    mut commands_out: MessageWriter<GameCommand>,
) {
    // Advancing turns (Skip N / Until) shares the newspaper wedge with End
    // Turn: dismiss the paper before resolving anything else.
    let parked = *screen.get() == Screen::News;
    for ButtonActivated(entity) in activations.read() {
        if skip_n.contains(*entity) {
            if parked {
                continue;
            }
            let count = count_inputs
                .iter()
                .next()
                .and_then(|i| i.value.trim().parse::<u32>().ok())
                .unwrap_or(5)
                .clamp(1, 500);
            commands_out.write(GameCommand::SkipTurns { count });
        } else if skip_until.contains(*entity) {
            if parked {
                continue;
            }
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
/// the current viewpoint. Removed from the layout entirely in non-observer
/// games (`Display::None`, not `Visibility` — a hidden row would still
/// leave a blank gap at the top of the burger menu).
pub fn sync_viewpoint_dropdown(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    mut rows: Query<&mut Node, With<ViewpointRow>>,
    mut dropdowns: Query<&mut UiDropdown, With<ViewpointDropdown>>,
) {
    let target = if meta.observer {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut rows {
        if node.display != target {
            node.display = target;
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
///
/// The newspaper interstitial parks End Turn: ending a turn while the paper
/// is still open used to start a fresh resolution behind the overlay, and
/// the between-turns session that opened underneath the paper could never be
/// reached — the UI wedged. The paper must be dismissed first.
pub fn end_turn_button(
    mut activations: MessageReader<ButtonActivated>,
    buttons: Query<(), With<EndTurnButton>>,
    screen: Res<State<Screen>>,
    mut commands_out: MessageWriter<GameCommand>,
) {
    let parked = *screen.get() == Screen::News;
    for ButtonActivated(entity) in activations.read() {
        if buttons.contains(*entity) && !parked {
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
    mut focus: ResMut<InputFocus>,
    screen: Res<State<Screen>>,
    modal_stack: Res<ModalStack>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    // Esc exits the full-screen views (e.g. the newspaper's "Close (Esc)")
    // even while one of their filter inputs holds keyboard focus — releasing
    // that focus on the way out. Transport keeps the map live, so its Esc
    // belongs to the map's cancel cascade, not here.
    if keys.just_pressed(KeyCode::Escape) && modal_stack.is_empty() && screen.get().is_full_screen()
    {
        focus.clear();
        next_screen.set(Screen::Map);
        return;
    }
    // A focused text input otherwise owns the keyboard (typing in a filter
    // must not trigger screen navigation).
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
}

/// Toggle the burger menu; triggering any action in it closes it (display
/// toggles and the Debug disclosure keep it open so several can be
/// flipped in one visit).
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
            With<QuitToTitleBtn>,
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

/// Industry inner-tab behavior for the top bar: the active tab grows a
/// gold underline and shows its text label; inactive tabs are icon-only.
pub fn update_screen_tabs(
    screen: Res<State<Screen>>,
    mut buttons: Query<(&ScreenTabButton, &mut BorderColor)>,
    mut labels: Query<(&ScreenTabLabel, &mut Node)>,
) {
    if !screen.is_changed() {
        return;
    }
    for (tab, mut border) in &mut buttons {
        let color = if tab.0 == *screen.get() {
            theme::GOLD
        } else {
            Color::NONE
        };
        let target = BorderColor::all(color);
        if *border != target {
            *border = target;
        }
    }
    for (label, mut node) in &mut labels {
        let display = if label.0 == *screen.get() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
}

/// The End Turn button is live only when the player can actually resolve a
/// turn: the phase is idle (no resolution in flight — the busy overlay
/// communicates why otherwise) and the newspaper interstitial is not parked
/// on screen (dismiss it first, else the second resolution wedges the
/// between-turns session). One state-driven source of truth keeps the button
/// and the click handler in agreement.
pub fn sync_end_turn_enabled(
    phase: Res<State<crate::state::TurnPhase>>,
    screen: Res<State<Screen>>,
    mut commands: Commands,
    buttons: Query<Entity, With<EndTurnButton>>,
) {
    if !phase.is_changed() && !screen.is_changed() {
        return;
    }
    let enabled = *phase.get() == crate::state::TurnPhase::Idle && *screen.get() != Screen::News;
    for button in &buttons {
        widgets::set_enabled(&mut commands, button, enabled);
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

// ── Quit to Title ────────────────────────────────────────────────────────

/// "Quit to Title" opens a small confirm modal (abandoning an unsaved game
/// deserves one); confirming leaves `InGame` for the title screen.
pub fn handle_quit_to_title(
    mut activations: MessageReader<ButtonActivated>,
    quit_buttons: Query<(), With<QuitToTitleBtn>>,
    confirm_buttons: Query<&QuitConfirmBtn>,
    mut commands: Commands,
    theme: Res<Theme>,
    mut stack: ResMut<ModalStack>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    for ButtonActivated(entity) in activations.read() {
        if quit_buttons.contains(*entity) {
            open_quit_modal(&mut commands, &mut stack, &theme);
        } else if let Ok(button) = confirm_buttons.get(*entity) {
            commands.entity(button.modal).despawn();
            // Back to the map screen first so any full-screen overlay's
            // OnExit teardown runs, then out to the title splash.
            next_screen.set(Screen::Map);
            next_app_state.set(AppState::Intro);
        }
    }
}

fn open_quit_modal(commands: &mut Commands, stack: &mut ModalStack, theme: &Theme) {
    let handles = open_modal(
        commands,
        stack,
        theme,
        ModalProps {
            title: "Quit to Title".into(),
            width: Val::Px(380.0),
        },
    );
    let modal = handles.root;
    commands.entity(handles.content).with_children(|content| {
        content.spawn((
            Text::new("Return to the title screen? Unsaved progress is lost."),
            theme.font(12.5),
            TextColor(theme::TEXT),
        ));
        content
            .spawn((Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::FlexEnd,
                column_gap: Val::Px(10.0),
                ..default()
            },))
            .with_children(|row| {
                let quit = widgets::spawn_button(
                    row,
                    theme,
                    ButtonProps {
                        label: "Quit".into(),
                        width: Some(Val::Px(120.0)),
                        ..default()
                    },
                );
                row.commands().entity(quit).insert(QuitConfirmBtn { modal });
            });
    });
}

/// Despawn the in-game chrome (top bar, side panel, map tooltip) when the
/// player leaves the game for the title screen; entering `InGame` again
/// respawns it fresh.
pub fn cleanup_ingame_chrome(mut commands: Commands, roots: Query<Entity, With<InGameChrome>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Screen;
    use bevy::state::app::StatesPlugin;

    /// Minimal app exercising only `screen_hotkeys` against the `Screen`
    /// state, keyboard input, keyboard focus and the modal stack.
    fn hotkey_app() -> App {
        let mut app = App::new();
        app.add_plugins(StatesPlugin);
        app.init_state::<Screen>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<InputFocus>();
        app.init_resource::<ModalStack>();
        app.add_systems(Update, screen_hotkeys);
        app
    }

    fn goto(app: &mut App, screen: Screen) {
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(screen);
        app.update();
    }

    fn current(app: &App) -> Screen {
        *app.world().resource::<State<Screen>>().get()
    }

    #[test]
    fn esc_closes_newspaper_even_with_a_focused_filter() {
        let mut app = hotkey_app();
        goto(&mut app, Screen::News);
        assert_eq!(current(&app), Screen::News);

        // A filter input (e.g. the newspaper search box) holds focus.
        let focused = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<InputFocus>().0 = Some(focused);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        // One update runs `screen_hotkeys` (queues the transition + releases
        // focus); a second applies the queued `Screen` transition.
        app.update();
        app.update();

        assert_eq!(current(&app), Screen::Map, "Esc must dismiss the newspaper");
        assert!(
            app.world().resource::<InputFocus>().0.is_none(),
            "Esc must release the filter focus on the way out"
        );
    }

    #[test]
    fn esc_leaves_map_to_the_cancel_cascade() {
        let mut app = hotkey_app();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        // The map is not a full-screen view; screen_hotkeys must not act.
        assert_eq!(current(&app), Screen::Map);
    }
}
