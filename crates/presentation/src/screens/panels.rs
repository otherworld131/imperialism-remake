//! Side-panel sections for the M6 player flows: mode banners, the unit
//! panel (selection / move / upgrade / dismiss), the civilian workforce
//! panel (deploy / recall / progress), and the naval panel (fleet scoping,
//! ship selection, queued fleet moves).
//!
//! Every section is an anchor node inside the side panel's scroll content;
//! update systems rebuild a section's children when its inputs change.
//! All affordances emit [`GameCommand`]s — nothing mutates the game here.

use bevy::prelude::*;

use crate::game::commands::GameCommand;
use crate::game::resources::TileIndex;
use crate::game::resources::{
    DeployMode, GameMeta, PendingMoveList, ProvinceUnits, SelectedCivilian, SelectedNavy,
    SelectedShips, SelectedUnits, ViewModels,
};
use crate::game::selection;
use crate::game::vm::{ArmyUnitVm, CivilianEntry, MapTile, NavyMarker, ShipVm};
use crate::map::camera::GameCamera;
use crate::map::geometry;
use crate::map::icons::IconAssets;
use crate::map::navy;
use crate::map::picking::SelectedHex;
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ButtonActivated, ButtonProps, CheckboxProps, CheckboxToggled, ModalProps, ModalStack,
};

// ── Section anchors (spawned by the side panel) ──────────────────────────

#[derive(Component)]
pub struct BannerSection;

#[derive(Component)]
pub struct UnitPanelSection;

#[derive(Component)]
pub struct CivilianPanelSection;

#[derive(Component)]
pub struct NavalPanelSection;

// ── Button / row markers ─────────────────────────────────────────────────

#[derive(Component)]
pub struct UnitCheckbox(pub u32);

#[derive(Component)]
pub struct SelectAllUnitsButton;

#[derive(Component)]
pub struct CancelSelectedMovesButton;

#[derive(Component)]
pub struct DismissSelectedButton;

#[derive(Component)]
pub struct UpgradeSelectedButton;

#[derive(Component)]
pub struct CancelMoveButton(pub u32);

#[derive(Component)]
pub struct UpgradeUnitButton(pub u32);

/// Confirm button inside the dismiss-confirmation modal (snapshot of ids).
#[derive(Component)]
pub struct ConfirmDismissButton(pub Vec<u32>);

#[derive(Component)]
pub struct CancelModalButton;

#[derive(Component, Clone)]
pub struct CivilianRowButton {
    pub id: i64,
    pub civ_type: String,
    pub working: bool,
    pub position: Option<(i32, i32)>,
}

#[derive(Component)]
pub struct RecallCivilianButton(pub i64);

#[derive(Component)]
pub struct ShipCheckbox(pub u32);

#[derive(Component)]
pub struct SelectAllShipsButton;

#[derive(Component)]
pub struct CancelFleetMoveButton(pub u32);

// ── Shared helpers ───────────────────────────────────────────────────────

fn split_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

fn unit_icon_name(category: &str) -> &'static str {
    match category {
        "Infantry" => "Infantry",
        "Cavalry" => "Cavalry",
        "Artillery" => "Artillery",
        "Special" => "Special",
        _ => "Army",
    }
}

fn health_color(pct: f32) -> Color {
    if pct > 60.0 {
        Color::srgb_u8(0x2a, 0xa2, 0x2a)
    } else if pct > 30.0 {
        Color::srgb_u8(0xca, 0xa2, 0x20)
    } else {
        Color::srgb_u8(0xa2, 0x22, 0x22)
    }
}

fn hull_color(pct: f32) -> Color {
    if pct > 66.0 {
        Color::srgb_u8(0x33, 0xaa, 0x77)
    } else if pct > 33.0 {
        theme::GOLD
    } else {
        Color::srgb_u8(0xaa, 0x33, 0x33)
    }
}

/// Thin colored bar (health / hull / progress) on a recessed track.
fn spawn_bar(parent: &mut ChildSpawnerCommands, width: f32, fraction: f32, color: Color) {
    parent
        .spawn((
            Node {
                width: Val::Px(width),
                height: Val::Px(5.0),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
        ))
        .with_children(|track| {
            track.spawn((
                Node {
                    width: Val::Percent(fraction.clamp(0.0, 1.0) * 100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(color),
            ));
        });
}

fn spawn_icon(
    parent: &mut ChildSpawnerCommands,
    icons: Option<&IconAssets>,
    group: &str,
    name: &str,
    size: f32,
) {
    if let Some(image) = icons.and_then(|i| i.get(group, name)) {
        parent.spawn((
            Node {
                width: Val::Px(size),
                height: Val::Px(size),
                flex_shrink: 0.0,
                ..default()
            },
            ImageNode::new(image),
        ));
    }
}

fn small_button(parent: &mut ChildSpawnerCommands, theme: &Theme, label: &str) -> Entity {
    widgets::spawn_button(
        parent,
        theme,
        ButtonProps {
            label: label.to_string(),
            font_size: 11.0,
            ..default()
        },
    )
}

fn selected_tile<'a>(
    selected_hex: &SelectedHex,
    vms: &'a ViewModels,
    index: &TileIndex,
) -> Option<&'a MapTile> {
    let coord = selected_hex.0?;
    let tiles = vms.map.as_ref()?;
    tiles.get(*index.by_coord.get(&coord)?)
}

// ── Banners ──────────────────────────────────────────────────────────────

pub fn update_banners(
    meta: Res<GameMeta>,
    selected_units: Res<SelectedUnits>,
    deploy: Res<DeployMode>,
    selected_civilian: Res<SelectedCivilian>,
    vms: Res<ViewModels>,
    theme: Res<Theme>,
    mut commands: Commands,
    sections: Query<Entity, With<BannerSection>>,
) {
    if !selected_units.is_changed()
        && !deploy.is_changed()
        && !selected_civilian.is_changed()
        && !vms.is_changed()
    {
        return;
    }
    let Ok(section) = sections.single() else {
        return;
    };
    commands.entity(section).despawn_children();
    if meta.observer {
        return;
    }

    commands.entity(section).with_children(|content| {
        if !selected_units.0.is_empty() {
            banner_box(
                content,
                &theme,
                Color::srgba(1.0, 200.0 / 255.0, 0.0, 0.15),
                Color::srgba(1.0, 200.0 / 255.0, 0.0, 0.4),
                &format!(
                    "Movement Mode — moving {} unit{} — click a highlighted province, or press Esc to cancel.",
                    selected_units.0.len(),
                    if selected_units.0.len() > 1 { "s" } else { "" }
                ),
            );
        }
        if let Some(state) = deploy.0.as_ref() {
            banner_box(
                content,
                &theme,
                Color::srgba(46.0 / 255.0, 204.0 / 255.0, 64.0 / 255.0, 0.15),
                Color::srgba(46.0 / 255.0, 204.0 / 255.0, 64.0 / 255.0, 0.4),
                &format!(
                    "Deploy {} — click a highlighted tile, or press Esc to cancel.",
                    state.civ_type
                ),
            );
        }
        // Selected deployed civilian → Recall affordance.
        if let Some(id) = selected_civilian.0
            && let Some(civ) = vms
                .civilians
                .as_ref()
                .and_then(|c| c.deployed.iter().find(|c| c.id == id))
        {
            content
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        margin: UiRect::bottom(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.18)),
                ))
                .with_children(|row| {
                    row.spawn((
                        Text::new(format!("Selected: {}", civ.civ_type)),
                        theme.font(12.0),
                        TextColor(theme::TEXT),
                    ));
                    let recall = small_button(row, &theme, "Recall");
                    row.commands()
                        .entity(recall)
                        .insert(RecallCivilianButton(civ.id));
                });
        }
    });
}

fn banner_box(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    bg: Color,
    border: Color,
    text: &str,
) {
    parent
        .spawn((
            Node {
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(border),
        ))
        .with_children(|inner| {
            inner.spawn((
                Text::new(text.to_string()),
                theme.font(12.0),
                TextColor(theme::TEXT),
            ));
        });
}

// ── Unit panel ───────────────────────────────────────────────────────────

pub fn update_unit_panel(
    meta: Res<GameMeta>,
    province_units: Res<ProvinceUnits>,
    selected_units: Res<SelectedUnits>,
    pending: Res<PendingMoveList>,
    selected_hex: Res<SelectedHex>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    mut commands: Commands,
    sections: Query<Entity, With<UnitPanelSection>>,
) {
    if !province_units.is_changed()
        && !selected_units.is_changed()
        && !pending.is_changed()
        && !selected_hex.is_changed()
    {
        return;
    }
    let Ok(section) = sections.single() else {
        return;
    };
    commands.entity(section).despawn_children();
    let Some(units) = province_units.vm.as_ref() else {
        return;
    };
    let icons = icons.as_deref();

    let tile = selected_tile(&selected_hex, &vms, &index);
    let is_player_province =
        !meta.observer && tile.is_some_and(|t| t.nation_id == i64::from(meta.player_nation));

    let selectable: Vec<u32> = units
        .army_units
        .iter()
        .filter(|u| u.category != "Garrison")
        .map(|u| u.id)
        .collect();
    let has_selection = !selected_units.0.is_empty();
    let selected_with_pending = selected_units
        .0
        .iter()
        .filter(|id| pending.0.iter().any(|m| m.unit_id == **id))
        .count();
    let selected_upgradable = units
        .army_units
        .iter()
        .filter(|u| selected_units.0.contains(&u.id) && u.upgrade_to.is_some())
        .count();

    commands.entity(section).with_children(|content| {
        content.spawn((
            Text::new(units.province_name.clone()),
            theme.font_bold(14.0),
            TextColor(theme::TEXT),
        ));
        if units.garrison_count > 0 {
            content
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_icon(row, icons, "units", "Army", 14.0);
                    row.spawn((
                        Text::new(format!("Garrison: {} militia", units.garrison_count)),
                        theme.font(12.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                });
        }
        if units.army_units.is_empty() && units.garrison_count == 0 {
            content.spawn((
                Text::new("No units in province"),
                theme.font_italic(12.0),
                TextColor(theme::TEXT_DIM),
            ));
            return;
        }
        if units.army_units.is_empty() {
            return;
        }

        content.spawn((
            Text::new(format!("Army Units ({})", units.army_units.len())),
            theme.font_bold(13.0),
            TextColor(theme::TEXT),
            Node {
                margin: UiRect::top(Val::Px(4.0)),
                ..default()
            },
        ));

        // Action buttons (web header buttons, wrapped to the panel width).
        if is_player_province {
            content
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(4.0),
                    row_gap: Val::Px(4.0),
                    margin: UiRect::vertical(Val::Px(4.0)),
                    ..default()
                })
                .with_children(|row| {
                    if has_selection && selected_with_pending > 0 {
                        let b = small_button(row, &theme, "Cancel Moves");
                        row.commands().entity(b).insert(CancelSelectedMovesButton);
                    }
                    if selected_upgradable > 0 {
                        let b =
                            small_button(row, &theme, &format!("Upgrade {selected_upgradable}"));
                        row.commands().entity(b).insert(UpgradeSelectedButton);
                    }
                    if has_selection {
                        let b = small_button(row, &theme, "Dismiss");
                        row.commands().entity(b).insert(DismissSelectedButton);
                    }
                    if has_selection || selectable.len() > 1 {
                        let label = if has_selection && selected_units.0.len() == selectable.len() {
                            "Deselect"
                        } else {
                            "Select All"
                        };
                        let b = small_button(row, &theme, label);
                        row.commands().entity(b).insert(SelectAllUnitsButton);
                    }
                });
            if has_selection {
                content.spawn((
                    Text::new(format!(
                        "Click a highlighted hex to move {} unit{} · Esc to cancel",
                        selected_units.0.len(),
                        if selected_units.0.len() > 1 { "s" } else { "" }
                    )),
                    theme.font_italic(11.0),
                    TextColor(theme::TEXT_DIM),
                ));
            }
        }

        for unit in &units.army_units {
            spawn_unit_row(
                content,
                &theme,
                icons,
                unit,
                is_player_province,
                selected_units.0.contains(&unit.id),
                pending.0.iter().find(|m| m.unit_id == unit.id),
            );
        }
    });
}

fn spawn_unit_row(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    unit: &ArmyUnitVm,
    is_player_province: bool,
    is_selected: bool,
    pending: Option<&crate::game::vm::PendingMoveVm>,
) {
    let selectable = unit.category != "Garrison" && is_player_province;
    let (bg, border) = if is_selected {
        (
            Color::srgba(218.0 / 255.0, 165.0 / 255.0, 32.0 / 255.0, 0.15),
            Color::srgba(218.0 / 255.0, 165.0 / 255.0, 32.0 / 255.0, 0.4),
        )
    } else {
        (Color::srgba(1.0, 1.0, 1.0, 0.05), Color::NONE)
    };
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                margin: UiRect::bottom(Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(border),
        ))
        .with_children(|row| {
            // Name line: checkbox / icon / name+medals / FP.
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|line| {
                line.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|left| {
                    if selectable {
                        let checkbox = widgets::spawn_checkbox(
                            left,
                            theme,
                            CheckboxProps {
                                label: String::new(),
                                checked: is_selected,
                                enabled: true,
                            },
                        );
                        left.commands()
                            .entity(checkbox)
                            .insert(UnitCheckbox(unit.id));
                    }
                    spawn_icon(left, icons, "units", unit_icon_name(&unit.category), 14.0);
                    left.spawn((
                        Text::new(split_camel(&unit.unit_type)),
                        theme.font(12.0),
                        TextColor(theme::TEXT),
                    ));
                    if unit.medals > 0 {
                        left.spawn((
                            Text::new("•".repeat(unit.medals as usize)),
                            theme.font(12.0),
                            TextColor(Color::srgb_u8(0xff, 0xd7, 0x00)),
                        ));
                    }
                });
                line.spawn((
                    Text::new(format!("FP {:.1}", unit.effective_firepower)),
                    theme.font(11.0),
                    TextColor(theme::TEXT_DIM),
                ));
            });
            // Health line.
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|line| {
                let pct = unit.health as f32;
                spawn_bar(line, 60.0, pct / 100.0, health_color(pct));
                line.spawn((
                    Text::new(format!("{}%", unit.health)),
                    theme.font(10.0),
                    TextColor(theme::TEXT_DIM),
                ));
            });
            // Pending move banner.
            if let Some(pending) = pending {
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|line| {
                    line.spawn((
                        Text::new(format!("→ {}", pending.dest_name)),
                        theme.font(11.0),
                        TextColor(Color::srgb_u8(0xff, 0xd7, 0x00)),
                    ));
                    if is_player_province {
                        let b = small_button(line, theme, "Cancel");
                        line.commands().entity(b).insert(CancelMoveButton(unit.id));
                    }
                });
            }
            // Upgrade affordance (unlocked target only).
            if is_player_province && let Some(target) = unit.upgrade_to.as_deref() {
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|line| {
                    let arms = match unit.upgrade_arms_delta {
                        Some(delta) if delta > 0 => format!(" + {delta} arms"),
                        _ => String::new(),
                    };
                    line.spawn((
                        Text::new(format!(
                            "↑ {} (${}{arms})",
                            split_camel(target),
                            unit.upgrade_cost.unwrap_or(0)
                        )),
                        theme.font(11.0),
                        TextColor(Color::srgb_u8(0x77, 0xaa, 0xff)),
                    ));
                    let b = small_button(line, theme, "Upgrade");
                    line.commands().entity(b).insert(UpgradeUnitButton(unit.id));
                });
            }
        });
}

// ── Civilian panel ───────────────────────────────────────────────────────

pub fn update_civilian_panel(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    selected_hex: Res<SelectedHex>,
    selected_civilian: Res<SelectedCivilian>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    mut commands: Commands,
    sections: Query<Entity, With<CivilianPanelSection>>,
) {
    if !vms.is_changed() && !selected_hex.is_changed() && !selected_civilian.is_changed() {
        return;
    }
    let Ok(section) = sections.single() else {
        return;
    };
    commands.entity(section).despawn_children();
    if meta.observer {
        return;
    }
    // Web parity: panel shows while one of the player's own tiles is pinned.
    let on_player_tile = selected_tile(&selected_hex, &vms, &index)
        .is_some_and(|t| t.nation_id == i64::from(meta.player_nation));
    if !on_player_tile {
        return;
    }
    let Some(civilians) = vms.civilians.as_ref() else {
        return;
    };
    let icons = icons.as_deref();

    commands.entity(section).with_children(|content| {
        content.spawn((
            Text::new("Civilian Workforce"),
            theme.font_bold(13.0),
            TextColor(theme::TEXT),
        ));
        if civilians.deployed.is_empty() && civilians.undeployed.is_empty() {
            content.spawn((
                Text::new("No civilians"),
                theme.font_italic(12.0),
                TextColor(theme::TEXT_DIM),
            ));
            return;
        }
        if !civilians.undeployed.is_empty() {
            content.spawn((
                Text::new(format!("Undeployed ({})", civilians.undeployed.len())),
                theme.font(12.0),
                TextColor(theme::TEXT_DIM),
            ));
            for civ in &civilians.undeployed {
                spawn_civilian_row(content, &theme, icons, civ, false);
            }
        }
        if !civilians.deployed.is_empty() {
            content.spawn((
                Text::new(format!("Deployed ({})", civilians.deployed.len())),
                theme.font(12.0),
                TextColor(theme::TEXT_DIM),
            ));
            for civ in &civilians.deployed {
                spawn_civilian_row(
                    content,
                    &theme,
                    icons,
                    civ,
                    selected_civilian.0 == Some(civ.id),
                );
            }
        }
    });
}

fn spawn_civilian_row(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    civ: &CivilianEntry,
    is_selected: bool,
) {
    let deployed = civ.position.is_some();
    let bg = if is_selected {
        Color::srgba(1.0, 1.0, 1.0, 0.12)
    } else if deployed {
        Color::srgba(1.0, 1.0, 1.0, 0.05)
    } else {
        Color::srgba(46.0 / 255.0, 204.0 / 255.0, 64.0 / 255.0, 0.08)
    };
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                margin: UiRect::bottom(Val::Px(2.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(if is_selected {
                Color::srgba(1.0, 1.0, 1.0, 0.25)
            } else {
                Color::NONE
            }),
        ))
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|line| {
                line.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|left| {
                    spawn_icon(left, icons, "civilians", &civ.civ_type, 14.0);
                    let label = match civ.position {
                        Some(pos) => format!("{} ({}, {})", civ.civ_type, pos.q, pos.r),
                        None => civ.civ_type.clone(),
                    };
                    let b = widgets::spawn_button(
                        left,
                        theme,
                        ButtonProps {
                            label,
                            font_size: 12.0,
                            flat: true,
                            ..default()
                        },
                    );
                    left.commands().entity(b).insert(CivilianRowButton {
                        id: civ.id,
                        civ_type: civ.civ_type.clone(),
                        working: civ.working,
                        position: civ.position.map(|p| (p.q, p.r)),
                    });
                });
                let status = if !deployed {
                    "Click to deploy"
                } else if !civ.working {
                    "Click to redeploy"
                } else if is_selected {
                    "selected"
                } else {
                    ""
                };
                if !status.is_empty() {
                    line.spawn((
                        Text::new(status.to_string()),
                        theme.font(10.0),
                        TextColor(Color::srgb_u8(0x88, 0xcc, 0x88)),
                    ));
                }
            });
            if civ.working && civ.turns_remaining > 0 {
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|line| {
                    const MAX_TURNS: f32 = 5.0;
                    let filled = (MAX_TURNS - civ.turns_remaining as f32).max(0.0) / MAX_TURNS;
                    spawn_bar(line, 50.0, filled, Color::srgb_u8(0x44, 0xaa, 0x88));
                    line.spawn((
                        Text::new(format!(
                            "{} turn{}",
                            civ.turns_remaining,
                            if civ.turns_remaining == 1 { "" } else { "s" }
                        )),
                        theme.font(10.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                });
                if let Some(resource) = civ.tile_resource.as_deref() {
                    row.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|line| {
                        spawn_icon(line, icons, "commodities", resource, 12.0);
                        line.spawn((
                            Text::new(format!("Improving {resource}")),
                            theme.font(10.0),
                            TextColor(theme::TEXT_DIM),
                        ));
                    });
                }
                if civ.civ_type == "Engineer"
                    && let Some(task) = civ.build_task.as_deref()
                {
                    row.spawn((
                        Text::new(format!("Building {task}")),
                        theme.font_italic(10.0),
                        TextColor(Color::srgb_u8(0x88, 0xcc, 0x88)),
                    ));
                }
            }
            if deployed && !civ.working {
                row.spawn((
                    Text::new("Idle"),
                    theme.font_italic(10.0),
                    TextColor(theme::TEXT_DIM),
                ));
            }
        });
}

// ── Naval panel ──────────────────────────────────────────────────────────

pub fn update_naval_panel(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    selected_hex: Res<SelectedHex>,
    selected_navy: Res<SelectedNavy>,
    selected_ships: Res<SelectedShips>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    mut commands: Commands,
    sections: Query<Entity, With<NavalPanelSection>>,
) {
    if !vms.is_changed()
        && !selected_hex.is_changed()
        && !selected_navy.is_changed()
        && !selected_ships.is_changed()
    {
        return;
    }
    let Ok(section) = sections.single() else {
        return;
    };
    commands.entity(section).despawn_children();
    if meta.observer {
        return;
    }
    let Some(ships) = vms.ships.as_ref() else {
        return;
    };

    let marker: Option<&NavyMarker> = selected_navy
        .0
        .as_deref()
        .and_then(|key| vms.navy_markers.iter().find(|m| navy::marker_key(m) == key));
    let player_fleet =
        marker.filter(|m| m.kind == "fleet" && m.nation_id == i64::from(meta.player_nation));
    let at_player_capital = selected_tile(&selected_hex, &vms, &index)
        .is_some_and(|t| t.is_country_capital && t.nation_id == i64::from(meta.player_nation));
    if player_fleet.is_none() && !at_player_capital {
        return;
    }
    let icons = icons.as_deref();

    // Fleet scoping: restrict to the selected fleet's sea zone.
    let (displayed, scope_label): (Vec<&ShipVm>, String) = match player_fleet {
        Some(m) => {
            let zone = m.sea_zone_id;
            (
                ships
                    .warships
                    .iter()
                    .filter(|s| zone.is_some() && s.sea_zone == zone)
                    .collect(),
                m.sea_zone_name
                    .as_deref()
                    .map(|n| format!(" — {n}"))
                    .unwrap_or_default(),
            )
        }
        None => (ships.warships.iter().collect(), String::new()),
    };
    let interactive = player_fleet.is_some();
    let all_selected =
        !displayed.is_empty() && displayed.iter().all(|s| selected_ships.0.contains(&s.id));
    let pending_dest = player_fleet
        .and_then(|m| m.pending_move_to_zone_id)
        .and_then(|id| vms.sea_zones.iter().find(|z| z.id == id));
    let from_zone = player_fleet.and_then(|m| m.sea_zone_id);

    commands.entity(section).with_children(|content| {
        content
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(format!("Warships{scope_label}")),
                    theme.font_bold(13.0),
                    TextColor(theme::TEXT),
                ));
                if interactive && displayed.len() > 1 {
                    let b = small_button(
                        row,
                        &theme,
                        if all_selected {
                            "Deselect"
                        } else {
                            "Select All"
                        },
                    );
                    row.commands().entity(b).insert(SelectAllShipsButton);
                }
            });
        let fp_suffix = if player_fleet.is_some() {
            String::new()
        } else {
            format!(" · {} FP", ships.total_naval_fp)
        };
        content.spawn((
            Text::new(format!(
                "{} ship{}{fp_suffix}",
                displayed.len(),
                if displayed.len() == 1 { "" } else { "s" }
            )),
            theme.font(11.0),
            TextColor(theme::TEXT_DIM),
        ));

        if let Some(dest) = pending_dest {
            content
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        margin: UiRect::vertical(Val::Px(3.0)),
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(
                        218.0 / 255.0,
                        165.0 / 255.0,
                        32.0 / 255.0,
                        0.12,
                    )),
                    BorderColor::all(Color::srgba(
                        218.0 / 255.0,
                        165.0 / 255.0,
                        32.0 / 255.0,
                        0.4,
                    )),
                ))
                .with_children(|row| {
                    row.spawn((
                        Text::new(format!("→ {} (end of turn)", dest.name)),
                        theme.font(11.0),
                        TextColor(Color::srgb_u8(0xff, 0xd7, 0x00)),
                    ));
                    if let Some(from_zone) = from_zone {
                        let b = small_button(row, &theme, "Cancel");
                        row.commands()
                            .entity(b)
                            .insert(CancelFleetMoveButton(from_zone));
                    }
                });
        } else if interactive && !selected_ships.0.is_empty() {
            content.spawn((
                Text::new("Click a highlighted sea hex to queue a move · Esc to cancel"),
                theme.font_italic(11.0),
                TextColor(theme::TEXT_DIM),
            ));
        }

        if displayed.is_empty() {
            content.spawn((
                Text::new("No warships"),
                theme.font_italic(11.0),
                TextColor(theme::TEXT_DIM),
            ));
        }
        for ship in &displayed {
            spawn_ship_row(
                content,
                &theme,
                icons,
                ship,
                interactive,
                selected_ships.0.contains(&ship.id),
            );
        }

        // Merchant marine summary at the capital (not fleet-scoped).
        if player_fleet.is_none() && !ships.merchants.is_empty() {
            content.spawn((
                Text::new(format!(
                    "Merchants ({}) · {} cargo",
                    ships.merchants.len(),
                    ships.total_cargo
                )),
                theme.font(12.0),
                TextColor(theme::TEXT_DIM),
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    ..default()
                },
            ));
            for ship in &ships.merchants {
                spawn_ship_row(content, &theme, icons, ship, false, false);
            }
        }
    });
}

fn spawn_ship_row(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    ship: &ShipVm,
    interactive: bool,
    is_selected: bool,
) {
    let (bg, border) = if is_selected {
        (
            Color::srgba(218.0 / 255.0, 165.0 / 255.0, 32.0 / 255.0, 0.15),
            Color::srgba(218.0 / 255.0, 165.0 / 255.0, 32.0 / 255.0, 0.4),
        )
    } else {
        (Color::srgba(1.0, 1.0, 1.0, 0.05), Color::NONE)
    };
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                margin: UiRect::bottom(Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(border),
        ))
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|line| {
                line.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|left| {
                    if interactive {
                        let checkbox = widgets::spawn_checkbox(
                            left,
                            theme,
                            CheckboxProps {
                                label: String::new(),
                                checked: is_selected,
                                enabled: true,
                            },
                        );
                        left.commands()
                            .entity(checkbox)
                            .insert(ShipCheckbox(ship.id));
                    }
                    spawn_icon(left, icons, "ships", &ship.ship_type, 14.0);
                    left.spawn((
                        Text::new(split_camel(&ship.ship_type)),
                        theme.font(12.0),
                        TextColor(theme::TEXT),
                    ));
                });
                let stat = match (ship.firepower, ship.cargo) {
                    (Some(fp), _) => format!("FP {fp}"),
                    (None, Some(cargo)) => format!("Cargo {cargo}"),
                    _ => String::new(),
                };
                line.spawn((
                    Text::new(stat),
                    theme.font(11.0),
                    TextColor(theme::TEXT_DIM),
                ));
            });
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|line| {
                let pct = if ship.hull_max > 0 {
                    (ship.hull as f32 / ship.hull_max as f32 * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                };
                spawn_bar(line, 60.0, pct / 100.0, hull_color(pct));
                line.spawn((
                    Text::new(format!("{}/{}", ship.hull, ship.hull_max)),
                    theme.font(10.0),
                    TextColor(theme::TEXT_DIM),
                ));
            });
        });
}

// ── Interaction handlers ─────────────────────────────────────────────────

pub fn handle_unit_checkboxes(
    mut toggles: MessageReader<CheckboxToggled>,
    unit_boxes: Query<&UnitCheckbox>,
    ship_boxes: Query<&ShipCheckbox>,
    mut selected_units: ResMut<SelectedUnits>,
    mut selected_ships: ResMut<SelectedShips>,
) {
    for toggle in toggles.read() {
        if let Ok(unit) = unit_boxes.get(toggle.entity) {
            if toggle.checked {
                if !selected_units.0.contains(&unit.0) {
                    selected_units.0.push(unit.0);
                }
            } else {
                selected_units.0.retain(|id| *id != unit.0);
            }
        }
        if let Ok(ship) = ship_boxes.get(toggle.entity) {
            if toggle.checked {
                if !selected_ships.0.contains(&ship.0) {
                    selected_ships.0.push(ship.0);
                }
            } else {
                selected_ships.0.retain(|id| *id != ship.0);
            }
        }
    }
}

pub fn handle_panel_buttons(
    mut activations: MessageReader<ButtonActivated>,
    meta: Res<GameMeta>,
    theme: Res<Theme>,
    province_units: Res<ProvinceUnits>,
    pending: Res<PendingMoveList>,
    vms: Res<ViewModels>,
    mut commands: Commands,
    mut modal_stack: ResMut<ModalStack>,
    mut game_commands: MessageWriter<GameCommand>,
    selections: (
        ResMut<SelectedUnits>,
        ResMut<SelectedShips>,
        ResMut<SelectedCivilian>,
        ResMut<SelectedHex>,
        ResMut<SelectedNavy>,
        ResMut<DeployMode>,
    ),
    mut cameras: Query<&mut Transform, With<GameCamera>>,
    buttons: (
        Query<(), With<SelectAllUnitsButton>>,
        Query<(), With<CancelSelectedMovesButton>>,
        Query<(), With<DismissSelectedButton>>,
        Query<(), With<UpgradeSelectedButton>>,
        Query<&CancelMoveButton>,
        Query<&UpgradeUnitButton>,
        Query<&ConfirmDismissButton>,
        Query<(), With<CancelModalButton>>,
        Query<&CivilianRowButton>,
        Query<&RecallCivilianButton>,
        Query<(), With<SelectAllShipsButton>>,
        Query<&CancelFleetMoveButton>,
    ),
) {
    let (
        select_all_units,
        cancel_selected_moves,
        dismiss_selected,
        upgrade_selected,
        cancel_move,
        upgrade_unit,
        confirm_dismiss,
        cancel_modal,
        civilian_rows,
        recall_civilian,
        select_all_ships,
        cancel_fleet_move,
    ) = buttons;
    let (
        mut selected_units,
        mut selected_ships,
        mut selected_civilian,
        mut selected_hex,
        mut selected_navy,
        mut deploy,
    ) = selections;
    if meta.observer {
        return;
    }

    for ButtonActivated(entity) in activations.read() {
        let entity = *entity;

        if select_all_units.contains(entity) {
            if let Some(units) = province_units.vm.as_ref() {
                let selectable: Vec<u32> = units
                    .army_units
                    .iter()
                    .filter(|u| u.category != "Garrison")
                    .map(|u| u.id)
                    .collect();
                if selected_units.0.len() == selectable.len() {
                    selected_units.0.clear();
                } else {
                    selected_units.0 = selectable;
                }
            }
        } else if cancel_selected_moves.contains(entity) {
            let cancelable: Vec<u32> = selected_units
                .0
                .iter()
                .copied()
                .filter(|id| pending.0.iter().any(|m| m.unit_id == *id))
                .collect();
            if !cancelable.is_empty() {
                game_commands.write(GameCommand::CancelUnitMoves {
                    unit_ids: cancelable,
                });
            }
        } else if dismiss_selected.contains(entity) {
            if !selected_units.0.is_empty() {
                open_dismiss_confirm(
                    &mut commands,
                    &mut modal_stack,
                    &theme,
                    selected_units.0.clone(),
                );
            }
        } else if upgrade_selected.contains(entity) {
            if let Some(units) = province_units.vm.as_ref() {
                let upgradable: Vec<u32> = units
                    .army_units
                    .iter()
                    .filter(|u| selected_units.0.contains(&u.id) && u.upgrade_to.is_some())
                    .map(|u| u.id)
                    .collect();
                if !upgradable.is_empty() {
                    game_commands.write(GameCommand::UpgradeUnits {
                        unit_ids: upgradable,
                    });
                }
            }
        } else if let Ok(cancel) = cancel_move.get(entity) {
            game_commands.write(GameCommand::CancelUnitMove { unit_id: cancel.0 });
        } else if let Ok(upgrade) = upgrade_unit.get(entity) {
            game_commands.write(GameCommand::UpgradeUnit { unit_id: upgrade.0 });
        } else if let Ok(confirm) = confirm_dismiss.get(entity) {
            game_commands.write(GameCommand::DisbandUnits {
                unit_ids: confirm.0.clone(),
            });
            widgets::close_top_modal(&mut commands, &mut modal_stack);
        } else if cancel_modal.contains(entity) {
            widgets::close_top_modal(&mut commands, &mut modal_stack);
        } else if let Ok(civ) = civilian_rows.get(entity) {
            handle_civilian_row(
                civ,
                &meta,
                &vms,
                &mut selected_civilian,
                &mut selected_hex,
                &mut selected_navy,
                &mut selected_units,
                &mut deploy,
                &mut cameras,
            );
        } else if let Ok(recall) = recall_civilian.get(entity) {
            game_commands.write(GameCommand::RecallCivilian {
                civilian_id: recall.0 as u32,
            });
        } else if select_all_ships.contains(entity) {
            let marker = selected_navy
                .0
                .as_deref()
                .and_then(|key| vms.navy_markers.iter().find(|m| navy::marker_key(m) == key));
            if let Some(zone) = marker.and_then(|m| m.sea_zone_id)
                && let Some(ships) = vms.ships.as_ref()
            {
                let all: Vec<u32> = ships
                    .warships
                    .iter()
                    .filter(|s| s.sea_zone == Some(zone))
                    .map(|s| s.id)
                    .collect();
                if selected_ships.0.len() == all.len() {
                    selected_ships.0.clear();
                } else {
                    selected_ships.0 = all;
                }
            }
        } else if let Ok(cancel) = cancel_fleet_move.get(entity) {
            game_commands.write(GameCommand::CancelFleetMove {
                from_zone: cancel.0,
            });
        }
    }
}

/// Sidebar civilian row click (web `handleSelectCivilian`): undeployed or
/// idle → deploy mode; busy → select + focus the camera on its tile.
fn handle_civilian_row(
    civ: &CivilianRowButton,
    meta: &GameMeta,
    vms: &ViewModels,
    selected_civilian: &mut SelectedCivilian,
    selected_hex: &mut SelectedHex,
    selected_navy: &mut SelectedNavy,
    selected_units: &mut SelectedUnits,
    deploy: &mut DeployMode,
    cameras: &mut Query<&mut Transform, With<GameCamera>>,
) {
    let Some(tiles) = vms.map.as_ref() else {
        return;
    };
    match civ.position {
        Some(pos) if civ.working => {
            selected_civilian.0 = Some(civ.id);
            selected_navy.0 = None;
            selected_units.0.clear();
            selected_hex.0 = Some(pos);
            if let Ok(mut transform) = cameras.single_mut() {
                let world = geometry::hex_to_world(pos.0, pos.1);
                transform.translation.x = world.x;
                transform.translation.y = world.y;
            }
        }
        position => {
            // Undeployed, or deployed-but-idle (recall happens on tile click).
            deploy.0 = Some(selection::compute_deploy_state(
                civ.id,
                &civ.civ_type,
                position,
                tiles,
                meta.player_nation,
            ));
        }
    }
}

fn open_dismiss_confirm(
    commands: &mut Commands,
    modal_stack: &mut ModalStack,
    theme: &Theme,
    unit_ids: Vec<u32>,
) {
    let n = unit_ids.len();
    let handles = widgets::open_modal(
        commands,
        modal_stack,
        theme,
        ModalProps {
            title: "Dismiss units".to_string(),
            width: Val::Px(340.0),
        },
    );
    commands.entity(handles.content).with_children(|content| {
        content.spawn((
            Text::new(format!(
                "Dismiss {n} unit{}? This cannot be undone.",
                if n > 1 { "s" } else { "" }
            )),
            theme.font(13.0),
            TextColor(theme::TEXT),
        ));
        content
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|row| {
                let confirm = widgets::spawn_button(row, theme, ButtonProps::label("Dismiss"));
                row.commands()
                    .entity(confirm)
                    .insert(ConfirmDismissButton(unit_ids));
                let cancel = widgets::spawn_button(row, theme, ButtonProps::label("Cancel"));
                row.commands().entity(cancel).insert(CancelModalButton);
            });
    });
}
