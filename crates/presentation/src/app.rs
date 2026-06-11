//! App wiring: window, resources, states, and system schedule.

use bevy::prelude::*;
use frontend_api::Session;

use crate::game::commands::{self, GameCommand};
use crate::game::refresh;
use crate::game::resources::{DataVersion, SessionRes, TileIndex, TurnInfo, ViewModels};
use crate::game::turn_runner::{self, ActiveTurn};
use crate::map::camera;
use crate::map::layers::{self, MapMode};
use crate::map::picking::{self, HoveredHex, SelectedHex};
use crate::screens::map_hud;
use crate::state::TurnPhase;

/// Run the Bevy game. M2 starts an observer game (every Great Power is
/// AI-driven) on the default map; nation choice and setup screens come in
/// later milestones.
pub fn run_game() {
    let game = frontend_api::setup::new_observer_game("imperialism", 2, 80, 50, 7, 16, "", "");
    let session = Session::from_game(game);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Imperialism Remake".to_string(),
                resolution: bevy::window::WindowResolution::new(1280, 720),
                present_mode: bevy::window::PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .init_state::<TurnPhase>()
        .insert_resource(SessionRes(Some(session)))
        .insert_resource(DataVersion(1))
        .init_resource::<ViewModels>()
        .init_resource::<TileIndex>()
        .init_resource::<TurnInfo>()
        .init_resource::<ActiveTurn>()
        .init_resource::<MapMode>()
        .init_resource::<HoveredHex>()
        .init_resource::<SelectedHex>()
        .add_message::<GameCommand>()
        .add_systems(
            Startup,
            (
                camera::setup_camera,
                layers::setup_rings,
                map_hud::setup_hud,
            ),
        )
        .add_systems(
            Update,
            (
                (refresh::refresh_view_models, layers::rebuild_layers).chain(),
                camera::center_camera_when_map_ready,
                layers::apply_map_mode,
                layers::update_rings,
                turn_runner::poll_turn_task.run_if(in_state(TurnPhase::Processing)),
            ),
        )
        .add_systems(
            Update,
            (
                (camera::camera_movement, camera::wrap_camera).chain(),
                layers::handle_map_mode_input,
                (picking::pick_hover, picking::pick_select).chain(),
                (
                    map_hud::end_turn_button,
                    map_hud::keyboard_commands,
                    commands::apply_command,
                )
                    .chain(),
            )
                .run_if(in_state(TurnPhase::Idle)),
        )
        .add_systems(
            Update,
            (
                map_hud::update_turn_display,
                map_hud::update_mode_display,
                map_hud::update_inspector,
            ),
        )
        .add_systems(OnEnter(TurnPhase::Processing), map_hud::show_busy_overlay)
        .add_systems(OnExit(TurnPhase::Processing), map_hud::hide_busy_overlay)
        .run();
}
