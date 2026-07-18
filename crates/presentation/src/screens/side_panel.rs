//! Map side panel: selected-tile / selected-navy info, per-mode legends,
//! UI & debug toggles, and the Great Power list — mirroring the web
//! frontend's right-hand panel. Also owns the bottom-right map-mode dropup.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::game::resources::{
    NewsDebugSettings, RenderSettings, SelectedNavy, TileIndex, ViewModels,
};
use crate::map::icons::IconAssets;
use crate::map::layers::MapMode;
use crate::map::navy;
use crate::map::picking::{PickingBlocker, SelectedHex};
use crate::screens::panels;
use crate::theme::{self, Theme};
use crate::widgets::{
    self, CheckboxProps, CheckboxToggled, DropdownChanged, DropdownOpenUp, DropdownProps,
    ScrollProps, SliderCommitted, UiDropdown,
};

pub const PANEL_WIDTH: f32 = 280.0;

#[derive(Component)]
pub struct SelectedInfoSection;

#[derive(Component)]
pub struct LegendSection;

#[derive(Component)]
pub struct NationsSection;

#[derive(Component)]
pub struct MapModeDropdown;

/// The "UI size" interface-scale slider in the UI section.
#[derive(Component)]
pub struct UiScaleSlider;

/// "Debug ▸ / ▾" disclosure row; developer toggles stay collapsed for
/// casual players. State persists to `settings.json`.
#[derive(Component)]
pub struct DebugDisclosureButton;

/// Label inside the disclosure button (arrow flips with state).
#[derive(Component)]
pub struct DebugDisclosureLabel;

/// Container holding the debug toggles; `Display` follows the state.
#[derive(Component)]
pub struct DebugSectionBody;

/// Generic side-panel sections hidden while the Diplomacy screen is open
/// (it shows only the legend + relation details).
#[derive(Component)]
pub struct GenericPanelSection;

#[derive(Resource)]
pub struct DebugPanelExpanded(pub bool);

impl Default for DebugPanelExpanded {
    fn default() -> Self {
        Self(crate::ui_scale::load_debug_expanded())
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub enum ToggleKind {
    OrganicBorders,
    HideHexGrid,
    ShowResources,
    ShowTransport,
    ShowArmies,
    ShowHiddenResources,
    ShowAiCivilians,
    DisableFog,
    ShowAiReasoning,
    ShowAiNonActions,
    ShowRetreatDebug,
    ShowBattleFirepower,
}

pub fn setup_side_panel(
    mut commands: Commands,
    theme: Res<Theme>,
    settings: Res<RenderSettings>,
    news_debug: Res<NewsDebugSettings>,
    ui_scale: Res<bevy::ui::UiScale>,
    debug_expanded: Res<DebugPanelExpanded>,
) {
    // ── Right-hand panel ─────────────────────────────────────────────────
    commands
        .spawn((
            Node {
                width: Val::Px(PANEL_WIDTH),
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                top: Val::Px(44.0),
                bottom: Val::Px(0.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_SOLID),
            Interaction::default(),
            PickingBlocker,
            crate::screens::session::SessionHiddenChrome,
        ))
        .with_children(|panel| {
            let scroll = widgets::spawn_scroll_area(
                panel,
                &theme,
                ScrollProps {
                    flex_grow: 1.0,
                    ..default()
                },
            );
            let mut commands = panel.commands();
            commands.entity(scroll.content).with_children(|content| {
                content.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        min_height: Val::Px(60.0),
                        ..default()
                    },
                    SelectedInfoSection,
                ));
                content.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(3.0),
                        margin: UiRect::top(Val::Px(8.0)),
                        ..default()
                    },
                    LegendSection,
                ));

                // M6 player-flow sections (banners, units, civilians, navy).
                content.spawn((panel_section(), panels::BannerSection));
                content.spawn((panel_section(), panels::UnitPanelSection));
                content.spawn((panel_section(), panels::NavalPanelSection));

                content
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            ..default()
                        },
                        GenericPanelSection,
                    ))
                    .with_children(|content| {
                section_title(content, &theme, "UI");
                let ui_toggles = [
                    (
                        ToggleKind::OrganicBorders,
                        "Organic borders",
                        settings.organic_borders,
                    ),
                    (
                        ToggleKind::HideHexGrid,
                        "Hide hex grid",
                        settings.hide_hex_grid,
                    ),
                    (
                        ToggleKind::ShowResources,
                        "Show resources",
                        settings.show_resources,
                    ),
                    (
                        ToggleKind::ShowTransport,
                        "Show transport network",
                        settings.show_transport_network,
                    ),
                    (ToggleKind::ShowArmies, "Show armies", settings.show_armies),
                ];
                spawn_toggles(content, &theme, &ui_toggles);

                // Interface scale (web parity: the side-panel font slider).
                content
                    .spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(8.0),
                        margin: UiRect::vertical(Val::Px(4.0)),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Text::new("UI size"),
                            theme.font(12.0),
                            TextColor(theme::TEXT),
                        ));
                        let slider = widgets::spawn_slider(
                            row,
                            &theme,
                            widgets::SliderProps {
                                min: crate::ui_scale::MIN_SCALE,
                                max: crate::ui_scale::MAX_SCALE,
                                step: 0.05,
                                value: ui_scale.0,
                                width: Val::Px(130.0),
                                format: Some(std::sync::Arc::new(|v: f32| {
                                    format!("{:.0}%", v * 100.0)
                                })),
                                ..default()
                            },
                        );
                        row.commands().entity(slider).insert((
                            UiScaleSlider,
                            widgets::TooltipText(
                                "Interface text & icon scale (also Ctrl + / Ctrl - / Ctrl 0 on any screen)".into(),
                            ),
                        ));
                    });

                // Nations (gameplay info) sits above Debug (developer UI).
                section_title(content, &theme, "Nations");
                content.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        ..default()
                    },
                    NationsSection,
                ));

                // Debug: collapsed disclosure row by default.
                let expanded = debug_expanded.0;
                let disclosure = widgets::spawn_button(
                    content,
                    &theme,
                    widgets::ButtonProps {
                        label: debug_disclosure_label(expanded),
                        font_size: 13.0,
                        flat: true,
                        auto_label_tint: false,
                        ..default()
                    },
                );
                {
                    let mut commands = content.commands();
                    let mut entity = commands.entity(disclosure);
                    entity.insert((
                        DebugDisclosureButton,
                        widgets::TooltipText("Developer toggles (fog, AI internals)".into()),
                        Node {
                            margin: UiRect::top(Val::Px(10.0)),
                            padding: UiRect::horizontal(Val::Px(0.0)),
                            ..default()
                        },
                    ));
                }
                content
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            display: if expanded {
                                Display::Flex
                            } else {
                                Display::None
                            },
                            ..default()
                        },
                        DebugSectionBody,
                    ))
                    .with_children(|body| {
                        let debug_toggles = [
                            (
                                ToggleKind::ShowHiddenResources,
                                "Show hidden resources",
                                settings.show_hidden_resources,
                            ),
                            (
                                ToggleKind::ShowAiCivilians,
                                "Show AI civilians",
                                settings.show_ai_civilians,
                            ),
                            (
                                ToggleKind::DisableFog,
                                "Disable fog of war",
                                settings.disable_fog,
                            ),
                            (
                                ToggleKind::ShowAiReasoning,
                                "Show AI reasoning",
                                news_debug.show_ai_reasoning,
                            ),
                            (
                                ToggleKind::ShowAiNonActions,
                                "Show AI non-actions",
                                news_debug.show_ai_non_actions,
                            ),
                            (
                                ToggleKind::ShowRetreatDebug,
                                "Battle retreat math",
                                news_debug.show_retreat_debug,
                            ),
                            (
                                ToggleKind::ShowBattleFirepower,
                                "Battle firepower detail",
                                news_debug.show_battle_firepower,
                            ),
                        ];
                        spawn_toggles(body, &theme, &debug_toggles);
                    });
                    });
            });
        });

    // ── Bottom-right map-mode dropup ─────────────────────────────────────
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(PANEL_WIDTH + 12.0),
                bottom: Val::Px(12.0),
                ..default()
            },
            Interaction::default(),
            PickingBlocker,
            crate::screens::session::SessionHiddenChrome,
        ))
        .with_children(|anchor| {
            let dropdown = widgets::spawn_dropdown(
                anchor,
                &theme,
                DropdownProps {
                    options: MapMode::ALL.iter().map(|m| m.label().to_string()).collect(),
                    selected: MapMode::default().index(),
                    width: Val::Px(150.0),
                },
            );
            anchor
                .commands()
                .entity(dropdown)
                .insert((MapModeDropdown, DropdownOpenUp));
        });
}

/// Layout shared by the M6 panel-section anchors.
fn panel_section() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(2.0),
        margin: UiRect::top(Val::Px(8.0)),
        ..default()
    }
}

fn section_title(parent: &mut ChildSpawnerCommands, theme: &Theme, title: &str) {
    parent.spawn((
        Text::new(title.to_string()),
        theme.font_bold(14.0),
        TextColor(theme::GOLD),
        Node {
            margin: UiRect::top(Val::Px(10.0)),
            ..default()
        },
    ));
}

fn spawn_toggles(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    toggles: &[(ToggleKind, &str, bool)],
) {
    for (kind, label, checked) in toggles {
        let checkbox = widgets::spawn_checkbox(
            parent,
            theme,
            CheckboxProps {
                label: (*label).to_string(),
                checked: *checked,
                enabled: true,
            },
        );
        parent.commands().entity(checkbox).insert(*kind);
    }
}

/// Hide the generic UI/Nations/Debug sections while Diplomacy is open.
pub fn sync_side_panel_for_diplomacy(
    screen: Res<State<crate::state::Screen>>,
    mut sections: Query<&mut Node, With<GenericPanelSection>>,
) {
    if !screen.is_changed() {
        return;
    }
    let hide = *screen.get() == crate::state::Screen::Diplomacy;
    for mut node in &mut sections {
        node.display = if hide { Display::None } else { Display::Flex };
    }
}

fn debug_disclosure_label(expanded: bool) -> String {
    // Only ▲/▼ exist in the patched pixel font (▸/▾ would render as tofu);
    // ▼ = closed matches the dropdown headers.
    if expanded {
        "Debug ▲".to_string()
    } else {
        "Debug ▼".to_string()
    }
}

/// Toggle the Debug disclosure: flip the body's `Display`, relabel the
/// arrow, and persist the state to `settings.json`.
pub fn handle_debug_disclosure(
    mut activations: MessageReader<widgets::ButtonActivated>,
    buttons: Query<&Children, With<DebugDisclosureButton>>,
    mut state: ResMut<DebugPanelExpanded>,
    mut bodies: Query<&mut Node, With<DebugSectionBody>>,
    mut labels: Query<&mut Text>,
) {
    for widgets::ButtonActivated(entity) in activations.read() {
        let Ok(children) = buttons.get(*entity) else {
            continue;
        };
        state.0 = !state.0;
        crate::ui_scale::save_debug_expanded(state.0);
        for mut node in &mut bodies {
            node.display = if state.0 {
                Display::Flex
            } else {
                Display::None
            };
        }
        for child in children {
            if let Ok(mut text) = labels.get_mut(*child) {
                **text = debug_disclosure_label(state.0);
            }
        }
    }
}

/// Apply commits from the "UI size" slider to the global interface scale.
pub fn handle_ui_scale_slider(
    mut commits: MessageReader<SliderCommitted>,
    sliders: Query<(), With<UiScaleSlider>>,
    mut ui_scale: ResMut<bevy::ui::UiScale>,
) {
    for commit in commits.read() {
        if sliders.get(commit.entity).is_ok() {
            crate::ui_scale::apply_scale(&mut ui_scale, commit.value);
        }
    }
}

/// Reverse sync: when the scale changes elsewhere (Ctrl+/-/0 hotkeys), push
/// the new value into the slider so its thumb and label stay truthful.
pub fn sync_ui_scale_slider(
    ui_scale: Res<bevy::ui::UiScale>,
    mut commands: Commands,
    mut sliders: Query<
        (
            Entity,
            &bevy::ui_widgets::SliderValue,
            &mut widgets::UiSliderDrag,
        ),
        With<UiScaleSlider>,
    >,
) {
    if !ui_scale.is_changed() {
        return;
    }
    for (entity, value, mut drag) in &mut sliders {
        if !drag.dragging && (value.0 - ui_scale.0).abs() > 0.001 {
            commands
                .entity(entity)
                .insert(bevy::ui_widgets::SliderValue(ui_scale.0));
            drag.value = ui_scale.0;
        }
    }
}

pub fn handle_toggles(
    mut toggles: MessageReader<CheckboxToggled>,
    kinds: Query<&ToggleKind>,
    mut settings: ResMut<RenderSettings>,
    mut news_debug: ResMut<NewsDebugSettings>,
) {
    for toggle in toggles.read() {
        let Ok(kind) = kinds.get(toggle.entity) else {
            continue;
        };
        match kind {
            ToggleKind::OrganicBorders => settings.organic_borders = toggle.checked,
            ToggleKind::HideHexGrid => settings.hide_hex_grid = toggle.checked,
            ToggleKind::ShowResources => settings.show_resources = toggle.checked,
            ToggleKind::ShowTransport => settings.show_transport_network = toggle.checked,
            ToggleKind::ShowArmies => settings.show_armies = toggle.checked,
            ToggleKind::ShowHiddenResources => settings.show_hidden_resources = toggle.checked,
            ToggleKind::ShowAiCivilians => settings.show_ai_civilians = toggle.checked,
            ToggleKind::DisableFog => settings.disable_fog = toggle.checked,
            ToggleKind::ShowAiReasoning => news_debug.show_ai_reasoning = toggle.checked,
            ToggleKind::ShowAiNonActions => news_debug.show_ai_non_actions = toggle.checked,
            ToggleKind::ShowRetreatDebug => news_debug.show_retreat_debug = toggle.checked,
            ToggleKind::ShowBattleFirepower => news_debug.show_battle_firepower = toggle.checked,
        }
    }
}

pub fn handle_mode_dropdown(
    mut changes: MessageReader<DropdownChanged>,
    dropdowns: Query<(), With<MapModeDropdown>>,
    mut mode: ResMut<MapMode>,
) {
    for change in changes.read() {
        if dropdowns.contains(change.entity)
            && let Some(next) = MapMode::ALL.get(change.index)
            && *mode != *next
        {
            *mode = *next;
        }
    }
}

/// Keep the dropup header in sync when the mode changes via keyboard.
pub fn sync_mode_dropdown(
    mode: Res<MapMode>,
    mut dropdowns: Query<&mut UiDropdown, With<MapModeDropdown>>,
) {
    if !mode.is_changed() {
        return;
    }
    for mut dropdown in &mut dropdowns {
        if dropdown.selected != mode.index() {
            dropdown.selected = mode.index();
        }
    }
}

// ── Selected info ────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn update_selected_info(
    selected: Res<SelectedHex>,
    selected_navy: Res<SelectedNavy>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    mode: Res<MapMode>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    settings: Res<RenderSettings>,
    mut commands: Commands,
    sections: Query<Entity, With<SelectedInfoSection>>,
) {
    if !selected.is_changed()
        && !selected_navy.is_changed()
        && !vms.is_changed()
        && !mode.is_changed()
        && !settings.is_changed()
    {
        return;
    }
    let Ok(section) = sections.single() else {
        return;
    };
    commands.entity(section).despawn_children();

    let tile = selected.0.and_then(|coord| {
        let tiles = vms.map.as_ref()?;
        tiles.get(*index.by_coord.get(&coord)?)
    });
    let marker = selected_navy
        .0
        .as_deref()
        .and_then(|key| vms.navy_markers.iter().find(|m| navy::marker_key(m) == key));

    if tile.is_none() && marker.is_none() {
        commands.spawn((
            Text::new("Select a hex for details — hover for a quick tooltip"),
            theme.font_italic(12.0),
            TextColor(theme::TEXT_DIM),
            ChildOf(section),
        ));
        return;
    }

    if let Some(tile) = tile {
        let owner_title = if tile.owner.is_empty() {
            "Unowned".to_string()
        } else if tile.is_minor {
            format!("{} (minor)", tile.owner)
        } else {
            tile.owner.clone()
        };
        commands.spawn((
            Text::new(owner_title),
            theme.font_bold(14.0),
            TextColor(theme::GOLD),
            ChildOf(section),
        ));
        let show_resource = tile
            .resource
            .as_deref()
            .filter(|_| !tile.resource_hidden || settings.show_hidden_resources);
        let row = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                ChildOf(section),
            ))
            .id();
        if let (Some(resource), Some(icons)) = (show_resource, icons.as_deref())
            && let Some(image) = icons.get("commodities", resource)
        {
            commands.spawn((
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    ..default()
                },
                ImageNode::new(image),
                ChildOf(row),
            ));
        }
        let title = match show_resource {
            Some(resource) => format!("{} — {resource}", tile.terrain),
            None => tile.terrain.clone(),
        };
        commands.spawn((
            Text::new(title),
            theme.font_bold(13.0),
            TextColor(theme::TEXT),
            ChildOf(row),
        ));
        if !tile.province.is_empty() {
            commands.spawn((
                Text::new(format!("Province: {}", tile.province)),
                theme.font(12.0),
                TextColor(theme::TEXT),
                ChildOf(section),
            ));
        }
        // Mode-specific strips for the selected tile.
        match *mode {
            MapMode::Diplomatic | MapMode::Relationship => {
                if let Some(overlay) = vms.diplomacy.as_ref() {
                    if tile.owner == overlay.selected_nation {
                        commands.spawn((
                            Text::new("Your nation"),
                            theme.font(12.0),
                            TextColor(theme::OVERLAY_SELF),
                            ChildOf(section),
                        ));
                    } else if let Some(rel) = overlay
                        .relations
                        .iter()
                        .find(|rel| rel.nation_name == tile.owner)
                    {
                        let sign = if rel.score >= 0 { "+" } else { "" };
                        commands.spawn((
                            Text::new(format!(
                                "{}: {} (score: {sign}{})",
                                rel.nation_name, rel.status, rel.score
                            )),
                            theme.font(12.0),
                            TextColor(theme::TEXT),
                            ChildOf(section),
                        ));
                        if !rel.treaties.is_empty() {
                            commands.spawn((
                                Text::new(format!("Treaties: {}", rel.treaties.join(", "))),
                                theme.font(11.0),
                                TextColor(theme::TEXT_DIM),
                                ChildOf(section),
                            ));
                        }
                        if rel.has_embassy {
                            commands.spawn((
                                Text::new("Embassy established"),
                                theme.font(11.0),
                                TextColor(theme::TEXT_DIM),
                                ChildOf(section),
                            ));
                        } else if rel.has_consulate {
                            commands.spawn((
                                Text::new("Consulate established"),
                                theme.font(11.0),
                                TextColor(theme::TEXT_DIM),
                                ChildOf(section),
                            ));
                        }
                    }
                }
            }
            MapMode::Military | MapMode::Naval => {
                if tile.is_capital && tile.army_unit_count > 0 {
                    commands.spawn((
                        Text::new(format!(
                            "Army: {} units, {:.1} FP",
                            tile.army_unit_count, tile.army_firepower
                        )),
                        theme.font(12.0),
                        TextColor(theme::TEXT),
                        ChildOf(section),
                    ));
                }
                if tile.is_country_capital && tile.naval_ship_count > 0 {
                    commands.spawn((
                        Text::new(format!(
                            "Navy: {} warships, {} FP",
                            tile.naval_ship_count, tile.naval_firepower
                        )),
                        theme.font(12.0),
                        TextColor(theme::TEXT),
                        ChildOf(section),
                    ));
                }
                if let Some(info) = vms
                    .military
                    .iter()
                    .find(|entry| entry.nation_name == tile.owner)
                {
                    let text = if *mode == MapMode::Military {
                        format!(
                            "{}: {} total units, {:.1} total FP",
                            info.nation_name, info.army_unit_count, info.total_army_fp
                        )
                    } else {
                        format!(
                            "{}: {} warships, {} total FP",
                            info.nation_name, info.warship_count, info.total_naval_fp as i64
                        )
                    };
                    commands.spawn((
                        Text::new(text),
                        theme.font(11.0),
                        TextColor(theme::TEXT_DIM),
                        ChildOf(section),
                    ));
                }
            }
            _ => {}
        }
    }

    if let Some(marker) = marker {
        let title = if marker.kind == "beachhead" {
            format!(
                "Beachhead → {}",
                marker.target_province.as_deref().unwrap_or("?")
            )
        } else {
            format!("Fleet — {}", marker.owner_name)
        };
        commands.spawn((
            Text::new("Selected navy"),
            theme.font(11.0),
            TextColor(theme::TEXT_DIM),
            Node {
                margin: UiRect::top(Val::Px(6.0)),
                ..default()
            },
            ChildOf(section),
        ));
        commands.spawn((
            Text::new(title),
            theme.font_bold(13.0),
            TextColor(if marker.kind == "beachhead" {
                Color::srgb_u8(0xff, 0x80, 0x59)
            } else {
                theme::TEXT
            }),
            ChildOf(section),
        ));
        commands.spawn((
            Text::new(format!(
                "{} ships · {} FP · {} hull",
                marker.ship_count, marker.total_fp, marker.total_hull
            )),
            theme.font(12.0),
            TextColor(theme::TEXT),
            ChildOf(section),
        ));
        if !marker.by_type.is_empty() {
            let text = marker
                .by_type
                .iter()
                .map(|(t, n)| format!("{n} {t}"))
                .collect::<Vec<_>>()
                .join(", ");
            commands.spawn((
                Text::new(text),
                theme.font(11.0),
                TextColor(theme::TEXT_DIM),
                ChildOf(section),
            ));
        }
        if !marker.by_operation.is_empty() {
            let text = marker
                .by_operation
                .iter()
                .map(|(op, n)| format!("{n} {op}"))
                .collect::<Vec<_>>()
                .join(" · ");
            commands.spawn((
                Text::new(text),
                theme.font(11.0),
                TextColor(theme::TEXT_DIM),
                ChildOf(section),
            ));
        }
    }
}

// ── Legend ───────────────────────────────────────────────────────────────

pub fn update_legend(
    mode: Res<MapMode>,
    theme: Res<Theme>,
    mut commands: Commands,
    sections: Query<Entity, With<LegendSection>>,
) {
    if !mode.is_changed() {
        return;
    }
    let Ok(section) = sections.single() else {
        return;
    };
    commands.entity(section).despawn_children();
    if !mode.is_overlay() {
        return;
    }

    let title = match *mode {
        MapMode::Diplomatic => "Legend",
        MapMode::Relationship => "Relationship Score",
        MapMode::Military => "Army Strength (vs average)",
        MapMode::Naval => "Naval Strength (vs average)",
        _ => unreachable!(),
    };
    commands.spawn((
        Text::new(title),
        theme.font(11.0),
        TextColor(theme::TEXT_DIM),
        ChildOf(section),
    ));

    match *mode {
        MapMode::Diplomatic => {
            let entries = [
                (theme::OVERLAY_SELF, "Self"),
                (theme::diplo_status_color("Alliance"), "Alliance"),
                (theme::diplo_status_color("NAP"), "NAP"),
                (theme::diplo_status_color("At War"), "At War"),
                (theme::diplo_status_color("Neutral"), "Neutral"),
            ];
            for (color, label) in entries {
                let row = commands
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            ..default()
                        },
                        ChildOf(section),
                    ))
                    .id();
                commands.spawn((
                    Node {
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(color),
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
                    ChildOf(row),
                ));
                commands.spawn((
                    Text::new(label.to_string()),
                    theme.font(12.0),
                    TextColor(theme::TEXT),
                    ChildOf(row),
                ));
            }
        }
        _ => {
            let (lo, hi) = if *mode == MapMode::Relationship {
                ("-100", "+100")
            } else {
                ("Weak", "Strong")
            };
            let row = commands
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
                    ChildOf(section),
                ))
                .id();
            commands.spawn((
                Text::new(lo.to_string()),
                theme.font(11.0),
                TextColor(theme::TEXT),
                ChildOf(row),
            ));
            let bar = commands
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        flex_grow: 1.0,
                        height: Val::Px(12.0),
                        ..default()
                    },
                    ChildOf(row),
                ))
                .id();
            const STEPS: usize = 20;
            for i in 0..STEPS {
                let score = -100.0 + 200.0 * i as f32 / (STEPS - 1) as f32;
                let color = if *mode == MapMode::Relationship {
                    theme::score_color(score)
                } else {
                    theme::strength_color(score)
                };
                commands.spawn((
                    Node {
                        flex_grow: 1.0,
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(color),
                    ChildOf(bar),
                ));
            }
            commands.spawn((
                Text::new(hi.to_string()),
                theme.font(11.0),
                TextColor(theme::TEXT),
                ChildOf(row),
            ));
        }
    }
}

// ── Nations list ─────────────────────────────────────────────────────────

pub fn update_nations(
    vms: Res<ViewModels>,
    theme: Res<Theme>,
    mut commands: Commands,
    sections: Query<Entity, With<NationsSection>>,
    mut built_version: Local<u64>,
) {
    if *built_version == vms.version {
        return;
    }
    let Some(tiles) = vms.map.as_ref() else {
        return;
    };
    let Ok(section) = sections.single() else {
        return;
    };
    *built_version = vms.version;
    commands.entity(section).despawn_children();

    // Great Powers = non-minor owners; province count = distinct ids.
    let mut provinces: HashMap<&str, HashSet<u64>> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for tile in tiles.iter() {
        if tile.is_minor || tile.owner.is_empty() || tile.is_sea() {
            continue;
        }
        let entry = provinces.entry(tile.owner.as_str()).or_insert_with(|| {
            order.push(tile.owner.as_str());
            HashSet::new()
        });
        if let Some(pid) = tile.province_id {
            entry.insert(pid);
        }
    }
    order.sort_unstable();
    for name in order {
        let count = provinces[name].len();
        let row = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                ChildOf(section),
            ))
            .id();
        commands.spawn((
            Text::new(name.to_string()),
            theme.font(12.0),
            TextColor(theme::TEXT),
            ChildOf(row),
        ));
        commands.spawn((
            Text::new(format!("{count} prov")),
            theme.font(12.0),
            TextColor(theme::TEXT_DIM),
            ChildOf(row),
        ));
    }
}
