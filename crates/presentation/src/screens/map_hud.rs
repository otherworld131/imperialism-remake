//! Map-screen HUD: top bar (turn, map mode, End Turn), tile inspector,
//! and the busy overlay shown while a turn resolves.

use bevy::prelude::*;

use crate::game::commands::GameCommand;
use crate::game::resources::{TileIndex, TurnInfo, ViewModels};
use crate::game::vm::MapTile;
use crate::map::layers::MapMode;
use crate::map::picking::{PickingBlocker, SelectedHex};
use crate::theme;

#[derive(Component)]
pub struct TurnDisplay;

#[derive(Component)]
pub struct ModeDisplay;

#[derive(Component)]
pub struct InspectorDisplay;

#[derive(Component)]
pub struct EndTurnButton;

#[derive(Component)]
pub struct BusyOverlay;

pub fn setup_hud(mut commands: Commands) {
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
                        TextFont {
                            font_size: 17.0,
                            ..default()
                        },
                        TextColor(theme::GOLD),
                    ));
                    left.spawn((
                        Text::new("1815 Q1"),
                        TextFont {
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(theme::TEXT),
                        TurnDisplay,
                    ));
                    left.spawn((
                        Text::new("Political map (M/Tab)"),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.74, 0.77, 0.70)),
                        ModeDisplay,
                    ));
                });

            bar.spawn((
                Button,
                Node {
                    width: Val::Px(110.0),
                    height: Val::Px(30.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(theme::GOLD),
                BackgroundColor(theme::BUTTON_BG),
                EndTurnButton,
            ))
            .with_children(|button| {
                button.spawn((
                    Text::new("End Turn"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(theme::TEXT),
                ));
            });
        });

    // Tile inspector.
    commands
        .spawn((
            Node {
                width: Val::Px(300.0),
                position_type: PositionType::Absolute,
                left: Val::Px(14.0),
                top: Val::Px(56.0),
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG),
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Click a tile to inspect it"),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(theme::TEXT),
                InspectorDisplay,
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
                TextFont {
                    font_size: 26.0,
                    ..default()
                },
                TextColor(theme::GOLD),
            ));
        });
}

/// End-turn button: hover/pressed tints, sends [`GameCommand::EndTurn`].
pub fn end_turn_button(
    mut interactions: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<EndTurnButton>),
    >,
    mut commands_out: MessageWriter<GameCommand>,
) {
    for (interaction, mut color) in &mut interactions {
        match *interaction {
            Interaction::Pressed => {
                *color = BackgroundColor(theme::BUTTON_BG_PRESSED);
                commands_out.write(GameCommand::EndTurn);
            }
            Interaction::Hovered => *color = BackgroundColor(theme::BUTTON_BG_HOVER),
            Interaction::None => *color = BackgroundColor(theme::BUTTON_BG),
        }
    }
}

pub fn keyboard_commands(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands_out: MessageWriter<GameCommand>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Space) {
        commands_out.write(GameCommand::EndTurn);
    }
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
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

pub fn update_inspector(
    selected: Res<SelectedHex>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    mut texts: Query<&mut Text, With<InspectorDisplay>>,
) {
    if !selected.is_changed() && !vms.is_changed() {
        return;
    }
    let content = selected
        .0
        .and_then(|coord| {
            let tiles = vms.map.as_ref()?;
            let tile = tiles.get(*index.by_coord.get(&coord)?)?;
            Some(describe_tile(tile))
        })
        .unwrap_or_else(|| "Click a tile to inspect it".to_string());
    for mut text in &mut texts {
        **text = content.clone();
    }
}

fn describe_tile(tile: &MapTile) -> String {
    let owner = if tile.owner.is_empty() {
        "Unowned".to_string()
    } else if tile.is_minor {
        format!("{} (minor)", tile.owner)
    } else {
        tile.owner.clone()
    };
    let province = if tile.province.is_empty() {
        "—".to_string()
    } else {
        tile.province.clone()
    };
    let resource = match (&tile.resource, tile.resource_hidden) {
        (Some(r), false) => r.clone(),
        (Some(_), true) | (None, true) => "Unprospected".to_string(),
        (None, false) => "None".to_string(),
    };

    let mut infra = Vec::new();
    if tile.has_railroad {
        infra.push("Railroad");
    }
    if tile.has_depot {
        infra.push("Depot");
    }
    if tile.has_port {
        infra.push("Port");
    }
    if tile.has_fort {
        infra.push("Fort");
    }
    if tile.has_river {
        infra.push("River");
    }
    let infrastructure = if infra.is_empty() {
        "None".to_string()
    } else {
        infra.join(", ")
    };

    let mut lines = vec![
        format!("Hex ({}, {})", tile.q, tile.r),
        format!("Terrain: {}", tile.terrain),
        format!("Owner: {owner}"),
        format!("Province: {province}"),
        format!("Resource: {resource}"),
        format!(
            "Improvement: {}/{}",
            tile.improvement_level, tile.max_improvement_level
        ),
        format!("Infrastructure: {infrastructure}"),
    ];
    if tile.is_country_capital {
        lines.push("Country capital".to_string());
    } else if tile.is_capital {
        lines.push("Provincial capital".to_string());
    }
    if tile.army_unit_count > 0 {
        lines.push(format!(
            "Army: {} units ({:.0} firepower)",
            tile.army_unit_count, tile.army_firepower
        ));
    }
    lines.join("\n")
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
