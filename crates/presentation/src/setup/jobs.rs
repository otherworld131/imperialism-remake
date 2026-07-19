//! Async world generation / load jobs for the setup flow, and the session
//! install pipeline shared by Begin Campaign, Restart and Load.
//!
//! Generation reuses the turn runner's pattern: the work runs on the
//! [`AsyncComputeTaskPool`] while the app parks in `TurnPhase::Processing`
//! (busy overlay, input frozen); a poll system applies the outcome.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::futures_lite::future;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on};
use frontend_api::Session;

use super::{PreviewStage, SetupConfig, SetupStep, SetupUi};
use crate::game::resources::{
    CameraCentered, CurrentTurnNews, DataVersion, DeferredProposals, DeployMode, DiploUi,
    EngineerPrompt, FleetTargets, FreshRail, GameMeta, MoveTargets, NewsArchive, PendingMoveList,
    PendingMoves, PerspectiveNation, PrevLedger, ProposalPrompt, ProvinceUnits, RenderSettings,
    SelectedCivilian, SelectedNavy, SelectedShips, SelectedUnits, SessionRes, TurnInfo,
};
use crate::map::layers::{MapBounds, MapMode};
use crate::map::picking::{HoveredHex, SelectedHex};
use crate::state::{AppState, Screen, TurnPhase};
use crate::widgets::Toast;

/// What a finished setup job should do with its session.
#[derive(Debug, Clone, PartialEq)]
pub enum SetupJobKind {
    /// Show the world in the preview step.
    Preview,
    /// Enter the game (Begin Campaign / Restart / Load).
    Start(StartKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKind {
    Begin,
    Restart,
    Load,
}

pub struct SetupJobOutcome {
    pub kind: SetupJobKind,
    pub result: Result<Session, String>,
    /// Start params to stash for Restart (None for Load — the original
    /// generation params aren't in the save).
    pub config: Option<SetupConfig>,
}

#[derive(Resource, Default)]
pub struct ActiveSetupJob(pub Option<Task<SetupJobOutcome>>);

/// A session waiting to be installed as the live game (with every per-game
/// resource reset). Written by the job poll and the load-modal handler.
#[derive(Resource, Default)]
pub struct PendingSession(pub Option<PendingInstall>);

pub struct PendingInstall {
    pub session: Session,
    pub organic_borders: bool,
    pub hide_hex_grid: bool,
    pub config: Option<SetupConfig>,
}

/// Build a game for the preview step (always seat 0, non-observer ctor —
/// web `buildPreview` parity; the observer/seat choice is applied on Begin).
fn generate_preview(config: &SetupConfig) -> Result<Session, String> {
    let game = match &config.scenario {
        Some(id) => frontend_api::setup::new_scenario_game(
            id,
            config.difficulty,
            0,
            &config.flavor_key,
            None,
        )
        .map_err(|e| e.message())?,
        None => frontend_api::setup::new_game(
            config.effective_map_key(),
            config.difficulty,
            0,
            config.width,
            config.height,
            config.num_great_powers,
            config.num_minor_nations,
            &config.flavor_key,
            &config.terrain.to_json(),
            None,
        ),
    };
    Ok(Session::from_game(game))
}

/// Build the final campaign game from the config (web `handleBegin`).
fn generate_campaign(config: &SetupConfig) -> Result<Session, String> {
    let idx = config.picked_nation.unwrap_or(0);
    let game = if config.observer {
        let mut game = match &config.scenario {
            Some(id) => frontend_api::setup::new_observer_scenario_game(
                id,
                config.difficulty,
                &config.flavor_key,
            )
            .map_err(|e| e.message())?,
            None => frontend_api::setup::new_observer_game(
                config.effective_map_key(),
                config.difficulty,
                config.width,
                config.height,
                config.num_great_powers,
                config.num_minor_nations,
                &config.flavor_key,
                &config.terrain.to_json(),
            ),
        };
        if idx != 0 {
            frontend_api::setup::set_human_player(&mut game, idx).map_err(|e| e.message())?;
        }
        game
    } else {
        let capital = config
            .capital
            .map(|(q, r)| frontend_api::setup::hex_coord(q, r));
        match &config.scenario {
            Some(id) => frontend_api::setup::new_scenario_game(
                id,
                config.difficulty,
                idx,
                &config.flavor_key,
                capital,
            )
            .map_err(|e| e.message())?,
            None => frontend_api::setup::new_game(
                config.effective_map_key(),
                config.difficulty,
                idx,
                config.width,
                config.height,
                config.num_great_powers,
                config.num_minor_nations,
                &config.flavor_key,
                &config.terrain.to_json(),
                capital,
            ),
        }
    };
    Ok(Session::from_game(game))
}

fn spawn_job(
    active: &mut ActiveSetupJob,
    next_phase: &mut NextState<TurnPhase>,
    kind: SetupJobKind,
    config: Option<SetupConfig>,
    work: impl FnOnce() -> Result<Session, String> + Send + 'static,
) {
    if active.0.is_some() {
        return;
    }
    let task = AsyncComputeTaskPool::get().spawn(async move {
        SetupJobOutcome {
            result: work(),
            kind,
            config,
        }
    });
    active.0 = Some(task);
    next_phase.set(TurnPhase::Processing);
}

/// Generate (or regenerate) the preview world.
pub fn start_preview(
    active: &mut ActiveSetupJob,
    next_phase: &mut NextState<TurnPhase>,
    config: &SetupConfig,
) {
    let config = config.clone();
    spawn_job(active, next_phase, SetupJobKind::Preview, None, move || {
        generate_preview(&config)
    });
}

/// Generate the final campaign game (Begin Campaign).
pub fn start_begin(
    active: &mut ActiveSetupJob,
    next_phase: &mut NextState<TurnPhase>,
    config: &SetupConfig,
) {
    let config = config.clone();
    let stash = config.clone();
    spawn_job(
        active,
        next_phase,
        SetupJobKind::Start(StartKind::Begin),
        Some(stash),
        move || generate_campaign(&config),
    );
}

/// Rebuild the active game from its stored start params (Restart button).
pub fn start_restart(
    active: &mut ActiveSetupJob,
    next_phase: &mut NextState<TurnPhase>,
    config: &SetupConfig,
) {
    let config = config.clone();
    let stash = config.clone();
    spawn_job(
        active,
        next_phase,
        SetupJobKind::Start(StartKind::Restart),
        Some(stash),
        move || generate_campaign(&config),
    );
}

/// Load a native save off the main thread.
pub fn start_load(
    active: &mut ActiveSetupJob,
    next_phase: &mut NextState<TurnPhase>,
    path: PathBuf,
) {
    spawn_job(
        active,
        next_phase,
        SetupJobKind::Start(StartKind::Load),
        None,
        move || Session::load(&path).map_err(|e| e.message()),
    );
}

/// Poll the in-flight setup job and apply its outcome.
#[allow(clippy::too_many_arguments)]
pub fn poll_setup_job(
    mut commands: Commands,
    mut active: ResMut<ActiveSetupJob>,
    mut ui: ResMut<SetupUi>,
    config: Res<SetupConfig>,
    mut session_res: ResMut<SessionRes>,
    mut pending: ResMut<PendingSession>,
    mut data_version: ResMut<DataVersion>,
    mut settings: ResMut<RenderSettings>,
    mut mode: ResMut<MapMode>,
    mut centered: ResMut<CameraCentered>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
    mut toasts: MessageWriter<Toast>,
) {
    let Some(task) = active.0.as_mut() else {
        return;
    };
    let Some(outcome) = block_on(future::poll_once(task)) else {
        return;
    };
    active.0 = None;
    next_phase.set(TurnPhase::Idle);

    let session = match outcome.result {
        Ok(session) => session,
        Err(err) => {
            ui.error = Some(err.clone());
            ui.config_dirty = true;
            ui.preview_dirty = true;
            toasts.write(Toast::error(err));
            return;
        }
    };

    match outcome.kind {
        SetupJobKind::Preview => {
            ui.error = None;
            session_res.0 = Some(session);
            data_version.0 += 1;
            // Web preview flags: fog off, all hidden resources revealed; in
            // non-observer previews the provisional ownership markers are
            // stripped (the player places the real capital).
            settings.disable_fog = true;
            settings.preview_reveal_resources = true;
            settings.preview_hide_ownership = !config.observer;
            settings.organic_borders = config.organic_borders;
            settings.hide_hex_grid = config.hide_hex_grid;
            centered.0 = false;
            commands.remove_resource::<MapBounds>();
            // Entering from the config step starts at the terrain step
            // (scenarios have no terrain knobs and skip straight to the
            // country step); a regeneration (slider commit / Randomize /
            // Re-roll) keeps the player on their current step.
            if ui.step != SetupStep::Preview {
                ui.stage = if config.scenario.is_some() {
                    PreviewStage::Nation
                } else {
                    PreviewStage::Terrain
                };
            }
            // Each step owns its map mode: terrain edits are shown on the
            // terrain map, country choice on the political map (#528/#545).
            *mode = if ui.stage == PreviewStage::Terrain {
                MapMode::Terrain
            } else {
                MapMode::Political
            };
            ui.step = SetupStep::Preview;
            ui.hovered_capital = None;
            ui.picked_capital = None;
            ui.sidebar_hovered = None;
            ui.suggestions.clear();
            ui.suggestions_version = 0;
            ui.gps.clear(); // refilled from the fresh view models
            ui.preview_dirty = true;
            ui.config_dirty = true;
        }
        SetupJobKind::Start(_) => {
            pending.0 = Some(PendingInstall {
                session,
                organic_borders: outcome
                    .config
                    .as_ref()
                    .map(|c| c.organic_borders)
                    .unwrap_or(config.organic_borders),
                hide_hex_grid: outcome
                    .config
                    .as_ref()
                    .map(|c| c.hide_hex_grid)
                    .unwrap_or(config.hide_hex_grid),
                config: outcome.config,
            });
        }
    }
}

/// Resettable per-game resources, split in two because of the system-param
/// tuple limit.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ResetSelections<'w> {
    pub pending_moves: ResMut<'w, PendingMoves>,
    pub pending_move_list: ResMut<'w, PendingMoveList>,
    pub move_targets: ResMut<'w, MoveTargets>,
    pub fleet_targets: ResMut<'w, FleetTargets>,
    pub selected_units: ResMut<'w, SelectedUnits>,
    pub selected_ships: ResMut<'w, SelectedShips>,
    pub selected_civilian: ResMut<'w, SelectedCivilian>,
    pub selected_navy: ResMut<'w, SelectedNavy>,
    pub deploy: ResMut<'w, DeployMode>,
    pub engineer: ResMut<'w, EngineerPrompt>,
    pub hovered_hex: ResMut<'w, HoveredHex>,
    pub selected_hex: ResMut<'w, SelectedHex>,
    pub province_units: ResMut<'w, ProvinceUnits>,
    pub diplo: ResMut<'w, DiploUi>,
    pub fresh_rail: ResMut<'w, FreshRail>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ResetArchives<'w> {
    pub news: ResMut<'w, CurrentTurnNews>,
    pub news_archive: ResMut<'w, NewsArchive>,
    pub battle_archive: ResMut<'w, crate::screens::battles::BattleArchive>,
    pub deferred: ResMut<'w, DeferredProposals>,
    pub proposal_prompt: ResMut<'w, ProposalPrompt>,
    pub prev_ledger: ResMut<'w, PrevLedger>,
    pub news_ui: ResMut<'w, crate::screens::news::NewsUi>,
    pub battles_ui: ResMut<'w, crate::screens::battles::BattlesUi>,
}

/// Install a pending session as the live game: reset every per-game
/// resource, point the meta/perspective at the session's human seat, and
/// enter `InGame` on the map screen.
#[allow(clippy::too_many_arguments)]
pub fn apply_pending_session(
    mut commands: Commands,
    mut pending: ResMut<PendingSession>,
    mut session_res: ResMut<SessionRes>,
    mut data_version: ResMut<DataVersion>,
    mut meta: ResMut<GameMeta>,
    mut perspective: ResMut<PerspectiveNation>,
    mut turn_info: ResMut<TurnInfo>,
    mut settings: ResMut<RenderSettings>,
    mut mode: ResMut<MapMode>,
    mut centered: ResMut<CameraCentered>,
    mut active_config: ResMut<super::ActiveGameConfig>,
    mut selections: ResetSelections,
    mut archives: ResetArchives,
    app_state: Res<State<AppState>>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    let Some(install) = pending.0.take() else {
        return;
    };
    let session = install.session;

    meta.observer = session.observer_mode();
    meta.player_nation = session.human_nation();
    perspective.0 = meta.player_nation;

    let turn = session.game().turn;
    turn_info.label = format!("{turn}");
    turn_info.year = turn.year();

    *settings = RenderSettings {
        organic_borders: install.organic_borders,
        hide_hex_grid: install.hide_hex_grid,
        disable_fog: meta.observer,
        ..default()
    };
    *mode = MapMode::default();
    centered.0 = false;
    commands.remove_resource::<MapBounds>();

    // Loaded saves carry no generation knobs (size, terrain mix, scenario),
    // so a synthesized config would silently regenerate a DIFFERENT world.
    // Restart is only offered when the full start parameters are known.
    active_config.0 = install.config;

    // Clear interaction and archive state from any previous game.
    selections.pending_moves.0.clear();
    selections.pending_move_list.0.clear();
    selections.move_targets.friendly.clear();
    selections.move_targets.hostile.clear();
    selections.fleet_targets.0.clear();
    selections.selected_units.0.clear();
    selections.selected_ships.0.clear();
    selections.selected_civilian.0 = None;
    selections.selected_navy.0 = None;
    selections.deploy.0 = None;
    selections.engineer.0 = None;
    selections.hovered_hex.0 = None;
    selections.selected_hex.0 = None;
    *selections.province_units = ProvinceUnits::default();
    *selections.diplo = DiploUi::default();
    // A new/loaded world's existing rail must not read as freshly laid.
    *selections.fresh_rail = FreshRail::default();

    *archives.news = CurrentTurnNews::default();
    *archives.news_archive = NewsArchive::default();
    *archives.battle_archive = default();
    archives.deferred.0 = None;
    archives.proposal_prompt.0 = None;
    *archives.prev_ledger = PrevLedger::default();
    *archives.news_ui = default();
    *archives.battles_ui = default();

    session_res.0 = Some(session);
    data_version.0 += 1;
    if *app_state.get() != AppState::InGame {
        next_app.set(AppState::InGame);
    }
    next_screen.set(Screen::Map);
}
