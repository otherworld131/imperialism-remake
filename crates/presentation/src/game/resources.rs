//! Core ECS resources tying the Bevy world to the game session.

use bevy::prelude::*;
use frontend_api::Session;
use std::collections::{HashMap, HashSet};

use crate::game::vm::{
    ArchivedNewspaperVm, BuildableUnitsVm, CiviliansVm, DiplomacyOverlay, DiplomacyScreenVm,
    GpLedgerEntryVm, HeadlineVm, IndustryVm, LandBattleVm, MapTile, MilitaryOverlayEntry,
    NationInfoVm, NavalBattleVm, NavyMarker, PendingMoveVm, ProposalsVm, ProvinceUnitsVm, SeaZone,
    ShipsVm, TechScreenVm, TradeVm, TransportVm,
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
    pub industry: Option<IndustryVm>,
    pub buildable: Option<BuildableUnitsVm>,
    pub transport: Option<TransportVm>,
    pub trade: Option<TradeVm>,
    pub diplomacy_screen: Option<DiplomacyScreenVm>,
    pub proposals: Option<ProposalsVm>,
    pub tech: Option<TechScreenVm>,
    pub ledger: Vec<GpLedgerEntryVm>,
    /// Nation roster (name, color, type, government title, flag SVG).
    pub nations: Vec<NationInfoVm>,
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
    /// Calendar year (Tech screen title), from `/report/year`.
    pub year: u32,
}

impl Default for TurnInfo {
    fn default() -> Self {
        // New games always begin at turn 1 = 1815 Q1; every later label
        // comes verbatim from the turn report.
        Self {
            label: "1815 Q1".to_string(),
            year: 1815,
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
    /// Setup preview: reveal every hidden resource (web sets
    /// `resource_hidden: false` on all preview tiles).
    pub preview_reveal_resources: bool,
    /// Setup preview, non-observer: strip the provisional capital, depots
    /// and armies the generator placed — the player overrides the capital.
    pub preview_hide_ownership: bool,
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
            preview_reveal_resources: false,
            preview_hide_ownership: false,
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

/// Game-mode metadata for the active session. `observer` games disable every
/// player command and hide the interactive panels (read-only map). Updated
/// whenever a session is installed (game start, restart, load, viewpoint
/// switch).
#[derive(Resource, Clone, Copy)]
pub struct GameMeta {
    pub observer: bool,
    /// Nation id of the human seat (viewpoint nation in observer mode).
    pub player_nation: u32,
}

impl Default for GameMeta {
    fn default() -> Self {
        Self {
            observer: true,
            player_nation: 0,
        }
    }
}

/// `false` re-centers the camera on the map once [`MapBounds`] is published
/// (initial boot, and again after restart / load / preview generation).
#[derive(Resource, Default)]
pub struct CameraCentered(pub bool);

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

/// Engineer build popover state (card #495): a compact icon strip
/// (Railroad / Depot / Port + costs) anchored near the click, opened when a
/// deployed idle engineer is clicked on the map. Esc or any map click
/// dismisses it; redeploy highlights stay active alongside it.
#[derive(Resource, Default)]
pub struct EngineerPrompt(pub Option<EngineerPromptState>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineerPromptState {
    pub civilian_id: i64,
    /// Popover root node; despawning it closes the popover.
    pub root: Entity,
}

/// Checked warships in the naval panel (scoped to the selected fleet).
#[derive(Resource, Default)]
pub struct SelectedShips(pub Vec<u32>);

// ── M8: Diplomacy / Tech / Ledger ────────────────────────────────────────

/// Armed diplomatic action (web `QueuedDiplomacyAction`): clicking an action
/// button arms it, clicking a nation on the map fires the command. Esc or
/// the ✕ in the banner cancels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueuedDiplomacyAction {
    Consulate,
    Embassy,
    Nap,
    Alliance,
    Peace,
    Grant { amount: i64 },
    BreakTreaty { treaty_type: String },
    War,
}

impl QueuedDiplomacyAction {
    pub fn label(&self) -> String {
        match self {
            Self::Consulate => "Consulate".into(),
            Self::Embassy => "Embassy".into(),
            Self::Nap => "NAP".into(),
            Self::Alliance => "Alliance".into(),
            Self::Peace => "Peace".into(),
            Self::Grant { amount } => format!("Grant ${amount}"),
            Self::BreakTreaty { treaty_type } => format!("Break {treaty_type}"),
            Self::War => "Declare War".into(),
        }
    }

    /// Diplomacy icon-group sprite for the banner.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Consulate => "Consulate",
            Self::Embassy => "Embassy",
            Self::Nap => "NonAggressionPact",
            Self::Alliance => "Alliance",
            Self::Peace => "Peace",
            Self::Grant { .. } => "Grant",
            Self::BreakTreaty { .. } => "BreakTreaty",
            Self::War => "War",
        }
    }
}

/// Diplomacy-screen UI state: the armed action, the inline pickers, and the
/// camera/map-mode snapshot restored when the screen closes.
#[derive(Resource, Default)]
pub struct DiploUi {
    pub queued: Option<QueuedDiplomacyAction>,
    pub show_grant_picker: bool,
    pub show_break_picker: bool,
    pub confirm_war: bool,
    /// `(map mode, camera translation, ortho scale)` captured on screen
    /// entry; restored on exit (the screen forces diplomatic mode + fit zoom).
    pub saved_view: Option<(crate::map::layers::MapMode, Vec3, f32)>,
}

/// Previous-turn snapshot of the Great-Power ledger, rotated only when the
/// turn advances (web `prevGpLedgerData` / `prevLedgerTurnRef` parity).
/// Drives the per-cell delta chips.
#[derive(Resource, Default)]
pub struct PrevLedger {
    pub entries: Vec<GpLedgerEntryVm>,
    /// Turn label the *current* `ViewModels::ledger` was fetched at.
    pub fetched_turn: Option<String>,
}

/// Pending-treaty / diplomatic-presence markers drawn under the nation
/// labels in diplomatic mode, in world space of the primary map copy.
/// Rebuilt with the marker layers; consumed by picking (clickable dismiss).
#[derive(Resource, Default)]
pub struct TreatyMarkerIndex(pub Vec<TreatyMarkerHit>);

#[derive(Debug, Clone, PartialEq)]
pub struct TreatyMarkerHit {
    pub pos: Vec2,
    pub radius: f32,
    pub nation_id: u32,
    /// `None` = presence icon (embassy/consulate already built, not
    /// clickable); `Some(key)` = pending action dismissable via
    /// `dismiss_pending_action` (`"consulate"`, `"embassy"`, `"nap"`,
    /// `"alliance"`, `"peace"`, `"grant:N"`, `"break_treaty:T"`, `"war"`).
    pub action_key: Option<String>,
}

/// Proposals surfaced after turn resolution (war declarations already
/// auto-acknowledged). `Some` opens the proposal modal; emptied / `None`
/// closes it.
#[derive(Resource, Default)]
pub struct ProposalPrompt(pub Option<ProposalsVm>);

// ── M9: Newspaper / Battles / Legend ─────────────────────────────────────

/// Proposals fetched during end turn but held back until the newspaper
/// interstitial is dismissed (web order: turn → newspaper → proposal modal).
/// Only the multi-turn skip path still uses this — single end turns answer
/// proposals inside the diplomatic session (card #494).
#[derive(Resource, Default)]
pub struct DeferredProposals(pub Option<ProposalsVm>);

/// While the diplomatic session shows an item, only the involved nations
/// keep their name label on the map (card #494). `None` = all labels.
#[derive(Resource, Default, Clone, PartialEq, Eq)]
pub struct SessionLabelFilter(pub Option<std::collections::BTreeSet<String>>);

/// Between-turns session UI state (card #494), populated while the turn is
/// paused between `begin_turn` and `finish_turn`, plus the trade summary
/// shown right after resolution.
#[derive(Resource, Default)]
pub struct TurnSessionUi {
    /// The current session view (refetched after every decision).
    pub view: Option<crate::game::vm::SessionViewVm>,
    /// Cursor over `view.diplo_events`; proposals are presented after the
    /// notifications, always answering the first remaining one.
    pub diplo_index: usize,
    /// `(seller_id, resource)` offers already answered (bought or passed).
    pub answered_offers: HashSet<(u32, String)>,
    /// Wishlist resources the player skipped remaining offers for.
    pub skipped_resources: HashSet<String>,
    /// Amount picked on the current offer card.
    pub amount: u32,
    /// Post-resolution trade summary rows (`report.player_trades`).
    pub summary: Vec<crate::game::vm::PlayerTradeVm>,
}

impl TurnSessionUi {
    /// The next offer to present: first offer not yet answered whose
    /// resource wasn't skipped and that still has stock. A full cargo hold
    /// ends the parade — nothing more can be bought this turn.
    pub fn current_offer(&self) -> Option<&crate::game::vm::SessionOfferVm> {
        let view = self.view.as_ref()?;
        if view.cargo_committed >= view.cargo_capacity {
            return None;
        }
        view.offers.iter().find(|o| {
            o.remaining > 0
                && !self.skipped_resources.contains(&o.resource)
                && !self
                    .answered_offers
                    .contains(&(o.seller_id, o.resource.clone()))
        })
    }

    /// Whether the diplomatic session still has anything to show.
    pub fn has_diplo_items(&self) -> bool {
        let Some(view) = self.view.as_ref() else {
            return false;
        };
        self.diplo_index < view.diplo_events.len() || !view.proposals.is_empty()
    }
}

/// The freshest turn report's newspaper + battle content (web `headlines` /
/// `currentBattles` / `currentNavalBattles`). Empty until the first end turn.
#[derive(Resource, Default)]
pub struct CurrentTurnNews {
    /// Whether at least one turn has resolved.
    pub has_report: bool,
    /// Current (new) turn number / calendar — the masthead date (web shows
    /// the post-turn date over the resolved turn's headlines).
    pub turn_number: u32,
    pub year: i64,
    pub quarter: u32,
    pub headlines: Vec<HeadlineVm>,
    pub battles: Vec<LandBattleVm>,
    pub naval_battles: Vec<NavalBattleVm>,
}

/// Lazily loaded newspaper archive, fetched incrementally via
/// `get_newspaper_archive_since(after_turn = loaded_through)` whenever the
/// Archive tab opens (ports the web's incremental cache idea).
#[derive(Resource, Default)]
pub struct NewsArchive {
    pub entries: Vec<ArchivedNewspaperVm>,
    /// Highest archived turn already loaded.
    pub loaded_through: u32,
    /// At least one (possibly empty) load completed.
    pub loaded: bool,
}

/// Debug toggles for the newspaper / battle screens, mirroring the web side
/// panel's debug section. Kept out of [`RenderSettings`] so flipping them
/// never rebuilds map layers.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub struct NewsDebugSettings {
    /// Show AI decision rationale under headlines.
    pub show_ai_reasoning: bool,
    /// Show AI declined-action headlines.
    pub show_ai_non_actions: bool,
    /// Battle screen: retreat math block.
    pub show_retreat_debug: bool,
    /// Battle screen: firepower walkthrough + round playout.
    pub show_battle_firepower: bool,
}
