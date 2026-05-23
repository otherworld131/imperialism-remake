use crate::economy::civilians::CivilianType;
#[cfg(test)]
use crate::economy::civilians::{Civilian, next_civilian_id};
use crate::economy::labor::worker_food_demand;
use crate::game_state::GameState;
use crate::types::*;
use std::collections::BTreeMap;

/// AI queues immigrant workers to staff its industrial chain.
///
/// `desired` is the genuine labor shortfall: the labor needed to staff current
/// building capacity (`required_chain_labor`) minus the workforce already on
/// hand. The only cap applied here is food-network sustainability — the AI
/// recruits whenever the food network can feed more workers.
///
/// The per-turn province immigration rate is *not* applied here: it is a hard
/// game rule enforced by `process_pending_immigration`, which also caps actual
/// recruitment by the canned food / clothing / furniture on hand. Queueing more
/// than those rules allow is harmless and self-correcting.
pub(crate) fn ai_recruit_workers(game: &mut GameState, nation_id: NationId) {
    let cfg = &game.game_data.game_config;
    let min_workers_floor = cfg.labor_min_workers_floor;
    let max_queue = transport_capped_immigration_capacity(game, nation_id);
    let Some(nation) = game.get_nation(nation_id) else {
        return;
    };

    let total_workers = nation.economy.labor.total_workers();
    let desired = if total_workers < min_workers_floor {
        min_workers_floor.saturating_sub(total_workers)
    } else {
        let current_labor_units = nation.economy.labor.total_labor_units_with(
            cfg.untrained_labor,
            cfg.trained_labor,
            cfg.expert_labor,
        );
        let assigned_demand_units = nation
            .required_chain_labor(cfg)
            .saturating_add(queued_labor_commitments(game, nation_id));
        let labor_gap = assigned_demand_units.saturating_sub(current_labor_units);
        if labor_gap == 0 {
            0
        } else {
            labor_gap.div_ceil(cfg.untrained_labor.max(1))
        }
    };

    if let Some(nation) = game.get_nation_mut(nation_id) {
        nation.economy.pending_immigration = desired.min(max_queue);
    }
}

fn queued_labor_commitments(game: &GameState, nation_id: NationId) -> u32 {
    let Some(nation) = game.get_nation(nation_id) else {
        return 0;
    };
    let cfg = &game.game_data.game_config;

    let training_labor = nation
        .economy
        .pending_train_to_trained
        .saturating_mul(cfg.train_to_trained_labor_cost)
        .saturating_add(
            nation
                .economy
                .pending_train_to_expert
                .saturating_mul(cfg.train_to_expert_labor_cost),
        );

    let freight_labor = nation
        .economy
        .pending_freight_cars
        .saturating_mul(crate::economy::transport::TransportSystem::build_freight_car_cost().0);

    let civilian_labor = if cfg.civilian_costs_expert {
        nation
            .economy
            .pending_civilian_hires
            .values()
            .copied()
            .sum::<u32>()
            .saturating_mul(cfg.expert_labor)
    } else {
        0
    };

    let army_labor = nation
        .economy
        .pending_army_recruits
        .iter()
        .filter_map(|unit_str| {
            unit_str
                .parse::<crate::military::units::ArmyUnitType>()
                .ok()
        })
        .map(|unit_type| match unit_type.stats().recruit_tier {
            crate::economy::labor::WorkerType::Untrained => cfg.untrained_labor,
            crate::economy::labor::WorkerType::Trained => cfg.trained_labor,
            crate::economy::labor::WorkerType::Expert => cfg.expert_labor,
        })
        .sum::<u32>();

    training_labor
        .saturating_add(freight_labor)
        .saturating_add(civilian_labor)
        .saturating_add(army_labor)
}

fn transport_capped_immigration_capacity(game: &GameState, nation_id: NationId) -> u32 {
    let Some(nation) = game.get_nation(nation_id) else {
        return 0;
    };

    let current_workers = nation.economy.labor.total_workers();
    let supported_workers = max_workers_supported_by_food_network(
        game,
        nation_id,
        nation.economy.transport.freight_cars / 2,
    );

    supported_workers.saturating_sub(current_workers)
}

fn max_workers_supported_by_food_network(
    game: &GameState,
    nation_id: NationId,
    freight_capacity: u32,
) -> u32 {
    let (local_items, remote_items) =
        crate::economy::current_collectable_resources(game, nation_id);
    max_workers_supported_by_food_supply(
        &resource_totals(&local_items),
        &resource_totals(&remote_items),
        freight_capacity,
    )
}

fn resource_totals(items: &[(ResourceType, u32)]) -> BTreeMap<ResourceType, u32> {
    let mut totals = BTreeMap::new();
    for (resource, qty) in items {
        *totals.entry(*resource).or_insert(0) += *qty;
    }
    totals
}

fn supply_amount(supply: &BTreeMap<ResourceType, u32>, resource: ResourceType) -> u32 {
    supply.get(&resource).copied().unwrap_or(0)
}

fn max_workers_supported_by_food_supply(
    local_supply: &BTreeMap<ResourceType, u32>,
    remote_supply: &BTreeMap<ResourceType, u32>,
    freight_capacity: u32,
) -> u32 {
    let local_grain = supply_amount(local_supply, ResourceType::Grain);
    let local_fruit = supply_amount(local_supply, ResourceType::Fruit);
    let local_meat = supply_amount(local_supply, ResourceType::Livestock)
        .saturating_add(supply_amount(local_supply, ResourceType::Fish));

    let remote_grain = supply_amount(remote_supply, ResourceType::Grain);
    let remote_fruit = supply_amount(remote_supply, ResourceType::Fruit);
    let remote_meat = supply_amount(remote_supply, ResourceType::Livestock)
        .saturating_add(supply_amount(remote_supply, ResourceType::Fish));

    let max_possible_workers = local_grain
        .saturating_add(remote_grain.min(freight_capacity))
        .saturating_add(local_fruit)
        .saturating_add(remote_fruit.min(freight_capacity))
        .saturating_add(local_meat)
        .saturating_add(remote_meat.min(freight_capacity));

    let mut supported_workers = 0;
    for workers in 0..=max_possible_workers {
        let (grain_need, fruit_need, meat_need) = worker_food_demand(workers);
        let remote_grain_needed = grain_need.saturating_sub(local_grain);
        let remote_fruit_needed = fruit_need.saturating_sub(local_fruit);
        let remote_meat_needed = meat_need.saturating_sub(local_meat);
        let remote_total_needed = remote_grain_needed
            .saturating_add(remote_fruit_needed)
            .saturating_add(remote_meat_needed);

        if remote_grain_needed <= remote_grain
            && remote_fruit_needed <= remote_fruit
            && remote_meat_needed <= remote_meat
            && remote_total_needed <= freight_capacity
        {
            supported_workers = workers;
        }
    }

    supported_workers
}

/// Manage civilian units: hire new ones and deploy idle ones to improvable tiles.
///
/// Hiring rules:
/// - If < 2 civilians and treasury > $1,000: hire a Farmer ($100)
/// - If < 4 civilians and treasury > $2,000: hire a Forester ($100) or Miner ($1,500)
///
/// Deployment: for each idle civilian, find an improvable tile in the nation's provinces
/// that matches the civilian type and has improvement_level < max_improvement_level.
#[cfg(test)]
pub(crate) fn ai_manage_civilians(game: &mut GameState, nation_id: NationId) {
    // Phase 1: Hire civilians
    ai_hire_civilians(game, nation_id);

    // Phase 2: Deploy idle civilians
    ai_deploy_civilians(game, nation_id);
}

/// Hire new civilian units if the nation can afford them.
#[cfg(test)]
fn ai_hire_civilians(game: &mut GameState, nation_id: NationId) {
    // Extract config values before borrowing nation mutably.
    let cfg = game.game_data.game_config.clone();
    let tier1_threshold = Money::dollars(cfg.labor_hire_civilian_tier1_treasury);
    let tier1_max = cfg.labor_hire_civilian_tier1_max as usize;
    let tier2_threshold = Money::dollars(cfg.labor_hire_civilian_tier2_treasury);
    let tier2_max = cfg.labor_hire_civilian_tier2_max as usize;

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let civilian_count = nation.military.civilians.len();
    let treasury = nation.economy.treasury;

    // Rule 1: below tier-1 cap and treasury above tier-1 threshold → hire a Farmer
    if civilian_count < tier1_max && treasury > tier1_threshold {
        let cost = CivilianType::Farmer.creation_cost(&cfg);
        nation.economy.treasury -= cost;
        let farmer = Civilian::new(next_civilian_id(), CivilianType::Farmer, nation_id);
        nation.military.civilians.push(farmer);
        return; // Only hire one per turn
    }

    // Rule 2: below tier-2 cap and treasury above tier-2 threshold → hire Forester or Miner
    if civilian_count < tier2_max && treasury > tier2_threshold {
        // Prefer Forester (cheaper) unless we already have one
        let has_forester = nation
            .military
            .civilians
            .iter()
            .any(|c| c.civilian_type == CivilianType::Forester);
        let civ_type = if has_forester {
            CivilianType::Miner
        } else {
            CivilianType::Forester
        };

        let cost = civ_type.creation_cost(&cfg);
        if let Some(remaining) = nation.economy.treasury.checked_sub(cost) {
            nation.economy.treasury = remaining;
            let civilian = Civilian::new(next_civilian_id(), civ_type, nation_id);
            nation.military.civilians.push(civilian);
        }
    }
}

/// Deploy idle civilians to improvable tiles in the nation's provinces.
pub(crate) fn ai_deploy_civilians(game: &mut GameState, nation_id: NationId) {
    // Collect province IDs owned by this nation
    let province_ids: Vec<ProvinceId> = match game.get_nation(nation_id) {
        Some(n) => n.province_ids.clone(),
        None => return,
    };

    // Early-out if no idle non-engineer civilians exist — avoids the
    // depot-plan / rail-adjacency precomputation when nothing will deploy.
    let has_idle = match game.get_nation(nation_id) {
        Some(n) => n.military.civilians.iter().any(|c| {
            !c.working && c.turns_remaining == 0 && c.civilian_type != CivilianType::Engineer
        }),
        None => return,
    };
    if !has_idle {
        return;
    }

    // Precompute the set of tiles currently harvesting to the capital. We
    // prefer deploying civilians onto these (their improvements turn into
    // real yield).
    //
    // Card #217: extend the preference into ordered buckets so improvements
    // queue up where rail will arrive next, not on arbitrary disconnected
    // tiles:
    //   collectable        already harvestable
    //   planned            on the depot planner's current path / radius
    //   rail_adjacent      adjacent to existing rail or depot
    //   (everything else is unconnected and least preferred)
    let collectable: std::collections::HashSet<crate::hex::HexCoord> = {
        let connected = super::super::turn::connected_provinces(game, nation_id);
        let owned_provinces: Vec<&crate::map::Province> = game
            .world
            .provinces
            .iter()
            .filter(|p| p.owner == nation_id)
            .collect();
        crate::map::infrastructure::collectable_hexes(
            &game.world.hex_map,
            &owned_provinces,
            &connected,
        )
    };
    // Tiles the depot planner intends to connect soon: every hex on the
    // current plan's path, plus the 1-hex radius around the planned
    // candidate (the area that becomes collectable once the depot is
    // built). Empty when the AI has no active plan.
    let planned: std::collections::HashSet<crate::hex::HexCoord> = {
        match super::economy::plan_next_depot(game, nation_id).as_plan() {
            Some(plan) => {
                let mut s: std::collections::HashSet<crate::hex::HexCoord> =
                    plan.path.iter().copied().collect();
                s.insert(plan.candidate);
                for n in plan.candidate.neighbors().iter().copied() {
                    s.insert(n);
                }
                s
            }
            None => std::collections::HashSet::new(),
        }
    };
    // Tiles adjacent to *our* existing rail or depot — easy targets for a
    // future minor rail extension. Foreign rail networks are excluded
    // because the nation can't piggyback off another power's infrastructure.
    let rail_adjacent: std::collections::HashSet<crate::hex::HexCoord> = {
        let owned_set: std::collections::HashSet<crate::hex::HexCoord> = game
            .world
            .provinces
            .iter()
            .filter(|p| p.owner == nation_id)
            .flat_map(|p| p.tiles.iter().copied())
            .collect();
        let mut s = std::collections::HashSet::new();
        for &coord in &owned_set {
            let Some(tile) = game.world.hex_map.get_tile(coord) else {
                continue;
            };
            if tile.infrastructure.has_railroad || tile.infrastructure.has_depot {
                for n in coord.neighbors().iter().copied() {
                    s.insert(n);
                }
            }
        }
        s
    };
    // Cash-rich softening: when treasury surplus over the personality's
    // spending reserve is large, compress the unconnected-bucket weights
    // toward zero so a rich AI doesn't sit idle waiting for rail.
    let softening: f64 = {
        let cfg = &game.game_data.game_config;
        let personality = super::common::get_personality(game, nation_id);
        let reserve =
            super::common::PersonalityConfig::for_personality(personality).spending_reserve;
        let treasury = game
            .get_nation(nation_id)
            .map(|n| n.economy.treasury)
            .unwrap_or(Money::ZERO);
        let surplus = (treasury - reserve).as_dollars().max(0) as f64;
        let threshold = cfg.civilian_connectivity_softening_threshold.max(1) as f64;
        1.0 / (1.0 + surplus / threshold)
    };
    let cfg = &game.game_data.game_config;
    let bucket_collectable: u32 = 0;
    // Weights are integer-truncated so heavy softening can collapse them
    // toward zero, at which point the improvement-level tiebreaker decides.
    let bucket_planned: u32 = (cfg.civilian_connectivity_planned_weight * softening) as u32;
    let bucket_adjacent: u32 = (cfg.civilian_connectivity_adjacent_weight * softening) as u32;
    let bucket_unconnected: u32 = (cfg.civilian_connectivity_unconnected_weight * softening) as u32;
    // Combined sort key: `bucket << 8 | improvement_level`. Connectivity
    // dominates while the bucket gap is wide; once softening has collapsed
    // the gap toward zero (cash-rich case) the improvement-level term takes
    // over and tiebreaks to the lower-level tile.
    let sort_key_for = |coord: crate::hex::HexCoord, improvement: u8| -> u32 {
        let bucket = if collectable.contains(&coord) {
            bucket_collectable
        } else if planned.contains(&coord) {
            bucket_planned
        } else if rail_adjacent.contains(&coord) {
            bucket_adjacent
        } else {
            bucket_unconnected
        };
        bucket
            .saturating_mul(256)
            .saturating_add(improvement as u32)
    };

    // Snapshot the nation's researched techs so we can consult the tech-gated
    // improvement cap without re-borrowing `game` each tile.
    let researched_techs: Vec<crate::events::TechId> = match game.get_nation(nation_id) {
        Some(n) => n.researched_techs.clone(),
        None => return,
    };

    // Find all improvable tiles across the nation's provinces.
    // Each entry: (coord, terrain, resource, improvement_level, max_level, has_civilian_assigned)
    let mut improvable_tiles: Vec<(
        crate::hex::HexCoord,
        TerrainType,
        Option<ResourceType>,
        u8,
        u8,
        bool,
    )> = Vec::new();
    // Un-prospected deposit-eligible tiles — Prospectors target these. Stored
    // separately because they don't have a `resource_deposit` known to the
    // nation and the existing improvable_tiles entries assume one is present.
    let mut unprospected_tiles: Vec<(crate::hex::HexCoord, TerrainType, bool)> = Vec::new();
    for &pid in &province_ids {
        for (coord, tile) in game.world.hex_map.tiles_in_province(pid) {
            let terrain = tile.terrain();
            // Prospector targets: deposit-capable terrain that hasn't been
            // searched AND has no visible resource. Hills with visible Wool
            // are deposit-capable terrain but a Prospector has nothing to do
            // there — the Wool is already known.
            if terrain.can_have_deposits() && !tile.is_prospected() && !tile.has_visible_resource()
            {
                unprospected_tiles.push((coord, terrain, tile.assigned_civilian.is_some()));
                continue;
            }
            // Per the manual, hidden minerals are unknown until prospected;
            // the AI must not deploy improvers to un-prospected hidden tiles.
            if !tile.has_visible_resource() {
                continue;
            }
            let resource = tile.resource_deposit();
            let max_level = game.game_data.tech_tree.effective_max_improvement_level(
                terrain,
                resource,
                &researched_techs,
            );
            if max_level > 0 && tile.improvement_level() < max_level {
                let has_assigned = tile.assigned_civilian.is_some();
                improvable_tiles.push((
                    coord,
                    terrain,
                    resource,
                    tile.improvement_level(),
                    max_level,
                    has_assigned,
                ));
            }
        }
    }

    // Get idle civilian indices and their types. Engineers are excluded —
    // they are driven by `ai/spending.rs::execute_infrastructure` (AI) and
    // `wasm_engineer_build` (player); deploying them here would start a
    // generic `start_work` with no `build_task`, producing a no-op completion.
    let idle_civilians: Vec<(usize, CivilianType)> = match game.get_nation(nation_id) {
        Some(n) => n
            .military
            .civilians
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                !c.working && c.turns_remaining == 0 && c.civilian_type != CivilianType::Engineer
            })
            .map(|(i, c)| (i, c.civilian_type))
            .collect(),
        None => return,
    };

    // For each idle civilian, try to find a matching tile. Preference order:
    // (a) tiles currently in `collectable` (so the improvement immediately
    //     produces yield), then
    // (b) lowest current improvement level (ramp weakest tiles first).
    //
    // Prospectors are routed to the `unprospected_tiles` pool — they don't
    // improve resources, they reveal hidden deposits.
    for (civ_idx, civ_type) in idle_civilians {
        if civ_type == CivilianType::Prospector {
            let best = unprospected_tiles
                .iter()
                .enumerate()
                .filter(|(_, (_, _, has_assigned))| !has_assigned)
                .min_by_key(|(_, (coord, _, _))| sort_key_for(*coord, 0));
            if let Some((tile_idx, &(coord, _, _))) = best {
                unprospected_tiles[tile_idx].2 = true;
                let Some(nation) = game.get_nation_mut(nation_id) else {
                    return;
                };
                let civilian_id = nation.military.civilians[civ_idx].id;
                if let Some(old_pos) = nation.military.civilians[civ_idx].position
                    && let Some(old_tile) = game.world.hex_map.get_tile_mut(old_pos)
                    && old_tile.assigned_civilian == Some(civilian_id)
                {
                    old_tile.assigned_civilian = None;
                }
                let Some(nation) = game.get_nation_mut(nation_id) else {
                    return;
                };
                nation.military.civilians[civ_idx].deploy(coord);
                nation.military.civilians[civ_idx].start_work(1);
                if let Some(tile) = game.world.hex_map.get_tile_mut(coord) {
                    tile.assigned_civilian = Some(civilian_id);
                }
            }
            continue;
        }

        let best_tile = improvable_tiles
            .iter()
            .enumerate()
            .filter(|(_, (_, terrain, resource, _, _, has_assigned))| {
                !has_assigned && civ_type.can_improve(*terrain, *resource)
            })
            .min_by_key(|(_, (coord, _, _, improvement, _, _))| {
                // Card #217: combined connectivity + improvement-level key
                // so heavy softening can collapse the bucket gap and let
                // the lowest-level tile win across buckets.
                sort_key_for(*coord, *improvement)
            });

        if let Some((tile_idx, &(coord, _, _, _, _, _))) = best_tile {
            // Mark the tile as assigned in our working list
            improvable_tiles[tile_idx].5 = true;

            // Deploy the civilian and start work
            let Some(nation) = game.get_nation_mut(nation_id) else {
                return;
            };
            let civilian_id = nation.military.civilians[civ_idx].id;
            // Clear the old tile's slot before redeploying so stale IDs don't
            // block the engineer from building railroad on previously-worked hexes.
            if let Some(old_pos) = nation.military.civilians[civ_idx].position
                && let Some(old_tile) = game.world.hex_map.get_tile_mut(old_pos)
                && old_tile.assigned_civilian == Some(civilian_id)
            {
                old_tile.assigned_civilian = None;
            }
            let Some(nation) = game.get_nation_mut(nation_id) else {
                return;
            };
            nation.military.civilians[civ_idx].deploy(coord);
            nation.military.civilians[civ_idx].start_work(2);

            // Mark the tile on the map
            if let Some(tile) = game.world.hex_map.get_tile_mut(coord) {
                tile.assigned_civilian = Some(civilian_id);
            }
        }
    }
}

/// AI queues worker training and promotion via the deferred end-of-turn
/// pipeline (`process_pending_worker_training` in the turn processor). The
/// queue size is driven by the **chain-labor gap**: how many labor units
/// short the workforce is of staffing the six core chain buildings at their
/// current `effective_capacity`. Each new trained worker yields +1 net labor
/// (untrained 1 → trained 2); each new expert yields +2 (trained 2 → expert 4).
///
/// When the gap is closed the AI falls back to the historic slow-drip
/// thresholds (Lua-tunable per personality) so the workforce keeps advancing
/// even with a healthy chain.
#[allow(unused_labels, unused_variables)] // labeled blocks + personality used only with cfg(feature = "lua")
pub(crate) fn ai_train_and_promote_workers(game: &mut GameState, nation_id: NationId) {
    let personality = super::common::get_personality(game, nation_id);
    let cfg = game.game_data.game_config.clone();

    // ── Read Lua config (feature-gated) ──────────────────────
    let lua_cfg = super::lua_bridge::get_personality_config(game, personality);

    let train_threshold: u32 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.worker_train_threshold) {
            break 'val v;
        }
        1
    };
    let promote_threshold: u32 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.worker_promote_threshold) {
            break 'val v;
        }
        2
    };

    let Some(nation) = game.get_nation(nation_id) else {
        return;
    };

    let required = nation.required_chain_labor(&cfg);
    let worker_labor = nation.economy.labor.total_labor_units_with(
        cfg.untrained_labor,
        cfg.trained_labor,
        cfg.expert_labor,
    );
    let untrained = nation.economy.labor.untrained;
    let trained = nation.economy.labor.trained;
    let gap = required.saturating_sub(worker_labor);

    let (queue_train, queue_promote) = if gap > 0 {
        let qt = gap.min(untrained);
        let remaining = gap.saturating_sub(qt);
        let qp = remaining.div_ceil(2).min(trained);
        (qt, qp)
    } else {
        let qt = if untrained > train_threshold { 1 } else { 0 };
        let qp = if trained > promote_threshold { 1 } else { 0 };
        (qt, qp)
    };

    if queue_train == 0 && queue_promote == 0 {
        return;
    }

    if let Some(nation) = game.get_nation_mut(nation_id) {
        nation.economy.pending_train_to_trained = nation
            .economy
            .pending_train_to_trained
            .saturating_add(queue_train);
        nation.economy.pending_train_to_expert = nation
            .economy
            .pending_train_to_expert
            .saturating_add(queue_promote);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::test_game_with_ai;
    use crate::economy::buildings::{Building, BuildingType};
    use crate::economy::civilians::{Civilian, CivilianType};
    use crate::hex::HexCoord;
    use crate::map::UnitId;
    use std::collections::BTreeMap;

    // ── Worker recruitment ───────────────────────────────────

    #[test]
    fn ai_bootstraps_to_worker_floor_when_inputs_exist() {
        let mut game = test_game_with_ai();
        seed_ai_food_network(&mut game, &[], 0);
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.add_material(MaterialType::CannedFood, 2);
        ai.add_goods(GoodsType::Clothing, 2);
        ai.add_goods(GoodsType::Furniture, 2);
        for i in 50..=53 {
            ai.add_province(ProvinceId(i));
        }

        ai_recruit_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.pending_immigration, 1,
            "AI should bootstrap toward the minimum worker floor"
        );
    }

    #[test]
    fn ai_does_not_queue_immigration_when_spare_labor_exists() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 6;
        ai.add_material(MaterialType::CannedFood, 5);
        ai.add_goods(GoodsType::Clothing, 5);
        ai.add_goods(GoodsType::Furniture, 5);
        for i in 60..=63 {
            ai.add_province(ProvinceId(i));
        }

        ai_recruit_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.pending_immigration, 0,
            "AI should not queue immigrants just to chase a province-count target"
        );
    }

    #[test]
    fn ai_does_not_queue_immigration_without_inputs() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 70..=73 {
            ai.add_province(ProvinceId(i));
        }

        ai_recruit_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.pending_immigration, 0,
            "AI should not queue immigration without the required inputs"
        );
    }

    // ── Civilian management ─────────────────────────────────

    #[test]
    fn ai_hires_farmer_when_few_civilians_and_can_afford() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(5000);
        ai.military.civilians.clear(); // Start with 0 civilians

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.military.civilians.len(),
            1,
            "AI should hire 1 Farmer when it has < 2 civilians"
        );
        assert_eq!(ai.military.civilians[0].civilian_type, CivilianType::Farmer);
        assert_eq!(
            ai.economy.treasury,
            Money::dollars(4900),
            "Treasury should be reduced by $100 (Farmer cost)"
        );
    }

    #[test]
    fn ai_does_not_hire_civilian_when_too_poor() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(500); // Below $1,000 threshold
        ai.military.civilians.clear(); // Start with 0 civilians

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.military.civilians.is_empty(),
            "AI should not hire civilians when treasury <= $1,000"
        );
    }

    #[test]
    fn ai_hires_forester_when_has_two_civilians_and_enough_money() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(5000);
        ai.military.civilians.clear();
        // Give AI 2 existing civilians (both farmers)
        ai.military.civilians.push(Civilian::new(
            UnitId(900),
            CivilianType::Farmer,
            NationId(2),
        ));
        ai.military.civilians.push(Civilian::new(
            UnitId(901),
            CivilianType::Farmer,
            NationId(2),
        ));

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.military.civilians.len(),
            3,
            "AI should hire a 3rd civilian (Forester)"
        );
        assert_eq!(
            ai.military.civilians[2].civilian_type,
            CivilianType::Forester,
            "3rd civilian should be a Forester"
        );
    }

    #[test]
    fn ai_hires_miner_when_already_has_forester() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(5000);
        ai.military.civilians.clear();
        // Give AI 2 existing civilians including a forester
        ai.military.civilians.push(Civilian::new(
            UnitId(900),
            CivilianType::Farmer,
            NationId(2),
        ));
        ai.military.civilians.push(Civilian::new(
            UnitId(901),
            CivilianType::Forester,
            NationId(2),
        ));

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.military.civilians.len(), 3);
        assert_eq!(
            ai.military.civilians[2].civilian_type,
            CivilianType::Miner,
            "Should hire Miner when Forester already exists"
        );
        assert_eq!(
            ai.economy.treasury,
            Money::dollars(3500),
            "Miner costs $1,500"
        );
    }

    #[test]
    fn ai_deploys_idle_civilian_to_improvable_tile() {
        let mut game = test_game_with_ai();

        // Set up a Grassland tile with Grain in AI's province
        let farm_coord = HexCoord::new(3, 3);
        let mut tile = crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        tile.set_resource(ResourceType::Grain);
        game.world.hex_map.set_tile(farm_coord, tile);

        // Give AI a Farmer civilian (idle), clear pre-populated ones, and
        // research Seed Drill so the Farm tile is improvable per the tech
        // tree's `EnableTerrainImprovement Farm L1` gate.
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(500); // Not enough for hiring
        ai.military.civilians.clear();
        ai.military.civilians.push(Civilian::new(
            UnitId(950),
            CivilianType::Farmer,
            NationId(2),
        ));
        ai.researched_techs.push(crate::events::TechId(2)); // Seed Drill

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.military.civilians.len(),
            1,
            "Should still have 1 civilian"
        );
        assert!(
            ai.military.civilians[0].working,
            "Civilian should be working"
        );
        assert_eq!(
            ai.military.civilians[0].position,
            Some(farm_coord),
            "Civilian should be deployed to the farm tile"
        );

        // Check that the tile has the civilian assigned
        let tile = game.world.hex_map.get_tile(farm_coord).unwrap();
        assert_eq!(
            tile.assigned_civilian,
            Some(UnitId(950)),
            "Tile should have the civilian assigned"
        );
    }

    // ── Card #217: connectivity-aware deployment ──────────────

    /// Helper: extend the AI's province (id 2) with extra owned tiles.
    fn add_owned_tiles(game: &mut GameState, coords: &[HexCoord]) {
        // Update the province's tile list.
        let prov = game
            .world
            .provinces
            .iter_mut()
            .find(|p| p.id == ProvinceId(2))
            .expect("AI province");
        for &c in coords {
            if !prov.tiles.contains(&c) {
                prov.tiles.push(c);
            }
        }
        // Add the AI nation as owner of the province (already so, but keep it
        // simple) and ensure each tile carries the province id.
        for &c in coords {
            // If the hex_map doesn't have a tile at c yet, set one.
            if game.world.hex_map.get_tile(c).is_none() {
                let t =
                    crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
                game.world.hex_map.set_tile(c, t);
            } else if let Some(tile) = game.world.hex_map.get_tile_mut(c) {
                tile.province_id = Some(ProvinceId(2));
            }
        }
    }

    fn seed_ai_food_network(
        game: &mut GameState,
        remote_resources: &[ResourceType],
        freight_cars: u32,
    ) {
        let capital = HexCoord::new(3, 3);
        let mut capital_tile =
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        capital_tile.set_resource(ResourceType::Grain);
        capital_tile.is_country_capital = true;
        game.world.hex_map.set_tile(capital, capital_tile);

        let remote_coords: Vec<HexCoord> = capital.neighbors().into_iter().collect();
        add_owned_tiles(game, &remote_coords[..remote_resources.len()]);
        for (coord, resource) in remote_coords.iter().zip(remote_resources.iter()) {
            let mut tile =
                crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
            tile.set_resource(*resource);
            game.world.hex_map.set_tile(*coord, tile);
        }

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.transport.freight_cars = freight_cars;
    }

    #[test]
    fn ai_deploy_prefers_rail_adjacent_over_unconnected() {
        let mut game = test_game_with_ai();

        // Two improvable Grain tiles: one adjacent to rail, one isolated.
        let near_rail = HexCoord::new(5, 5);
        let isolated = HexCoord::new(8, 8);
        let rail_hex = HexCoord::new(6, 5); // neighbor of near_rail; not owned

        add_owned_tiles(&mut game, &[near_rail, isolated]);

        let mut t1 = crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        t1.set_resource(ResourceType::Grain);
        game.world.hex_map.set_tile(near_rail, t1);

        let mut t2 = crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        t2.set_resource(ResourceType::Grain);
        game.world.hex_map.set_tile(isolated, t2);

        // Lay a railroad somewhere unowned but adjacent to `near_rail`. The
        // sort key looks at *any* rail tile in the map, not just owned ones.
        let mut rt = crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(99));
        rt.infrastructure.has_railroad = true;
        game.world.hex_map.set_tile(rail_hex, rt);

        // Single idle Farmer; need Seed Drill so Farm tiles are improvable.
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(0); // no hiring
        ai.military.civilians.clear();
        ai.military.civilians.push(Civilian::new(
            UnitId(700),
            CivilianType::Farmer,
            NationId(2),
        ));
        ai.researched_techs.push(crate::events::TechId(2)); // Seed Drill

        ai_deploy_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.military.civilians[0].position,
            Some(near_rail),
            "Farmer must prefer rail-adjacent tile over isolated tile"
        );
    }

    #[test]
    fn ai_deploy_cash_rich_softening_flips_choice_between_buckets() {
        // Card #217: when treasury surplus is small the connectivity bucket
        // dominates and the AI picks the rail-adjacent tile; when the
        // surplus dwarfs `civilian_connectivity_softening_threshold` the
        // bucket gap collapses and the (improvement_level) tiebreaker takes
        // over, flipping the choice to the unconnected (but lower-level,
        // higher-headroom) tile.
        //
        // Layout (both improvable to L2 with Seed Drill + Steel/Iron Plows):
        //   - owned_rail (6,5): owned, has railroad (no resource).
        //   - near_rail  (5,5): rail-adjacent (neighbor of owned_rail),
        //                       Grain at improvement_level 1.
        //   - isolated   (8,8): unconnected, Grain at improvement_level 0.

        fn run_with_treasury(treasury_dollars: i64) -> Option<HexCoord> {
            let mut game = test_game_with_ai();

            let owned_rail = HexCoord::new(6, 5);
            let near_rail = HexCoord::new(5, 5);
            let isolated = HexCoord::new(8, 8);

            add_owned_tiles(&mut game, &[owned_rail, near_rail, isolated]);

            let mut rt =
                crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
            rt.infrastructure.has_railroad = true;
            game.world.hex_map.set_tile(owned_rail, rt);

            let mut t1 =
                crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
            t1.set_resource(ResourceType::Grain);
            t1.set_improvement_level(1);
            game.world.hex_map.set_tile(near_rail, t1);

            let mut t2 =
                crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
            t2.set_resource(ResourceType::Grain);
            t2.set_improvement_level(0);
            game.world.hex_map.set_tile(isolated, t2);

            let ai = game.get_nation_mut(NationId(2)).unwrap();
            ai.economy.treasury = Money::dollars(treasury_dollars);
            ai.military.civilians.clear();
            ai.military.civilians.push(Civilian::new(
                UnitId(710),
                CivilianType::Farmer,
                NationId(2),
            ));
            // Seed Drill (Farm L1) + Steel and Iron Plows (Farm L2) so both
            // tiles remain improvable: near_rail (1 → 2) and isolated (0 → 2).
            ai.researched_techs.push(crate::events::TechId(2));
            ai.researched_techs.push(crate::events::TechId(10));

            ai_deploy_civilians(&mut game, NationId(2));
            game.get_nation(NationId(2))
                .and_then(|n| n.military.civilians.first().and_then(|c| c.position))
        }

        let near_rail = HexCoord::new(5, 5);
        let isolated = HexCoord::new(8, 8);

        // Cash-tight (treasury == reserve, no surplus): bucket dominates.
        // PersonalityConfig::Aggressive uses spending_reserve = $500.
        let cash_tight = run_with_treasury(500).expect("cash-tight AI must deploy a civilian");
        assert_eq!(
            cash_tight, near_rail,
            "cash-tight: bucket preference must beat the improvement-level tiebreaker"
        );

        // Cash-rich (treasury vastly above the $20k softening threshold):
        // bucket gap collapses → tiebreaker on improvement_level wins, and
        // isolated (level 0) beats near_rail (level 1).
        let cash_rich = run_with_treasury(50_000_000).expect("cash-rich AI must deploy a civilian");
        assert_eq!(
            cash_rich, isolated,
            "cash-rich softening must let the lower-improvement unconnected tile win"
        );
    }

    #[test]
    fn ai_deploy_softening_lets_isolated_tile_win_when_only_unconnected_improvable() {
        // Direct test of softening: with treasury well above reserve and
        // the rail-adjacent tile already maxed, the AI must happily deploy
        // on the disconnected tile (the only improvable one). This case
        // also passes without softening (the legacy fall-through), but
        // makes the regression detectable if the unconnected bucket is
        // ever made truly prohibitive.
        let mut game = test_game_with_ai();

        let near_rail = HexCoord::new(5, 5);
        let isolated = HexCoord::new(8, 8);
        let owned_rail = HexCoord::new(6, 5);

        add_owned_tiles(&mut game, &[owned_rail, near_rail, isolated]);

        let mut rt = crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        rt.infrastructure.has_railroad = true;
        game.world.hex_map.set_tile(owned_rail, rt);

        // near_rail: maxed → not improvable.
        let mut t1 = crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        t1.set_resource(ResourceType::Grain);
        t1.set_improvement_level(1);
        game.world.hex_map.set_tile(near_rail, t1);

        // isolated: L0 → improvable.
        let mut t2 = crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        t2.set_resource(ResourceType::Grain);
        game.world.hex_map.set_tile(isolated, t2);

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(10_000_000);
        ai.military.civilians.clear();
        ai.military.civilians.push(Civilian::new(
            UnitId(720),
            CivilianType::Farmer,
            NationId(2),
        ));
        ai.researched_techs.push(crate::events::TechId(2));

        ai_deploy_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.military.civilians[0].position,
            Some(isolated),
            "Cash-rich AI must deploy to the only improvable tile, even disconnected"
        );
    }

    #[test]
    fn ai_does_not_deploy_civilian_to_maxed_tile() {
        let mut game = test_game_with_ai();

        // Set up a Grassland tile with Grain at max improvement
        let farm_coord = HexCoord::new(3, 3);
        let mut tile = crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        tile.set_resource(ResourceType::Grain);
        tile.set_improvement_level(3); // max for Grain
        game.world.hex_map.set_tile(farm_coord, tile);

        // Give AI a Farmer civilian (idle), clear pre-populated ones
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(500);
        ai.military.civilians.clear();
        ai.military.civilians.push(Civilian::new(
            UnitId(960),
            CivilianType::Farmer,
            NationId(2),
        ));

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        // Civilian should stay idle because no improvable tiles
        assert!(
            !ai.military.civilians[0].working,
            "Civilian should remain idle when no improvable tiles exist"
        );
        assert_eq!(
            ai.military.civilians[0].position, None,
            "Civilian should not be deployed"
        );
    }

    #[test]
    fn ai_queues_immigration_when_inputs_and_capacity_exist() {
        let mut game = test_game_with_ai();
        seed_ai_food_network(&mut game, &[], 0);
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 0;
        ai.economy.labor.trained = 0;
        ai.economy.labor.expert = 0;
        ai.add_goods(GoodsType::Furniture, 3);
        for i in 50..=53 {
            ai.add_province(ProvinceId(i));
        }
        ai.add_material(MaterialType::CannedFood, 3);
        ai.add_goods(GoodsType::Clothing, 3);

        ai_recruit_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.pending_immigration, 1,
            "AI should queue up to its province-based immigration rate"
        );
    }

    #[test]
    fn ai_recruit_decision_ignores_missing_goods_stockpiles() {
        // The AI no longer gates its recruit decision on canned food / clothing
        // / furniture stockpiles — those are consumed (and capped) later by the
        // turn processor. With unstaffed capacity and a sustaining food network
        // it queues immigrants even with zero clothing/furniture/canned food.
        let mut game = test_game_with_ai();
        seed_ai_food_network(
            &mut game,
            &[
                ResourceType::Grain,
                ResourceType::Grain,
                ResourceType::Grain,
                ResourceType::Fruit,
                ResourceType::Fruit,
                ResourceType::Livestock,
            ],
            20,
        );
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 6;
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 4));
        ai.add_resource(ResourceType::Grain, 6);
        ai.add_resource(ResourceType::Fruit, 6);
        ai.add_resource(ResourceType::Fish, 6);
        // Deliberately no clothing, furniture, or canned food on hand.
        for i in 60..=63 {
            ai.add_province(ProvinceId(i));
        }

        ai_recruit_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.economy.pending_immigration > 0,
            "AI should queue immigration despite empty goods stockpiles (got {})",
            ai.economy.pending_immigration
        );
    }

    #[test]
    fn ai_can_queue_immigration_from_projected_food_processing_output() {
        let mut game = test_game_with_ai();
        seed_ai_food_network(&mut game, &[ResourceType::Grain, ResourceType::Fruit], 4);
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 2;
        ai.economy.labor.trained = 0;
        ai.economy.labor.expert = 0;
        ai.economy
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 2));
        for i in 70..=73 {
            ai.add_province(ProvinceId(i));
        }
        ai.add_resource(ResourceType::Grain, 6);
        ai.add_resource(ResourceType::Fruit, 6);
        ai.add_resource(ResourceType::Fish, 6);
        ai.add_goods(GoodsType::Clothing, 2);
        ai.add_goods(GoodsType::Furniture, 2);

        ai_recruit_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.pending_immigration, 1,
            "AI should account for canned food that will be produced later in the turn"
        );
        assert_eq!(
            ai.material_amount(MaterialType::CannedFood),
            0,
            "Planning immigration should not mutate stockpiles during AI setup"
        );
    }

    #[test]
    fn ai_counts_pending_freight_car_labor_before_queueing_immigration() {
        let mut game = test_game_with_ai();
        seed_ai_food_network(
            &mut game,
            &[
                ResourceType::Grain,
                ResourceType::Grain,
                ResourceType::Grain,
                ResourceType::Fruit,
                ResourceType::Fruit,
                ResourceType::Livestock,
            ],
            20,
        );
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 6;
        ai.economy.pending_freight_cars = 4;
        ai.add_resource(ResourceType::Grain, 6);
        ai.add_resource(ResourceType::Fruit, 6);
        ai.add_resource(ResourceType::Fish, 6);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Steel, 10);
        ai.add_material(MaterialType::CannedFood, 5);
        ai.add_goods(GoodsType::Clothing, 5);
        ai.add_goods(GoodsType::Furniture, 5);
        for i in 90..=96 {
            ai.add_province(ProvinceId(i));
        }

        ai_recruit_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.pending_immigration, 1,
            "Pending freight-car labor should consume spare labor before immigration is capped by per-turn immigration capacity"
        );
    }

    #[test]
    fn ai_recruits_to_staff_current_building_capacity() {
        // A LumberMill at capacity 4 needs 8 labor units (cap * labor_per_production);
        // 6 untrained workers supply only 6. The AI must queue immigrants to close
        // that gap, not settle for whatever its current workforce already staffs.
        let mut game = test_game_with_ai();
        seed_ai_food_network(
            &mut game,
            &[
                ResourceType::Grain,
                ResourceType::Grain,
                ResourceType::Grain,
                ResourceType::Fruit,
                ResourceType::Fruit,
                ResourceType::Livestock,
            ],
            20,
        );
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 6;
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 4));
        ai.economy.chain_targets.timber_mill = 12;
        ai.add_resource(ResourceType::Timber, 24);
        ai.add_resource(ResourceType::Grain, 6);
        ai.add_resource(ResourceType::Fruit, 6);
        ai.add_resource(ResourceType::Fish, 6);
        ai.add_material(MaterialType::CannedFood, 5);
        ai.add_goods(GoodsType::Clothing, 5);
        ai.add_goods(GoodsType::Furniture, 5);
        for i in 80..=86 {
            ai.add_province(ProvinceId(i));
        }

        ai_recruit_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.economy.pending_immigration > 0,
            "AI should recruit to staff current building capacity (got {})",
            ai.economy.pending_immigration
        );
    }

    #[test]
    fn food_support_cap_limits_immigration_to_half_freight_food_network() {
        let local_supply = BTreeMap::from([
            (ResourceType::Grain, 2),
            (ResourceType::Fruit, 1),
            (ResourceType::Fish, 1),
        ]);
        let remote_supply = BTreeMap::from([
            (ResourceType::Grain, 4),
            (ResourceType::Fruit, 4),
            (ResourceType::Fish, 4),
        ]);

        assert_eq!(
            max_workers_supported_by_food_supply(&local_supply, &remote_supply, 2),
            6,
            "With only two freight units, the network should support at most six workers"
        );
    }

    // ── AI worker training/promotion tests ──────────────────

    #[test]
    fn ai_queues_training_when_untrained_above_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 5; // > default train_threshold (1)
        ai.economy.labor.trained = 0;
        ai.add_material(MaterialType::Paper, 2);

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        // Test nation has no chain buildings → required_chain_labor = 0 →
        // gap = 0 → slow-drip path queues 1 training.
        assert_eq!(
            ai.economy.pending_train_to_trained, 1,
            "Should queue 1 training when gap=0 and untrained > threshold"
        );
        assert_eq!(
            ai.economy.labor.untrained, 5,
            "Queueing does not mutate the pool — the end-turn processor does"
        );
        assert_eq!(
            ai.material_amount(MaterialType::Paper),
            2,
            "Queueing does not consume paper — the processor does"
        );
    }

    #[test]
    fn ai_does_not_queue_training_at_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 1; // at default threshold, not above
        ai.economy.labor.trained = 0;
        ai.add_material(MaterialType::Paper, 2);

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.economy.pending_train_to_trained, 0);
        assert_eq!(ai.economy.pending_train_to_expert, 0);
    }

    #[test]
    fn ai_queues_promotion_when_trained_above_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 0;
        ai.economy.labor.trained = 5; // > default promote_threshold (2)
        ai.economy.labor.expert = 0;

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.economy.pending_train_to_expert, 1);
        assert_eq!(
            ai.economy.labor.trained, 5,
            "Queueing does not mutate the pool"
        );
    }

    #[test]
    fn ai_does_not_queue_promotion_at_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 0;
        ai.economy.labor.trained = 2; // at threshold
        ai.economy.labor.expert = 0;

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.economy.pending_train_to_expert, 0);
    }

    #[test]
    fn ai_queues_training_when_chain_labor_gap() {
        // Six chain buildings @ tier 2 → required = 18 labor.
        // 7 untrained = 7 labor → gap = 11. Queue 7 trainings (capped by
        // untrained); remaining 4 → ceil(4/2) = 2 promotions but 0 trained
        // so 0 queued.
        use crate::economy::buildings::{Building, BuildingType};
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for bt in [
            BuildingType::LumberMill,
            BuildingType::FurnitureFactory,
            BuildingType::SteelMill,
            BuildingType::HardwareFactory,
            BuildingType::TextileMill,
            BuildingType::ClothingFactory,
        ] {
            ai.economy.buildings.push(Building::new(bt, 2));
        }
        ai.economy.labor.untrained = 7;

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.pending_train_to_trained, 7,
            "Gap-driven path queues 1 training per missing labor unit, \
             capped by untrained pool"
        );
        assert_eq!(ai.economy.pending_train_to_expert, 0);
    }
}
