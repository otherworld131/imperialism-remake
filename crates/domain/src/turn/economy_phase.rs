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
//! The heavy execution functions (`collect_resources`, `run_production`,
//! `resolve_trade_session`) remain in `processor.rs` and are invoked from
//! `execute_reserved_economy`.

use crate::economy::buildings::BuildingType;
use crate::events::{Headline, HeadlineCategory};
use crate::game_state::GameState;
use crate::military::naval::calculate_blockade_effect;
use crate::turn::processor::TurnReport;
use crate::types::*;
use std::collections::HashMap;

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

/// An economic order that has been validated and whose resource requirements
/// have been pre-committed. In Phase 3 this is a structural placeholder —
/// the `ReservationId` API is implemented (#162) but full per-commodity wiring
/// into the execution pipeline is deferred to Trello #169 and #174.
#[derive(Debug, Clone)]
pub struct ReservedAction {
    pub order: EconomicOrder,
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

/// Validate orders and mark resource reservations.
///
/// Returns the subset of orders that are feasible and ready to execute.
/// In Phase 3 this is a thin pass-through — the structural seam exists so
/// Phase 4 can add real per-commodity reservation logic without a refactor
/// (tracked under Trello #169: pre-execution observability).
pub(super) fn validate_and_reserve(
    _game: &mut GameState,
    orders: Vec<EconomicOrder>,
) -> Vec<ReservedAction> {
    // DEFERRED (#169/#174): no reservation guarantees are active. Phase 4 replaces
    // this with per-nation NationEconomy::reserve calls; callers must not assume
    // executing a ReservedAction has committed any inventory.
    orders.into_iter().map(|order| ReservedAction { order }).collect()
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
        match action.order.kind {
            EconomicOrderKind::CollectTileResources => {
                super::processor::collect_resources(game, report);
            }
            EconomicOrderKind::RunProduction => {
                super::processor::run_production(game, report);
            }
            EconomicOrderKind::ExecuteTrade => {
                let blockade_capacity = compute_blockade_capacity(game);
                super::processor::resolve_trade_session(game, report, &blockade_capacity);
            }
        }
    }
}

/// Bankruptcy floor: treasury cannot go below $0.
const BANKRUPTCY_FLOOR: Money = Money::ZERO;

/// Tick all buildings for all nations, advancing expansion timers.
pub(super) fn tick_buildings(game: &mut GameState) {
    for nation in &mut game.nations {
        for building in &mut nation.economy.buildings {
            building.tick();
        }
    }
}

/// Apply army maintenance, bankruptcy clamp, and bankruptcy headline.
pub(super) fn apply_maintenance(game: &mut GameState, report: &mut TurnReport) {
    for nation in &mut game.nations {
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
        .nations
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
                .diplomacy
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
        .nations
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
                .diplomacy
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
    for nation in &mut game.nations {
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
