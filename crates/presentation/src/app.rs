//! App wiring: window, resources, states, and system schedule.

use bevy::prelude::*;
use frontend_api::Session;

use crate::game::commands::{self, GameCommand};
use crate::game::refresh;
use crate::game::resources::{
    Blink, DataVersion, FleetTargets, MoveTargets, PendingMoves, PerspectiveNation, RenderSettings,
    SelectedCivilian, SelectedNavy, SessionRes, TileIndex, TurnInfo, ViewModels, tick_blink,
};
use crate::game::turn_runner::{self, ActiveTurn};
use crate::map::camera;
use crate::map::icons;
use crate::map::layers::{self, BordersCache, MapMode};
use crate::map::lod::{self, ZoomLod};
use crate::map::markers;
use crate::map::picking::{self, HoverTarget, HoveredHex, SelectedHex};
use crate::map::tooltip::{self, MapTooltipState};
use crate::screens::{gallery, map_hud, side_panel};
use crate::state::TurnPhase;
use crate::widgets::{self, WidgetsPlugin};

/// Debug hook: when `MAP_SCREENSHOT=<path>` is set, capture the primary
/// window after the map settles, then exit. `MAP_DEBUG_MODE` (a map-mode
/// label) and `MAP_DEBUG_ZOOM` (orthographic scale) tweak the captured view.
fn debug_screenshot(
    mut commands: Commands,
    mut frames: Local<u32>,
    mut mode: ResMut<MapMode>,
    mut settings: ResMut<RenderSettings>,
    mut camera: Query<&mut Projection, With<camera::GameCamera>>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let Ok(path) = std::env::var("MAP_SCREENSHOT") else {
        return;
    };
    *frames += 1;
    if *frames == 1 {
        if let Ok(label) = std::env::var("MAP_DEBUG_MODE")
            && let Some(target) = MapMode::ALL
                .iter()
                .find(|m| m.label().eq_ignore_ascii_case(&label))
        {
            *mode = *target;
        }
        if std::env::var("MAP_DEBUG_FOG").as_deref() == Ok("1") {
            settings.disable_fog = false;
        }
        if std::env::var("MAP_DEBUG_STRAIGHT").as_deref() == Ok("1") {
            settings.organic_borders = false;
        }
        if let Ok(zoom) = std::env::var("MAP_DEBUG_ZOOM")
            && let Ok(zoom) = zoom.parse::<f32>()
            && let Ok(mut projection) = camera.single_mut()
            && let Projection::Orthographic(ref mut ortho) = *projection
        {
            ortho.scale = zoom;
        }
    }
    if *frames == 150 {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    if *frames == 220 {
        exit.write(AppExit::Success);
    }
}

/// Run the Bevy game. M5 runs an observer game (every Great Power is
/// AI-driven) on the default map; nation choice and setup screens come in
/// later milestones.
pub fn run_game() {
    let game = frontend_api::setup::new_observer_game("imperialism", 2, 80, 50, 7, 16, "", "");
    let session = Session::from_game(game);
    // Debug widget gallery overlay (cheap; sits on top of the map).
    let widget_gallery = std::env::var("WIDGET_GALLERY").as_deref() == Ok("1");

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Imperialism Remake".to_string(),
                    resolution: bevy::window::WindowResolution::new(1280, 720),
                    present_mode: bevy::window::PresentMode::AutoNoVsync,
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                // Serve from the presentation crate's assets dir in dev
                // builds so icon sprites resolve regardless of the cwd.
                file_path: icons::asset_root().to_string_lossy().into_owned(),
                ..default()
            }),
    )
    .add_plugins(WidgetsPlugin);
    if widget_gallery {
        app.add_systems(Startup, gallery::setup_gallery)
            .add_systems(Update, gallery::gallery_interactions);
    }
    app.init_state::<TurnPhase>()
        .insert_resource(SessionRes(Some(session)))
        .insert_resource(DataVersion(1))
        .init_resource::<ViewModels>()
        .init_resource::<TileIndex>()
        .init_resource::<TurnInfo>()
        .init_resource::<ActiveTurn>()
        .init_resource::<MapMode>()
        .init_resource::<HoveredHex>()
        .init_resource::<SelectedHex>()
        .init_resource::<HoverTarget>()
        .init_resource::<SelectedNavy>()
        .init_resource::<SelectedCivilian>()
        .init_resource::<RenderSettings>()
        .init_resource::<PerspectiveNation>()
        .init_resource::<BordersCache>()
        .init_resource::<ZoomLod>()
        .init_resource::<Blink>()
        .init_resource::<PendingMoves>()
        .init_resource::<MoveTargets>()
        .init_resource::<FleetTargets>()
        .init_resource::<MapTooltipState>()
        .add_message::<GameCommand>()
        .add_systems(
            Startup,
            (
                camera::setup_camera,
                layers::setup_rings,
                icons::load_icons,
                map_hud::setup_hud,
                side_panel::setup_side_panel,
                tooltip::setup_map_tooltip,
            ),
        )
        .add_systems(
            Update,
            (
                (
                    refresh::refresh_view_models,
                    layers::rebuild_layers,
                    markers::rebuild_marker_layers,
                    layers::rebuild_highlight_layers,
                )
                    .chain(),
                camera::center_camera_when_map_ready,
                (lod::update_zoom_lod, lod::apply_lod_gates).chain(),
                layers::update_rings,
                tick_blink,
                markers::blink_selected_markers,
                debug_screenshot,
                turn_runner::poll_turn_task.run_if(in_state(TurnPhase::Processing)),
            ),
        )
        .add_systems(
            Update,
            (
                (camera::camera_movement, camera::wrap_camera).chain(),
                layers::handle_map_mode_input,
                (picking::pick_hover, picking::pick_select).chain(),
                tooltip::update_map_tooltip,
                (
                    map_hud::end_turn_button,
                    // Esc precedence: quit only sees the key press before the
                    // modal system pops the top modal with it.
                    map_hud::keyboard_commands.before(widgets::modal::esc_pops_top_modal),
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
                side_panel::handle_toggles,
                side_panel::handle_mode_dropdown,
                side_panel::sync_mode_dropdown,
                side_panel::update_selected_info,
                side_panel::update_legend,
                side_panel::update_nations,
            ),
        )
        .add_systems(
            OnEnter(TurnPhase::Processing),
            (map_hud::show_busy_overlay, map_hud::disable_end_turn),
        )
        .add_systems(
            OnExit(TurnPhase::Processing),
            (map_hud::hide_busy_overlay, map_hud::enable_end_turn),
        )
        .run();
}
