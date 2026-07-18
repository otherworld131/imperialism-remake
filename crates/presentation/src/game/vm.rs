//! View models deserialized from `frontend_api` JSON. The shapes mirror the
//! API contract pinned by the fixtures under
//! `crates/wasm-bridge/tests/fixtures/contract/` — they are the only game
//! data the presentation layer ever sees.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

/// One tile from `frontend_api::map::get_map_data`. The struct carries the
/// full tile contract; serde fills every field.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MapTile {
    pub q: i32,
    pub r: i32,
    pub map_width: i32,
    pub map_height: i32,
    pub terrain: String,
    pub owner: String,
    pub owner_color: String,
    pub nation_id: i64,
    pub province: String,
    pub province_id: Option<u64>,
    pub is_capital: bool,
    pub is_country_capital: bool,
    pub is_minor: bool,
    pub is_incorporated_minor: bool,
    pub incorporated_nation_id: Option<i64>,
    pub is_anarchic: bool,
    pub is_prospected: bool,
    pub resource: Option<String>,
    pub resource_hidden: bool,
    pub improvement_level: u32,
    pub max_improvement_level: u32,
    /// Direction indices (0-5, matching `domain::hex::HEX_DIRECTIONS` order as
    /// emitted by `frontend_api::map`) of rail links leaving this hex.
    #[serde(default)]
    pub rail_links: Vec<u8>,
    pub has_depot: bool,
    pub has_port: bool,
    pub has_fort: bool,
    pub has_river: bool,
    pub fort_level: u32,
    pub port_blockaded: bool,
    pub army_unit_count: u32,
    pub army_firepower: f64,
    pub army_composition: Option<HashMap<String, u32>>,
    pub naval_ship_count: u32,
    pub naval_firepower: i64,
    pub civilian_on_tile: Option<CivilianOnTile>,
    pub visible: bool,
    pub visual_group: Option<String>,
}

impl MapTile {
    pub fn is_sea(&self) -> bool {
        self.terrain == "Sea"
    }

    /// Visual group used for country borders (incorporated-minor parent),
    /// falling back to the owner like the web frontend.
    pub fn visual_group_or_owner(&self) -> &str {
        match self.visual_group.as_deref() {
            Some(vg) if !vg.is_empty() => vg,
            _ => &self.owner,
        }
    }
}

/// Civilian standing on a map tile (`civilian_on_tile`).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CivilianOnTile {
    pub id: i64,
    #[serde(rename = "type")]
    pub civ_type: String,
    pub working: bool,
    pub turns_remaining: u32,
    pub build_task: Option<String>,
    pub owner: String,
    pub owner_color: String,
    pub is_human: bool,
}

/// `{q, r}` coordinate pair used by several queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct HexRef {
    pub q: i32,
    pub r: i32,
}

/// One marker from `frontend_api::map::get_navy_markers`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct NavyMarker {
    pub q: i32,
    pub r: i32,
    pub nation_id: i64,
    pub owner_name: String,
    pub owner_color: String,
    /// `"fleet"` or `"beachhead"`.
    pub kind: String,
    pub ship_count: u32,
    pub total_fp: i64,
    pub total_hull: i64,
    pub by_type: BTreeMap<String, u32>,
    pub by_operation: BTreeMap<String, u32>,
    pub visible: bool,
    #[serde(default)]
    pub sea_zone_id: Option<u32>,
    #[serde(default)]
    pub sea_zone_name: Option<String>,
    #[serde(default)]
    pub pending_move_to_zone_id: Option<u32>,
    #[serde(default)]
    pub target_province: Option<String>,
    #[serde(default)]
    pub target_hex: Option<HexRef>,
}

/// One zone from `frontend_api::map::get_sea_zones`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SeaZone {
    pub id: u32,
    pub name: String,
    pub is_lake: bool,
    pub center_q: i32,
    pub center_r: i32,
    pub hexes: Vec<HexRef>,
    pub adjacent_zone_ids: Vec<u32>,
}

/// `frontend_api::map::get_diplomacy_overlay` — relations as seen from a
/// perspective nation.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DiplomacyOverlay {
    pub selected_nation: String,
    pub selected_nation_id: u32,
    pub relations: Vec<DiplomacyRelation>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DiplomacyRelation {
    pub nation_name: String,
    pub nation_id: i64,
    pub nation_color: String,
    pub score: i64,
    pub at_war: bool,
    pub status: String,
    pub treaties: Vec<String>,
    pub has_consulate: bool,
    pub has_embassy: bool,
    pub has_pending_consulate: bool,
    pub has_pending_embassy: bool,
    pub has_pending_war: bool,
    pub pending_grant_amount_dollars: Option<i64>,
    pub pending_break_treaties: Vec<String>,
    pub has_pending_nap: bool,
    pub has_pending_alliance: bool,
    pub has_pending_peace: bool,
}

/// One entry from `frontend_api::map::get_military_overlay`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MilitaryOverlayEntry {
    pub nation_name: String,
    pub nation_id: i64,
    pub nation_color: String,
    pub army_unit_count: u32,
    pub total_army_fp: f64,
    pub total_naval_fp: f64,
    pub warship_count: u32,
}

/// `frontend_api::units::get_civilians` for one nation.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CiviliansVm {
    pub deployed: Vec<CivilianEntry>,
    pub undeployed: Vec<CivilianEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CivilianEntry {
    pub id: i64,
    #[serde(rename = "type")]
    pub civ_type: String,
    pub position: Option<HexRef>,
    pub working: bool,
    pub turns_remaining: u32,
    #[serde(default)]
    pub build_task: Option<String>,
    /// Terrain under a deployed civilian (deployed entries only).
    #[serde(default)]
    pub tile_terrain: Option<String>,
    /// Visible resource under a deployed civilian (deployed entries only).
    #[serde(default)]
    pub tile_resource: Option<String>,
}

/// `frontend_api::units::get_units_in_province` — the unit panel VM.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProvinceUnitsVm {
    pub army_units: Vec<ArmyUnitVm>,
    pub garrison_count: u32,
    pub province_name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[allow(dead_code)]
pub struct ArmyUnitVm {
    pub id: u32,
    pub unit_type: String,
    pub category: String,
    pub owner_id: u32,
    pub owner_name: String,
    pub health: u32,
    pub medals: u32,
    pub firepower: f64,
    pub effective_firepower: f64,
    pub movement: u32,
    pub movement_remaining: u32,
    pub upgrade_to: Option<String>,
    pub upgrade_cost: Option<i64>,
    pub upgrade_arms_delta: Option<u32>,
    pub heal_blocked_reason: Option<String>,
}

/// `frontend_api::units::get_ships` for one nation.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[allow(dead_code)]
pub struct ShipsVm {
    pub merchants: Vec<ShipVm>,
    pub warships: Vec<ShipVm>,
    pub total_cargo: u32,
    pub total_naval_fp: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[allow(dead_code)]
pub struct ShipVm {
    pub id: u32,
    #[serde(rename = "type")]
    pub ship_type: String,
    pub hull: i64,
    pub hull_max: i64,
    /// Merchants only.
    #[serde(default)]
    pub cargo: Option<u32>,
    /// Warships only.
    #[serde(default)]
    pub firepower: Option<i64>,
    pub sea_zone: Option<u32>,
}

/// `frontend_api::units::get_valid_move_targets` for one unit.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MoveTargetsVm {
    pub friendly: Vec<MoveTargetVm>,
    pub hostile: Vec<MoveTargetVm>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[allow(dead_code)]
pub struct MoveTargetVm {
    pub province_id: u64,
    pub name: String,
    #[serde(default)]
    pub owner: Option<String>,
}

/// One entry from `frontend_api::units::get_pending_unit_moves`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PendingMoveVm {
    pub unit_id: u32,
    pub source_province_id: u64,
    pub dest_province_id: u64,
    pub dest_name: String,
}

// ── Industry (`frontend_api::industry::get_industry_data`) ──────────────

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct IndustryVm {
    pub buildings: Vec<BuildingVm>,
    pub warehouse: WarehouseVm,
    #[serde(default)]
    pub warehouse_targets: WarehouseVm,
    pub labor: LaborVm,
    pub production_forecast: ProductionForecastVm,
    pub chain_targets: BTreeMap<String, u32>,
    pub can_expand: BTreeMap<String, bool>,
    pub pending_civilian_hires: BTreeMap<String, u32>,
    pub pending_immigration: u32,
    pub max_pending_immigration: u32,
    pub pending_training: PendingTrainingVm,
    pub immigration_costs: ImmigrationCostsVm,
    pub training_costs: TrainingCostsVm,
    pub pending_freight_cars: u32,
    pub max_freight_cars: u32,
    #[serde(default)]
    pub pending_ships: Vec<String>,
    #[serde(default)]
    pub pending_army_recruits: Vec<String>,
    #[serde(default)]
    pub army_committed_arms: u32,
    #[serde(default)]
    pub army_committed_horses: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct BuildingVm {
    #[serde(rename = "type")]
    pub building_type: String,
    pub display_name: String,
    pub capacity: u32,
    pub next_capacity: u32,
    pub is_expanding: bool,
    pub turns_remaining: u32,
    pub pending_capacity: u32,
    pub expansion_cost: ExpansionCostVm,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ExpansionCostVm {
    pub lumber: u32,
    pub steel: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WarehouseVm {
    #[serde(default)]
    pub resources: BTreeMap<String, u32>,
    #[serde(default)]
    pub materials: BTreeMap<String, u32>,
    #[serde(default)]
    pub goods: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct LaborVm {
    pub untrained: u32,
    pub trained: u32,
    pub expert: u32,
    pub committed_untrained: u32,
    pub committed_trained: u32,
    pub committed_expert: u32,
    pub total_labor_units: u32,
    pub committed_labor_units: u32,
    pub total_workers: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProductionForecastVm {
    pub timber_chain: ChainForecastVm,
    pub metal_chain: ChainForecastVm,
    pub textile_chain: ChainForecastVm,
    #[serde(default)]
    pub arms_chain: ArmsChainVm,
    #[serde(default)]
    pub paper_chain: PaperChainVm,
    #[serde(default)]
    pub food_chain: FoodChainVm,
}

/// Mill + factory forecast shared by the timber/metal/textile chains; the
/// per-chain committed-input fields default to zero where absent.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct ChainForecastVm {
    pub mill_cap: u32,
    pub mill_max_output: u32,
    pub mill_output: u32,
    pub mill_labor: u32,
    pub mill_target: u32,
    pub mill_committed_timber: u32,
    pub mill_committed_coal: u32,
    pub mill_committed_iron: u32,
    pub mill_committed_cotton: u32,
    pub mill_committed_wool: u32,
    pub factory_cap: u32,
    pub factory_max_output: u32,
    pub factory_output: u32,
    pub factory_labor: u32,
    pub factory_target: u32,
    pub factory_committed_lumber: u32,
    pub factory_committed_steel: u32,
    pub factory_committed_fabric: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct ArmsChainVm {
    pub armory_cap: u32,
    pub armory_max_output: u32,
    pub armory_output: u32,
    pub armory_labor: u32,
    pub armory_target: u32,
    pub armory_committed_steel: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct PaperChainVm {
    pub factory_cap: u32,
    pub factory_max_output: u32,
    pub factory_output: u32,
    pub factory_labor: u32,
    pub factory_target: u32,
    pub factory_committed_lumber: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct FoodChainVm {
    pub factory_cap: u32,
    pub factory_max_output: u32,
    pub factory_output: u32,
    pub factory_labor: u32,
    pub factory_target: u32,
    pub factory_committed_grain: u32,
    pub factory_committed_fruit: u32,
    pub factory_committed_fish: u32,
    pub factory_committed_livestock: u32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct PendingTrainingVm {
    pub to_trained: u32,
    pub to_expert: u32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ImmigrationCostsVm {
    pub canned_food: u32,
    pub clothing: u32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TrainingCostsVm {
    pub to_trained_paper: u32,
    pub to_trained_labor: u32,
    pub to_expert_paper: u32,
    pub to_expert_labor: u32,
}

// ── Buildable units (`frontend_api::units::get_buildable_units`) ────────

#[derive(Debug, Clone, Deserialize)]
pub struct BuildableUnitsVm {
    pub treasury: i64,
    pub arms: u32,
    pub army: Vec<BuildableEntryVm>,
    pub ships: Vec<BuildableEntryVm>,
    pub civilians: Vec<BuildableEntryVm>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct BuildableEntryVm {
    #[serde(rename = "type")]
    pub unit_type: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub cost: Option<i64>,
    #[serde(default)]
    pub arms_required: Option<u32>,
    #[serde(default)]
    pub resources_needed: Option<BTreeMap<String, u32>>,
    #[serde(default)]
    pub expert_required: Option<bool>,
    pub tech_met: bool,
    #[serde(default)]
    pub max_count: u32,
    #[serde(default)]
    pub reason: Option<String>,
}

// ── Transport (`frontend_api::transport::get_transport_data`) ───────────

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TransportVm {
    pub total_capacity: u32,
    #[serde(default)]
    pub remote_delivery_capacity: Option<u32>,
    pub military_transport_capacity: u32,
    pub allocations: Vec<TransportAllocationVm>,
    pub deliveries: Vec<TransportDeliveryVm>,
    #[serde(default)]
    pub demand: Vec<TransportDemandVm>,
    #[serde(default)]
    pub food_requirement: Option<FoodRequirementVm>,
    #[serde(default)]
    pub starvation: Option<StarvationVm>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransportAllocationVm {
    pub resource: String,
    pub units: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TransportDeliveryVm {
    pub resource: String,
    pub available: u32,
    pub delivered: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransportDemandVm {
    pub resource: String,
    pub demand: u32,
}

/// Per-slot food shortfall projected by the application layer (stock minus
/// queued sell orders, plus this turn's deliveries; canned food covers the
/// rest). `workers_unfed > 0` means starvation this turn.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct StarvationVm {
    pub grain_short: u32,
    pub fruit_short: u32,
    pub meat_short: u32,
    pub workers_unfed: u32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FoodRequirementVm {
    pub workers: u32,
    pub grain: u32,
    pub fruit: u32,
    pub meat: u32,
}

// ── Trade (`frontend_api::trade::get_trade_data`) ───────────────────────

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TradeVm {
    #[serde(default = "default_true")]
    pub auto_trade_with_minors: bool,
    /// Current market price per resource (wishlist price hints).
    #[serde(default)]
    pub market_prices: Vec<MarketPriceVm>,
    pub available_offers: Vec<TradeOfferVm>,
    #[serde(default)]
    pub market_archive: Vec<MarketTurnVm>,
    pub minor_nations: Vec<MinorNationVm>,
    pub sellable_resources: Vec<SellableVm>,
    #[serde(default)]
    pub player_sell_orders: Vec<PlayerSellOrderVm>,
    /// Resources the player wants offered in the end-turn trade session
    /// (card #494), as `ResourceType` debug names.
    #[serde(default)]
    pub buy_wishlist: Vec<String>,
    pub remaining_cargo: u32,
    pub total_cargo: u32,
    #[serde(default)]
    pub subsidies: Vec<SubsidyVm>,
    pub trade_balance: TradeBalanceVm,
    pub trade_history: Vec<TradeHistoryVm>,
    pub treasury: i64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MarketPriceVm {
    pub resource: String,
    pub base_price: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradeOfferVm {
    pub resource: String,
    pub seller_id: u32,
    pub seller_name: String,
    pub quantity: u32,
    pub price: i64,
    pub is_great_power: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketTurnVm {
    pub turn: u32,
    pub offers: Vec<MarketOfferVm>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketOfferVm {
    pub resource: String,
    pub seller_id: u32,
    pub seller_name: String,
    pub seller_is_great_power: bool,
    pub offered: u32,
    pub sold: u32,
    pub price_per_unit: i64,
    #[serde(default)]
    pub fills: Vec<MarketFillVm>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MarketFillVm {
    pub buyer_id: u32,
    pub buyer_name: String,
    pub buyer_is_great_power: bool,
    pub quantity: u32,
    pub price_per_unit: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MinorNationVm {
    pub nation_id: u32,
    pub name: String,
    pub has_consulate: bool,
    pub has_embassy: bool,
    #[serde(default)]
    pub resources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SellableVm {
    pub name: String,
    pub stock: u32,
    pub price: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PlayerSellOrderVm {
    pub commodity_name: String,
    pub quantity: u32,
    #[serde(default)]
    pub price: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SubsidyVm {
    pub nation_id: u32,
    pub amount: i64,
    #[serde(default)]
    pub nation_name: String,
    #[serde(default)]
    pub has_consulate: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TradeBalanceVm {
    pub total_bought: i64,
    pub total_sold: i64,
    pub net: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradeHistoryVm {
    pub turn: u32,
    pub resource: String,
    pub bought: bool,
    pub quantity: u32,
    pub total_cost: i64,
    pub partner_id: u32,
    pub partner_name: String,
    pub partner_is_great_power: bool,
}

// ── Diplomacy screen (`frontend_api::diplomacy::get_diplomacy_screen_data`) ─

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DiplomacyScreenVm {
    pub player_standing: i64,
    pub treasury: i64,
    pub player_already_at_war: bool,
    pub relations: Vec<DiploScreenRelationVm>,
}

impl DiplomacyScreenVm {
    pub fn relation(&self, nation_id: u32) -> Option<&DiploScreenRelationVm> {
        self.relations.iter().find(|r| r.nation_id == nation_id)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DiploScreenRelationVm {
    pub nation_id: u32,
    pub nation_name: String,
    pub nation_color: String,
    pub nation_type: String,
    pub score: i64,
    pub at_war: bool,
    pub status: String,
    pub treaties: Vec<String>,
    pub has_consulate: bool,
    pub has_embassy: bool,
    pub has_pending_consulate: bool,
    pub has_pending_embassy: bool,
    pub has_pending_war: bool,
    pub pending_grant_amount_dollars: Option<i64>,
    pub pending_break_treaties: Vec<String>,
    pub has_pending_nap: bool,
    pub has_pending_alliance: bool,
    pub has_pending_peace: bool,
    pub is_in_anarchy: bool,
    pub actions: DiploActionsVm,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DiploActionsVm {
    pub can_build_consulate: bool,
    pub consulate_cost: i64,
    pub can_build_embassy: bool,
    pub embassy_cost: i64,
    pub can_propose_nap: bool,
    pub can_propose_alliance: bool,
    pub can_declare_war: bool,
    pub can_send_grant: bool,
    pub can_break_treaty: bool,
    pub breakable_treaties: Vec<String>,
    pub can_propose_peace: bool,
}

// ── Proposals (`frontend_api::diplomacy::get_pending_proposals`) ────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProposalsVm {
    pub proposals: Vec<ProposalVm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[allow(dead_code)]
pub struct ProposalVm {
    pub index: u32,
    pub from_nation_id: u32,
    pub from_nation_name: String,
    pub from_nation_color: String,
    /// `"NonAggressionPact" | "Alliance" | "PeaceTreaty" |
    /// "RequestToJoinEmpire" | "WarDeclaration" | "PactDefenseRequest"`.
    pub proposal_type: String,
    pub display_text: String,
    pub turn_proposed: u32,
    pub turns_until_expiry: u32,
}

// ── Between-turns session (`frontend_api::turn_session::session_view`) ──

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SessionViewVm {
    pub observer: bool,
    pub diplo_events: Vec<DiploEventVm>,
    pub proposals: Vec<ProposalVm>,
    pub offers: Vec<SessionOfferVm>,
    pub treasury: i64,
    pub money_committed: i64,
    pub cargo_capacity: u32,
    pub cargo_committed: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DiploEventVm {
    pub text: String,
    /// Headline category (`"War" | "Diplomacy" | "Politics"`).
    pub category: String,
    pub nation_ids: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionOfferVm {
    pub seller_id: u32,
    pub seller_name: String,
    pub resource: String,
    pub remaining: u32,
    pub price: i64,
    pub relation_score: i64,
}

/// One row of the post-turn trade summary (`report.player_trades`).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PlayerTradeVm {
    pub resource: String,
    pub quantity: u32,
    pub partner_id: u32,
    pub partner_name: String,
    pub price_per_unit: i64,
    pub total_cost: i64,
    pub bought: bool,
}

// ── Tech screen (`frontend_api::tech::get_tech_screen_data`) ────────────

#[derive(Debug, Clone, Deserialize)]
pub struct TechScreenVm {
    pub available: Vec<TechAvailableVm>,
    pub researched: Vec<TechResearchedVm>,
    pub pending: Option<TechPendingVm>,
    pub treasury: i64,
    /// Every tech in the tree ordered by availability year (adopted,
    /// available, and future/locked alike).
    #[serde(default)]
    pub timeline: Vec<TechAvailableVm>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TechAvailableVm {
    pub id: u32,
    pub name: String,
    pub cost: i64,
    pub earliest_year: i32,
    pub latest_year: i32,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TechResearchedVm {
    pub id: u32,
    pub name: String,
    pub year: i32,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TechPendingVm {
    pub id: u32,
    pub name: String,
    pub cost: i64,
    pub description: String,
}

// ── Ledger (`frontend_api::ledger::get_all_gp_ledger_data`) ─────────────

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct GpLedgerEntryVm {
    pub nation_id: u32,
    pub nation_name: String,
    pub nation_color: String,
    pub is_human: bool,
    pub economy: GpEconomyVm,
    pub cash_flow: Option<CashFlowVm>,
    pub resource_flow: Option<ResourceFlowVm>,
    pub cumulative: CumulativeTotalsVm,
    pub labor: GpLaborVm,
    pub military: GpMilitaryVm,
    pub diplomacy: GpDiplomacyVm,
    #[serde(default)]
    pub resources_detail: BTreeMap<String, i64>,
    #[serde(default)]
    pub materials_detail: BTreeMap<String, i64>,
    #[serde(default)]
    pub goods_detail: BTreeMap<String, i64>,
    pub technology: GpTechnologyVm,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpEconomyVm {
    pub treasury: i64,
    pub provinces: i64,
    pub buildings: i64,
    pub goods_revenue: i64,
    pub total_resources: i64,
    pub total_materials: i64,
    pub total_goods: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CashFlowVm {
    pub opening_treasury: i64,
    pub closing_treasury: i64,
    pub total_income: i64,
    pub total_expense: i64,
    pub observed_delta: i64,
    pub accounted_delta: i64,
    pub reconciliation_mismatch: i64,
    pub reconciles: bool,
    #[serde(default)]
    pub income_totals: BTreeMap<String, i64>,
    #[serde(default)]
    pub expense_totals: BTreeMap<String, i64>,
    #[serde(default)]
    pub income_by_category: BTreeMap<String, i64>,
    #[serde(default)]
    pub expense_by_category: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ResourceFlowVm {
    #[serde(default)]
    pub inflow: Vec<FlowEntryVm>,
    #[serde(default)]
    pub outflow: Vec<FlowEntryVm>,
    #[serde(default)]
    pub inflow_by_stockpile_category: BTreeMap<String, BTreeMap<String, i64>>,
    #[serde(default)]
    pub outflow_by_stockpile_category: BTreeMap<String, BTreeMap<String, i64>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct FlowEntryVm {
    pub stockpile: String,
    /// Inflow entries carry `source`; outflow entries carry `sink`.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub sink: Option<String>,
    pub category: String,
    pub amount: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CumulativeTotalsVm {
    #[serde(default)]
    pub income_totals: BTreeMap<String, i64>,
    #[serde(default)]
    pub expense_totals: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpLaborVm {
    pub untrained: i64,
    pub trained: i64,
    pub expert: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct GpMilitaryVm {
    pub total_army_count: i64,
    pub total_army_fp: i64,
    pub field_army_count: i64,
    pub militia_count: i64,
    pub total_warship_count: i64,
    pub merchant_ships: i64,
    pub generals_earned: i64,
    pub total_arms_built: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpDiplomacyVm {
    pub standing: i64,
    pub consulates: i64,
    pub embassies: i64,
    pub alliances: i64,
    #[serde(default)]
    pub alliance_names: Vec<String>,
    pub wars: i64,
    #[serde(default)]
    pub war_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpTechnologyVm {
    pub researched_count: i64,
    #[serde(default)]
    pub researched_names: Vec<String>,
}

/// One entry from `frontend_api::flavor::get_nation_flags` — the nation
/// roster (identity card + flag SVG).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct NationInfoVm {
    pub nation_id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: String,
    /// `"GreatPower" | "MinorNation"`.
    #[serde(default)]
    pub nation_type: String,
    #[serde(default)]
    pub government_title: String,
    #[serde(default)]
    pub flag_svg: String,
}

impl NationInfoVm {
    pub fn is_great_power(&self) -> bool {
        self.nation_type == "GreatPower"
    }
}

// ── Newspaper (turn report + `frontend_api::newspaper`) ─────────────────

/// One newspaper headline (turn report `headlines` / archive entries).
#[derive(Debug, Clone, Deserialize)]
pub struct HeadlineVm {
    pub text: String,
    /// Category name as emitted by the backend (`"War"`, `"Battle"`, …);
    /// match case-insensitively against the lowercase color keys.
    #[serde(default)]
    pub category: String,
    /// AI decision rationale (debug toggle).
    #[serde(default)]
    pub reason: Option<String>,
    /// AI declined-action headline, hidden unless the debug toggle is on.
    #[serde(default)]
    pub is_non_action: bool,
    /// Nations involved (country filter).
    #[serde(default)]
    pub nation_ids: Vec<i64>,
}

/// One archived turn from `frontend_api::newspaper::get_newspaper_archive_since`.
#[derive(Debug, Clone, Deserialize)]
pub struct ArchivedNewspaperVm {
    pub turn: u32,
    pub year: i64,
    pub quarter: u32,
    pub headlines: Vec<HeadlineVm>,
}

// ── Battles (turn report + `frontend_api::battles::get_battle_data`) ────

/// A surviving unit in a battle result.
#[derive(Debug, Clone, Deserialize)]
pub struct BattleUnitVm {
    pub unit_type: String,
    pub health: u32,
    pub medals: u32,
    pub effective_firepower: f64,
}

/// Per-unit battle log (firepower debug toggle).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct BattleUnitLogVm {
    pub unit_type: String,
    pub medals_initial: u32,
    pub medals_final: u32,
    pub initial_health: u32,
    pub final_health: u32,
    pub initial_firepower: f64,
    pub final_firepower: f64,
    #[serde(default)]
    pub defender_breakdown: Option<DefenderBreakdownVm>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DefenderBreakdownVm {
    pub applied_firepower: f64,
    pub fort_multiplier: f64,
    pub entrenchment_fp: f64,
    pub initial_total_contribution: f64,
}

/// One round of the battle playout trace (round 0 = first-strike volley).
#[derive(Debug, Clone, Deserialize)]
pub struct BattleRoundLogVm {
    pub round: u32,
    #[serde(default)]
    pub first_strike_side: Option<String>,
    pub atk_fp: f64,
    pub def_fp: f64,
    pub atk_shots: u32,
    pub def_shots: u32,
    #[serde(default)]
    pub atk_casualties: Vec<String>,
    #[serde(default)]
    pub def_casualties: Vec<String>,
    /// `"attacker"` / `"defender"` when a mid-battle retreat fired.
    #[serde(default)]
    pub retreat_triggered: Option<String>,
}

/// Non-finite f64s (e.g. an infinite FP ratio against a zero-FP side)
/// serialize to JSON `null`; decode those as NaN instead of failing.
fn f64_or_nan<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(f64::NAN))
}

/// Retreat-math debug block. The ratio fields divide by a side's FP and can
/// be non-finite (→ JSON `null` → NaN here).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RetreatDebugVm {
    pub side: String,
    /// `"pre_battle" | "mid_battle" | "none"`.
    pub stage: String,
    #[serde(deserialize_with = "f64_or_nan")]
    pub measured_value: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    pub threshold: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    pub attacker_prebattle_ratio: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    pub defender_prebattle_ratio: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    pub attacker_prebattle_threshold: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    pub defender_prebattle_threshold: f64,
    pub round: u32,
}

/// One land battle from the turn report / battle archive.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct LandBattleVm {
    pub attacker: String,
    pub attacker_id: u32,
    pub defender: String,
    pub defender_id: u32,
    pub province: String,
    pub province_id: u64,
    pub attacker_won: bool,
    pub retreated: bool,
    #[serde(default)]
    pub defender_retreated: bool,
    pub attacker_casualties: Vec<String>,
    pub defender_casualties: Vec<String>,
    pub attacker_survivors: Vec<BattleUnitVm>,
    pub defender_survivors: Vec<BattleUnitVm>,
    #[serde(default)]
    pub terrain: Option<String>,
    pub fort_level: u32,
    #[serde(default)]
    pub siege_reduced_fort: bool,
    pub attacker_initial_count: u32,
    pub defender_initial_count: u32,
    pub attacker_initial_fp: f64,
    pub defender_initial_fp: f64,
    pub attacker_survivors_count: u32,
    pub defender_survivors_count: u32,
    pub medal_awards: Vec<MedalAwardVm>,
    #[serde(default)]
    pub capital_tile: Option<HexRef>,
    #[serde(default)]
    pub province_tiles: Vec<HexRef>,
    #[serde(default)]
    pub origin_tiles: Vec<HexRef>,
    #[serde(default)]
    pub origin_province_names: Vec<String>,
    #[serde(default)]
    pub is_naval_landing: bool,
    #[serde(default)]
    pub retreat_debug: Option<RetreatDebugVm>,
    #[serde(default)]
    pub attacker_unit_logs: Vec<BattleUnitLogVm>,
    #[serde(default)]
    pub defender_unit_logs: Vec<BattleUnitLogVm>,
    #[serde(default)]
    pub round_logs: Vec<BattleRoundLogVm>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MedalAwardVm {
    pub unit_type: String,
    pub medals: u32,
}

/// One naval battle from the turn report / battle archive.
#[derive(Debug, Clone, Deserialize)]
pub struct NavalBattleVm {
    pub attacker: String,
    pub attacker_id: u32,
    pub defender: String,
    pub defender_id: u32,
    pub attacker_won: bool,
    pub attacker_ships_lost: Vec<String>,
    pub defender_ships_lost: Vec<String>,
    pub attacker_survivors_count: u32,
    pub defender_survivors_count: u32,
}

/// One archived turn from `frontend_api::battles::get_battle_data`.
#[derive(Debug, Clone, Deserialize)]
pub struct ArchivedBattleTurnVm {
    pub turn: u32,
    pub year: i64,
    pub quarter: u32,
    pub battles: Vec<LandBattleVm>,
    pub naval_battles: Vec<NavalBattleVm>,
}

// ── Political snapshot (`frontend_api::map::get_political_snapshot`) ────

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PoliticalSnapshotVm {
    pub turn: u32,
    pub year: i64,
    pub quarter: u32,
    pub map_width: i32,
    pub map_height: i32,
    pub tiles: Vec<SnapshotTileVm>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SnapshotTileVm {
    pub q: i32,
    pub r: i32,
    pub terrain: String,
    pub owner: String,
    pub owner_color: String,
    pub province: String,
    pub is_capital: bool,
    pub is_country_capital: bool,
    pub is_minor: bool,
    pub is_incorporated_minor: bool,
    #[serde(default)]
    pub visual_group: Option<String>,
}

impl SnapshotTileVm {
    /// Border group: incorporated minors keep their own country border.
    pub fn visual_group_or_owner(&self) -> &str {
        match self.visual_group.as_deref() {
            Some(vg) if !vg.is_empty() => vg,
            _ => &self.owner,
        }
    }
}

pub fn parse_map_tiles(value: serde_json::Value) -> Result<Vec<MapTile>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_navy_markers(value: serde_json::Value) -> Result<Vec<NavyMarker>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_sea_zones(value: serde_json::Value) -> Result<Vec<SeaZone>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_diplomacy_overlay(
    value: serde_json::Value,
) -> Result<DiplomacyOverlay, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_military_overlay(
    value: serde_json::Value,
) -> Result<Vec<MilitaryOverlayEntry>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_civilians(value: serde_json::Value) -> Result<CiviliansVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_province_units(
    value: serde_json::Value,
) -> Result<ProvinceUnitsVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_ships(value: serde_json::Value) -> Result<ShipsVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_move_targets(value: serde_json::Value) -> Result<MoveTargetsVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_pending_moves(
    value: serde_json::Value,
) -> Result<Vec<PendingMoveVm>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_industry(value: serde_json::Value) -> Result<IndustryVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_buildable_units(
    value: serde_json::Value,
) -> Result<BuildableUnitsVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_transport(value: serde_json::Value) -> Result<TransportVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_trade(value: serde_json::Value) -> Result<TradeVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_diplomacy_screen(
    value: serde_json::Value,
) -> Result<DiplomacyScreenVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_proposals(value: serde_json::Value) -> Result<ProposalsVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_session_view(value: serde_json::Value) -> Result<SessionViewVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_player_trades(
    value: serde_json::Value,
) -> Result<Vec<PlayerTradeVm>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_tech_screen(value: serde_json::Value) -> Result<TechScreenVm, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_gp_ledger(
    value: serde_json::Value,
) -> Result<Vec<GpLedgerEntryVm>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_nation_roster(
    value: serde_json::Value,
) -> Result<Vec<NationInfoVm>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_headlines(value: serde_json::Value) -> Result<Vec<HeadlineVm>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_newspaper_archive(
    value: serde_json::Value,
) -> Result<Vec<ArchivedNewspaperVm>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_land_battles(
    value: serde_json::Value,
) -> Result<Vec<LandBattleVm>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_naval_battles(
    value: serde_json::Value,
) -> Result<Vec<NavalBattleVm>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_battle_archive(
    value: serde_json::Value,
) -> Result<Vec<ArchivedBattleTurnVm>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_political_snapshot(
    value: serde_json::Value,
) -> Result<PoliticalSnapshotVm, serde_json::Error> {
    serde_json::from_value(value)
}
