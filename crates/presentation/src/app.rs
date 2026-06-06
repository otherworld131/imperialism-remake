use application::{Difficulty, new_game};
use bevy::prelude::*;

use crate::camera;
use crate::hex_renderer::{self, GameStateResource};
use crate::ui;

/// Run the Bevy graphical game.
pub fn run_game(map_key: &str, difficulty: Difficulty, nation_index: usize) {
    let game = new_game(map_key, difficulty, nation_index);
    let title = format!(
        "Imperialism Remake — {}",
        game.get_nation(game.human_player_nation)
            .map(|n| n.name.as_str())
            .unwrap_or("Unknown"),
    );

    println!("Starting Bevy app: {}", title);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title,
                resolution: bevy::window::WindowResolution::new(1280, 720),
                present_mode: bevy::window::PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(GameStateResource(game))
        .insert_resource(hex_renderer::SelectedTile::default())
        .insert_resource(hex_renderer::HoveredTile::default())
        .insert_resource(hex_renderer::MapModeState::default())
        .insert_resource(ui::TurnLog::default())
        .add_systems(
            Startup,
            (
                hex_renderer::render_hex_map,
                camera::setup_camera,
                ui::setup_hud,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                camera::camera_movement,
                hex_renderer::handle_map_mode_input,
                hex_renderer::update_tile_materials,
                hex_renderer::update_tile_highlights,
                ui::handle_tile_hover,
                ui::handle_tile_selection,
                ui::handle_input,
                ui::update_hud,
            ),
        )
        .run();
}
