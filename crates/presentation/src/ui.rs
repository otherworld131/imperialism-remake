use bevy::prelude::*;

use crate::hex_renderer::GameStateResource;

/// Marker for the HUD root node.
#[derive(Component)]
pub struct HudRoot;

/// Marker for the turn display text.
#[derive(Component)]
pub struct TurnDisplay;

/// Marker for the treasury display text.
#[derive(Component)]
pub struct TreasuryDisplay;

/// Marker for the nation name display.
#[derive(Component)]
pub struct NationDisplay;

/// Marker for the info panel text.
#[derive(Component)]
pub struct InfoPanel;

/// Set up the HUD overlay.
pub fn setup_hud(mut commands: Commands, game: Res<GameStateResource>) {
    let game_state = &game.0;
    let player = game_state
        .get_nation(game_state.human_player_nation)
        .unwrap();

    // Root UI node
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            HudRoot,
        ))
        .with_children(|parent| {
            // Top bar
            parent
                .spawn(Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(40.0),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::horizontal(Val::Px(10.0)),
                    ..default()
                })
                .insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)))
                .with_children(|bar| {
                    // Nation name
                    bar.spawn((
                        Text::new(format!(
                            "Empire of {} | {} | {}",
                            player.name, game_state.turn, player.treasury
                        )),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        NationDisplay,
                    ));

                    // Turn info
                    bar.spawn((
                        Text::new("[WASD] Pan  [Q/E] Zoom  [Space] End Turn  [Esc] Quit"),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.7, 0.7, 0.7)),
                    ));
                });

            // Bottom info panel
            parent
                .spawn(Node {
                    width: Val::Px(350.0),
                    min_height: Val::Px(120.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(10.0)),
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(10.0),
                    left: Val::Px(10.0),
                    ..default()
                })
                .insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Hover over a tile for info"),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
                        InfoPanel,
                    ));
                });
        });
}

/// Update the HUD each frame.
pub fn update_hud(
    game: Res<GameStateResource>,
    mut nation_query: Query<&mut Text, With<NationDisplay>>,
) {
    let game_state = &game.0;
    if let Some(player) = game_state.get_nation(game_state.human_player_nation) {
        for mut text in &mut nation_query {
            **text = format!(
                "Empire of {} | {} | {} | {} provinces",
                player.name,
                game_state.turn,
                player.treasury,
                player.province_count()
            );
        }
    }
}

/// Handle tile hover — show info in the bottom panel.
pub fn handle_tile_hover(
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<crate::camera::GameCamera>>,
    game: Res<GameStateResource>,
    mut info_query: Query<&mut Text, With<InfoPanel>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, cam_transform)) = camera_query.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor_pos) else {
        return;
    };

    // Convert pixel to hex coordinate
    let coord = domain::hex::HexCoord::from_pixel(
        world_pos.x as f64,
        -world_pos.y as f64, // flip Y back
        crate::hex_renderer::HEX_SIZE as f64,
    );

    let game_state = &game.0;
    if let Some(tile) = game_state.hex_map.get_tile(coord) {
        let terrain = format!("{:?}", tile.terrain());
        let yield_info = tile
            .calculate_yield()
            .map(|y| format!("{:?}: {}", y.resource, y.quantity))
            .unwrap_or_else(|| "No yield".to_string());

        let province_info = tile
            .province_id
            .and_then(|pid| game_state.get_province(pid))
            .map(|p| {
                let owner = game_state
                    .get_nation(p.owner)
                    .map(|n| n.name.as_str())
                    .unwrap_or("Unknown");
                format!("{} ({})", p.name, owner)
            })
            .unwrap_or_else(|| "Unowned".to_string());

        let infra = &tile.infrastructure;
        let mut infra_parts = Vec::new();
        if infra.has_railroad {
            infra_parts.push("Rail");
        }
        if infra.has_depot {
            infra_parts.push("Depot");
        }
        if infra.has_port {
            infra_parts.push("Port");
        }
        if infra.has_fort {
            infra_parts.push("Fort");
        }
        let infra_str = if infra_parts.is_empty() {
            "None".to_string()
        } else {
            infra_parts.join(", ")
        };

        let info = format!(
            "Tile ({}, {})\nTerrain: {}\nYield: {}\nProvince: {}\nLevel: {}\nInfra: {}",
            coord.q,
            coord.r,
            terrain,
            yield_info,
            province_info,
            tile.improvement_level(),
            infra_str
        );

        for mut text in &mut info_query {
            **text = info.clone();
        }
    }
}

/// Handle keyboard input for game actions.
pub fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut game: ResMut<GameStateResource>,
    mut exit: MessageWriter<AppExit>,
) {
    // Space = end turn
    if keys.just_pressed(KeyCode::Space) {
        let _report = domain::turn::process_turn(&mut game.0);
    }

    // Escape = quit
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
