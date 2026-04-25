//! Economy phase of the turn pipeline.
//!
//! First chunk of the C-1 processor split: small, well-bounded economy
//! helpers (warehouse caps, building tick, blockade-aware capacity, unit
//! maintenance, and blockade headline generation) live here. Heavier
//! phases (`collect_resources`, `run_production`, `resolve_trade_session`)
//! remain in `processor.rs` for follow-up PRs.

use crate::economy::buildings::BuildingType;
use crate::events::{Headline, HeadlineCategory};
use crate::game_state::GameState;
use crate::military::naval::calculate_blockade_effect;
use crate::turn::processor::TurnReport;
use crate::types::*;
use std::collections::HashMap;

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
        if nation.is_in_anarchy {
            continue;
        }
        let total_cost: Money = nation
            .army
            .iter()
            .map(|u| u.maintenance_cost())
            .fold(Money::ZERO, |acc, c| acc + c);
        if total_cost != Money::ZERO {
            nation.economy.treasury -= total_cost;
            report.maintenance_costs.push((nation.id, total_cost));
        }

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

        // Generate bankruptcy headline if treasury went negative
        if nation.is_bankrupt() {
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
        .filter(|n| n.is_great_power() && !n.is_in_anarchy && !n.province_ids.is_empty())
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
