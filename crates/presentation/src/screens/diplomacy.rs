//! Diplomacy screen (F4): the map stays visible (zoom-locked to fit, forced
//! into diplomatic mode) with the web `DiplomacyBottomBar` along the bottom.
//! Queue-then-target flow: an action button arms the action, clicking a
//! nation on the map fires the command; everything resolves at end turn.

use bevy::prelude::*;

use crate::game::resources::{DiploUi, GameMeta, QueuedDiplomacyAction, TileIndex, ViewModels};
use crate::game::vm::{DiploScreenRelationVm, DiplomacyScreenVm};
use crate::map::camera::GameCamera;
use crate::map::icons::IconAssets;
use crate::map::layers::{MapBounds, MapMode};
use crate::map::picking::{HoveredHex, PickingBlocker, SelectedHex};
use crate::screens::common::spawn_icon;
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, TooltipText};

const BAR_BG: Color = Color::srgb_u8(0x16, 0x16, 0x25);
const STANDING_GREEN: Color = Color::srgb_u8(0x44, 0xaa, 0x44);
const STANDING_AMBER: Color = Color::srgb_u8(0xcc, 0xaa, 0x44);
const STANDING_RED: Color = Color::srgb_u8(0xee, 0x44, 0x44);
const WAR_RED: Color = Color::srgb_u8(0xaa, 0x33, 0x33);

/// Bottom-bar status colors (web `STATUS_COLORS`).
fn status_color(status: &str) -> Color {
    match status {
        "At War" => Color::srgb_u8(0xee, 0x44, 0x44),
        "Anarchy" => Color::srgb_u8(0xaa, 0x00, 0xaa),
        "Alliance" => Color::srgb_u8(0x44, 0xaa, 0x44),
        "NAP" => Color::srgb_u8(0x44, 0xaa, 0xaa),
        "Neutral" => Color::srgb_u8(0x88, 0x88, 0x88),
        _ => Color::srgb_u8(0x55, 0x55, 0x55),
    }
}

// ── Target validation (web canTargetNationWithAction) ───────────────────

pub fn can_target_nation(
    action: &QueuedDiplomacyAction,
    target_nation_id: u32,
    screen: Option<&DiplomacyScreenVm>,
) -> bool {
    let Some(rel) = screen.and_then(|s| s.relation(target_nation_id)) else {
        return false;
    };
    if rel.is_in_anarchy {
        return false;
    }
    let a = &rel.actions;
    match action {
        QueuedDiplomacyAction::Consulate => a.can_build_consulate,
        QueuedDiplomacyAction::Embassy => a.can_build_embassy,
        QueuedDiplomacyAction::Nap => a.can_propose_nap,
        QueuedDiplomacyAction::Alliance => a.can_propose_alliance,
        QueuedDiplomacyAction::Peace => a.can_propose_peace,
        QueuedDiplomacyAction::Grant { .. } => a.can_send_grant,
        QueuedDiplomacyAction::BreakTreaty { treaty_type } => {
            a.can_break_treaty && a.breakable_treaties.contains(treaty_type)
        }
        QueuedDiplomacyAction::War => a.can_declare_war,
    }
}

/// `None` = the target is valid (web `diplomacyInvalidReasonFor`).
pub fn invalid_target_reason(
    action: &QueuedDiplomacyAction,
    target_nation_id: Option<u32>,
    player_nation_id: u32,
    screen: Option<&DiplomacyScreenVm>,
) -> Option<String> {
    let Some(target) = target_nation_id else {
        return Some("Click on a foreign nation to target this action.".into());
    };
    if target == player_nation_id {
        return Some("Cannot target your own nation.".into());
    }
    let Some(rel) = screen.and_then(|s| s.relation(target)) else {
        return Some("Cannot target this nation.".into());
    };
    let name = rel.nation_name.as_str();
    if rel.is_in_anarchy {
        return Some(format!("{name} is in anarchy — diplomacy is unavailable."));
    }
    let a = &rel.actions;
    let has = |t: &str| rel.treaties.iter().any(|x| x == t);
    match action {
        QueuedDiplomacyAction::Consulate => {
            if a.can_build_consulate {
                None
            } else if rel.has_consulate || rel.has_embassy {
                Some(format!("{name} already has a consulate."))
            } else {
                Some(format!(
                    "Cannot build consulate with {name} — check your treasury."
                ))
            }
        }
        QueuedDiplomacyAction::Embassy => {
            if a.can_build_embassy {
                None
            } else if rel.has_embassy {
                Some(format!("{name} already has an embassy."))
            } else if !rel.has_consulate {
                Some(format!(
                    "Need a consulate with {name} before opening an embassy."
                ))
            } else {
                Some(format!(
                    "Cannot build embassy with {name} — check your treasury."
                ))
            }
        }
        QueuedDiplomacyAction::Nap => {
            if a.can_propose_nap {
                None
            } else if has("NonAggressionPact") || has("NAP") || has("Alliance") {
                Some(format!("Already have a NAP / alliance with {name}."))
            } else if rel.has_pending_nap {
                Some(format!("NAP proposal already pending with {name}."))
            } else if rel.at_war {
                Some(format!("At war with {name} — make peace first."))
            } else {
                Some(format!(
                    "Cannot propose NAP to {name} — improve relations first (grants help)."
                ))
            }
        }
        QueuedDiplomacyAction::Alliance => {
            if a.can_propose_alliance {
                None
            } else if has("Alliance") {
                Some(format!("Already allied with {name}."))
            } else if rel.has_pending_alliance {
                Some(format!("Alliance proposal already pending with {name}."))
            } else if rel.at_war {
                Some(format!("At war with {name} — make peace first."))
            } else {
                Some(format!(
                    "Cannot propose alliance to {name} — improve relations first (grants help)."
                ))
            }
        }
        QueuedDiplomacyAction::Peace => {
            if a.can_propose_peace {
                None
            } else if !rel.at_war {
                Some(format!("Not at war with {name}."))
            } else if rel.has_pending_peace {
                Some(format!("Peace proposal already pending with {name}."))
            } else {
                Some(format!("Cannot propose peace to {name}."))
            }
        }
        QueuedDiplomacyAction::Grant { .. } => {
            if a.can_send_grant {
                None
            } else {
                Some(format!(
                    "Cannot send grant to {name} right now — check your treasury."
                ))
            }
        }
        QueuedDiplomacyAction::BreakTreaty { treaty_type } => {
            if a.can_break_treaty && a.breakable_treaties.contains(treaty_type) {
                None
            } else {
                Some(format!("No {treaty_type} to break with {name}."))
            }
        }
        QueuedDiplomacyAction::War => {
            if a.can_declare_war {
                None
            } else if rel.at_war {
                Some(format!("Already at war with {name}."))
            } else {
                Some(format!(
                    "Cannot declare war on {name} — break existing treaties first."
                ))
            }
        }
    }
}

// ── Markers / components ─────────────────────────────────────────────────

#[derive(Component)]
pub struct DiplomacyRoot;

#[derive(Component)]
pub struct DiploBarContent;

/// Cursor-following reason tooltip shown while an armed action hovers an
/// invalid target.
#[derive(Component)]
pub struct DiploReasonTooltip;

#[derive(Component)]
pub struct DiploReasonText;

/// Arms a queued action directly (consulate / embassy / NAP / alliance /
/// peace / a specific grant amount / a specific treaty break / confirmed
/// war).
#[derive(Component, Clone)]
pub struct DiploArmButton(pub QueuedDiplomacyAction);

/// Toggle buttons for the inline pickers / war confirm.
#[derive(Component, Clone, Copy)]
pub enum DiploToggleButton {
    GrantPicker,
    BreakPicker,
    WarConfirm,
    WarCancel,
    CancelQueued,
}

// ── Lifecycle ────────────────────────────────────────────────────────────

/// Enter: snapshot the camera + map mode, force diplomatic mode, fit-zoom
/// the whole map (web `lockZoom`), and spawn the bottom bar.
pub fn enter_diplomacy(
    mut commands: Commands,
    theme: Res<Theme>,
    mut ui: ResMut<DiploUi>,
    mut mode: ResMut<MapMode>,
    bounds: Option<Res<MapBounds>>,
    windows: Query<&Window>,
    mut camera: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    ui.queued = None;
    ui.show_grant_picker = false;
    ui.show_break_picker = false;
    ui.confirm_war = false;

    if let Ok((mut transform, mut projection)) = camera.single_mut()
        && let Projection::Orthographic(ref mut ortho) = *projection
    {
        ui.saved_view = Some((*mode, transform.translation, ortho.scale));
        if let (Some(bounds), Ok(window)) = (bounds.as_deref(), windows.single()) {
            // Fit the whole map between the top bar (44) and the bottom
            // bar (~110), with a small margin.
            let usable_h = (window.height() - 44.0 - 110.0).max(200.0);
            let usable_w = window.width().max(200.0);
            let world_h = bounds.max.y - bounds.min.y + 60.0;
            let fit = (bounds.width_px / usable_w).max(world_h / usable_h);
            ortho.scale = fit;
            transform.translation.x = bounds.center.x;
            // Bias the view center up a little so the bottom bar doesn't
            // cover the southern map edge.
            transform.translation.y = bounds.center.y - 33.0 * fit;
        }
    }
    *mode = MapMode::Diplomatic;

    // Bottom bar.
    commands
        .spawn((
            DiplomacyRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::new(Val::Px(16.0), Val::Px(16.0), Val::Px(10.0), Val::Px(12.0)),
                border: UiRect::top(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(BAR_BG),
            BorderColor::all(theme::BORDER),
            GlobalZIndex(70),
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|bar| {
            bar.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    ..default()
                },
                DiploBarContent,
            ));
        });

    // Cursor-following invalid-target reason tooltip.
    commands
        .spawn((
            DiplomacyRoot,
            DiploReasonTooltip,
            Node {
                position_type: PositionType::Absolute,
                max_width: Val::Px(300.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.10, 0.04, 0.04, 0.95)),
            BorderColor::all(WAR_RED),
            GlobalZIndex(610),
            Visibility::Hidden,
            bevy::picking::Pickable::IGNORE,
        ))
        .with_children(|tooltip| {
            tooltip.spawn((
                Text::new(""),
                theme.font(12.0),
                TextColor(theme::TEXT),
                DiploReasonText,
                bevy::picking::Pickable::IGNORE,
            ));
        });
}

/// Exit: despawn the bar, clear the armed action, and restore the map mode
/// and camera captured on entry.
pub fn exit_diplomacy(
    mut commands: Commands,
    roots: Query<Entity, With<DiplomacyRoot>>,
    mut ui: ResMut<DiploUi>,
    mut mode: ResMut<MapMode>,
    mut camera: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    ui.queued = None;
    ui.show_grant_picker = false;
    ui.show_break_picker = false;
    ui.confirm_war = false;
    if let Some((saved_mode, translation, scale)) = ui.saved_view.take() {
        *mode = saved_mode;
        if let Ok((mut transform, mut projection)) = camera.single_mut()
            && let Projection::Orthographic(ref mut ortho) = *projection
        {
            transform.translation = translation;
            ortho.scale = scale;
        }
    }
}

// ── Focused nation ───────────────────────────────────────────────────────

/// Hover (transient) takes priority over the pinned selection (web parity).
pub fn focused_nation_id(
    hovered: &HoveredHex,
    selected: &SelectedHex,
    vms: &ViewModels,
    index: &TileIndex,
    player: u32,
) -> Option<u32> {
    let nation_at = |coord: (i32, i32)| -> Option<u32> {
        let tiles = vms.map.as_ref()?;
        let tile = tiles.get(*index.by_coord.get(&coord)?)?;
        (!tile.is_sea() && tile.nation_id >= 0).then_some(tile.nation_id as u32)
    };
    hovered
        .0
        .and_then(nation_at)
        .filter(|id| *id != player)
        .or_else(|| selected.0.and_then(nation_at).filter(|id| *id != player))
}

// ── Bottom-bar rebuild ───────────────────────────────────────────────────

pub fn update_diplomacy_bar(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    ui: Res<DiploUi>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    hovered: Res<HoveredHex>,
    selected: Res<SelectedHex>,
    index: Res<TileIndex>,
    mut commands: Commands,
    contents: Query<Entity, With<DiploBarContent>>,
    added: Query<(), Added<DiploBarContent>>,
    mut last_focus: Local<Option<Option<u32>>>,
) {
    let Ok(content) = contents.single() else {
        return;
    };
    let focus = focused_nation_id(&hovered, &selected, &vms, &index, meta.player_nation);
    let focus_changed = *last_focus != Some(focus);
    if !vms.is_changed() && !ui.is_changed() && added.is_empty() && !focus_changed {
        return;
    }
    *last_focus = Some(focus);

    commands.entity(content).despawn_children();
    let Some(screen) = vms.diplomacy_screen.as_ref() else {
        return;
    };
    let icons = icons.as_deref();
    let observer = meta.observer;
    let rel = focus.and_then(|id| screen.relation(id));

    commands.entity(content).with_children(|bar| {
        // ── Top row: standing + focused nation + queued banner ─────────
        bar.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(16.0),
            min_height: Val::Px(28.0),
            ..default()
        })
        .with_children(|row| {
            // Standing with the focused nation; hidden until one is
            // focused so an unlabeled bar never floats in the corner.
            if let Some(rel) = rel {
                let score = rel.score;
                let percent = ((score + 100) as f32 / 2.0).clamp(0.0, 100.0);
                let standing_color = if score > 20 {
                    STANDING_GREEN
                } else if score >= -20 {
                    STANDING_AMBER
                } else {
                    STANDING_RED
                };
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    min_width: Val::Px(220.0),
                    ..default()
                })
                .with_children(|group| {
                    group.spawn((
                        Text::new(format!("Standing with {}", rel.nation_name)),
                        theme.font(12.5),
                        TextColor(theme::TEXT_DIM),
                    ));
                    group
                        .spawn((
                            Node {
                                width: Val::Px(90.0),
                                height: Val::Px(8.0),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
                        ))
                        .with_children(|track| {
                            track.spawn((
                                Node {
                                    width: Val::Percent(percent),
                                    height: Val::Percent(100.0),
                                    border_radius: BorderRadius::all(Val::Px(4.0)),
                                    ..default()
                                },
                                BackgroundColor(standing_color),
                            ));
                        });
                    group.spawn((
                        Text::new(format!("{score}")),
                        theme.font_bold(13.0),
                        TextColor(standing_color),
                    ));
                });
            }

            // Focused nation card / hint.
            match rel {
                Some(rel) => spawn_nation_card(row, &theme, rel),
                None => {
                    row.spawn((
                        Text::new(
                            "Hover over a nation on the map, or click an action to queue it.",
                        ),
                        theme.font_italic(12.5),
                        TextColor(theme::TEXT_DIM),
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                    ));
                }
            }

            // Queued banner.
            if let Some(queued) = ui.queued.as_ref() {
                row.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(8.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(
                        218.0 / 255.0,
                        165.0 / 255.0,
                        32.0 / 255.0,
                        0.18,
                    )),
                    BorderColor::all(theme::GOLD),
                ))
                .with_children(|banner| {
                    spawn_icon(banner, icons, "diplomacy", queued.icon(), 15.0);
                    banner.spawn((
                        Text::new(queued.label()),
                        theme.font_bold(13.0),
                        TextColor(theme::GOLD),
                    ));
                    banner.spawn((
                        Text::new("— click a nation on the map"),
                        theme.font(12.0),
                        TextColor(theme::GOLD),
                    ));
                    let cancel = widgets::spawn_button(
                        banner,
                        &theme,
                        ButtonProps {
                            label: "✕".into(),
                            font_size: 12.0,
                            width: Some(Val::Px(26.0)),
                            ..default()
                        },
                    );
                    banner.commands().entity(cancel).insert((
                        DiploToggleButton::CancelQueued,
                        TooltipText("Cancel queued action (Esc)".into()),
                    ));
                });
            }
        });

        // ── Picker row (grant amounts / breakable treaties) ─────────────
        if ui.show_grant_picker {
            bar.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|picker| {
                picker.spawn((
                    Text::new("Grant amount:"),
                    theme.font(12.5),
                    TextColor(theme::TEXT_DIM),
                ));
                for amount in [500_i64, 1000, 2000, 5000] {
                    let button = widgets::spawn_button(
                        picker,
                        &theme,
                        ButtonProps {
                            label: format!("${amount}"),
                            font_size: 12.0,
                            ..default()
                        },
                    );
                    picker
                        .commands()
                        .entity(button)
                        .insert(DiploArmButton(QueuedDiplomacyAction::Grant { amount }));
                }
            });
        }
        if ui.show_break_picker {
            let breakable = rel
                .map(|r| r.actions.breakable_treaties.clone())
                .unwrap_or_default();
            bar.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|picker| {
                if breakable.is_empty() {
                    picker.spawn((
                        Text::new("Hover a nation with an active treaty to pick one to break."),
                        theme.font_italic(12.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                }
                for treaty in breakable {
                    let button = widgets::spawn_button(
                        picker,
                        &theme,
                        ButtonProps {
                            label: treaty.clone(),
                            font_size: 12.0,
                            ..default()
                        },
                    );
                    picker.commands().entity(button).insert(DiploArmButton(
                        QueuedDiplomacyAction::BreakTreaty {
                            treaty_type: treaty,
                        },
                    ));
                }
            });
        }

        // ── Action buttons row ──────────────────────────────────────────
        if !observer {
            let is_anarchy = rel.map(|r| r.is_in_anarchy).unwrap_or(false);
            let a = rel.map(|r| &r.actions);
            // Buttons stay disabled until a nation is focused, then enable
            // per that nation's eligibility (CC-3: tooltips say why not).
            let enabled = |can: Option<bool>| !is_anarchy && can.unwrap_or(false);
            let tooltip_for = |action: &QueuedDiplomacyAction| -> String {
                match invalid_target_reason(
                    action,
                    focus,
                    meta.player_nation,
                    vms.diplomacy_screen.as_ref(),
                ) {
                    Some(reason) => reason,
                    None => "Click to arm, then click the target nation on the map".into(),
                }
            };

            bar.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            })
            .with_children(|row| {
                let action_button = |row: &mut ChildSpawnerCommands,
                                     icon: &str,
                                     label: &str,
                                     enabled: bool,
                                     active: bool,
                                     tooltip: String,
                                     marker: DiploArmButton| {
                    let button = widgets::spawn_button(
                        row,
                        &theme,
                        ButtonProps {
                            label: if active {
                                format!("» {label}")
                            } else {
                                label.to_string()
                            },
                            font_size: 13.0,
                            enabled,
                            ..default()
                        },
                    );
                    row.commands()
                        .entity(button)
                        .insert((marker, TooltipText(tooltip)));
                    // Append the diplomacy icon inside the button.
                    row.commands().entity(button).with_children(|inner| {
                        spawn_icon(inner, icons, "diplomacy", icon, 15.0);
                    });
                    button
                };

                action_button(
                    row,
                    "Consulate",
                    "Consulate",
                    enabled(a.map(|a| a.can_build_consulate)),
                    ui.queued == Some(QueuedDiplomacyAction::Consulate),
                    tooltip_for(&QueuedDiplomacyAction::Consulate),
                    DiploArmButton(QueuedDiplomacyAction::Consulate),
                );
                action_button(
                    row,
                    "Embassy",
                    "Embassy",
                    enabled(a.map(|a| a.can_build_embassy)),
                    ui.queued == Some(QueuedDiplomacyAction::Embassy),
                    tooltip_for(&QueuedDiplomacyAction::Embassy),
                    DiploArmButton(QueuedDiplomacyAction::Embassy),
                );
                action_button(
                    row,
                    "NonAggressionPact",
                    "Propose NAP",
                    enabled(a.map(|a| a.can_propose_nap)),
                    ui.queued == Some(QueuedDiplomacyAction::Nap),
                    tooltip_for(&QueuedDiplomacyAction::Nap),
                    DiploArmButton(QueuedDiplomacyAction::Nap),
                );
                action_button(
                    row,
                    "Alliance",
                    "Propose Alliance",
                    enabled(a.map(|a| a.can_propose_alliance)),
                    ui.queued == Some(QueuedDiplomacyAction::Alliance),
                    tooltip_for(&QueuedDiplomacyAction::Alliance),
                    DiploArmButton(QueuedDiplomacyAction::Alliance),
                );
                action_button(
                    row,
                    "Peace",
                    "Propose Peace",
                    enabled(a.map(|a| a.can_propose_peace)),
                    ui.queued == Some(QueuedDiplomacyAction::Peace),
                    tooltip_for(&QueuedDiplomacyAction::Peace),
                    DiploArmButton(QueuedDiplomacyAction::Peace),
                );

                // Grant / Break Treaty open inline pickers.
                let grant_active = ui.show_grant_picker
                    || matches!(ui.queued, Some(QueuedDiplomacyAction::Grant { .. }));
                let grant = widgets::spawn_button(
                    row,
                    &theme,
                    ButtonProps {
                        label: if grant_active {
                            "» Send Grant".into()
                        } else {
                            "Send Grant".into()
                        },
                        font_size: 13.0,
                        enabled: enabled(a.map(|a| a.can_send_grant)),
                        ..default()
                    },
                );
                row.commands().entity(grant).insert((
                    DiploToggleButton::GrantPicker,
                    TooltipText(tooltip_for(&QueuedDiplomacyAction::Grant { amount: 500 })),
                ));
                row.commands().entity(grant).with_children(|inner| {
                    spawn_icon(inner, icons, "diplomacy", "Grant", 15.0);
                });

                let break_active = ui.show_break_picker
                    || matches!(ui.queued, Some(QueuedDiplomacyAction::BreakTreaty { .. }));
                let break_button = widgets::spawn_button(
                    row,
                    &theme,
                    ButtonProps {
                        label: if break_active {
                            "» Break Treaty".into()
                        } else {
                            "Break Treaty".into()
                        },
                        font_size: 13.0,
                        enabled: enabled(a.map(|a| a.can_break_treaty)),
                        ..default()
                    },
                );
                let break_tooltip = match (focus, a) {
                    (None, _) => tooltip_for(&QueuedDiplomacyAction::War),
                    (Some(_), Some(a)) if !a.can_break_treaty => rel
                        .map(|r| format!("No active treaty with {} to break.", r.nation_name))
                        .unwrap_or_else(|| "No treaty to break.".into()),
                    _ => "Pick which treaty to break".into(),
                };
                row.commands()
                    .entity(break_button)
                    .insert((DiploToggleButton::BreakPicker, TooltipText(break_tooltip)));
                row.commands().entity(break_button).with_children(|inner| {
                    spawn_icon(inner, icons, "diplomacy", "BreakTreaty", 15.0);
                });

                // Declare War: inline confirm.
                if ui.confirm_war {
                    row.spawn((
                        Text::new("Breaks all treaties. Sure?"),
                        theme.font(12.0),
                        TextColor(STANDING_RED),
                    ));
                    let confirm = widgets::spawn_button(
                        row,
                        &theme,
                        ButtonProps {
                            label: "Confirm War".into(),
                            font_size: 13.0,
                            ..default()
                        },
                    );
                    row.commands()
                        .entity(confirm)
                        .insert((DiploToggleButton::WarConfirm, BorderColor::all(WAR_RED)));
                    let cancel = widgets::spawn_button(
                        row,
                        &theme,
                        ButtonProps {
                            label: "Cancel".into(),
                            font_size: 13.0,
                            ..default()
                        },
                    );
                    row.commands()
                        .entity(cancel)
                        .insert(DiploToggleButton::WarCancel);
                } else {
                    let war = widgets::spawn_button(
                        row,
                        &theme,
                        ButtonProps {
                            label: if ui.queued == Some(QueuedDiplomacyAction::War) {
                                "» Declare War".into()
                            } else {
                                "Declare War".into()
                            },
                            font_size: 13.0,
                            enabled: enabled(a.map(|a| a.can_declare_war)),
                            ..default()
                        },
                    );
                    row.commands().entity(war).insert((
                        DiploToggleButton::WarConfirm,
                        TooltipText(tooltip_for(&QueuedDiplomacyAction::War)),
                    ));
                    row.commands().entity(war).with_children(|inner| {
                        spawn_icon(inner, icons, "diplomacy", "War", 15.0);
                    });
                }
            });
        } else {
            bar.spawn((
                Text::new("(observer — read only)"),
                theme.font_italic(12.0),
                TextColor(theme::TEXT_DIM),
                Node {
                    align_self: AlignSelf::Center,
                    ..default()
                },
            ));
        }
    });
}

/// The focused-nation info card in the bar's top row.
fn spawn_nation_card(row: &mut ChildSpawnerCommands, theme: &Theme, rel: &DiploScreenRelationVm) {
    row.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(10.0),
        flex_grow: 1.0,
        flex_wrap: FlexWrap::Wrap,
        ..default()
    })
    .with_children(|card| {
        card.spawn((
            Text::new(rel.nation_name.clone()),
            theme.font_bold(15.0),
            TextColor(theme::TEXT),
        ));
        // Status badge.
        let color = status_color(&rel.status);
        card.spawn((
            Node {
                padding: UiRect::axes(Val::Px(7.0), Val::Px(2.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(color.with_alpha(0.2)),
            BorderColor::all(color),
        ))
        .with_children(|badge| {
            badge.spawn((
                Text::new(rel.status.clone()),
                theme.font_bold(11.0),
                TextColor(color),
            ));
        });
        // Relationship score.
        let score_color = if rel.score > 0 {
            STANDING_GREEN
        } else if rel.score < 0 {
            STANDING_RED
        } else {
            Color::srgb_u8(0x88, 0x88, 0x88)
        };
        card.spawn((
            Text::new(if rel.score >= 0 {
                format!("+{}", rel.score)
            } else {
                format!("{}", rel.score)
            }),
            theme.font_bold(13.0),
            TextColor(score_color),
        ));
        // Treaty chips.
        for treaty in &rel.treaties {
            card.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(
                    218.0 / 255.0,
                    165.0 / 255.0,
                    32.0 / 255.0,
                    0.2,
                )),
            ))
            .with_children(|chip| {
                chip.spawn((
                    Text::new(treaty.clone()),
                    theme.font(11.0),
                    TextColor(theme::GOLD),
                ));
            });
        }
        // Embassy / consulate presence.
        if rel.has_embassy {
            card.spawn((
                Text::new("Embassy"),
                theme.font(11.0),
                TextColor(Color::srgb_u8(0xaa, 0xaa, 0xaa)),
            ));
        } else if rel.has_consulate {
            card.spawn((
                Text::new("Consulate"),
                theme.font(11.0),
                TextColor(Color::srgb_u8(0xaa, 0xaa, 0xaa)),
            ));
        }
        // Pending-action list.
        let mut pending: Vec<String> = Vec::new();
        if rel.has_pending_consulate {
            pending.push("Consulate".into());
        }
        if rel.has_pending_embassy {
            pending.push("Embassy".into());
        }
        if rel.has_pending_nap {
            pending.push("NAP".into());
        }
        if rel.has_pending_alliance {
            pending.push("Alliance".into());
        }
        if rel.has_pending_peace {
            pending.push("Peace".into());
        }
        if let Some(amount) = rel.pending_grant_amount_dollars {
            pending.push(format!("Grant ${amount}"));
        }
        for treaty in &rel.pending_break_treaties {
            pending.push(format!("Break {treaty}"));
        }
        if rel.has_pending_war {
            pending.push("War".into());
        }
        if !pending.is_empty() {
            card.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(
                    218.0 / 255.0,
                    165.0 / 255.0,
                    32.0 / 255.0,
                    0.25,
                )),
                BorderColor::all(theme::GOLD),
            ))
            .with_children(|chip| {
                chip.spawn((
                    Text::new(format!("⏳ Pending {}", pending.join(", "))),
                    theme.font(11.0),
                    TextColor(theme::GOLD),
                ));
            });
        }
        if rel.is_in_anarchy {
            card.spawn((
                Text::new("In anarchy"),
                theme.font_italic(11.0),
                TextColor(Color::srgb_u8(0xaa, 0x00, 0xaa)),
            ));
        }
    });
}

// ── Button handling ──────────────────────────────────────────────────────

pub fn handle_diplo_buttons(
    mut activations: MessageReader<ButtonActivated>,
    arm_buttons: Query<&DiploArmButton>,
    toggle_buttons: Query<&DiploToggleButton>,
    mut ui: ResMut<DiploUi>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(arm) = arm_buttons.get(*entity) {
            ui.queued = Some(arm.0.clone());
            ui.show_grant_picker = false;
            ui.show_break_picker = false;
            ui.confirm_war = false;
            continue;
        }
        let Ok(toggle) = toggle_buttons.get(*entity) else {
            continue;
        };
        match toggle {
            DiploToggleButton::GrantPicker => {
                ui.show_grant_picker = !ui.show_grant_picker;
                ui.show_break_picker = false;
                ui.confirm_war = false;
            }
            DiploToggleButton::BreakPicker => {
                ui.show_break_picker = !ui.show_break_picker;
                ui.show_grant_picker = false;
                ui.confirm_war = false;
            }
            DiploToggleButton::WarConfirm => {
                if ui.confirm_war {
                    ui.queued = Some(QueuedDiplomacyAction::War);
                    ui.confirm_war = false;
                } else {
                    ui.confirm_war = true;
                    ui.show_grant_picker = false;
                    ui.show_break_picker = false;
                }
            }
            DiploToggleButton::WarCancel => {
                ui.confirm_war = false;
            }
            DiploToggleButton::CancelQueued => {
                ui.queued = None;
            }
        }
    }
}

// ── Invalid-target reason tooltip ────────────────────────────────────────

pub fn update_reason_tooltip(
    meta: Res<GameMeta>,
    ui: Res<DiploUi>,
    vms: Res<ViewModels>,
    hovered: Res<HoveredHex>,
    index: Res<TileIndex>,
    windows: Query<&Window>,
    mut node: Query<(&mut Node, &mut Visibility), With<DiploReasonTooltip>>,
    mut text: Query<&mut Text, With<DiploReasonText>>,
) {
    let Ok((mut tooltip, mut visibility)) = node.single_mut() else {
        return;
    };
    let reason = ui.queued.as_ref().and_then(|action| {
        let coord = hovered.0?;
        let tiles = vms.map.as_ref()?;
        let tile = tiles.get(*index.by_coord.get(&coord)?)?;
        let target = (!tile.is_sea() && tile.nation_id >= 0).then_some(tile.nation_id as u32);
        invalid_target_reason(
            action,
            target,
            meta.player_nation,
            vms.diplomacy_screen.as_ref(),
        )
    });
    let Some(reason) = reason else {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if let Ok(mut label) = text.single_mut()
        && **label != reason
    {
        **label = reason;
    }
    let position = (cursor + Vec2::new(16.0, 20.0)).min(Vec2::new(
        (window.width() - 310.0).max(0.0),
        (window.height() - 60.0).max(0.0),
    ));
    tooltip.left = Val::Px(position.x);
    tooltip.top = Val::Px(position.y);
    if *visibility != Visibility::Visible {
        *visibility = Visibility::Visible;
    }
}
