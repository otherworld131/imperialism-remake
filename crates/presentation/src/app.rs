//! App wiring: window, resources, states, and system schedule.

use bevy::prelude::*;
use frontend_api::Session;

use crate::game::commands::{self, GameCommand};
use crate::game::refresh;
use crate::game::resources::{
    Blink, DataVersion, DeployMode, EngineerPrompt, FleetTargets, GameMeta, MoveTargets,
    PendingMoveList, PendingMoves, PerspectiveNation, ProvinceUnits, RenderSettings,
    SelectedCivilian, SelectedNavy, SelectedShips, SelectedUnits, SessionRes, TileIndex, TurnInfo,
    ViewModels, tick_blink,
};
use crate::game::selection;
use crate::game::turn_runner::{self, ActiveTurn};
use crate::map::camera;
use crate::map::icons;
use crate::map::layers::{self, BordersCache, MapMode};
use crate::map::lod::{self, ZoomLod};
use crate::map::markers;
use crate::map::picking::{self, HoverTarget, HoveredHex, MapClick, SelectedHex};
use crate::map::tooltip::{self, MapTooltipState};
use crate::screens::{gallery, map_hud, panels, side_panel};
use crate::state::TurnPhase;
use crate::widgets::{self, WidgetsPlugin};

/// Debug hook: when `MAP_SCREENSHOT=<path>` is set, capture the primary
/// window after the map settles, then exit. `MAP_DEBUG_MODE` (a map-mode
/// label) and `MAP_DEBUG_ZOOM` (orthographic scale) tweak the captured view.
/// Frames only count while idle so async turn resolution never races the
/// capture.
fn debug_screenshot(
    mut commands: Commands,
    mut frames: Local<u32>,
    mut mode: ResMut<MapMode>,
    mut settings: ResMut<RenderSettings>,
    phase: Res<State<TurnPhase>>,
    mut camera: Query<&mut Projection, With<camera::GameCamera>>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let Ok(path) = std::env::var("MAP_SCREENSHOT") else {
        return;
    };
    if *phase.get() != TurnPhase::Idle {
        return;
    }
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

/// Debug driver for the M6 flows: `M6_DEBUG=<script>` replays a player
/// interaction through the real click path so screenshots show live state.
/// Scripts: `units`, `move`, `endturn`, `deploy[:CivilianType]`, `fleet`,
/// `fleetmove`.
fn m6_debug_driver(
    mut frames: Local<u32>,
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    move_targets: Res<MoveTargets>,
    fleet_targets: Res<FleetTargets>,
    phase: Res<State<TurnPhase>>,
    mut clicks: MessageWriter<MapClick>,
    mut game_commands: MessageWriter<GameCommand>,
    mut deploy: ResMut<DeployMode>,
    mut selected_navy: ResMut<SelectedNavy>,
    mut camera: Query<&mut Transform, With<camera::GameCamera>>,
) {
    let Ok(script) = std::env::var("M6_DEBUG") else {
        return;
    };
    if *phase.get() != TurnPhase::Idle {
        return;
    }
    let Some(tiles) = vms.map.as_ref() else {
        return;
    };
    *frames += 1;
    let (verb, arg) = match script.split_once(':') {
        Some((v, a)) => (v, Some(a)),
        None => (script.as_str(), None),
    };
    let player = i64::from(meta.player_nation);
    let capital = tiles
        .iter()
        .find(|t| t.is_country_capital && t.nation_id == player);

    let focus_camera =
        |camera: &mut Query<&mut Transform, With<camera::GameCamera>>, q: i32, r: i32| {
            if let Ok(mut transform) = camera.single_mut() {
                let world = crate::map::geometry::hex_to_world(q, r);
                transform.translation.x = world.x;
                transform.translation.y = world.y;
            }
        };

    if matches!(verb, "units" | "move" | "endturn")
        && *frames == 30
        && let Some(capital) = capital
    {
        focus_camera(&mut camera, capital.q, capital.r);
        clicks.write(MapClick(HoverTarget::Hex(capital.q, capital.r)));
    }
    if matches!(verb, "move" | "endturn")
        && *frames == 70
        && let Some(&pid) = move_targets.friendly.first()
        && let Some(tile) = tiles
            .iter()
            .find(|t| t.province_id == Some(pid) && t.is_capital)
            .or_else(|| tiles.iter().find(|t| t.province_id == Some(pid)))
    {
        clicks.write(MapClick(HoverTarget::Hex(tile.q, tile.r)));
    }
    if verb == "endturn" && *frames == 100 {
        game_commands.write(GameCommand::EndTurn);
    }
    if matches!(verb, "deploy" | "deploydo")
        && *frames == 30
        && let Some(civs) = vms.civilians.as_ref()
    {
        let pick = civs
            .undeployed
            .iter()
            .find(|c| arg.is_none_or(|w| c.civ_type == w))
            .or_else(|| civs.undeployed.first());
        if let Some(civ) = pick {
            deploy.0 = Some(selection::compute_deploy_state(
                civ.id,
                &civ.civ_type,
                None,
                tiles,
                meta.player_nation,
            ));
            if let Some(capital) = capital {
                focus_camera(&mut camera, capital.q, capital.r);
            }
        }
    }
    if verb == "deploydo"
        && *frames == 70
        && let Some(&(q, r)) = deploy.0.as_ref().and_then(|s| s.deployable.iter().min())
    {
        focus_camera(&mut camera, q, r);
        clicks.write(MapClick(HoverTarget::Hex(q, r)));
    }
    if matches!(verb, "fleet" | "fleetmove")
        && *frames == 30
        && let Some(marker) = vms
            .navy_markers
            .iter()
            .find(|m| m.kind == "fleet" && m.nation_id == player)
    {
        focus_camera(&mut camera, marker.q, marker.r);
        selected_navy.0 = Some(crate::map::navy::marker_key(marker));
    }
    if verb == "fleetmove"
        && *frames == 70
        && let Some(&(q, r)) = fleet_targets.0.iter().min()
    {
        clicks.write(MapClick(HoverTarget::Hex(q, r)));
    }
}

/// Run the Bevy game. By default this is an observer game (every Great
/// Power AI-driven); `HUMAN_GAME=1` starts a normal game with the human in
/// seat 0 so the unit/civilian/naval flows are live. Nation choice and
/// setup screens come in later milestones.
pub fn run_game() {
    let human = std::env::var("HUMAN_GAME").as_deref() == Ok("1");
    let game = if human {
        frontend_api::setup::new_game("imperialism", 2, 0, 80, 50, 7, 16, "", "", None)
    } else {
        frontend_api::setup::new_observer_game("imperialism", 2, 80, 50, 7, 16, "", "")
    };
    let session = Session::from_game(game);
    let meta = GameMeta {
        observer: session.observer_mode(),
        player_nation: session.human_nation(),
    };
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
        .insert_resource(meta)
        .insert_resource(PerspectiveNation(meta.player_nation))
        .insert_resource(RenderSettings {
            // Observer games watch the whole board; human games play under
            // fog of war like the web frontend.
            disable_fog: meta.observer,
            ..default()
        })
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
        .init_resource::<BordersCache>()
        .init_resource::<ZoomLod>()
        .init_resource::<Blink>()
        .init_resource::<PendingMoves>()
        .init_resource::<PendingMoveList>()
        .init_resource::<MoveTargets>()
        .init_resource::<FleetTargets>()
        .init_resource::<ProvinceUnits>()
        .init_resource::<SelectedUnits>()
        .init_resource::<SelectedShips>()
        .init_resource::<DeployMode>()
        .init_resource::<EngineerPrompt>()
        .init_resource::<MapTooltipState>()
        .add_message::<GameCommand>()
        .add_message::<MapClick>()
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
                m6_debug_driver,
                turn_runner::poll_turn_task.run_if(in_state(TurnPhase::Processing)),
            ),
        )
        .add_systems(
            Update,
            (
                (camera::camera_movement, camera::wrap_camera).chain(),
                layers::handle_map_mode_input,
                (
                    picking::pick_hover,
                    picking::pick_select,
                    selection::handle_map_click,
                )
                    .chain(),
                tooltip::update_map_tooltip,
                (
                    selection::sync_province_units,
                    selection::recompute_move_targets,
                    selection::sync_fleet_selection,
                    selection::sync_engineer_prompt,
                    selection::sync_pending_move_arrows,
                )
                    .chain(),
                (
                    map_hud::end_turn_button,
                    map_hud::keyboard_commands,
                    // Esc precedence: the cascade only sees the key press
                    // before the modal system pops the top modal with it.
                    selection::esc_cascade.before(widgets::modal::esc_pops_top_modal),
                    panels::handle_unit_checkboxes,
                    panels::handle_panel_buttons,
                    selection::handle_engineer_choice,
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
                panels::update_banners,
                panels::update_unit_panel,
                panels::update_civilian_panel,
                panels::update_naval_panel,
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
