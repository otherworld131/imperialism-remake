//! Setup-flow UI: the config panel, the preview chrome (header / terrain
//! sidebar / nation picker / capital panel / footer), and the interaction
//! systems wiring widgets and map picks into [`SetupConfig`] / [`SetupUi`].

use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;

use super::capital::{self, CapitalPreview};
use super::jobs::{self, ActiveSetupJob};
use super::{
    DIFFICULTIES, PreviewStage, SIZE_PRESETS, SetupAction, SetupActionBtn, SetupConfig, SetupRng,
    SetupStep, SetupUi, TerrainField, randomize_terrain,
};
use crate::game::resources::{DataVersion, SessionRes, TileIndex, ViewModels};
use crate::map::camera::GameCamera;
use crate::map::icons::IconAssets;
use crate::map::layers::MapMode;
use crate::map::picking::{HoverTarget, HoveredHex, MapClick, PickingBlocker, SelectedHex};
use crate::screens::common::spawn_icon;
use crate::screens::ledger::FlagCache;
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ButtonProps, CheckboxProps, CheckboxToggled, ModalStack, SliderCommitted, SliderProps,
    TextInputChanged, TextInputProps,
};

// ── Markers ─────────────────────────────────────────────────────────────

#[derive(Component)]
pub struct ConfigRoot;

#[derive(Component)]
pub struct PreviewChrome;

#[derive(Component)]
pub struct MapKeyInput;

#[derive(Component)]
pub struct WidthInput;

#[derive(Component)]
pub struct HeightInput;

#[derive(Component)]
pub struct GpSlider;

#[derive(Component)]
pub struct MinorSlider;

#[derive(Component)]
pub struct ObserverCheckbox;

#[derive(Component)]
pub struct OrganicCheckbox;

#[derive(Component)]
pub struct HideGridCheckbox;

/// Clickable nation row in the preview sidebar (GP index).
#[derive(Component)]
pub struct NationRow(pub usize);

/// Terrain/Political tab group in the preview header.
#[derive(Component)]
pub struct PreviewModeTabs;

/// Clickable suggested-placement row (index into `SetupUi::suggestions`).
#[derive(Component)]
pub struct SuggestionRow(pub usize);

/// The capital-yields panel body; rebuilt when the active preview changes.
#[derive(Component)]
pub struct YieldsPanel;

// ── Startup ─────────────────────────────────────────────────────────────

/// Queue the first config build. (The scenario picker was removed — only
/// the random map generator is playable today; scenario support returns
/// once real scenarios exist.)
pub fn init_setup(mut ui: ResMut<SetupUi>) {
    ui.config_dirty = true;
}

// ── Config step UI ──────────────────────────────────────────────────────

/// Rebuild the config panel whenever it is dirty (or tear it down once the
/// flow moved on to the preview step).
pub fn rebuild_config_ui(
    mut commands: Commands,
    theme: Res<Theme>,
    mut ui: ResMut<SetupUi>,
    config: Res<SetupConfig>,
    roots: Query<Entity, With<ConfigRoot>>,
) {
    if !ui.config_dirty {
        return;
    }
    ui.config_dirty = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    if ui.step != SetupStep::Config {
        return;
    }

    let error = ui.error.clone();
    let show_advanced = ui.show_advanced;
    let cfg = config.clone();

    commands
        .spawn((
            ConfigRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme::BG),
            GlobalZIndex(60),
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|page| {
            page.spawn((
                Node {
                    width: Val::Px(720.0),
                    max_height: Val::Percent(94.0),
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(theme::PANEL_BG_SOLID),
                BorderColor::all(theme::BORDER),
            ))
            .with_children(|panel| {
                // Header.
                panel
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(Val::Px(30.0), Val::Px(14.0)),
                            border: UiRect::bottom(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(theme::INSET_BG),
                        BorderColor::all(theme::BORDER),
                    ))
                    .with_children(|header| {
                        header.spawn((
                            Text::new("Imperialism"),
                            theme.font_bold(26.0),
                            TextColor(theme::GOLD),
                        ));
                        header.spawn((
                            Text::new(
                                "A game of diplomacy, trade, and conquest in the age of empire",
                            ),
                            theme.font(12.0),
                            TextColor(theme::TEXT_DIM),
                        ));
                    });

                // Body (scrolls when the window is short).
                let body = widgets::spawn_scroll_area(
                    panel,
                    &theme,
                    widgets::ScrollProps {
                        width: Val::Percent(100.0),
                        height: Val::Auto,
                        flex_grow: 1.0,
                    },
                );
                panel
                    .commands()
                    .entity(body.content)
                    .with_children(|content| {
                        spawn_config_body(content, &theme, &cfg, show_advanced, &error);
                    });

                // Footer.
                panel
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(10.0),
                            padding: UiRect::axes(Val::Px(30.0), Val::Px(12.0)),
                            border: UiRect::top(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(theme::INSET_BG),
                        BorderColor::all(theme::BORDER),
                    ))
                    .with_children(|footer| {
                        let load =
                            widgets::spawn_button(footer, &theme, ButtonProps::label("Load Save"));
                        footer
                            .commands()
                            .entity(load)
                            .insert(SetupActionBtn(SetupAction::OpenLoadModal));
                        footer.spawn((Node {
                            flex_grow: 1.0,
                            ..default()
                        },));
                        let preview = widgets::spawn_button(
                            footer,
                            &theme,
                            ButtonProps {
                                label: "Preview Map".into(),
                                width: Some(Val::Px(180.0)),
                                font_size: 15.0,
                                ..default()
                            },
                        );
                        footer
                            .commands()
                            .entity(preview)
                            .insert(SetupActionBtn(SetupAction::PreviewMap));
                    });
            });
        });
}

fn group_label(parent: &mut ChildSpawnerCommands, theme: &Theme, label: &str) {
    parent.spawn((
        Text::new(label.to_uppercase()),
        theme.font_bold(12.0),
        TextColor(theme::GOLD),
        Node {
            margin: UiRect::bottom(Val::Px(4.0)),
            ..default()
        },
    ));
}

fn spawn_config_body(
    content: &mut ChildSpawnerCommands,
    theme: &Theme,
    cfg: &SetupConfig,
    show_advanced: bool,
    error: &Option<String>,
) {
    content
        .spawn((Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(16.0),
            padding: UiRect::axes(Val::Px(30.0), Val::Px(18.0)),
            ..default()
        },))
        .with_children(|body| {
            // Difficulty.
            body.spawn((Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },))
                .with_children(|group| {
                    group_label(group, theme, "Difficulty");
                    group
                        .spawn((Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: Val::Px(8.0),
                            row_gap: Val::Px(6.0),
                            ..default()
                        },))
                        .with_children(|row| {
                            for (i, label) in DIFFICULTIES.iter().enumerate() {
                                let button = widgets::spawn_button(
                                    row,
                                    theme,
                                    ButtonProps {
                                        label: (*label).into(),
                                        width: Some(Val::Px(118.0)),
                                        font_size: 12.0,
                                        auto_label_tint: false,
                                        ..default()
                                    },
                                );
                                row.commands()
                                    .entity(button)
                                    .insert(SetupActionBtn(SetupAction::SetDifficulty(i as u8)));
                            }
                        });
                });

            // Two columns below the full-width scenario/difficulty rows so
            // the Nations sliders sit above the fold at 720p (wraps back
            // to one column on narrow windows).
            body.spawn((Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(40.0),
                row_gap: Val::Px(16.0),
                width: Val::Percent(100.0),
                align_items: AlignItems::FlexStart,
                ..default()
            },))
                .with_children(|columns| {
                    columns
                        .spawn((Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(16.0),
                            flex_grow: 1.0,
                            flex_basis: Val::Px(0.0),
                            min_width: Val::Px(370.0),
                            ..default()
                        },))
                        .with_children(|body| {
                            if cfg.scenario.is_none() {
                                // Map key.
                                body.spawn((Node {
                                    flex_direction: FlexDirection::Column,
                                    ..default()
                                },))
                                    .with_children(|group| {
                                        group_label(group, theme, "Map Key (optional)");
                                        let input = widgets::spawn_text_input(
                                            group,
                                            theme,
                                            TextInputProps {
                                                width: Val::Percent(100.0),
                                                max_len: 32,
                                                value: cfg.map_key.clone(),
                                                ..default()
                                            },
                                        );
                                        group.commands().entity(input).insert(MapKeyInput);
                                        group.spawn((
                                            Text::new(
                                                "Leave blank for the default \"imperialism\" seed.",
                                            ),
                                            theme.font(11.0),
                                            TextColor(theme::TEXT_DIM),
                                        ));
                                    });

                                // Map size.
                                body.spawn((Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px(6.0),
                                    ..default()
                                },))
                                    .with_children(|group| {
                                        group_label(group, theme, "Map Size");
                                        group
                                            .spawn((Node {
                                                flex_direction: FlexDirection::Row,
                                                flex_wrap: FlexWrap::Wrap,
                                                column_gap: Val::Px(8.0),
                                                row_gap: Val::Px(6.0),
                                                ..default()
                                            },))
                                            .with_children(|row| {
                                                for (label, w, h) in SIZE_PRESETS {
                                                    let button = widgets::spawn_button(
                                                        row,
                                                        theme,
                                                        ButtonProps {
                                                            label: label.into(),
                                                            width: Some(Val::Px(150.0)),
                                                            font_size: 12.0,
                                                            auto_label_tint: false,
                                                            ..default()
                                                        },
                                                    );
                                                    row.commands().entity(button).insert(
                                                        SetupActionBtn(SetupAction::SizePreset(
                                                            w, h,
                                                        )),
                                                    );
                                                }
                                            });
                                        let advanced = widgets::spawn_button(
                                            group,
                                            theme,
                                            ButtonProps {
                                                label: if show_advanced {
                                                    "Hide advanced".into()
                                                } else {
                                                    "Advanced size…".into()
                                                },
                                                font_size: 11.0,
                                                flat: true,
                                                ..default()
                                            },
                                        );
                                        group
                                            .commands()
                                            .entity(advanced)
                                            .insert(SetupActionBtn(SetupAction::ToggleAdvanced));
                                        if show_advanced {
                                            group
                                                .spawn((Node {
                                                    flex_direction: FlexDirection::Row,
                                                    align_items: AlignItems::Center,
                                                    column_gap: Val::Px(10.0),
                                                    ..default()
                                                },))
                                                .with_children(|row| {
                                                    row.spawn((
                                                        Text::new("Width (30–200):"),
                                                        theme.font(12.0),
                                                        TextColor(theme::TEXT_DIM),
                                                    ));
                                                    let width_input = widgets::spawn_text_input(
                                                        row,
                                                        theme,
                                                        TextInputProps {
                                                            width: Val::Px(70.0),
                                                            max_len: 3,
                                                            value: cfg.width.to_string(),
                                                            ..default()
                                                        },
                                                    );
                                                    row.commands()
                                                        .entity(width_input)
                                                        .insert(WidthInput);
                                                    row.spawn((
                                                        Text::new("Height (20–150):"),
                                                        theme.font(12.0),
                                                        TextColor(theme::TEXT_DIM),
                                                    ));
                                                    let height_input = widgets::spawn_text_input(
                                                        row,
                                                        theme,
                                                        TextInputProps {
                                                            width: Val::Px(70.0),
                                                            max_len: 3,
                                                            value: cfg.height.to_string(),
                                                            ..default()
                                                        },
                                                    );
                                                    row.commands()
                                                        .entity(height_input)
                                                        .insert(HeightInput);
                                                });
                                        }
                                    });
                            }
                        });
                    columns
                        .spawn((Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(16.0),
                            flex_grow: 1.0,
                            flex_basis: Val::Px(0.0),
                            min_width: Val::Px(370.0),
                            ..default()
                        },))
                        .with_children(|body| {
                            if cfg.scenario.is_none() {
                                // Nations sliders.
                                body.spawn((Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px(8.0),
                                    ..default()
                                },))
                                    .with_children(|group| {
                                        group_label(group, theme, "Nations");
                                        group
                                            .spawn((Node {
                                                flex_direction: FlexDirection::Row,
                                                align_items: AlignItems::Center,
                                                column_gap: Val::Px(10.0),
                                                ..default()
                                            },))
                                            .with_children(|row| {
                                                row.spawn((
                                                    Text::new("Great Powers"),
                                                    theme.font(12.0),
                                                    TextColor(theme::TEXT),
                                                    Node {
                                                        width: Val::Px(110.0),
                                                        ..default()
                                                    },
                                                ));
                                                let slider = widgets::spawn_slider(
                                                    row,
                                                    theme,
                                                    SliderProps {
                                                        min: 1.0,
                                                        max: 20.0,
                                                        step: 1.0,
                                                        value: cfg.num_great_powers as f32,
                                                        width: Val::Px(320.0),
                                                        ..default()
                                                    },
                                                );
                                                row.commands().entity(slider).insert(GpSlider);
                                            });
                                        group
                                            .spawn((Node {
                                                flex_direction: FlexDirection::Row,
                                                align_items: AlignItems::Center,
                                                column_gap: Val::Px(10.0),
                                                ..default()
                                            },))
                                            .with_children(|row| {
                                                row.spawn((
                                                    Text::new("Minor Nations"),
                                                    theme.font(12.0),
                                                    TextColor(theme::TEXT),
                                                    Node {
                                                        width: Val::Px(110.0),
                                                        ..default()
                                                    },
                                                ));
                                                let slider = widgets::spawn_slider(
                                                    row,
                                                    theme,
                                                    SliderProps {
                                                        min: 0.0,
                                                        max: 32.0,
                                                        step: 1.0,
                                                        value: cfg.num_minor_nations as f32,
                                                        width: Val::Px(320.0),
                                                        ..default()
                                                    },
                                                );
                                                row.commands().entity(slider).insert(MinorSlider);
                                            });
                                    });
                            }
                            // Toggles.
                            body.spawn((Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(6.0),
                                ..default()
                            },))
                                .with_children(|group| {
                                    let observer = widgets::spawn_checkbox(
                                        group,
                                        theme,
                                        CheckboxProps {
                                            label: format!(
                                                "Observer Mode — watch AI play all {} Great Powers",
                                                if cfg.scenario.is_some() {
                                                    7
                                                } else {
                                                    cfg.num_great_powers
                                                }
                                            ),
                                            checked: cfg.observer,
                                            ..default()
                                        },
                                    );
                                    group.commands().entity(observer).insert(ObserverCheckbox);
                                    let organic = widgets::spawn_checkbox(
                                        group,
                                        theme,
                                        CheckboxProps {
                                            label: "Organic Borders — smooth coasts & borders"
                                                .into(),
                                            checked: cfg.organic_borders,
                                            ..default()
                                        },
                                    );
                                    group.commands().entity(organic).insert(OrganicCheckbox);
                                    let hide_grid = widgets::spawn_checkbox(
                        group,
                        theme,
                        CheckboxProps {
                            label: "Hide Hex Grid — hide the faint interior hex outlines".into(),
                            checked: cfg.hide_hex_grid,
                            ..default()
                        },
                    );
                                    group.commands().entity(hide_grid).insert(HideGridCheckbox);
                                });
                        });
                });
            if let Some(error) = error {
                body.spawn((
                    Text::new(error.clone()),
                    theme.font(12.0),
                    TextColor(theme::ERROR),
                ));
            }
        });
}

/// Plain-`Interaction` clickables (cards, nation rows, suggestion rows)
/// fire their [`SetupAction`] on press.
pub fn handle_interaction_clicks(
    mut interactions: Query<
        (&Interaction, &SetupActionBtn),
        (Changed<Interaction>, Without<widgets::UiButton>),
    >,
    mut actions: MessageWriter<SetupAction>,
) {
    for (interaction, SetupActionBtn(action)) in &mut interactions {
        if *interaction == Interaction::Pressed {
            actions.write(action.clone());
        }
    }
}

/// Tint setup buttons / cards that represent the currently selected value.
/// Kit buttons (which restyle their own borders) get a gold *label*; plain
/// `Interaction` nodes (cards, nation rows, suggestion rows) get a gold
/// *border*.
pub fn tint_selected_buttons(
    config: Res<SetupConfig>,
    ui: Res<SetupUi>,
    mode: Res<MapMode>,
    buttons: Query<(Entity, &SetupActionBtn, Option<&Children>)>,
    kit: Query<(), With<widgets::UiButton>>,
    mut borders: Query<&mut BorderColor>,
    mut labels: Query<&mut TextColor>,
) {
    for (entity, SetupActionBtn(action), children) in &buttons {
        let selected = match action {
            SetupAction::SelectScenario(s) => *s == config.scenario,
            SetupAction::SetDifficulty(d) => *d == config.difficulty,
            SetupAction::SizePreset(w, h) => *w == config.width && *h == config.height,
            SetupAction::PickNation(idx) => Some(*idx) == config.picked_nation,
            SetupAction::SetMapMode(political) => {
                (*mode == MapMode::Political) == *political
                    && matches!(*mode, MapMode::Political | MapMode::Terrain)
            }
            SetupAction::PickSuggestion(i) => ui
                .suggestions
                .get(*i)
                .zip(ui.picked_capital.as_ref())
                .is_some_and(|(s, picked)| s.preview.q == picked.q && s.preview.r == picked.r),
            _ => continue,
        };
        if kit.contains(entity) {
            let label_color = if selected {
                theme::GOLD
            } else {
                theme::TEXT_DIM
            };
            if let Some(children) = children {
                for child in children {
                    if let Ok(mut text) = labels.get_mut(*child)
                        && text.0 != label_color
                    {
                        text.0 = label_color;
                    }
                }
            }
        } else if let Ok(mut border) = borders.get_mut(entity) {
            let color = if selected { theme::GOLD } else { theme::BORDER };
            let target = BorderColor::all(color);
            if *border != target {
                *border = target;
            }
        }
    }
}

// ── Preview chrome ──────────────────────────────────────────────────────

/// Rebuild the preview header / sidebar / footer whenever dirty (and tear
/// them down outside the preview step).
#[allow(clippy::too_many_arguments)]
pub fn rebuild_preview_ui(
    mut commands: Commands,
    theme: Res<Theme>,
    mut ui: ResMut<SetupUi>,
    config: Res<SetupConfig>,
    flags: Res<FlagCache>,
    icons: Option<Res<IconAssets>>,
    roots: Query<Entity, With<PreviewChrome>>,
) {
    if !ui.preview_dirty {
        return;
    }
    ui.preview_dirty = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    if ui.step != SetupStep::Preview {
        return;
    }

    let stage = ui.stage;
    let observer = config.observer;
    let icons = icons.as_deref();

    // Header.
    let sub = format!(
        "{} · Names: {} · {}{}{}",
        match &config.scenario {
            Some(id) => format!("Scenario: {id}"),
            None => format!("Seed: {}", config.effective_map_key()),
        },
        if config.flavor_key.is_empty() {
            config.effective_map_key()
        } else {
            &config.flavor_key
        },
        config.difficulty_label(),
        if observer { " · Observer Mode" } else { "" },
        if !observer && stage == PreviewStage::Capital {
            " · Place Capital"
        } else {
            ""
        },
    );
    commands
        .spawn((
            PreviewChrome,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Px(44.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(14.0),
                padding: UiRect::horizontal(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_SOLID),
            GlobalZIndex(55),
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|bar| {
            bar.spawn((
                Text::new("Preview"),
                theme.font_bold(16.0),
                TextColor(theme::GOLD),
            ));
            bar.spawn((Text::new(sub), theme.font(11.5), TextColor(theme::TEXT_DIM)));
            bar.spawn((Node {
                flex_grow: 1.0,
                ..default()
            },));
            // Terrain/Political as the shared tab widget (CC-5).
            let tabs = widgets::spawn_tabs(bar, &theme, &["Terrain", "Political"], 1);
            let mut bar_commands = bar.commands();
            bar_commands.entity(tabs.root).insert((
                PreviewModeTabs,
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
            ));
            for panel in &tabs.panels {
                bar_commands.entity(*panel).insert(Node {
                    display: Display::None,
                    ..default()
                });
            }
            for (label, tooltip, action) in [
                ("−", "Zoom out", SetupAction::ZoomOut),
                ("+", "Zoom in", SetupAction::ZoomIn),
            ] {
                let b = widgets::spawn_button(
                    bar,
                    &theme,
                    ButtonProps {
                        label: label.into(),
                        width: Some(Val::Px(32.0)),
                        ..default()
                    },
                );
                bar.commands()
                    .entity(b)
                    .insert((SetupActionBtn(action), widgets::TooltipText(tooltip.into())));
            }
        });

    // Sidebar.
    let sidebar = commands
        .spawn((
            PreviewChrome,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(44.0),
                right: Val::Px(0.0),
                bottom: Val::Px(52.0),
                width: Val::Px(320.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_SOLID),
            GlobalZIndex(55),
            Interaction::default(),
            PickingBlocker,
        ))
        .id();
    commands.entity(sidebar).with_children(|side| {
        let scroll = widgets::spawn_scroll_area(
            side,
            &theme,
            widgets::ScrollProps {
                width: Val::Percent(100.0),
                height: Val::Auto,
                flex_grow: 1.0,
            },
        );
        side.commands()
            .entity(scroll.content)
            .with_children(|content| {
                content
                    .spawn((Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(14.0),
                        padding: UiRect::all(Val::Px(12.0)),
                        ..default()
                    },))
                    .with_children(|col| {
                        let show_terrain =
                            stage == PreviewStage::Nation && config.scenario.is_none();
                        if show_terrain {
                            spawn_terrain_section(col, &theme, &config);
                        }
                        if observer || stage == PreviewStage::Nation {
                            spawn_nation_picker(col, &theme, &ui, &config, &flags, observer);
                        } else {
                            spawn_capital_section(col, &theme, &ui, &config, icons);
                        }
                    });
            });
    });

    // Footer.
    let can_place = config.picked_nation.is_some();
    let can_begin = observer || ui.picked_capital.is_some();
    commands
        .spawn((
            PreviewChrome,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Px(52.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                padding: UiRect::horizontal(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_SOLID),
            GlobalZIndex(55),
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|footer| {
            let back = widgets::spawn_button(footer, &theme, ButtonProps::label("Back"));
            footer.commands().entity(back).insert(SetupActionBtn(
                if !observer && stage == PreviewStage::Capital {
                    SetupAction::LeaveCapitalStage
                } else {
                    SetupAction::BackToConfig
                },
            ));
            if stage == PreviewStage::Nation {
                let reroll = widgets::spawn_button(footer, &theme, ButtonProps::label("Re-roll"));
                footer
                    .commands()
                    .entity(reroll)
                    .insert(SetupActionBtn(SetupAction::Reroll));
                let names =
                    widgets::spawn_button(footer, &theme, ButtonProps::label("Re-roll Names"));
                footer.commands().entity(names).insert((
                    SetupActionBtn(SetupAction::RerollNames),
                    widgets::TooltipText(
                        "Re-roll only the country names and flags. Map layout stays the same."
                            .into(),
                    ),
                ));
            }
            footer.spawn((Node {
                flex_grow: 1.0,
                ..default()
            },));
            if observer {
                let begin = widgets::spawn_button(
                    footer,
                    &theme,
                    ButtonProps {
                        label: "Begin Campaign".into(),
                        width: Some(Val::Px(190.0)),
                        font_size: 14.0,
                        ..default()
                    },
                );
                footer
                    .commands()
                    .entity(begin)
                    .insert(SetupActionBtn(SetupAction::BeginCampaign));
            } else if stage == PreviewStage::Nation {
                let place = widgets::spawn_button(
                    footer,
                    &theme,
                    ButtonProps {
                        label: "Place Capital".into(),
                        width: Some(Val::Px(190.0)),
                        font_size: 14.0,
                        enabled: can_place,
                        ..default()
                    },
                );
                footer
                    .commands()
                    .entity(place)
                    .insert(SetupActionBtn(SetupAction::EnterCapitalStage));
            } else {
                let begin = widgets::spawn_button(
                    footer,
                    &theme,
                    ButtonProps {
                        label: "Begin Campaign".into(),
                        width: Some(Val::Px(190.0)),
                        font_size: 14.0,
                        enabled: can_begin,
                        ..default()
                    },
                );
                footer
                    .commands()
                    .entity(begin)
                    .insert(SetupActionBtn(SetupAction::BeginCampaign));
            }
        });
}

fn section_title(parent: &mut ChildSpawnerCommands, theme: &Theme, label: &str) {
    parent.spawn((
        Text::new(label.to_uppercase()),
        theme.font_bold(12.5),
        TextColor(theme::GOLD),
    ));
}

fn hint_text(parent: &mut ChildSpawnerCommands, theme: &Theme, text: &str) {
    parent.spawn((
        Text::new(text),
        theme.font(11.0),
        TextColor(theme::TEXT_DIM),
    ));
}

fn spawn_terrain_section(col: &mut ChildSpawnerCommands, theme: &Theme, config: &SetupConfig) {
    col.spawn((Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(6.0),
        ..default()
    },))
        .with_children(|section| {
            section
                .spawn((Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    ..default()
                },))
                .with_children(|row| {
                    section_title(row, theme, "Terrain");
                    for (label, action) in [
                        ("Randomize", SetupAction::RandomizeTerrain),
                        ("Reset", SetupAction::ResetTerrain),
                    ] {
                        let b = widgets::spawn_button(
                            row,
                            theme,
                            ButtonProps {
                                label: label.into(),
                                font_size: 11.0,
                                flat: true,
                                ..default()
                            },
                        );
                        row.commands().entity(b).insert(SetupActionBtn(action));
                    }
                });
            hint_text(
                section,
                theme,
                "Same seed — only the world regenerates as you adjust.",
            );
            // Grouped sliders: world shape, the normalized terrain mix
            // (with its live sum so "relative weights" is visible), and
            // clustering knobs.
            const WORLD_SHAPE: [TerrainField; 4] = [
                TerrainField::LandAmount,
                TerrainField::SeaRing,
                TerrainField::Falloff,
                TerrainField::RiverSources,
            ];
            const TERRAIN_MIX: [TerrainField; 7] = [
                TerrainField::Grassland,
                TerrainField::Forest,
                TerrainField::Hills,
                TerrainField::Mountain,
                TerrainField::Desert,
                TerrainField::Swamp,
                TerrainField::Tundra,
            ];
            const CLUSTERING: [TerrainField; 6] = [
                TerrainField::ForestCluster,
                TerrainField::HillsCluster,
                TerrainField::MountainCluster,
                TerrainField::DesertCluster,
                TerrainField::SwampCluster,
                TerrainField::PoleTundra,
            ];
            let mix_sum: f32 = TERRAIN_MIX
                .iter()
                .map(|field| field.get(&config.terrain))
                .sum();
            let groups: [(&str, Option<String>, &[TerrainField]); 3] = [
                ("World shape", None, &WORLD_SHAPE),
                (
                    "Terrain mix",
                    Some(format!(
                        "Relative weights, normalized — currently sum {mix_sum:.2}"
                    )),
                    &TERRAIN_MIX,
                ),
                ("Clustering", None, &CLUSTERING),
            ];
            for (group_title, note, fields) in groups {
                section.spawn((
                    Text::new(group_title.to_string()),
                    theme.font_bold(12.0),
                    TextColor(theme::GOLD),
                    Node {
                        margin: UiRect::top(Val::Px(6.0)),
                        ..default()
                    },
                ));
                if let Some(note) = note {
                    section.spawn((
                        Text::new(note),
                        theme.font_italic(10.5),
                        TextColor(theme::TEXT_DIM),
                    ));
                }
                for &field in fields {
                    let (min, max, step) = field.range(&config.terrain);
                    let value = field.get(&config.terrain);
                    section
                        .spawn((Node {
                            flex_direction: FlexDirection::Column,
                            ..default()
                        },))
                        .with_children(|block| {
                            block.spawn((
                                Text::new(field.label()),
                                theme.font(11.0),
                                TextColor(theme::TEXT),
                            ));
                            let format: Option<widgets::slider::SliderFormatFn> =
                                if field == TerrainField::LandAmount {
                                    Some(std::sync::Arc::new(|v: f32| format!("{v:.2}×")))
                                } else {
                                    None
                                };
                            let slider = widgets::spawn_slider(
                                block,
                                theme,
                                SliderProps {
                                    min,
                                    max,
                                    step,
                                    value,
                                    width: Val::Px(220.0),
                                    format,
                                    ..default()
                                },
                            );
                            block.commands().entity(slider).insert(field);
                        });
                }
            }
        });
}

fn spawn_nation_picker(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    ui: &SetupUi,
    config: &SetupConfig,
    flags: &FlagCache,
    observer: bool,
) {
    col.spawn((Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(6.0),
        ..default()
    },))
        .with_children(|section| {
            section_title(
                section,
                theme,
                if observer {
                    "Viewpoint Nation"
                } else {
                    "Choose Your Empire"
                },
            );
            hint_text(
                section,
                theme,
                if observer {
                    "Pick a nation whose ledger and diplomacy screens to view. You can switch in-game."
                } else {
                    "Choose your nation first. Then place the capital on a hex inside your country."
                },
            );
            for gp in &ui.gps {
                let picked = config.picked_nation == Some(gp.idx);
                section
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(8.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::all(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(theme::INSET_BG),
                        BorderColor::all(if picked { theme::GOLD } else { theme::BORDER }),
                        Interaction::default(),
                        NationRow(gp.idx),
                        SetupActionBtn(SetupAction::PickNation(gp.idx)),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Node {
                                width: Val::Px(12.0),
                                height: Val::Px(12.0),
                                border_radius: BorderRadius::all(Val::Px(6.0)),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            BackgroundColor(theme::nation_color(&gp.color)),
                            Pickable::IGNORE,
                        ));
                        if let Some(flag) = flags.get(gp.id as u32) {
                            row.spawn((
                                Node {
                                    width: Val::Px(36.0),
                                    height: Val::Px(24.0),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                ImageNode::new(flag),
                                Pickable::IGNORE,
                            ));
                        }
                        row.spawn((Node {
                            flex_direction: FlexDirection::Column,
                            ..default()
                        },))
                            .with_children(|name_block| {
                                name_block.spawn((
                                    Text::new(gp.name.clone()),
                                    theme.font_bold(12.5),
                                    TextColor(theme::TEXT),
                                    Pickable::IGNORE,
                                ));
                                if !gp.government_title.is_empty()
                                    && gp.government_title != gp.name
                                {
                                    name_block.spawn((
                                        Text::new(gp.government_title.clone()),
                                        theme.font(10.5),
                                        TextColor(theme::TEXT_DIM),
                                        Pickable::IGNORE,
                                    ));
                                }
                            });
                    });
            }
        });
}

fn spawn_capital_section(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    ui: &SetupUi,
    config: &SetupConfig,
    icons: Option<&IconAssets>,
) {
    let nation_name = config
        .picked_nation
        .and_then(|idx| ui.gps.iter().find(|gp| gp.idx == idx))
        .map(|gp| gp.name.clone())
        .unwrap_or_else(|| "your empire".to_string());

    col.spawn((Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(6.0),
        ..default()
    },))
        .with_children(|section| {
            section_title(section, theme, "Place Capital");
            hint_text(
                section,
                theme,
                &format!(
                    "Hover a hex inside {nation_name} to preview its opening capital yield. \
                     Click a valid hex to place the capital, then begin the campaign."
                ),
            );
        });

    if !ui.suggestions.is_empty() {
        col.spawn((Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        },))
            .with_children(|section| {
                section_title(section, theme, "Suggested Placements");
                for (i, suggestion) in ui.suggestions.iter().enumerate() {
                    // Two lines so same-named provinces stay distinguishable:
                    // where the site sits + what it yields.
                    let place = format!(
                        "{} · {}",
                        suggestion.direction,
                        if suggestion.coastal {
                            "coastal"
                        } else {
                            "inland"
                        }
                    );
                    let yields = suggestion
                        .preview
                        .resources
                        .iter()
                        .take(4)
                        .map(|(name, amount)| format!("{name} {amount}"))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    section
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(2.0),
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(3.0)),
                                ..default()
                            },
                            BackgroundColor(theme::INSET_BG),
                            BorderColor::all(theme::BORDER),
                            Interaction::default(),
                            Hovered::default(),
                            SuggestionRow(i),
                            SetupActionBtn(SetupAction::PickSuggestion(i)),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                Node {
                                    flex_direction: FlexDirection::Row,
                                    justify_content: JustifyContent::SpaceBetween,
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(8.0),
                                    ..default()
                                },
                                Pickable::IGNORE,
                            ))
                            .with_children(|line| {
                                line.spawn((
                                    Text::new(format!("{} ({place})", suggestion.province_name)),
                                    theme.font(11.5),
                                    TextColor(theme::TEXT),
                                    Pickable::IGNORE,
                                ));
                                line.spawn((
                                    Text::new(format!("Workers {}", suggestion.preview.support)),
                                    theme.font_bold(11.5),
                                    TextColor(theme::GOLD),
                                    Pickable::IGNORE,
                                ));
                            });
                            if !yields.is_empty() {
                                row.spawn((
                                    Text::new(yields),
                                    theme.font(10.5),
                                    TextColor(theme::TEXT_DIM),
                                    Pickable::IGNORE,
                                ));
                            }
                        });
                }
            });
    }

    col.spawn((Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(6.0),
        ..default()
    },))
        .with_children(|section| {
            section_title(section, theme, "Capital Yields");
            section.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(5.0),
                    ..default()
                },
                YieldsPanel,
            ));
        });
    let _ = icons;
}

/// Rebuild the yields-panel body when the displayed preview changes
/// (sidebar hover → map hover → picked, web `activeCapitalPreview`).
pub fn update_yields_panel(
    mut commands: Commands,
    theme: Res<Theme>,
    ui: Res<SetupUi>,
    icons: Option<Res<IconAssets>>,
    panels: Query<Entity, With<YieldsPanel>>,
    mut last: Local<Option<Option<CapitalPreview>>>,
) {
    let active: Option<CapitalPreview> = ui
        .sidebar_hovered
        .and_then(|i| ui.suggestions.get(i).map(|s| s.preview.clone()))
        .or_else(|| ui.hovered_capital.clone())
        .or_else(|| ui.picked_capital.clone());
    if last.as_ref() == Some(&active) {
        return;
    }
    *last = Some(active.clone());

    let Ok(panel) = panels.single() else {
        return;
    };
    commands.entity(panel).despawn_children();
    commands.entity(panel).with_children(|body| {
        spawn_yields_body(body, &theme, active.as_ref(), icons.as_deref());
    });
}

fn spawn_yields_body(
    body: &mut ChildSpawnerCommands,
    theme: &Theme,
    active: Option<&CapitalPreview>,
    icons: Option<&IconAssets>,
) {
    body.spawn((Node {
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    },))
        .with_children(|row| {
            row.spawn((
                Text::new("Supported workers"),
                theme.font(11.5),
                TextColor(theme::TEXT),
            ));
            row.spawn((
                Text::new(
                    active
                        .map(|p| p.support.to_string())
                        .unwrap_or_else(|| "—".to_string()),
                ),
                theme.font_bold(11.5),
                TextColor(theme::GOLD),
            ));
        });
    match active {
        Some(preview) if !preview.resources.is_empty() => {
            for (resource, amount) in &preview.resources {
                body.spawn((Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    ..default()
                },))
                    .with_children(|row| {
                        row.spawn((Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            ..default()
                        },))
                            .with_children(|label| {
                                spawn_icon(label, icons, "commodities", resource, 14.0);
                                label.spawn((
                                    Text::new(resource.clone()),
                                    theme.font(11.5),
                                    TextColor(theme::TEXT),
                                ));
                            });
                        row.spawn((
                            Text::new(amount.to_string()),
                            theme.font_bold(11.5),
                            TextColor(theme::TEXT),
                        ));
                    });
            }
        }
        _ => {
            hint_text(
                body,
                theme,
                "Hover a valid hex to preview its opening capital yields.",
            );
        }
    }
}

// ── Widget → config plumbing ────────────────────────────────────────────

/// Text inputs (map key, advanced width/height) → config. No rebuild: the
/// inputs own their own visuals.
pub fn handle_config_inputs(
    mut changes: MessageReader<TextInputChanged>,
    map_key: Query<(), With<MapKeyInput>>,
    width: Query<(), With<WidthInput>>,
    height: Query<(), With<HeightInput>>,
    mut config: ResMut<SetupConfig>,
) {
    for change in changes.read() {
        if map_key.contains(change.entity) {
            config.map_key = change.value.clone();
        } else if width.contains(change.entity) {
            if let Ok(v) = change.value.trim().parse::<i32>() {
                config.width = v.clamp(30, 200);
            }
        } else if height.contains(change.entity)
            && let Ok(v) = change.value.trim().parse::<i32>()
        {
            config.height = v.clamp(20, 150);
        }
    }
}

pub fn handle_config_sliders(
    mut commits: MessageReader<SliderCommitted>,
    gps: Query<(), With<GpSlider>>,
    minors: Query<(), With<MinorSlider>>,
    mut config: ResMut<SetupConfig>,
) {
    for commit in commits.read() {
        if gps.contains(commit.entity) {
            config.num_great_powers = (commit.value as u32).clamp(1, 20);
        } else if minors.contains(commit.entity) {
            config.num_minor_nations = (commit.value as u32).min(32);
        }
    }
}

pub fn handle_config_checkboxes(
    mut toggles: MessageReader<CheckboxToggled>,
    observer: Query<(), With<ObserverCheckbox>>,
    organic: Query<(), With<OrganicCheckbox>>,
    hide_grid: Query<(), With<HideGridCheckbox>>,
    mut config: ResMut<SetupConfig>,
) {
    for toggle in toggles.read() {
        if observer.contains(toggle.entity) {
            config.observer = toggle.checked;
        } else if organic.contains(toggle.entity) {
            config.organic_borders = toggle.checked;
        } else if hide_grid.contains(toggle.entity) {
            config.hide_hex_grid = toggle.checked;
        }
    }
}

/// Terrain-slider commits regenerate the preview world with the same seed.
pub fn handle_terrain_sliders(
    mut commits: MessageReader<SliderCommitted>,
    fields: Query<&TerrainField>,
    mut config: ResMut<SetupConfig>,
    mut ui: ResMut<SetupUi>,
    mut active: ResMut<ActiveSetupJob>,
    mut next_phase: ResMut<NextState<crate::state::TurnPhase>>,
) {
    let mut changed = false;
    for commit in commits.read() {
        if let Ok(field) = fields.get(commit.entity) {
            field.set(&mut config.terrain, commit.value);
            changed = true;
        }
    }
    if changed {
        // Ranges of the sea sliders are interdependent; rebuild them.
        ui.preview_dirty = true;
        jobs::start_preview(&mut active, &mut next_phase, &config);
    }
}

// ── The action dispatcher ───────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn handle_setup_actions(
    mut actions: MessageReader<SetupAction>,
    mut commands: Commands,
    theme: Res<Theme>,
    mut ui: ResMut<SetupUi>,
    mut config: ResMut<SetupConfig>,
    mut active: ResMut<ActiveSetupJob>,
    mut next_phase: ResMut<NextState<crate::state::TurnPhase>>,
    mut session_res: ResMut<SessionRes>,
    mut data_version: ResMut<DataVersion>,
    mut mode: ResMut<MapMode>,
    mut selected_hex: ResMut<SelectedHex>,
    mut modal_stack: ResMut<ModalStack>,
    vms: Res<ViewModels>,
    windows: Query<&Window>,
    mut camera: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    for action in actions.read() {
        match action {
            SetupAction::SelectScenario(s) => {
                config.scenario = s.clone();
                ui.config_dirty = true;
            }
            SetupAction::SetDifficulty(d) => {
                config.difficulty = *d;
            }
            SetupAction::SizePreset(w, h) => {
                config.width = *w;
                config.height = *h;
                ui.config_dirty = true;
            }
            SetupAction::ToggleAdvanced => {
                ui.show_advanced = !ui.show_advanced;
                ui.config_dirty = true;
            }
            SetupAction::OpenLoadModal => {
                crate::screens::saveload::open_load_modal(&mut commands, &mut modal_stack, &theme);
            }
            SetupAction::PreviewMap => {
                jobs::start_preview(&mut active, &mut next_phase, &config);
            }
            SetupAction::BackToConfig => {
                ui.step = SetupStep::Config;
                ui.stage = PreviewStage::Nation;
                ui.config_dirty = true;
                ui.preview_dirty = true;
            }
            SetupAction::Reroll => {
                let mut rng = SetupRng::from_time();
                let fresh = rng.seed_string();
                config.flavor_key = fresh.clone();
                if config.scenario.is_none() {
                    config.map_key = fresh;
                }
                config.picked_nation = None;
                config.capital = None;
                jobs::start_preview(&mut active, &mut next_phase, &config);
            }
            SetupAction::RerollNames => {
                let mut rng = SetupRng::from_time();
                let fresh = rng.seed_string();
                config.flavor_key = fresh.clone();
                if let Some(session) = session_res.0.as_mut() {
                    frontend_api::flavor::reroll_flavor(session.game_mut(), &fresh);
                    data_version.0 += 1;
                    ui.gps.clear(); // refilled from the fresh view models
                    ui.preview_dirty = true;
                }
            }
            SetupAction::RandomizeTerrain => {
                randomize_terrain(&mut config.terrain);
                ui.preview_dirty = true;
                jobs::start_preview(&mut active, &mut next_phase, &config);
            }
            SetupAction::ResetTerrain => {
                config.terrain = default();
                ui.preview_dirty = true;
                jobs::start_preview(&mut active, &mut next_phase, &config);
            }
            SetupAction::PickNation(idx) => {
                config.picked_nation = Some(*idx);
                config.capital = None;
                ui.stage = PreviewStage::Nation;
                ui.hovered_capital = None;
                ui.picked_capital = None;
                ui.sidebar_hovered = None;
                ui.suggestions.clear();
                ui.suggestions_version = 0;
                selected_hex.0 = None;
                ui.preview_dirty = true;
            }
            SetupAction::EnterCapitalStage => {
                if config.observer || config.picked_nation.is_none() {
                    continue;
                }
                ui.stage = PreviewStage::Capital;
                *mode = MapMode::Terrain;
                ui.hovered_capital = None;
                ui.sidebar_hovered = None;
                ui.suggestions_version = 0;
                ui.preview_dirty = true;
                // Auto-zoom to the picked nation's territory.
                let gp_id = config
                    .picked_nation
                    .and_then(|idx| ui.gps.iter().find(|gp| gp.idx == idx))
                    .map(|gp| gp.id);
                if let Some(gp_id) = gp_id
                    && let Some(tiles) = vms.map.as_ref()
                    && let Ok(window) = windows.single()
                    && let Ok((mut transform, mut projection)) = camera.single_mut()
                {
                    let usable = Vec2::new(
                        (window.width() - 320.0 - 96.0).max(120.0),
                        (window.height() - 44.0 - 52.0 - 96.0).max(120.0),
                    );
                    if let Some((center, scale)) = capital::nation_view_fit(tiles, gp_id, usable) {
                        transform.translation.x = center.x;
                        transform.translation.y = center.y;
                        if let Projection::Orthographic(ref mut ortho) = *projection {
                            ortho.scale = scale;
                        }
                    }
                }
            }
            SetupAction::LeaveCapitalStage => {
                ui.stage = PreviewStage::Nation;
                *mode = MapMode::Political;
                ui.hovered_capital = None;
                ui.sidebar_hovered = None;
                ui.preview_dirty = true;
            }
            SetupAction::PickSuggestion(i) => {
                if let Some(suggestion) = ui.suggestions.get(*i).cloned() {
                    selected_hex.0 = Some((suggestion.preview.q, suggestion.preview.r));
                    config.capital = Some((suggestion.preview.q, suggestion.preview.r));
                    // Pan the camera onto the picked hex so the selection
                    // is visible even when it was off-screen.
                    if let Ok((mut transform, _)) = camera.single_mut() {
                        let world = crate::map::geometry::hex_to_world(
                            suggestion.preview.q,
                            suggestion.preview.r,
                        );
                        transform.translation.x = world.x;
                        transform.translation.y = world.y;
                    }
                    ui.picked_capital = Some(suggestion.preview);
                    ui.preview_dirty = true;
                }
            }
            SetupAction::BeginCampaign => {
                if !config.observer && ui.picked_capital.is_none() {
                    continue;
                }
                config.capital = ui.picked_capital.as_ref().map(|p| (p.q, p.r));
                if config.picked_nation.is_none() {
                    config.picked_nation = Some(0);
                }
                jobs::start_begin(&mut active, &mut next_phase, &config);
            }
            SetupAction::ZoomIn | SetupAction::ZoomOut => {
                if let Ok((_, mut projection)) = camera.single_mut()
                    && let Projection::Orthographic(ref mut ortho) = *projection
                {
                    let factor = if *action == SetupAction::ZoomIn {
                        0.8
                    } else {
                        1.25
                    };
                    ortho.scale = (ortho.scale * factor).clamp(0.25, 2.8);
                }
            }
            SetupAction::SetMapMode(political) => {
                *mode = if *political {
                    MapMode::Political
                } else {
                    MapMode::Terrain
                };
            }
        }
    }
}

// ── Preview sync / picking ──────────────────────────────────────────────

/// Refill the Great-Power list from the view models after a (re)generation
/// or a name re-roll.
pub fn sync_preview_gps(
    vms: Res<ViewModels>,
    mut ui: ResMut<SetupUi>,
    mut config: ResMut<SetupConfig>,
) {
    if ui.step != SetupStep::Preview || vms.nations.is_empty() {
        return;
    }
    let gps: Vec<super::GpInfo> = vms
        .nations
        .iter()
        .filter(|n| n.nation_type == "GreatPower")
        .enumerate()
        .map(|(idx, n)| super::GpInfo {
            idx,
            id: i64::from(n.nation_id),
            name: n.name.clone(),
            color: n.color.clone(),
            government_title: n.government_title.clone(),
        })
        .collect();
    if ui.gps != gps {
        if let Some(picked) = config.picked_nation
            && picked >= gps.len()
        {
            config.picked_nation = None;
        }
        ui.gps = gps;
        ui.preview_dirty = true;
    }
}

/// Map clicks during the preview: nation pick (observer / nation stage) or
/// capital placement (capital stage).
pub fn handle_preview_map_clicks(
    mut clicks: MessageReader<MapClick>,
    mut actions: MessageWriter<SetupAction>,
    mut ui: ResMut<SetupUi>,
    mut config: ResMut<SetupConfig>,
    mut selected_hex: ResMut<SelectedHex>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
) {
    for MapClick(target) in clicks.read() {
        let HoverTarget::Hex(q, r) = target else {
            continue;
        };
        if ui.step != SetupStep::Preview {
            continue;
        }
        let Some(tiles) = vms.map.as_ref() else {
            continue;
        };
        let Some(&tile_index) = index.by_coord.get(&(*q, *r)) else {
            continue;
        };
        let tile = &tiles[tile_index];
        if config.observer || ui.stage == PreviewStage::Nation {
            if let Some(gp) = ui.gps.iter().find(|gp| gp.id == tile.nation_id) {
                actions.write(SetupAction::PickNation(gp.idx));
            }
            continue;
        }
        // Capital stage: place on a valid hex.
        let gp_id = config
            .picked_nation
            .and_then(|idx| ui.gps.iter().find(|gp| gp.idx == idx))
            .map(|gp| gp.id);
        let Some(gp_id) = gp_id else {
            continue;
        };
        if let Some(preview) = capital::evaluate_capital_site(tile, &index.by_coord, tiles, gp_id) {
            selected_hex.0 = Some((preview.q, preview.r));
            config.capital = Some((preview.q, preview.r));
            ui.picked_capital = Some(preview);
            ui.preview_dirty = true;
        }
    }
}

/// Hovering the map in the capital stage previews the yield of the hovered
/// hex.
pub fn capital_hover_preview(
    hovered: Res<HoveredHex>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    config: Res<SetupConfig>,
    mut ui: ResMut<SetupUi>,
) {
    if ui.step != SetupStep::Preview || ui.stage != PreviewStage::Capital || config.observer {
        return;
    }
    if !hovered.is_changed() {
        return;
    }
    let gp_id = config
        .picked_nation
        .and_then(|idx| ui.gps.iter().find(|gp| gp.idx == idx))
        .map(|gp| gp.id);
    let preview = (|| {
        let (q, r) = hovered.0?;
        let tiles = vms.map.as_ref()?;
        let &tile_index = index.by_coord.get(&(q, r))?;
        capital::evaluate_capital_site(&tiles[tile_index], &index.by_coord, tiles, gp_id?)
    })();
    if ui.hovered_capital != preview {
        ui.hovered_capital = preview;
    }
}

/// Compute the top-5 suggested placements when the capital stage opens (and
/// after every regeneration).
pub fn compute_suggestions(
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    config: Res<SetupConfig>,
    mut ui: ResMut<SetupUi>,
) {
    if ui.step != SetupStep::Preview
        || ui.stage != PreviewStage::Capital
        || config.observer
        || ui.suggestions_version == vms.version
    {
        return;
    }
    let Some(tiles) = vms.map.as_ref() else {
        return;
    };
    let gp_id = config
        .picked_nation
        .and_then(|idx| ui.gps.iter().find(|gp| gp.idx == idx))
        .map(|gp| gp.id);
    let Some(gp_id) = gp_id else {
        return;
    };
    ui.suggestions = capital::suggest_capitals(tiles, &index.by_coord, gp_id);
    ui.suggestions_version = vms.version;
    ui.preview_dirty = true;
}

/// Preview header tabs → map mode, and back (mode can change elsewhere).
pub fn handle_preview_mode_tabs(
    mut changes: MessageReader<widgets::TabChanged>,
    tabs: Query<(), With<PreviewModeTabs>>,
    mut actions: MessageWriter<SetupAction>,
) {
    for change in changes.read() {
        if tabs.contains(change.group) {
            actions.write(SetupAction::SetMapMode(change.index == 1));
        }
    }
}

pub fn sync_preview_mode_tabs(
    mode: Res<MapMode>,
    mut groups: Query<&mut widgets::TabGroup, With<PreviewModeTabs>>,
) {
    if !mode.is_changed() {
        return;
    }
    let index = usize::from(*mode == MapMode::Political);
    for mut group in &mut groups {
        if group.active != index && matches!(*mode, MapMode::Political | MapMode::Terrain) {
            group.active = index;
        }
    }
}

/// Hovering a suggestion row previews it in the yields panel (priority over
/// the map hover, web parity).
pub fn suggestion_row_hover(
    rows: Query<(&SuggestionRow, &Hovered)>,
    mut ui: ResMut<SetupUi>,
    mut hovered_hex: ResMut<crate::map::picking::HoveredHex>,
    mut override_coord: Local<Option<(i32, i32)>>,
) {
    let hovered = rows
        .iter()
        .find(|(_, hovered)| hovered.get())
        .map(|(row, _)| row.0);
    if ui.sidebar_hovered != hovered {
        ui.sidebar_hovered = hovered;
    }
    // Highlight the suggested hex on the map (the hover ring follows
    // `HoveredHex`; the cursor is over the sidebar so map picking is idle).
    // On leaving the list, clear the override only while `HoveredHex`
    // still holds OUR coordinate — a real map hover written earlier this
    // frame (this system runs after `pick_hover`) must survive.
    match hovered.and_then(|i| ui.suggestions.get(i)) {
        Some(preview) => {
            let coord = Some((preview.preview.q, preview.preview.r));
            if hovered_hex.0 != coord {
                hovered_hex.0 = coord;
            }
            *override_coord = coord;
        }
        None => {
            if let Some(owned) = override_coord.take()
                && hovered_hex.0 == Some(owned)
            {
                hovered_hex.0 = None;
            }
        }
    }
}

/// Tear down all setup chrome when the game starts.
pub fn cleanup_setup_ui(
    mut commands: Commands,
    config_roots: Query<Entity, With<ConfigRoot>>,
    chrome: Query<Entity, With<PreviewChrome>>,
    mut ui: ResMut<SetupUi>,
) {
    for entity in config_roots.iter().chain(chrome.iter()) {
        commands.entity(entity).despawn();
    }
    ui.hovered_capital = None;
    ui.sidebar_hovered = None;
}
