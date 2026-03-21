use bevy::prelude::*;
use domain::game_state::new_game;
use domain::types::Difficulty;

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
                ..default()
            }),
            ..default()
        }))
        .insert_resource(GameStateResource(game))
        .add_systems(
            Startup,
            (
                camera::setup_camera,
                hex_renderer::render_hex_map,
                ui::setup_hud,
            ),
        )
        .add_systems(
            Update,
            (
                camera::camera_movement,
                ui::update_hud,
                ui::handle_tile_hover,
                ui::handle_input,
            ),
        )
        .run();
}
