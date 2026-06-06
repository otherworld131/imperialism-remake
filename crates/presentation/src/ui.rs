use application::{HexCoord, TurnReport, process_turn, queries::get_map_screen};
use bevy::prelude::*;

use crate::{
    camera::GameCamera,
    civilian_assets,
    hex_renderer::{
        GameStateResource, HEX_SIZE, HoveredTile, MapModeState, SelectedTile, hex_to_pixel,
    },
};

#[derive(Component)]
pub struct StatusDisplay;

#[derive(Component)]
pub struct ModeDisplay;

#[derive(Component)]
pub struct HoverDisplay;

#[derive(Component)]
pub struct InspectorDisplay;

#[derive(Component)]
pub struct TurnLogDisplay;

#[derive(Component)]
pub struct EndTurnButton;

#[derive(Resource)]
pub struct TurnLog {
    lines: Vec<String>,
}

impl Default for TurnLog {
    fn default() -> Self {
        Self {
            lines: vec!["Ready. Select a province or press Space to end the turn.".to_string()],
        }
    }
}

pub fn setup_hud(mut commands: Commands) {
    commands
        .spawn((Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },))
        .with_children(|root| {
            root.spawn((
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
                BackgroundColor(Color::srgba(0.025, 0.027, 0.024, 0.92)),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Text::new("Imperialism Remake"),
                    TextFont {
                        font_size: 17.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.96, 0.93, 0.84)),
                    StatusDisplay,
                ));

                bar.spawn((
                    Text::new("Political map"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.74, 0.77, 0.70)),
                    ModeDisplay,
                ));
            });

            root.spawn(panel_node(
                Val::Px(18.0),
                Val::Px(58.0),
                Val::Px(330.0),
                None,
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("No province selected"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.90, 0.88, 0.78)),
                    InspectorDisplay,
                ));
            });

            civilian_assets::spawn_civilian_asset_strip(root);

            root.spawn((
                Node {
                    width: Val::Px(350.0),
                    min_height: Val::Px(142.0),
                    position_type: PositionType::Absolute,
                    right: Val::Px(18.0),
                    top: Val::Px(58.0),
                    padding: UiRect::all(Val::Px(12.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.027, 0.024, 0.84)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Turn Log"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.98, 0.92, 0.70)),
                ));
                panel.spawn((
                    Text::new("Ready. Select a province or press Space to end the turn."),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.82, 0.84, 0.76)),
                    TurnLogDisplay,
                ));
            });

            root.spawn((
                Node {
                    width: Val::Auto,
                    height: Val::Px(42.0),
                    position_type: PositionType::Absolute,
                    left: Val::Px(18.0),
                    bottom: Val::Px(18.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    padding: UiRect::horizontal(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.027, 0.024, 0.84)),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Button,
                    Node {
                        width: Val::Px(104.0),
                        height: Val::Px(28.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgb(0.72, 0.61, 0.32)),
                    BackgroundColor(Color::srgb(0.18, 0.15, 0.08)),
                    EndTurnButton,
                ))
                .with_children(|button| {
                    button.spawn((
                        Text::new("End Turn"),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.96, 0.93, 0.84)),
                    ));
                });

                bar.spawn((
                    Text::new("Space: end turn   M/Tab: map mode   RMB/MMB: pan   Wheel: zoom"),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.72, 0.75, 0.68)),
                ));
            });

            root.spawn((
                Node {
                    width: Val::Px(330.0),
                    min_height: Val::Px(36.0),
                    position_type: PositionType::Absolute,
                    left: Val::Px(18.0),
                    bottom: Val::Px(70.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.027, 0.024, 0.72)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Hover over a tile"),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.78, 0.80, 0.72)),
                    HoverDisplay,
                ));
            });
        });
}

pub fn update_hud(
    game: Res<GameStateResource>,
    mode: Res<MapModeState>,
    selected: Res<SelectedTile>,
    log: Res<TurnLog>,
    mut text_queries: ParamSet<(
        Query<&mut Text, With<StatusDisplay>>,
        Query<&mut Text, With<ModeDisplay>>,
        Query<&mut Text, With<InspectorDisplay>>,
        Query<&mut Text, With<TurnLogDisplay>>,
    )>,
) {
    if game.is_changed() {
        if let Ok(screen) = get_map_screen(&game.0) {
            for mut text in &mut text_queries.p0() {
                **text = format!(
                    "{} | {} | {} | {} provinces | Army {} | Civilians {}",
                    screen.nation_name,
                    screen.turn,
                    screen.treasury,
                    screen.province_count,
                    screen.army_count,
                    screen.civilian_count
                );
            }
        }
    }

    if mode.is_changed() {
        for mut text in &mut text_queries.p1() {
            **text = format!("{} map | M/Tab to switch", mode.mode.label());
        }
    }

    if selected.is_changed() || game.is_changed() {
        let inspector_text = selected
            .coord
            .map(|coord| describe_tile(&game.0, coord))
            .unwrap_or_else(|| "No province selected".to_string());
        for mut text in &mut text_queries.p2() {
            **text = inspector_text.clone();
        }
    }

    if log.is_changed() {
        let joined = log.lines.join("\n");
        for mut text in &mut text_queries.p3() {
            **text = joined.clone();
        }
    }
}

pub fn handle_tile_hover(
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    game: Res<GameStateResource>,
    mut hovered: ResMut<HoveredTile>,
    mut hover_query: Query<&mut Text, With<HoverDisplay>>,
) {
    let coord = cursor_hex(&windows, &camera_query)
        .filter(|coord| game.0.world.hex_map.get_tile(*coord).is_some());

    if hovered.coord != coord {
        hovered.coord = coord;
    }

    if hovered.is_changed() {
        let hover_text = coord
            .map(|coord| compact_tile_label(&game.0, coord))
            .unwrap_or_else(|| "Hover over a tile".to_string());
        for mut text in &mut hover_query {
            **text = hover_text.clone();
        }
    }
}

pub fn handle_tile_selection(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    hovered: Res<HoveredTile>,
    mut selected: ResMut<SelectedTile>,
) {
    if mouse_buttons.just_pressed(MouseButton::Left) {
        selected.coord = hovered.coord;
    }
}

pub fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut game: ResMut<GameStateResource>,
    mut log: ResMut<TurnLog>,
    mut exit: MessageWriter<AppExit>,
    mut button_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<EndTurnButton>),
    >,
) {
    let mut should_end_turn = keys.just_pressed(KeyCode::Space);

    for (interaction, mut color) in &mut button_query {
        match *interaction {
            Interaction::Pressed => {
                should_end_turn = true;
                *color = BackgroundColor(Color::srgb(0.36, 0.28, 0.10));
            }
            Interaction::Hovered => {
                *color = BackgroundColor(Color::srgb(0.26, 0.21, 0.10));
            }
            Interaction::None => {
                *color = BackgroundColor(Color::srgb(0.18, 0.15, 0.08));
            }
        }
    }

    if should_end_turn {
        let report = process_turn(&mut game.0);
        log.lines = summarize_turn(&game.0, &report);
    }

    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

fn panel_node(left: Val, top: Val, width: Val, height: Option<Val>) -> impl Bundle {
    (
        Node {
            width,
            min_height: height.unwrap_or(Val::Px(156.0)),
            position_type: PositionType::Absolute,
            left,
            top,
            padding: UiRect::all(Val::Px(12.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.025, 0.027, 0.024, 0.84)),
    )
}

fn cursor_hex(
    windows: &Query<&Window>,
    camera_query: &Query<(&Camera, &GlobalTransform), With<GameCamera>>,
) -> Option<HexCoord> {
    let window = windows.single().ok()?;
    let (camera, camera_transform) = camera_query.single().ok()?;
    let cursor_pos = window.cursor_position()?;
    let world_pos = camera
        .viewport_to_world_2d(camera_transform, cursor_pos)
        .ok()?;
    Some(HexCoord::from_pixel(
        world_pos.x as f64,
        -world_pos.y as f64,
        HEX_SIZE as f64,
    ))
}

fn compact_tile_label(game: &application::GameState, coord: HexCoord) -> String {
    let Some(tile) = game.world.hex_map.get_tile(coord) else {
        return "Outside map".to_string();
    };
    let terrain = format!("{:?}", tile.terrain());
    let province = tile
        .province_id
        .and_then(|pid| game.get_province(pid))
        .map(|province| province.name.as_str())
        .unwrap_or("Unowned");
    format!("({}, {}) | {} | {}", coord.q, coord.r, province, terrain)
}

fn describe_tile(game: &application::GameState, coord: HexCoord) -> String {
    let Some(tile) = game.world.hex_map.get_tile(coord) else {
        return "No province selected".to_string();
    };

    let terrain = format!("{:?}", tile.terrain());
    let resource = tile
        .resource_deposit()
        .map(|resource| format!("{resource:?}"))
        .unwrap_or_else(|| "None".to_string());
    let province = tile
        .province_id
        .and_then(|pid| game.get_province(pid))
        .map(|province| {
            let owner = game
                .get_nation(province.owner)
                .map(|nation| nation.name.as_str())
                .unwrap_or("Unknown");
            format!("{} ({})", province.name, owner)
        })
        .unwrap_or_else(|| "Unowned".to_string());
    let pos = hex_to_pixel(coord.q, coord.r);

    let mut infra = Vec::new();
    if tile.infrastructure.has_railroad {
        infra.push("Railroad");
    }
    if tile.infrastructure.has_depot {
        infra.push("Depot");
    }
    if tile.infrastructure.has_port {
        infra.push("Port");
    }
    if tile.infrastructure.has_fort {
        infra.push("Fort");
    }
    let infrastructure = if infra.is_empty() {
        "None".to_string()
    } else {
        infra.join(", ")
    };

    format!(
        "Selected Tile\nHex: ({}, {})\nWorld: {:.0}, {:.0}\nProvince: {}\nTerrain: {}\nResource: {}\nImprovement: {}\nInfrastructure: {}",
        coord.q,
        coord.r,
        pos.x,
        pos.y,
        province,
        terrain,
        resource,
        tile.improvement_level(),
        infrastructure
    )
}

fn summarize_turn(game: &application::GameState, report: &TurnReport) -> Vec<String> {
    let mut lines = vec![format!("Advanced to {}", game.turn)];

    for headline in report
        .newspaper_headlines
        .iter()
        .filter(|headline| !headline.is_non_action)
        .take(4)
    {
        lines.push(headline.text.clone());
    }

    if lines.len() == 1 {
        lines.push(format!(
            "Production: {} resource entries, {} factory outputs",
            report.resource_production.len(),
            report.production_output.len()
        ));
        lines.push(format!(
            "Trade: {} transactions | Battles: {} land, {} naval",
            report.trade_transactions.len(),
            report.battles.len(),
            report.naval_battles.len()
        ));
    }

    lines.truncate(5);
    lines
}
