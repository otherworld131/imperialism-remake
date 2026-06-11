//! Core ECS resources tying the Bevy world to the game session.

use bevy::prelude::*;
use frontend_api::Session;
use std::collections::{HashMap, HashSet};

use crate::game::vm::{
    CiviliansVm, DiplomacyOverlay, MapTile, MilitaryOverlayEntry, NavyMarker, PendingMoveVm,
    ProvinceUnitsVm, SeaZone, ShipsVm,
};

/// The game session. `None` only while a turn is resolving on the async
/// task pool (the task owns the session for that window).
#[derive(Resource)]
pub struct SessionRes(pub Option<Session>);

/// Monotonic counter bumped whenever game state changes (end turn, commands
/// in later milestones). View models and map layers compare against it to
/// know when to recompute.
#[derive(Resource)]
pub struct DataVersion(pub u64);

/// JSON-derived view models recomputed when `version` falls behind
/// [`DataVersion`].
#[derive(Resource, Default)]
pub struct ViewModels {
    pub map: Option<Vec<MapTile>>,
    pub navy_markers: Vec<NavyMarker>,
    pub sea_zones: Vec<SeaZone>,
    pub diplomacy: Option<DiplomacyOverlay>,
    pub military: Vec<MilitaryOverlayEntry>,
    pub civilians: Option<CiviliansVm>,
    pub ships: Option<ShipsVm>,
    pub version: u64,
    /// Whether the map VM was fetched with fog disabled; refetched when the
    /// debug toggle flips.
    pub fetched_fog_disabled: bool,
}

/// Fast (q, r) → index lookup into `ViewModels::map`, rebuilt alongside it.
#[derive(Resource, Default)]
pub struct TileIndex {
    pub by_coord: HashMap<(i32, i32), usize>,
}

/// Calendar display for the HUD, updated from each turn report.
#[derive(Resource)]
pub struct TurnInfo {
    pub label: String,
}

impl Default for TurnInfo {
    fn default() -> Self {
        // New games always begin at turn 1 = 1815 Q1; every later label
        // comes verbatim from the turn report.
        Self {
            label: "1815 Q1".to_string(),
        }
    }
}

/// Nation whose perspective drives the diplomatic/relationship overlays and
/// the civilian roster. Observer mode watches nation 0.
#[derive(Resource, Default)]
pub struct PerspectiveNation(pub u32);

/// UI/debug toggles mirrored from the web frontend's side panel. Map layers
/// rebuild whenever any of these change.
#[derive(Resource, Clone, Copy, PartialEq, Eq)]
pub struct RenderSettings {
    pub organic_borders: bool,
    pub hide_hex_grid: bool,
    pub show_resources: bool,
    pub show_transport_network: bool,
    pub show_armies: bool,
    pub show_hidden_resources: bool,
    pub show_ai_civilians: bool,
    /// Debug: when true the whole board is visible (observer default).
    pub disable_fog: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            organic_borders: true,
            hide_hex_grid: false,
            show_resources: true,
            show_transport_network: true,
            show_armies: true,
            show_hidden_resources: false,
            show_ai_civilians: false,
            disable_fog: true,
        }
    }
}

/// 2 Hz blink shared by the selected troop indicator and selected civilian,
/// mirroring the web frontend's 500 ms interval.
#[derive(Resource)]
pub struct Blink {
    pub on: bool,
    pub timer: Timer,
}

impl Default for Blink {
    fn default() -> Self {
        Self {
            on: true,
            timer: Timer::from_seconds(0.5, TimerMode::Repeating),
        }
    }
}

pub fn tick_blink(time: Res<Time>, mut blink: ResMut<Blink>) {
    if blink.timer.tick(time.delta()).just_finished() {
        blink.on = !blink.on;
    }
}

/// Pending army move arrows (source/destination province ids). Filled by the
/// movement UI in a later milestone; the map renders whatever is here.
#[derive(Resource, Default)]
pub struct PendingMoves(pub Vec<PendingMoveArrow>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingMoveArrow {
    pub source_province_id: u64,
    pub dest_province_id: u64,
}

/// Move-target highlight plumbing (friendly = green, hostile = red), keyed
/// by province id. Filled by the movement UI in a later milestone.
#[derive(Resource, Default)]
pub struct MoveTargets {
    pub friendly: Vec<u64>,
    pub hostile: Vec<u64>,
}

/// Sea hexes a selected fleet may sail to (blue tint). Filled by the naval
/// UI in a later milestone.
#[derive(Resource, Default)]
pub struct FleetTargets(pub std::collections::HashSet<(i32, i32)>);

/// Stable key of the selected navy marker (see `map::navy::marker_key`).
#[derive(Resource, Default)]
pub struct SelectedNavy(pub Option<String>);

/// Id of the selected civilian — its map marker blinks like selected armies.
#[derive(Resource, Default)]
pub struct SelectedCivilian(pub Option<i64>);

/// Game-mode metadata captured at startup. `observer` games disable every
/// player command and hide the interactive panels (read-only map).
#[derive(Resource, Clone, Copy)]
pub struct GameMeta {
    pub observer: bool,
    /// Nation id of the human seat (viewpoint nation in observer mode).
    pub player_nation: u32,
}

/// Units loaded for the selected capital province (the unit panel VM),
/// kept in sync with [`SelectedHex`](crate::map::picking::SelectedHex) and
/// the data version.
#[derive(Resource, Default)]
pub struct ProvinceUnits {
    pub province_id: Option<u64>,
    pub vm: Option<ProvinceUnitsVm>,
    /// Data version the VM was fetched at.
    pub version: u64,
}

/// Checked army units in the unit panel. A non-empty selection implicitly
/// arms movement mode (valid targets = per-unit intersection).
#[derive(Resource, Default)]
pub struct SelectedUnits(pub Vec<u32>);

/// Queued army moves for the perspective nation, refreshed with the view
/// models. Drives the per-unit "→ destination" banners; the map arrows in
/// [`PendingMoves`] are derived from the same list.
#[derive(Resource, Default)]
pub struct PendingMoveList(pub Vec<PendingMoveVm>);

/// Civilian deploy mode: a civilian was picked for (re)deployment and the
/// map highlights every tile it may work.
#[derive(Resource, Default)]
pub struct DeployMode(pub Option<DeployState>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployState {
    pub civilian_id: i64,
    pub civ_type: String,
    /// `Some` when redeploying an idle deployed civilian (recall first).
    pub redeploy_from: Option<(i32, i32)>,
    /// Tiles the civilian may be deployed to (client-side rules, mirroring
    /// the web frontend's `handleDeployCivilian`).
    pub deployable: HashSet<(i32, i32)>,
    /// Prospector only: already-searched tiles, shown with a red ✗.
    pub prospected: HashSet<(i32, i32)>,
}

/// Engineer build-choice popup state (Railroad / Depot / Port), opened when
/// an engineer deploy tile is clicked. The modal entity lets Esc / ✕ cancel
/// the prompt without leaving deploy mode (web parity).
#[derive(Resource, Default)]
pub struct EngineerPrompt(pub Option<EngineerPromptState>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineerPromptState {
    pub civilian_id: i64,
    pub redeploy: bool,
    pub q: i32,
    pub r: i32,
    pub modal: Entity,
}

/// Checked warships in the naval panel (scoped to the selected fleet).
#[derive(Resource, Default)]
pub struct SelectedShips(pub Vec<u32>);
