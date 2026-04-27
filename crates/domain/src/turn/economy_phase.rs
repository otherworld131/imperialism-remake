//! Economy phase of the turn pipeline.
//!
//! Implements the plan → reserve → execute structure (Trello #161).
//! `processor.rs` calls `collect_economic_orders` → `validate_and_reserve` →
//! `execute_reserved_economy` three times (collect, production, trade) interleaved
//! with non-economy steps, then calls `release_all_reservations` once at the end
//! of all three batches to honour the documented reservation lifetime contract.
//!
//! Lighter helpers (warehouse caps, building tick, blockade capacity,
//! maintenance, blockade headlines) live here alongside the phase types.
//! The heavy execution functions (`collect_resources`, `run_production`)
//! remain in `processor.rs`; trade resolution now lives in `trade_phase.rs`.

use crate::economy::buildings::BuildingType;
use crate::economy::labor::WorkerType;
use crate::economy::observability::{PendingEconomyOrder, PendingOrderPhase};
use crate::economy::production::{
    ProductionChain, calculate_factory_production, calculate_mill_production,
};
use crate::economy::trade::{self, Commodity};
use crate::events::{Headline, HeadlineCategory};
use crate::game_state::GameState;
use crate::military::naval::calculate_blockade_effect;
use crate::turn::processor::TurnReport;
use crate::types::*;
use std::collections::{BTreeMap, HashMap};

// ── Plan / Reserve / Execute types (#161) ────────────────────────────────────

/// An economic action gathered during the collect phase.
/// No game state is mutated during collection — this represents *intent*.
#[derive(Debug, Clone)]
pub struct EconomicOrder {
    pub kind: EconomicOrderKind,
}

/// The kind of economy-phase work each order represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconomicOrderKind {
    /// Collect raw resources from owned, connected tiles.
    CollectTileResources,
    /// Run production chains (mills → materials, factories → goods).
    RunProduction,
    /// Execute the trade session with minor nations and the world market.
    ExecuteTrade,
}

#[derive(Debug, Clone)]
pub struct NationReservation {
    pub nation_id: NationId,
    pub inventory_reservations: Vec<ReservationId>,
    pub inventory_summary: BTreeMap<Commodity, u32>,
    pub treasury_reserved: Money,
    pub labor_reserved: HashMap<WorkerType, u32>,
}

impl NationReservation {
    fn commitment_count(&self) -> usize {
        let _ = self.nation_id;
        self.inventory_reservations.len()
            + self.inventory_summary.len()
            + self.labor_reserved.len()
            + usize::from(self.treasury_reserved > Money::ZERO)
    }
}

/// An economic order that has been validated and whose resource requirements
/// have been pre-committed.
#[derive(Debug, Clone)]
pub struct ReservedAction {
    pub order: EconomicOrder,
    pub reservations: Vec<NationReservation>,
}

impl ReservedAction {
    fn commitment_count(&self) -> usize {
        self.reservations
            .iter()
            .map(NationReservation::commitment_count)
            .sum()
    }
}

/// Gather the standard economy orders for this turn.
///
/// **No mutation.** Reads game state to decide which standard operations
/// should run; the actual work happens in the matching `run_*_phase` entry points.
pub(super) fn collect_economic_orders(_game: &GameState) -> Vec<EconomicOrder> {
    vec![
        EconomicOrder { kind: EconomicOrderKind::CollectTileResources },
        EconomicOrder { kind: EconomicOrderKind::RunProduction },
        EconomicOrder { kind: EconomicOrderKind::ExecuteTrade },
    ]
}

/// Validate orders and register the reservations needed to execute them.
///
/// Returns reserved actions ready for execution in the current economy batch.
/// Reservations and pending-order summaries are batch-local and must be
/// cleared after the matching batch executes so later batches observe fresh
/// `available()` state.
pub(super) fn validate_and_reserve(
    game: &mut GameState,
    orders: Vec<EconomicOrder>,
) -> Vec<ReservedAction> {
    orders
        .into_iter()
        .map(|order| {
            let reservations = match order.kind {
                EconomicOrderKind::CollectTileResources => Vec::new(),
                EconomicOrderKind::RunProduction => reserve_production_phase(game),
                EconomicOrderKind::ExecuteTrade => reserve_trade_phase(game),
            };
            ReservedAction { order, reservations }
        })
        .collect()
}

fn reserve_production_phase(game: &mut GameState) -> Vec<NationReservation> {
    let untrained_mult = game.game_data.game_config.untrained_labor;
    let trained_mult = game.game_data.game_config.trained_labor;
    let expert_mult = game.game_data.game_config.expert_labor;
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    let mut out = Vec::new();

    for nation_id in nation_ids {
        let Some(nation) = game.get_nation(nation_id) else {
            continue;
        };
        if nation.diplomacy.is_in_anarchy {
            continue;
        }

        let resources: Vec<(ResourceType, u32)> =
            nation.economy.warehouse.iter().map(|(r, q)| (*r, *q)).collect();
        let starting_materials: HashMap<MaterialType, u32> =
            nation.economy.materials.iter().map(|(m, q)| (*m, *q)).collect();

        let mut remaining_labor = nation.economy.labor.total_labor_units_with(
            untrained_mult,
            trained_mult,
            expert_mult,
        );
        let mut resource_needs: BTreeMap<ResourceType, u32> = BTreeMap::new();

        let timber_cap = building_capacity(nation, BuildingType::LumberMill);
        let timber_result = if timber_cap > 0 {
            let result = calculate_mill_production(
                ProductionChain::Timber,
                &resources,
                timber_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            for (resource, qty) in &result.resources_consumed {
                *resource_needs.entry(*resource).or_insert(0) += *qty;
            }
            Some(result)
        } else {
            None
        };

        let steel_cap = building_capacity(nation, BuildingType::SteelMill);
        let metal_result = if steel_cap > 0 {
            let result = calculate_mill_production(
                ProductionChain::Metal,
                &resources,
                steel_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            for (resource, qty) in &result.resources_consumed {
                *resource_needs.entry(*resource).or_insert(0) += *qty;
            }
            Some(result)
        } else {
            None
        };

        let textile_cap = building_capacity(nation, BuildingType::TextileMill);
        let textile_result = if textile_cap > 0 {
            let result = calculate_mill_production(
                ProductionChain::Textile,
                &resources,
                textile_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            for (resource, qty) in &result.resources_consumed {
                *resource_needs.entry(*resource).or_insert(0) += *qty;
            }
            Some(result)
        } else {
            None
        };

        let mut materials_inventory = starting_materials.clone();
        for result in [&timber_result, &metal_result, &textile_result]
            .into_iter()
            .flatten()
        {
            for (material, qty) in &result.materials_produced {
                *materials_inventory.entry(*material).or_insert(0) += *qty;
            }
        }
        let materials_inventory: Vec<(MaterialType, u32)> = materials_inventory.into_iter().collect();

        let furniture_cap = building_capacity(nation, BuildingType::FurnitureFactory);
        let furniture_result = if furniture_cap > 0 {
            let result = calculate_factory_production(
                ProductionChain::Timber,
                &materials_inventory,
                furniture_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            Some(result)
        } else {
            None
        };
        let hardware_cap = building_capacity(nation, BuildingType::HardwareFactory);
        let hardware_result = if hardware_cap > 0 {
            let result = calculate_factory_production(
                ProductionChain::Metal,
                &materials_inventory,
                hardware_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            Some(result)
        } else {
            None
        };
        let clothing_cap = building_capacity(nation, BuildingType::ClothingFactory);
        let clothing_result = if clothing_cap > 0 {
            let result = calculate_factory_production(
                ProductionChain::Textile,
                &materials_inventory,
                clothing_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            Some(result)
        } else {
            None
        };

        let mut material_needs: BTreeMap<MaterialType, u32> = BTreeMap::new();
        for result in [&furniture_result, &hardware_result, &clothing_result]
            .into_iter()
            .flatten()
        {
            for (material, qty) in &result.materials_consumed {
                let starting_stock = starting_materials.get(material).copied().unwrap_or(0);
                let already_reserved = material_needs.get(material).copied().unwrap_or(0);
                let reservable = starting_stock.saturating_sub(already_reserved).min(*qty);
                if reservable > 0 {
                    *material_needs.entry(*material).or_insert(0) += reservable;
                }
            }
        }

        let labor_used: u32 = [&timber_result, &metal_result, &textile_result]
            .into_iter()
            .flatten()
            .map(|r| r.labor_used)
            .sum::<u32>()
            + [&furniture_result, &hardware_result, &clothing_result]
                .into_iter()
                .flatten()
                .map(|r| r.labor_used)
                .sum::<u32>();

        let Some(nation) = game.get_nation_mut(nation_id) else {
            continue;
        };
        let mut inventory_reservations = Vec::new();
        let mut inventory_summary = BTreeMap::new();
        for (resource, qty) in resource_needs {
            if qty == 0 {
                continue;
            }
            if let Ok(id) = nation.economy.reserve(Commodity::Resource(resource), qty) {
                inventory_reservations.push(id);
                inventory_summary.insert(Commodity::Resource(resource), qty);
            }
        }
        for (material, qty) in material_needs {
            if qty == 0 {
                continue;
            }
            if let Ok(id) = nation.economy.reserve(Commodity::Material(material), qty) {
                inventory_reservations.push(id);
                inventory_summary.insert(Commodity::Material(material), qty);
            }
        }
        let labor_reserved = nation
            .economy
            .reserve_labor_units_with(
                labor_used,
                untrained_mult,
                trained_mult,
                expert_mult,
            )
            .unwrap_or_default();
        if !inventory_summary.is_empty() || !labor_reserved.is_empty() {
            register_pending_order(
                game,
                nation_id,
                PendingEconomyOrder {
                    phase: PendingOrderPhase::Produce,
                    inventory: inventory_summary.clone(),
                    treasury: Money::ZERO,
                    labor: labor_reserved.clone(),
                },
            );
        }
        out.push(NationReservation {
            nation_id,
            inventory_reservations,
            inventory_summary,
            treasury_reserved: Money::ZERO,
            labor_reserved,
        });
    }

    out
}

fn reserve_trade_phase(game: &mut GameState) -> Vec<NationReservation> {
    let human_id = game.human_player_nation;
    let blockade_capacity = compute_blockade_capacity(game);
    let gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    let offers =
        trade::generate_minor_nation_offers(&game.world.nations, &game.world.provinces, &game.world.hex_map);
    let mut all_bids = Vec::new();
    for gp_id in &gp_ids {
        if *gp_id == human_id {
            if let Some(human) = game.get_nation(*gp_id) {
                for order in &human.diplomacy.player_buy_orders {
                    if order.quantity > 0 {
                        all_bids.push(trade::TradeBid {
                            buyer: *gp_id,
                            resource: order.resource,
                            quantity: order.quantity,
                            max_price_per_unit: order.max_price_per_unit,
                        });
                    }
                }
            }
            continue;
        }
        if let Some(nation) = game.get_nation(*gp_id) {
            let cargo_capacity = blockade_capacity
                .get(gp_id)
                .copied()
                .unwrap_or_else(|| nation.total_cargo_capacity());
            all_bids.extend(trade::generate_smart_bids(
                nation,
                &offers,
                &game.world.diplomacy,
                cargo_capacity,
            ));
        }
    }

    let mut bid_spend: HashMap<NationId, Money> = HashMap::new();
    for bid in &all_bids {
        let cost = Money::dollars(
            bid.max_price_per_unit.as_dollars() * i64::from(bid.quantity),
        );
        *bid_spend.entry(bid.buyer).or_insert(Money::ZERO) += cost;
    }

    let mut out = Vec::new();
    for nation_id in gp_ids {
        let Some(nation_ro) = game.get_nation(nation_id) else {
            continue;
        };
        let subsidy_total: Money = nation_ro
            .diplomacy
            .trade_subsidies
            .values()
            .copied()
            .fold(Money::ZERO, |acc, cost| acc + cost);
        let sell_orders = if nation_id == human_id {
            nation_ro.diplomacy.player_sell_orders.clone()
        } else {
            Vec::new()
        };
        let reserve_requests: Vec<(Commodity, u32)> = sell_orders
            .iter()
            .filter_map(|order| match order.commodity {
                Commodity::Resource(resource) => {
                    if order.quantity > 0 && nation_ro.resource_amount(resource) >= order.quantity {
                        Some((Commodity::Resource(resource), order.quantity))
                    } else {
                        None
                    }
                }
                Commodity::Material(material) => {
                    let stock = nation_ro.economy.materials.get(&material).copied().unwrap_or(0);
                    let qty = stock.min(order.quantity);
                    (qty > 0).then_some((Commodity::Material(material), qty))
                }
                Commodity::Goods(goods) => {
                    let stock = nation_ro.economy.goods.get(&goods).copied().unwrap_or(0);
                    let qty = stock.min(order.quantity);
                    (qty > 0).then_some((Commodity::Goods(goods), qty))
                }
            })
            .collect();

        let mut inventory_summary = BTreeMap::new();
        let mut inventory_reservations = Vec::new();
        for (commodity, qty) in reserve_requests {
            if let Ok(id) = reserve_inventory(game, nation_id, commodity, qty) {
                inventory_reservations.push(id);
                inventory_summary.insert(commodity, qty);
            }
        }

        let target_bid_spend = bid_spend.get(&nation_id).copied().unwrap_or(Money::ZERO);
        let Some(nation) = game.get_nation_mut(nation_id) else {
            continue;
        };
        let mut treasury_reserved = Money::ZERO;
        if subsidy_total > Money::ZERO && nation.economy.reserve_treasury(subsidy_total).is_ok() {
            treasury_reserved += subsidy_total;
        }
        let bid_reserve = if target_bid_spend <= nation.economy.available_treasury() {
            target_bid_spend
        } else {
            nation.economy.available_treasury()
        };
        if bid_reserve > Money::ZERO && nation.economy.reserve_treasury(bid_reserve).is_ok() {
            treasury_reserved += bid_reserve;
        }

        if !inventory_summary.is_empty() || treasury_reserved > Money::ZERO {
            register_pending_order(
                game,
                nation_id,
                PendingEconomyOrder {
                    phase: PendingOrderPhase::Trade,
                    inventory: inventory_summary.clone(),
                    treasury: treasury_reserved,
                    labor: HashMap::new(),
                },
            );
        }
        out.push(NationReservation {
            nation_id,
            inventory_reservations,
            inventory_summary,
            treasury_reserved,
            labor_reserved: HashMap::new(),
        });
    }
    out
}

fn building_capacity(nation: &crate::nation::Nation, building_type: BuildingType) -> u32 {
    nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == building_type)
        .map(|b| b.effective_capacity())
        .unwrap_or(0)
}

fn reserve_inventory(
    game: &mut GameState,
    nation_id: NationId,
    commodity: Commodity,
    qty: u32,
) -> Result<ReservationId, crate::DomainError> {
    let nation = game
        .get_nation_mut(nation_id)
        .ok_or(crate::DomainError::NationNotFound(nation_id))?;
    nation.economy.reserve(commodity, qty)
}

fn register_pending_order(game: &mut GameState, nation_id: NationId, order: PendingEconomyOrder) {
    game.transient
        .pending_economy_orders
        .entry(nation_id)
        .or_default()
        .push(order);
}

/// Execute a set of reserved economy actions.
///
/// Calls into `processor.rs` for the heavy execution logic. Does NOT release
/// reservations — the caller (`processor.rs`) is responsible for calling
/// `release_all_reservations` once after all economy batches complete so that
/// reservations survive across collect → production → trade within a single turn.
pub(super) fn execute_reserved_economy(
    game: &mut GameState,
    report: &mut TurnReport,
    reserved: Vec<ReservedAction>,
) {
    for action in reserved {
        let _ = action.commitment_count();
        match action.order.kind {
            EconomicOrderKind::CollectTileResources => {
                super::processor::collect_resources(game, report);
            }
            EconomicOrderKind::RunProduction => {
                super::processor::run_production(game, report);
            }
            EconomicOrderKind::ExecuteTrade => {
                let blockade_capacity = compute_blockade_capacity(game);
                super::trade_phase::resolve_trade_session(game, report, &blockade_capacity);
            }
        }
    }
}

/// Bankruptcy floor: treasury cannot go below $0.
const BANKRUPTCY_FLOOR: Money = Money::ZERO;

/// Tick all buildings for all nations, advancing expansion timers.
pub(super) fn tick_buildings(game: &mut GameState) {
    for nation in &mut game.world.nations {
        for building in &mut nation.economy.buildings {
            building.tick();
        }
    }
}

/// Apply army maintenance, bankruptcy clamp, and bankruptcy headline.
pub(super) fn apply_maintenance(game: &mut GameState, report: &mut TurnReport) {
    for nation in &mut game.world.nations {
        if nation.diplomacy.is_in_anarchy {
            continue;
        }
        let total_cost: Money = nation
            .military.army
            .iter()
            .map(|u| u.maintenance_cost())
            .fold(Money::ZERO, |acc, c| acc + c);
        if total_cost != Money::ZERO {
            nation.economy.treasury -= total_cost;
            report.maintenance_costs.push((nation.id, total_cost));
        }

        // Track negativity BEFORE clamping — is_bankrupt() checks treasury < 0,
        // which is always false after the floor is applied (F-017 fix).
        let went_bankrupt = nation.economy.treasury < Money::ZERO;

        // Bankruptcy protection: treasury cannot go below $0. The clamp
        // represents debt forgiven mid-turn — we surface it as income in the
        // cash-flow ledger so the reconciliation invariant closes.
        if nation.economy.treasury < BANKRUPTCY_FLOOR {
            let writeoff = Money::from_cents(BANKRUPTCY_FLOOR.cents() - nation.economy.treasury.cents());
            nation.economy.treasury = BANKRUPTCY_FLOOR;
            if writeoff > Money::ZERO {
                report.bankruptcy_writeoff.push((nation.id, writeoff));
            }
        }

        if went_bankrupt {
            report.newspaper_headlines.push(
                Headline::new(
                    format!("FINANCIAL CRISIS: {} faces bankruptcy!", nation.name),
                    HeadlineCategory::Crisis,
                )
                .for_nation(nation.id),
            );
        }
    }
}

/// Compute blockade-adjusted cargo capacity for all Great Powers.
///
/// For each GP at war with an enemy that has warships, reduce their effective
/// cargo capacity using `calculate_blockade_effect`. This map is passed to the
/// trade session so blockades actually reduce trade volume.
pub(super) fn compute_blockade_capacity(game: &GameState) -> HashMap<NationId, u32> {
    // Only consider active Great Powers (not anarchic, not eliminated)
    let active_gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy && !n.province_ids.is_empty())
        .map(|n| n.id)
        .collect();

    let mut capacity_map = HashMap::new();

    for &nation_id in &active_gp_ids {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => continue,
        };
        let raw_cargo = nation.total_cargo_capacity();

        // Only count warships from active enemy nations, and only if the war
        // is past its one-turn grace period (card #104: blockade, like every
        // other hostile action, doesn't fire on the declaration turn).
        let mut enemy_warship_count: u32 = 0;
        for &other_id in &active_gp_ids {
            if other_id == nation_id {
                continue;
            }
            let hostile = game
                .world.diplomacy
                .get_relation(nation_id, other_id)
                .is_some_and(|r| r.hostilities_active_on(game.turn));
            if hostile && let Some(other) = game.get_nation(other_id) {
                enemy_warship_count += other.warship_count() as u32;
            }
        }

        let effective = if enemy_warship_count > 0 {
            calculate_blockade_effect(raw_cargo, enemy_warship_count)
        } else {
            raw_cargo
        };
        capacity_map.insert(nation_id, effective);
    }

    capacity_map
}

/// Apply blockade effects: emit headlines for nations whose enemy warships
/// are reducing trade cargo capacity.
///
/// Trade resolution itself already consumed the blockade-adjusted capacity
/// from `compute_blockade_capacity`; this pass only generates the player-
/// facing newspaper line.
pub(super) fn apply_blockade_effects(game: &GameState, report: &mut TurnReport) {
    let gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    for &nation_id in &gp_ids {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => continue,
        };

        let cargo = nation.total_cargo_capacity();
        if cargo == 0 {
            continue;
        }

        // Sum up enemy warship counts, gated by the one-turn grace period
        // (card #104: blockade effects don't apply on the declaration turn).
        let mut enemy_warship_count: u32 = 0;
        for &other_id in &gp_ids {
            if other_id == nation_id {
                continue;
            }
            let hostile = game
                .world.diplomacy
                .get_relation(nation_id, other_id)
                .is_some_and(|r| r.hostilities_active_on(game.turn));
            if hostile && let Some(other) = game.get_nation(other_id) {
                enemy_warship_count += other.warship_count() as u32;
            }
        }

        if enemy_warship_count > 0 {
            let effective_cargo = calculate_blockade_effect(cargo, enemy_warship_count);
            let blocked = cargo - effective_cargo;
            if blocked > 0 {
                let nation_name = &nation.name;
                report.newspaper_headlines.push(
                    Headline::new(
                        format!(
                            "BLOCKADE: {} merchant fleet loses {} cargo capacity to enemy warships",
                            nation_name, blocked
                        ),
                        HeadlineCategory::Battle,
                    )
                    .for_nation(nation_id),
                );
            }
        }
    }
}

/// Apply warehouse capacity caps to prevent infinite resource accumulation.
///
/// Each nation's Warehouse building capacity determines storage limits:
/// - Raw resources: capped at `50 * warehouse_capacity` per resource type
/// - Materials: capped at `50 * warehouse_capacity` per material type
/// - Finished goods: capped at `25 * warehouse_capacity` per goods type
///
/// Excess resources above the cap are silently lost (spoilage/waste).
/// Nations without a Warehouse building use a default capacity of 1.
pub(super) fn apply_warehouse_caps(game: &mut GameState) {
    for nation in &mut game.world.nations {
        let warehouse_capacity = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::Warehouse)
            .map(|b| b.effective_capacity())
            .unwrap_or(1);

        let raw_cap = 50 * warehouse_capacity;
        let material_cap = 50 * warehouse_capacity;
        let goods_cap = 25 * warehouse_capacity;

        for amount in nation.economy.warehouse.values_mut() {
            if *amount > raw_cap {
                *amount = raw_cap;
            }
        }

        for amount in nation.economy.materials.values_mut() {
            if *amount > material_cap {
                *amount = material_cap;
            }
        }

        for amount in nation.economy.goods.values_mut() {
            if *amount > goods_cap {
                *amount = goods_cap;
            }
        }
    }
}
