//! App wiring: window, resources, states, and system schedule.

use bevy::prelude::*;
use frontend_api::Session;

use crate::game::commands::{self, GameCommand};
use crate::game::refresh;
use crate::game::resources::{
    Blink, CameraCentered, CurrentTurnNews, DataVersion, DeferredProposals, DeployMode, DiploUi,
    EngineerPrompt, FleetTargets, GameMeta, MoveTargets, NewsArchive, NewsDebugSettings,
    PendingMoveList, PendingMoves, PerspectiveNation, PrevLedger, ProposalPrompt, ProvinceUnits,
    QueuedDiplomacyAction, RenderSettings, SelectedCivilian, SelectedNavy, SelectedShips,
    SelectedUnits, SessionRes, TileIndex, TreatyMarkerIndex, TurnInfo, ViewModels, tick_blink,
};
use crate::game::selection;
use crate::game::turn_runner::{self, ActiveSkip, ActiveTurn, BusyProgress};
use crate::intro;
use crate::map::camera;
use crate::map::icons;
use crate::map::layers::{self, BordersCache, MapMode};
use crate::map::lod::{self, ZoomLod};
use crate::map::markers;
use crate::map::picking::{self, HoverTarget, HoveredHex, MapClick, SelectedHex};
use crate::map::tooltip::{self, MapTooltipState};
use crate::screens::{
    battles, diplomacy, gallery, industry, ledger, legend, map_hud, news, panels, proposals,
    saveload, side_panel, tech, trade, transport,
};
use crate::setup::{self, SetupAction, SetupConfig, SetupUi};
use crate::state::{AppState, Screen, TurnPhase, map_interactive};
use crate::widgets::{self, ButtonActivated, TabGroup, WidgetsPlugin};

/// Debug hook: when `MAP_SCREENSHOT=<path>` is set, capture the primary
/// window after the map settles, then exit. `MAP_DEBUG_MODE` (a map-mode
/// label) and `MAP_DEBUG_ZOOM` (orthographic scale) tweak the captured view;
/// `MAP_DEBUG_SKIP=<n>` fast-forwards n turns (no newspaper interstitials)
/// before the capture. Frames only count while idle so async turn
/// resolution never races the capture.
fn debug_screenshot(
    mut commands: Commands,
    mut frames: Local<u32>,
    mut mode: ResMut<MapMode>,
    mut settings: ResMut<RenderSettings>,
    phase: Res<State<TurnPhase>>,
    mut camera: Query<&mut Projection, With<camera::GameCamera>>,
    mut game_commands: MessageWriter<GameCommand>,
    screen: Res<State<Screen>>,
    mut next_screen: ResMut<NextState<Screen>>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let Ok(path) = std::env::var("MAP_SCREENSHOT") else {
        return;
    };
    if *phase.get() != TurnPhase::Idle {
        return;
    }
    // Skip mode wants the map: dismiss the newspaper the skip lands on.
    if std::env::var("MAP_DEBUG_SKIP").is_ok() && *screen.get() == Screen::News {
        next_screen.set(Screen::Map);
        return;
    }
    *frames += 1;
    // Re-applied every frame: the game session inserts fresh RenderSettings
    // after this hook's first frames tick.
    if std::env::var("MAP_DEBUG_AI_CIVS").as_deref() == Ok("1") && !settings.show_ai_civilians {
        settings.show_ai_civilians = true;
    }
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
    }
    // Fires after the M6/M7 drivers' scripted clicks (latest at frame 70) so
    // a queued order (e.g. a civilian deploy) resolves during the skip.
    if *frames == 100
        && let Ok(skip) = std::env::var("MAP_DEBUG_SKIP")
        && let Ok(count) = skip.parse::<u32>()
        && count > 0
    {
        game_commands.write(GameCommand::SkipTurns { count });
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
    // Long-running drivers (e.g. the M9 battle hunt) override the capture
    // frame via MAP_SCREENSHOT_FRAME.
    let capture_frame: u32 = std::env::var("MAP_SCREENSHOT_FRAME")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(150);
    if *frames == capture_frame {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    if *frames == capture_frame + 70 {
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

/// Debug driver for the M7 screens: `M7_DEBUG=<script>` switches screens and
/// replays interactions so `MAP_SCREENSHOT` captures live state.
/// Scripts: `industry`, `industrydrag`, `industryarms`, `transport`,
/// `transportfill`, `trade`, `tradebuy`, `tradehist`, `trademarket`,
/// `queue`, `queuetrade`, `queueendturn`.
fn m7_debug_driver(
    mut frames: Local<u32>,
    phase: Res<State<TurnPhase>>,
    mut next_screen: ResMut<NextState<Screen>>,
    mut game_commands: MessageWriter<GameCommand>,
    mut activations: MessageWriter<ButtonActivated>,
    buy_buttons: Query<Entity, With<trade::TradeBuyButton>>,
    autofill_buttons: Query<Entity, With<transport::AutoFillButton>>,
    mut tab_groups: Query<&mut TabGroup>,
    mut chain_sliders: Query<(
        &widgets::UiSlider,
        &mut widgets::UiSliderDrag,
        &industry::IndustryAction,
    )>,
) {
    let Ok(script) = std::env::var("M7_DEBUG") else {
        return;
    };
    if *phase.get() != TurnPhase::Idle {
        return;
    }
    *frames += 1;

    // Arms-unlocked Industry variant: resolve two turns so minor-nation
    // auto-buy stocks some arms, then open the screen (recruitment rows
    // expand once anything is recruitable).
    if script == "industryarms" {
        if *frames == 20 || *frames == 40 {
            game_commands.write(GameCommand::EndTurn);
        }
        if *frames == 70 {
            next_screen.set(Screen::Industry);
        }
        return;
    }

    if *frames == 20 {
        match script.as_str() {
            "industry" | "industrydrag" => next_screen.set(Screen::Industry),
            "transport" | "transportfill" => next_screen.set(Screen::Transport),
            "trade" | "tradebuy" | "tradehist" | "trademarket" => next_screen.set(Screen::Trade),
            "queue" | "queuetrade" | "queueendturn" => {
                // Queue pending state: a chain target and a sell order.
                game_commands.write(GameCommand::SetChainTarget {
                    chain: "timber",
                    step: "mill",
                    target: 2,
                });
                game_commands.write(GameCommand::SetSellOrder {
                    resource: "Grain".to_string(),
                    quantity: 3,
                });
            }
            _ => {}
        }
    }
    if *frames == 60 {
        match script.as_str() {
            "queue" | "queueendturn" => next_screen.set(Screen::Industry),
            "queuetrade" => next_screen.set(Screen::Trade),
            _ => {}
        }
    }
    if *frames == 80 {
        match script.as_str() {
            // Buffer a mid-drag value on the first chain slider so the
            // screenshot shows the drag-in-progress visuals.
            "industrydrag" => {
                if let Some((ui, mut drag, _)) = chain_sliders
                    .iter_mut()
                    .find(|(_, _, action)| matches!(action, industry::IndustryAction::Chain { .. }))
                {
                    drag.dragging = true;
                    drag.value = (ui.max / 2.0).round();
                }
            }
            "tradehist" => {
                for mut group in &mut tab_groups {
                    group.active = 1;
                }
            }
            "trademarket" => {
                for mut group in &mut tab_groups {
                    group.active = 2;
                }
            }
            "queueendturn" => {
                game_commands.write(GameCommand::EndTurn);
            }
            _ => {}
        }
    }
    if *frames == 100
        && script == "tradebuy"
        && let Some(button) = buy_buttons.iter().next()
    {
        activations.write(ButtonActivated(button));
    }
    if *frames == 100
        && script == "transportfill"
        && let Some(button) = autofill_buttons.iter().next()
    {
        activations.write(ButtonActivated(button));
    }
    // History scripts: run two turns so the archives have data, then open
    // the requested Trade tab. Frames only advance while idle, so the two
    // EndTurns resolve sequentially.
    if matches!(script.as_str(), "histdata" | "marketdata") {
        if *frames == 20 || *frames == 40 {
            game_commands.write(GameCommand::EndTurn);
        }
        if *frames == 70 {
            next_screen.set(Screen::Trade);
        }
        if *frames == 100 {
            for mut group in &mut tab_groups {
                group.active = if script == "histdata" { 1 } else { 2 };
            }
        }
    }
}

/// Debug driver for the M8 screens: `M8_DEBUG=<script>` switches screens and
/// replays interactions so `MAP_SCREENSHOT` captures live state.
/// Scripts: `diplomacy`, `diploselect`, `diploarm`, `diploqueue`, `diploendturn`,
/// `proposal`, `tech`, `techqueue`, `ledger`, `ledgerflow`.
fn m8_debug_driver(
    mut frames: Local<u32>,
    phase: Res<State<TurnPhase>>,
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    mut next_screen: ResMut<NextState<Screen>>,
    mut game_commands: MessageWriter<GameCommand>,
    mut clicks: MessageWriter<MapClick>,
    mut diplo: ResMut<DiploUi>,
    mut ledger_ui: ResMut<ledger::LedgerUi>,
    mut prompt: ResMut<ProposalPrompt>,
    mut tab_groups: Query<&mut TabGroup>,
) {
    let Ok(script) = std::env::var("M8_DEBUG") else {
        return;
    };
    if *phase.get() != TurnPhase::Idle {
        return;
    }
    *frames += 1;

    // A minor-nation capital the player can target with a consulate.
    let consulate_target = || -> Option<(i32, i32)> {
        let screen = vms.diplomacy_screen.as_ref()?;
        let tiles = vms.map.as_ref()?;
        let target = screen
            .relations
            .iter()
            .find(|r| r.nation_type == "MinorNation" && r.actions.can_build_consulate)?;
        tiles
            .iter()
            .find(|t| t.is_country_capital && t.nation_id == i64::from(target.nation_id))
            .map(|t| (t.q, t.r))
    };

    if *frames == 20 {
        match script.as_str() {
            "diplomacy" | "diploselect" | "diploarm" | "diploqueue" | "diploendturn" => {
                next_screen.set(Screen::Diplomacy);
            }
            "tech" => next_screen.set(Screen::Tech),
            "techqueue" => {
                if let Some(tech) = vms.tech.as_ref()
                    && let Some(entry) = tech.available.iter().find(|t| t.cost <= tech.treasury)
                {
                    game_commands.write(GameCommand::QueueTechResearch {
                        name: entry.name.clone(),
                    });
                }
            }
            // One end turn first so cash flow + delta baselines exist.
            "ledger" | "ledgerflow" => {
                game_commands.write(GameCommand::EndTurn);
            }
            "proposal" => {
                // Synthetic prompt: proposals come from AI turns and aren't
                // deterministic in a 1-turn screenshot run.
                prompt.0 = serde_json::from_value(serde_json::json!({
                    "proposals": [
                        {
                            "index": 0,
                            "from_nation_id": 1,
                            "from_nation_name": "Shenia",
                            "from_nation_color": "Orange",
                            "proposal_type": "NonAggressionPact",
                            "display_text": "Shenia proposes a Non-Aggression Pact",
                            "turn_proposed": 1,
                            "turns_until_expiry": 3,
                        },
                        {
                            "index": 1,
                            "from_nation_id": 2,
                            "from_nation_name": "Gringrinlaria",
                            "from_nation_color": "LightBlue",
                            "proposal_type": "Alliance",
                            "display_text": "Gringrinlaria proposes an Alliance",
                            "turn_proposed": 1,
                            "turns_until_expiry": 1,
                        }
                    ]
                }))
                .ok();
            }
            _ => {}
        }
    }
    if *frames == 50 {
        match script.as_str() {
            // Select (pin) a foreign Great Power so the bar shows the
            // labeled standing + per-eligibility buttons.
            "diploselect" => {
                if let Some(tiles) = vms.map.as_ref()
                    && let Some(tile) = tiles.iter().find(|t| {
                        t.is_country_capital
                            && !t.is_minor
                            && t.nation_id >= 0
                            && t.nation_id != i64::from(meta.player_nation)
                    })
                {
                    clicks.write(MapClick(HoverTarget::Hex(tile.q, tile.r)));
                }
            }
            "diploarm" | "diploqueue" | "diploendturn" => {
                diplo.queued = Some(QueuedDiplomacyAction::Consulate);
            }
            "techqueue" => next_screen.set(Screen::Tech),
            "ledger" | "ledgerflow" => next_screen.set(Screen::Ledger),
            _ => {}
        }
    }
    if *frames == 80 {
        match script.as_str() {
            "diploqueue" | "diploendturn" => {
                if let Some((q, r)) = consulate_target() {
                    clicks.write(MapClick(HoverTarget::Hex(q, r)));
                }
            }
            "ledger" => ledger_ui.expanded = Some(meta.player_nation),
            "ledgerflow" => {
                // Cash-flow tab, human row expanded.
                for mut group in &mut tab_groups {
                    group.active = 1;
                }
                ledger_ui.expanded = Some(meta.player_nation);
            }
            _ => {}
        }
    }
    if *frames == 110 && script == "diploendturn" {
        game_commands.write(GameCommand::EndTurn);
    }
}

/// Debug driver for the M9 screens: `M9_DEBUG=<script>` ends turns and
/// switches screens so `MAP_SCREENSHOT` captures live state. Scripts:
/// `news` (end turn → newspaper interstitial), `newsproposal` (end turn →
/// newspaper → dismiss → proposal modal), `newsarchive` (two turns →
/// Archive tab → turn selected → Show Map modal), `battles` /
/// `battlesdebug` (fast-forwards turns until the battle archive has
/// entries, then opens the battle screen's Archive tab), `legend` /
/// `legendflags` (scrolled to the nation flags).
fn m9_debug_driver(
    mut frames: Local<u32>,
    phase: Res<State<TurnPhase>>,
    mut session: ResMut<SessionRes>,
    mut data_version: ResMut<DataVersion>,
    mut turn_info: ResMut<TurnInfo>,
    mut next_screen: ResMut<NextState<Screen>>,
    mut game_commands: MessageWriter<GameCommand>,
    mut activations: MessageWriter<ButtonActivated>,
    mut news_debug: ResMut<NewsDebugSettings>,
    news_tabs: Query<(Entity, &news::NewsModeTab)>,
    news_turns: Query<(Entity, &news::NewsArchiveTurnButton)>,
    show_map: Query<Entity, With<news::NewsShowMapButton>>,
    mut battles_ui: ResMut<battles::BattlesUi>,
    battle_archive: Res<battles::BattleArchive>,
    mut scroll_areas: Query<&mut bevy::ui::ScrollPosition, With<widgets::UiScrollArea>>,
    mut deferred: ResMut<DeferredProposals>,
) {
    let Ok(script) = std::env::var("M9_DEBUG") else {
        return;
    };
    if *phase.get() != TurnPhase::Idle {
        return;
    }
    *frames += 1;

    match script.as_str() {
        "news" => {
            if *frames == 20 {
                game_commands.write(GameCommand::EndTurn);
            }
        }
        // End-turn flow order proof: turn resolves → newspaper opens; a
        // (synthetic) proposal batch is parked behind it; dismissing the
        // paper returns to the map and the proposal modal opens.
        "newsproposal" => {
            if *frames == 20 {
                game_commands.write(GameCommand::EndTurn);
            }
            if *frames == 60 {
                deferred.0 = serde_json::from_value(serde_json::json!({
                    "proposals": [{
                        "index": 0,
                        "from_nation_id": 1,
                        "from_nation_name": "Shenia",
                        "from_nation_color": "Orange",
                        "proposal_type": "NonAggressionPact",
                        "display_text": "Shenia proposes a Non-Aggression Pact",
                        "turn_proposed": 1,
                        "turns_until_expiry": 3,
                    }]
                }))
                .ok();
            }
            if *frames == 90 {
                // Dismiss the newspaper (Space/Esc equivalent).
                next_screen.set(Screen::Map);
            }
        }
        "newsarchive" => {
            // Two turns of history, then Archive tab → newest turn →
            // political-map modal.
            if *frames == 20 || *frames == 40 {
                game_commands.write(GameCommand::EndTurn);
            }
            if *frames == 80
                && let Some((tab, _)) = news_tabs.iter().find(|(_, tab)| tab.0)
            {
                activations.write(ButtonActivated(tab));
            }
            if *frames == 100
                && let Some((button, _)) = news_turns.iter().max_by_key(|(_, turn)| turn.0)
            {
                activations.write(ButtonActivated(button));
            }
            if *frames == 120
                && let Some(button) = show_map.iter().next()
            {
                activations.write(ButtonActivated(button));
            }
        }
        "battles" | "battlesdebug" => {
            if *frames == 1 && script == "battlesdebug" {
                news_debug.show_battle_firepower = true;
                news_debug.show_retreat_debug = true;
            }
            // Fast-forward turns synchronously (blocking is fine for a
            // capture run) until the battle archive has entries — battles
            // appear within ~10–30 turns once AIs go to war — then open
            // the battle screen and (later) its Archive tab.
            if *frames == 10 {
                if let Some(session) = session.0.as_mut() {
                    let mut turns = 0;
                    loop {
                        let _ = frontend_api::turn::process_turns(session.game_mut(), 10);
                        turns += 10;
                        let has_battles = frontend_api::battles::get_battle_data(session.game())
                            .ok()
                            .and_then(|v| v.as_array().map(|a| !a.is_empty()))
                            .unwrap_or(false);
                        if has_battles || turns >= 150 {
                            break;
                        }
                    }
                    let turn = session.game().turn;
                    turn_info.label = format!("{turn}");
                    turn_info.year = turn.year();
                    data_version.0 += 1;
                }
                next_screen.set(Screen::Battles);
            }
            if *frames == 60 {
                // Same effect as clicking the Archive tab.
                battles_ui.archive_mode = true;
                battles_ui.selected = 0;
                if battles_ui.selected_turn.is_none() {
                    battles_ui.selected_turn = battle_archive.turns.iter().map(|t| t.turn).max();
                }
            }
        }
        "legend" | "legendflags" => {
            if *frames == 20 {
                next_screen.set(Screen::Legend);
            }
            if script == "legendflags" && *frames == 60 {
                for mut position in &mut scroll_areas {
                    position.y = 100_000.0;
                }
            }
        }
        _ => {}
    }
}

/// Grouped parameters for [`m10_debug_driver`] (system-param limit).
#[derive(bevy::ecs::system::SystemParam)]
struct M10Driver<'w, 's> {
    commands: Commands<'w, 's>,
    theme: Res<'w, crate::theme::Theme>,
    modal_stack: ResMut<'w, widgets::ModalStack>,
    config: ResMut<'w, SetupConfig>,
    ui: ResMut<'w, SetupUi>,
    session: Res<'w, SessionRes>,
    actions: MessageWriter<'w, SetupAction>,
    game_commands: MessageWriter<'w, GameCommand>,
    active_skip: Res<'w, ActiveSkip>,
    active_job: ResMut<'w, setup::jobs::ActiveSetupJob>,
    next_phase: ResMut<'w, NextState<TurnPhase>>,
    toasts: MessageWriter<'w, widgets::Toast>,
    exit: MessageWriter<'w, AppExit>,
    load_rows: Query<'w, 's, (Entity, &'static saveload::LoadSaveBtn)>,
    restart_rows: Query<'w, 's, Entity, With<saveload::RestartConfirmBtn>>,
    save_rows: Query<'w, 's, Entity, With<saveload::SaveConfirmBtn>>,
    overwrite_rows: Query<'w, 's, Entity, With<saveload::OverwriteConfirmBtn>>,
    activations: MessageWriter<'w, ButtonActivated>,
}

/// Debug driver for the M10 setup flow: `M10_DEBUG=<script>` drives the
/// real setup actions (button paths) so screenshots and the end-to-end
/// check exercise live code. Scripts:
/// - `config` — config step as booted.
/// - `preview` — generate and show the preview step.
/// - `capital` — non-observer flow into capital placement (yield preview +
///   suggestions visible).
/// - `save` — begin an observer game, open the Save modal.
/// - `load` — begin, write two saves, open the Load modal.
/// - `loadcli` — loads CLI-written saves (`save_1815_Q1.json` / `.bin`)
///   through the real Load-modal buttons; prints `M10_LOADCLI OK/FAIL`.
/// - `restart` — begin → end turn → confirm Restart → same seed at turn 1;
///   prints `M10_RESTART OK/FAIL`.
/// - `e2e` — setup → preview → re-roll names → place capital → begin →
///   two turns → save → load → verify; prints `M10_E2E OK/FAIL` and exits.
/// - `skip` — begin observer; Skip 5, Skip Until, cancel mid-skip; prints
///   `M10_SKIP OK/FAIL` and exits.
fn m10_debug_driver(
    mut frames: Local<u32>,
    mut step: Local<u32>,
    mut stash: Local<Vec<String>>,
    phase: Res<State<TurnPhase>>,
    app_state: Res<State<AppState>>,
    mut p: M10Driver,
) {
    let Ok(script) = std::env::var("M10_DEBUG") else {
        return;
    };
    let idle = *phase.get() == TurnPhase::Idle;
    // The cancel step is the one place the driver acts mid-Processing (it
    // pokes the running skip's cancel flag, like the overlay button).
    if !idle {
        if script == "skip"
            && *step == 5
            && let Some(run) = p.active_skip.0.as_ref()
        {
            run.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            if !stash.iter().any(|s| s == "cancel-requested") {
                stash.push("cancel-requested".to_string());
            }
        }
        return;
    }
    *frames += 1;
    let in_game = *app_state.get() == AppState::InGame;
    let preview_ready =
        p.ui.step == setup::SetupStep::Preview && p.session.0.is_some() && !p.ui.gps.is_empty();
    let turn = p.session.0.as_ref().map(|s| s.turn_number()).unwrap_or(0);
    let fail = |exit: &mut MessageWriter<AppExit>, tag: &str, message: &str| {
        println!("{tag} FAIL: {message}");
        exit.write(AppExit::error());
    };
    if *frames > 4000 {
        fail(&mut p.exit, "M10", "timed out");
        return;
    }

    match script.as_str() {
        "config" => {}
        "preview" => {
            if *frames == 20 {
                p.actions.write(SetupAction::PreviewMap);
            }
        }
        "capital" => match *step {
            0 => {
                p.config.observer = false;
                p.ui.config_dirty = true;
                *step = 1;
            }
            1 if *frames >= 10 => {
                p.actions.write(SetupAction::PreviewMap);
                *step = 2;
            }
            2 if preview_ready => {
                p.actions.write(SetupAction::PickNation(0));
                *step = 3;
            }
            3 if p.config.picked_nation == Some(0) => {
                p.actions.write(SetupAction::EnterCapitalStage);
                *step = 4;
            }
            4 if !p.ui.suggestions.is_empty() => {
                p.actions.write(SetupAction::PickSuggestion(0));
                *step = 5;
            }
            _ => {}
        },
        "save" | "load" => match *step {
            0 => {
                p.actions.write(SetupAction::PreviewMap);
                *step = 1;
            }
            1 if preview_ready => {
                p.actions.write(SetupAction::BeginCampaign);
                *step = 2;
            }
            2 if in_game => {
                if script == "load" {
                    saveload::write_save(&p.session, "alpha-1815.json.gz", &mut p.toasts);
                    saveload::write_save(&p.session, "beta-1815.json.gz", &mut p.toasts);
                    saveload::open_load_modal(&mut p.commands, &mut p.modal_stack, &p.theme);
                } else {
                    let name = saveload::default_save_name(&p.session);
                    saveload::open_save_modal(&mut p.commands, &mut p.modal_stack, &p.theme, &name);
                }
                *step = 3;
            }
            _ => {}
        },
        // CLI-compat proof: requires `saves/save_1815_Q1.json` + `.bin`
        // written by the CLI binary. Advances one turn, then loads each CLI
        // save through the real Load-modal button path and verifies the
        // session rolled back to turn 1.
        "loadcli" => match *step {
            0 => {
                p.actions.write(SetupAction::PreviewMap);
                *step = 1;
            }
            1 if preview_ready => {
                p.actions.write(SetupAction::BeginCampaign);
                *step = 2;
            }
            2 if in_game => {
                p.game_commands.write(GameCommand::EndTurn);
                *step = 3;
            }
            3 | 6 if turn == 2 => {
                saveload::open_load_modal(&mut p.commands, &mut p.modal_stack, &p.theme);
                *step += 1;
            }
            4 | 7 => {
                let want = if *step == 4 {
                    "save_1815_Q1.json"
                } else {
                    "save_1815_Q1.bin"
                };
                let Some((entity, _)) = p
                    .load_rows
                    .iter()
                    .find(|(_, row)| row.path.file_name().is_some_and(|n| n == want))
                else {
                    if *frames > 600 {
                        fail(&mut p.exit, "M10_LOADCLI", &format!("{want} not listed"));
                    }
                    return;
                };
                p.activations.write(ButtonActivated(entity));
                *step += 1;
            }
            5 if turn == 1 => {
                println!("M10_LOADCLI json OK: CLI save loaded, back at turn 1");
                p.game_commands.write(GameCommand::EndTurn);
                *step = 6;
            }
            8 if turn == 1 => {
                println!("M10_LOADCLI bin OK: CLI binary save loaded, back at turn 1");
                println!("M10_LOADCLI OK");
                p.exit.write(AppExit::Success);
                *step = 9;
            }
            _ => {}
        },
        // Setup-screen load proof: the config step's "Load Save" button
        // loads straight into the game (needs `saves/m10-e2e.json.gz` from
        // an earlier `e2e` run).
        "loadsetup" => {
            match *step {
                0 if *frames >= 10 => {
                    p.actions.write(SetupAction::OpenLoadModal);
                    *step = 1;
                }
                1 => {
                    if let Some((entity, _)) = p.load_rows.iter().find(|(_, row)| {
                        row.path.file_name().is_some_and(|n| n == "m10-e2e.json.gz")
                    }) {
                        p.activations.write(ButtonActivated(entity));
                        *step = 2;
                    } else if *frames > 600 {
                        fail(&mut p.exit, "M10_LOADSETUP", "m10-e2e.json.gz not listed");
                    }
                }
                2 if in_game && turn == 3 => {
                    println!("M10_LOADSETUP OK: loaded from the setup screen, turn 3");
                    p.exit.write(AppExit::Success);
                    *step = 3;
                }
                _ => {}
            }
        }
        // Overwrite proof: saving onto an existing file must raise the
        // confirm modal, and confirming must write and close everything.
        "overwrite" => match *step {
            0 => {
                p.actions.write(SetupAction::PreviewMap);
                *step = 1;
            }
            1 if preview_ready => {
                p.actions.write(SetupAction::BeginCampaign);
                *step = 2;
            }
            2 if in_game => {
                saveload::write_save(&p.session, "dup.json.gz", &mut p.toasts);
                saveload::open_save_modal(&mut p.commands, &mut p.modal_stack, &p.theme, "dup");
                *step = 3;
            }
            3 => {
                if let Some(entity) = p.save_rows.iter().next() {
                    p.activations.write(ButtonActivated(entity));
                    *step = 4;
                }
            }
            4 => {
                if let Some(entity) = p.overwrite_rows.iter().next() {
                    println!("M10_OVERWRITE confirm modal appeared");
                    p.activations.write(ButtonActivated(entity));
                    *step = 5;
                } else if *frames > 600 {
                    fail(&mut p.exit, "M10_OVERWRITE", "confirm modal never appeared");
                }
            }
            5 if p.save_rows.is_empty() && p.overwrite_rows.is_empty() => {
                if saveload::saves_dir().join("dup.json.gz").exists() {
                    println!("M10_OVERWRITE OK: confirmed overwrite, modals closed");
                    p.exit.write(AppExit::Success);
                } else {
                    fail(&mut p.exit, "M10_OVERWRITE", "file missing after overwrite");
                }
                *step = 6;
            }
            _ => {}
        },
        // Restart proof: begin → end a turn → confirm Restart → the same
        // seed rebuilds at turn 1.
        "restart" => match *step {
            0 => {
                p.actions.write(SetupAction::PreviewMap);
                *step = 1;
            }
            1 if preview_ready => {
                p.actions.write(SetupAction::BeginCampaign);
                *step = 2;
            }
            2 if in_game => {
                stash.push(
                    p.session
                        .0
                        .as_ref()
                        .map(|s| s.map_key().to_string())
                        .unwrap_or_default(),
                );
                p.game_commands.write(GameCommand::EndTurn);
                *step = 3;
            }
            3 if turn == 2 => {
                saveload::open_restart_modal(&mut p.commands, &mut p.modal_stack, &p.theme);
                *step = 4;
            }
            4 => {
                if let Some(entity) = p.restart_rows.iter().next() {
                    p.activations.write(ButtonActivated(entity));
                    *step = 5;
                }
            }
            5 if turn == 1 => {
                let map_key = p
                    .session
                    .0
                    .as_ref()
                    .map(|s| s.map_key().to_string())
                    .unwrap_or_default();
                if Some(&map_key) == stash.first() {
                    println!("M10_RESTART OK: back at turn 1, same seed {map_key}");
                    p.exit.write(AppExit::Success);
                } else {
                    fail(
                        &mut p.exit,
                        "M10_RESTART",
                        &format!("seed changed: {map_key}"),
                    );
                }
                *step = 6;
            }
            _ => {}
        },
        "e2e" => match *step {
            0 => {
                p.config.observer = false;
                p.ui.config_dirty = true;
                p.actions.write(SetupAction::PreviewMap);
                *step = 1;
            }
            1 if preview_ready => {
                stash.push(p.ui.gps[0].name.clone()); // pre-reroll name
                p.actions.write(SetupAction::RerollNames);
                *step = 2;
            }
            2 if preview_ready && Some(&p.ui.gps[0].name) != stash.first() => {
                println!("M10_E2E reroll-names: {} -> {}", stash[0], p.ui.gps[0].name);
                p.actions.write(SetupAction::PickNation(0));
                *step = 3;
            }
            3 if p.config.picked_nation == Some(0) => {
                p.actions.write(SetupAction::EnterCapitalStage);
                *step = 4;
            }
            4 if !p.ui.suggestions.is_empty() => {
                let s = &p.ui.suggestions[0];
                println!(
                    "M10_E2E capital pick: {} ({}, {}) support {}",
                    s.province_name, s.preview.q, s.preview.r, s.preview.support
                );
                stash.push(format!("{},{}", s.preview.q, s.preview.r));
                p.actions.write(SetupAction::PickSuggestion(0));
                *step = 5;
            }
            5 if p.ui.picked_capital.is_some() => {
                p.actions.write(SetupAction::BeginCampaign);
                *step = 6;
            }
            6 if in_game => {
                let Some(session) = p.session.0.as_ref() else {
                    return;
                };
                // The override capital must be the picked tile — read the
                // authoritative map straight from the session.
                let coords: Vec<i32> = stash[1].split(',').filter_map(|v| v.parse().ok()).collect();
                let placed = frontend_api::map::get_map_data(session.game(), true)
                    .ok()
                    .and_then(|v| crate::game::vm::parse_map_tiles(v).ok())
                    .is_some_and(|tiles| {
                        tiles
                            .iter()
                            .any(|t| t.q == coords[0] && t.r == coords[1] && t.is_country_capital)
                    });
                if !placed {
                    fail(
                        &mut p.exit,
                        "M10_E2E",
                        "capital override not on picked tile",
                    );
                    return;
                }
                stash.push(session.map_key().to_string());
                println!(
                    "M10_E2E in game: map_key={} turn={}",
                    session.map_key(),
                    session.turn_number()
                );
                p.game_commands.write(GameCommand::EndTurn);
                *step = 7;
            }
            7 if turn == 2 => {
                p.game_commands.write(GameCommand::EndTurn);
                *step = 8;
            }
            8 if turn == 3 => {
                if !saveload::write_save(&p.session, "m10-e2e.json.gz", &mut p.toasts) {
                    fail(&mut p.exit, "M10_E2E", "save failed");
                    return;
                }
                setup::jobs::start_load(
                    &mut p.active_job,
                    &mut p.next_phase,
                    saveload::saves_dir().join("m10-e2e.json.gz"),
                );
                *step = 9;
                *frames = 0;
            }
            9 if *frames > 10 => {
                let Some(session) = p.session.0.as_ref() else {
                    return;
                };
                let turn_ok = session.turn_number() == 3;
                let map_ok = stash.get(2).map(String::as_str) == Some(session.map_key());
                if turn_ok && map_ok {
                    println!(
                        "M10_E2E OK: loaded turn {} map_key {}",
                        session.turn_number(),
                        session.map_key()
                    );
                    p.exit.write(AppExit::Success);
                } else {
                    let want = stash.get(2).cloned().unwrap_or_default();
                    fail(
                        &mut p.exit,
                        "M10_E2E",
                        &format!(
                            "after load: turn {} (want 3), map_key {} (want {want})",
                            session.turn_number(),
                            session.map_key(),
                        ),
                    );
                }
                *step = 10;
            }
            _ => {}
        },
        "skip" => match *step {
            0 => {
                p.actions.write(SetupAction::PreviewMap);
                *step = 1;
            }
            1 if preview_ready => {
                p.actions.write(SetupAction::BeginCampaign);
                *step = 2;
            }
            2 if in_game => {
                p.game_commands.write(GameCommand::SkipTurns { count: 5 });
                *step = 3;
            }
            3 if p.active_skip.0.is_none() && turn >= 6 => {
                if turn != 6 {
                    fail(
                        &mut p.exit,
                        "M10_SKIP",
                        &format!("skip 5: turn {turn}, want 6"),
                    );
                    return;
                }
                println!("M10_SKIP skip-5 OK: turn {turn}");
                p.game_commands
                    .write(GameCommand::SkipUntil { text: "the".into() });
                *step = 4;
            }
            4 if p.active_skip.0.is_none() && turn > 6 => {
                println!("M10_SKIP skip-until matched at turn {turn}");
                stash.push(format!("baseline:{turn}"));
                p.game_commands.write(GameCommand::SkipTurns { count: 400 });
                *step = 5;
            }
            5 if stash.iter().any(|s| s == "cancel-requested") && p.active_skip.0.is_none() => {
                let baseline: u32 = stash
                    .iter()
                    .find_map(|s| s.strip_prefix("baseline:"))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                if turn >= baseline + 400 {
                    fail(&mut p.exit, "M10_SKIP", "cancel did not stop the run");
                    return;
                }
                println!(
                    "M10_SKIP cancel OK: stopped at turn {turn} (started {baseline}, cap {})",
                    baseline + 400
                );
                println!("M10_SKIP OK");
                p.exit.write(AppExit::Success);
                *step = 6;
            }
            _ => {}
        },
        _ => {}
    }
}

/// Frame-pacing measurement driver, active under `PERF_STATS=1`. Waits for
/// the map to settle, runs one End Turn (the rebuild logs time the post-turn
/// map rebuild), then zooms all the way out and pans for 300 frames before
/// logging a frame-time summary and exiting.
#[allow(clippy::too_many_arguments)]
fn perf_driver(
    mut frames: Local<u32>,
    mut samples: Local<Vec<f32>>,
    mut start: Local<Option<std::time::Instant>>,
    mut announced_first_map: Local<bool>,
    time: Res<Time>,
    phase: Res<State<TurnPhase>>,
    bounds: Option<Res<layers::MapBounds>>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    mut game_commands: MessageWriter<GameCommand>,
    mut camera: Query<(&mut Transform, &mut Projection), With<camera::GameCamera>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = start.get_or_insert_with(std::time::Instant::now);
    if bounds.is_none() {
        return;
    }
    if !*announced_first_map {
        *announced_first_map = true;
        info!(
            "PERF first map build ready {:.2?} after app start",
            start.elapsed()
        );
    }
    // Only count settled frames so async turn resolution never eats the
    // measurement window.
    if *phase.get() != TurnPhase::Idle {
        return;
    }
    *frames += 1;
    match *frames {
        60 => {
            info!("PERF ending turn (post-turn rebuild timed by the rebuild logs)");
            game_commands.write(GameCommand::EndTurn);
        }
        120 => {
            if let Ok((_, mut projection)) = camera.single_mut()
                && let Projection::Orthographic(ref mut ortho) = *projection
            {
                ortho.scale = 2.8;
            }
            info!("PERF pan start (zoomed out, 300 frames)");
        }
        121..=420 => {
            if let Ok((mut transform, projection)) = camera.single_mut() {
                let scale = match &*projection {
                    Projection::Orthographic(ortho) => ortho.scale,
                    _ => 1.0,
                };
                transform.translation.x += 420.0 * scale * time.delta_secs();
            }
            samples.push(time.delta_secs() * 1000.0);
            if *frames == 420 {
                let mut sorted = samples.clone();
                sorted.sort_by(|a, b| a.total_cmp(b));
                let avg = sorted.iter().sum::<f32>() / sorted.len() as f32;
                let p95 = sorted[sorted.len() * 95 / 100 - 1];
                let max = *sorted.last().unwrap_or(&0.0);
                let fps = diagnostics
                    .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS)
                    .and_then(|d| d.average())
                    .unwrap_or(0.0);
                info!(
                    "PERF pan frame pacing over {} frames: avg {avg:.1} ms ({:.0} FPS), p95 {p95:.1} ms, max {max:.1} ms, diagnostic FPS {fps:.0}",
                    sorted.len(),
                    1000.0 / avg,
                );
                exit.write(AppExit::Success);
            }
        }
        _ => {}
    }
}

/// Run the Bevy game. The app boots into the setup flow (config → preview →
/// capital placement → campaign). Debug shortcuts skip setup and start a
/// game directly: `HUMAN_GAME=1` (human in seat 0), `OBSERVER_GAME=1`, or
/// any `MAP_SCREENSHOT` capture without an `M10_DEBUG` script (the M6–M9
/// drivers rely on booting straight into a game). `MAP_W`/`MAP_H` override
/// the 80×50 default for those shortcut games; `PERF_STATS=1` adds the FPS
/// diagnostic and the [`perf_driver`] measurement run.
pub fn run_game() {
    let human = std::env::var("HUMAN_GAME").as_deref() == Ok("1");
    let observer_shortcut = std::env::var("OBSERVER_GAME").as_deref() == Ok("1");
    let m10 = std::env::var("M10_DEBUG").is_ok();
    let screenshot = std::env::var("MAP_SCREENSHOT").is_ok();
    let perf_stats = std::env::var("PERF_STATS").as_deref() == Ok("1");
    // INTRO_DEBUG=1 keeps the title splash even under MAP_SCREENSHOT so it
    // can be captured like any other screen. It scopes to the screenshot
    // shortcut only — HUMAN_GAME / OBSERVER_GAME fast boots are unaffected.
    let intro_debug = std::env::var("INTRO_DEBUG").as_deref() == Ok("1");
    let skip_setup = human || observer_shortcut || (screenshot && !m10 && !intro_debug);

    let (initial_state, session) = if skip_setup {
        let env_dim = |key: &str, default: i32| -> i32 {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        };
        let (w, h) = (env_dim("MAP_W", 80), env_dim("MAP_H", 50));
        let generation_started = std::time::Instant::now();
        let game = if human {
            frontend_api::setup::new_game("imperialism", 2, 0, w, h, 7, 16, "", "", None)
        } else {
            frontend_api::setup::new_observer_game("imperialism", 2, w, h, 7, 16, "", "")
        };
        if perf_stats {
            eprintln!(
                "PERF {w}x{h} map generated in {:.2?}",
                generation_started.elapsed()
            );
        }
        (AppState::InGame, Some(Session::from_game(game)))
    } else if m10 {
        // The M10 setup-flow driver scripts clicks against the setup UI —
        // it must land there directly, not on the title splash.
        (AppState::Setup, None)
    } else {
        (AppState::Intro, None)
    };
    let meta = session
        .as_ref()
        .map(|session| GameMeta {
            observer: session.observer_mode(),
            player_nation: session.human_nation(),
        })
        .unwrap_or_default();
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
    if perf_stats {
        app.add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
            .add_systems(Update, perf_driver.run_if(in_state(AppState::InGame)));
    }
    if widget_gallery {
        app.add_systems(Startup, gallery::setup_gallery)
            .add_systems(Update, gallery::gallery_interactions);
    }
    app.insert_state(initial_state)
        // Persisted interface scale (side-panel slider + Ctrl+/-/0).
        .insert_resource(bevy::ui::UiScale(crate::ui_scale::load_ui_scale()))
        .add_systems(Update, crate::ui_scale::ui_scale_hotkeys)
        .init_state::<TurnPhase>()
        .init_state::<Screen>()
        .init_resource::<side_panel::DebugPanelExpanded>()
        .init_resource::<industry::IndustryUi>()
        .init_resource::<transport::TransportUi>()
        .init_resource::<trade::TradeUi>()
        .init_resource::<DiploUi>()
        .init_resource::<PrevLedger>()
        .init_resource::<TreatyMarkerIndex>()
        .init_resource::<ProposalPrompt>()
        .init_resource::<ledger::LedgerUi>()
        .init_resource::<ledger::FlagCache>()
        .init_resource::<proposals::ProposalModalState>()
        .init_resource::<CurrentTurnNews>()
        .init_resource::<NewsArchive>()
        .init_resource::<DeferredProposals>()
        .init_resource::<NewsDebugSettings>()
        .init_resource::<news::NewsUi>()
        .init_resource::<battles::BattlesUi>()
        .init_resource::<battles::BattleArchive>()
        .insert_resource(SessionRes(session))
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
        .init_resource::<ActiveSkip>()
        .init_resource::<BusyProgress>()
        .init_resource::<CameraCentered>()
        .init_resource::<SetupConfig>()
        .init_resource::<SetupUi>()
        .init_resource::<setup::ActiveGameConfig>()
        .init_resource::<setup::jobs::ActiveSetupJob>()
        .init_resource::<setup::jobs::PendingSession>()
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
        .add_message::<SetupAction>()
        .add_systems(
            Startup,
            (
                camera::setup_camera,
                layers::setup_rings,
                icons::load_icons,
                map_hud::spawn_busy_overlay,
                setup::ui::init_setup,
            ),
        )
        // Title splash: shown before setup, dismissed by any input. The
        // spawn runs in Update (guarded, once) rather than OnEnter — the
        // initial state transition precedes Startup's asset loading.
        .add_systems(OnExit(AppState::Intro), intro::cleanup_intro)
        .add_systems(
            Update,
            (
                intro::setup_intro,
                // Ordered before the modal kit's Esc handling so one key
                // press never both closes the Load dialog and quits.
                intro::intro_input.before(widgets::modal::esc_pops_top_modal),
                intro::intro_menu,
            )
                .run_if(in_state(AppState::Intro)),
        )
        // The in-game HUD chrome exists only once a game starts.
        .add_systems(
            OnEnter(AppState::InGame),
            (
                setup::ui::cleanup_setup_ui,
                map_hud::setup_hud,
                side_panel::setup_side_panel,
                tooltip::setup_map_tooltip,
            ),
        )
        // Shared map/world systems (setup preview + in-game).
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
                markers::animate_map_markers,
                debug_screenshot,
                turn_runner::poll_turn_task.run_if(in_state(TurnPhase::Processing)),
                turn_runner::poll_skip_task.run_if(in_state(TurnPhase::Processing)),
                setup::jobs::poll_setup_job,
                setup::jobs::apply_pending_session,
                map_hud::update_busy_text,
                map_hud::handle_skip_cancel,
            ),
        )
        // Debug drivers.
        .add_systems(
            Update,
            (
                (
                    m6_debug_driver,
                    m7_debug_driver,
                    m8_debug_driver,
                    m9_debug_driver,
                )
                    .run_if(in_state(AppState::InGame)),
                m10_debug_driver,
            ),
        )
        // Setup-flow systems.
        .add_systems(
            Update,
            (
                (
                    setup::route_action_buttons,
                    setup::ui::handle_interaction_clicks,
                    setup::ui::handle_setup_actions,
                    setup::ui::handle_config_inputs,
                    setup::ui::handle_config_sliders,
                    setup::ui::handle_config_checkboxes,
                    setup::ui::handle_terrain_sliders,
                    setup::ui::handle_preview_map_clicks,
                )
                    .chain(),
                (
                    setup::ui::sync_preview_gps,
                    setup::ui::compute_suggestions,
                    setup::ui::capital_hover_preview,
                    setup::ui::suggestion_row_hover.after(picking::pick_hover),
                    setup::ui::handle_preview_mode_tabs,
                    setup::ui::sync_preview_mode_tabs,
                    ledger::ensure_flags.run_if(|ui: Res<SetupUi>| ui.preview_dirty),
                    setup::ui::rebuild_config_ui,
                    setup::ui::rebuild_preview_ui,
                    setup::ui::update_yields_panel,
                    setup::ui::tint_selected_buttons,
                )
                    .chain(),
            )
                .run_if(in_state(AppState::Setup).and(in_state(TurnPhase::Idle))),
        )
        // Save/load modal plumbing (config step + in-game top bar).
        .add_systems(
            Update,
            saveload::handle_saveload_buttons.run_if(in_state(TurnPhase::Idle)),
        )
        .add_systems(
            Update,
            // Map interaction — only while the map is visible (Map /
            // Transport / Diplomacy screens); the full-screen overlays
            // block it. Diplomacy zoom-locks the camera and pins the
            // diplomatic map mode.
            (
                (camera::camera_movement, camera::wrap_camera)
                    .chain()
                    .run_if(not(in_state(Screen::Diplomacy))),
                layers::handle_map_mode_input.run_if(not(in_state(Screen::Diplomacy))),
                (
                    picking::pick_hover,
                    picking::pick_select,
                    selection::handle_map_click.run_if(in_state(AppState::InGame)),
                )
                    .chain(),
                tooltip::update_map_tooltip.run_if(in_state(AppState::InGame)),
                // Esc precedence: the cascade only sees the key press
                // before the modal system pops the top modal with it.
                selection::esc_cascade
                    .before(widgets::modal::esc_pops_top_modal)
                    .run_if(in_state(AppState::InGame)),
            )
                .run_if(in_state(TurnPhase::Idle).and(map_interactive)),
        )
        .add_systems(
            Update,
            (
                (
                    selection::sync_province_units,
                    selection::recompute_move_targets,
                    selection::sync_fleet_selection,
                    selection::sync_engineer_prompt,
                    selection::sync_pending_move_arrows,
                )
                    .chain(),
                (
                    (
                        map_hud::end_turn_button,
                        map_hud::keyboard_commands,
                        map_hud::handle_convenience_buttons,
                        map_hud::handle_burger_menu,
                        map_hud::handle_viewpoint_dropdown,
                        panels::handle_unit_checkboxes,
                        panels::handle_panel_buttons,
                        selection::handle_engineer_choice,
                    ),
                    // M7 screen affordances → GameCommand.
                    (
                        industry::handle_industry_sliders,
                        industry::handle_industry_buttons,
                        industry::handle_show_targets,
                        transport::handle_transport_buttons,
                        trade::handle_trade_buttons,
                        trade::handle_auto_trade_checkbox,
                        trade::handle_trade_sliders,
                        trade::handle_trade_filters,
                        trade::handle_hist_split,
                    ),
                    // M8 screen affordances → GameCommand / UI state.
                    (
                        diplomacy::handle_diplo_buttons,
                        tech::handle_tech_buttons,
                        ledger::handle_ledger_buttons,
                        ledger::handle_ledger_row_clicks,
                        proposals::handle_proposal_buttons,
                    ),
                    // M9 screen affordances.
                    (
                        news::handle_news_buttons,
                        news::handle_news_filters,
                        battles::handle_battles_buttons,
                        battles::handle_battles_tabs,
                        legend::handle_legend_buttons,
                    ),
                    commands::apply_command,
                )
                    .chain(),
            )
                .run_if(in_state(TurnPhase::Idle).and(in_state(AppState::InGame))),
        )
        // Screen navigation + M7/M8 screen rebuilds.
        .add_systems(
            Update,
            (
                map_hud::handle_screen_tabs,
                map_hud::screen_hotkeys.before(widgets::modal::esc_pops_top_modal),
                map_hud::update_screen_tabs,
                industry::update_industry,
                transport::update_transport,
                (trade::update_trade_static, trade::update_trade_tables).chain(),
                diplomacy::update_diplomacy_bar,
                diplomacy::update_reason_tooltip.run_if(in_state(Screen::Diplomacy)),
                tech::update_tech,
                ledger::update_ledger,
                proposals::sync_proposal_modal,
                // M9 screen rebuilds.
                (news::update_news_chrome, news::update_news_content),
                (battles::ensure_battle_archive, battles::update_battles)
                    .chain()
                    .run_if(in_state(Screen::Battles)),
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            OnEnter(Screen::Industry),
            (industry::enter_industry, tooltip::hide_map_tooltip),
        )
        .add_systems(OnExit(Screen::Industry), industry::exit_industry)
        .add_systems(OnEnter(Screen::Transport), transport::enter_transport)
        .add_systems(OnExit(Screen::Transport), transport::exit_transport)
        .add_systems(
            OnEnter(Screen::Trade),
            (trade::enter_trade, tooltip::hide_map_tooltip),
        )
        .add_systems(OnExit(Screen::Trade), trade::exit_trade)
        .add_systems(
            OnEnter(Screen::Diplomacy),
            (diplomacy::enter_diplomacy, tooltip::hide_map_tooltip),
        )
        .add_systems(OnExit(Screen::Diplomacy), diplomacy::exit_diplomacy)
        .add_systems(
            OnEnter(Screen::Tech),
            (tech::enter_tech, tooltip::hide_map_tooltip),
        )
        .add_systems(OnExit(Screen::Tech), tech::exit_tech)
        .add_systems(
            OnEnter(Screen::Ledger),
            (
                ledger::ensure_flags,
                ledger::enter_ledger,
                tooltip::hide_map_tooltip,
            )
                .chain(),
        )
        .add_systems(OnExit(Screen::Ledger), ledger::exit_ledger)
        .add_systems(
            OnEnter(Screen::News),
            (news::enter_news, tooltip::hide_map_tooltip),
        )
        .add_systems(OnExit(Screen::News), news::exit_news)
        .add_systems(
            OnEnter(Screen::Battles),
            (
                ledger::ensure_flags,
                battles::enter_battles,
                tooltip::hide_map_tooltip,
            )
                .chain(),
        )
        .add_systems(OnExit(Screen::Battles), battles::exit_battles)
        .add_systems(
            OnEnter(Screen::Legend),
            (
                ledger::ensure_flags,
                legend::enter_legend,
                tooltip::hide_map_tooltip,
            )
                .chain(),
        )
        .add_systems(OnExit(Screen::Legend), legend::exit_legend)
        .add_systems(
            Update,
            (
                map_hud::update_turn_display,
                map_hud::update_mode_display,
                map_hud::sync_viewpoint_dropdown,
                side_panel::handle_toggles,
                side_panel::handle_debug_disclosure,
                side_panel::sync_side_panel_for_diplomacy,
                side_panel::handle_ui_scale_slider,
                side_panel::sync_ui_scale_slider,
                side_panel::handle_mode_dropdown,
                side_panel::sync_mode_dropdown,
                side_panel::update_selected_info,
                side_panel::update_legend,
                side_panel::update_nations,
                panels::update_banners,
                panels::update_unit_panel,
                panels::update_civilian_panel,
                panels::update_naval_panel,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            OnEnter(TurnPhase::Processing),
            (map_hud::show_busy_overlay, map_hud::disable_end_turn),
        )
        .add_systems(
            OnExit(TurnPhase::Processing),
            (map_hud::hide_busy_overlay, map_hud::enable_end_turn),
        )
        .add_systems(
            Update,
            map_hud::sync_restart_enabled.run_if(in_state(AppState::InGame)),
        )
        .run();
}
