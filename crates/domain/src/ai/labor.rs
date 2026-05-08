#[cfg(test)]
use crate::economy::buildings::{Building, BuildingType};
use crate::economy::civilians::CivilianType;
#[cfg(test)]
use crate::economy::civilians::{Civilian, next_civilian_id};
use crate::game_state::GameState;
use crate::types::*;

/// AI turns canned food + clothing into queued immigrant workers using the
/// same projected end-of-turn capacity contract exposed to the player UI.
pub(crate) fn ai_recruit_workers(game: &mut GameState, nation_id: NationId) {
    // Extract config values before borrowing nation mutably.
    let wealthy_threshold =
        Money::dollars(game.game_data.game_config.labor_wealthy_treasury_threshold);
    let workers_per_province_base = game.game_data.game_config.labor_workers_per_province_base;
    let workers_per_province_wealthy = game
        .game_data
        .game_config
        .labor_workers_per_province_wealthy;
    let min_workers_floor = game.game_data.game_config.labor_min_workers_floor;

    let max_queue = crate::turn::projected_immigration_queue_capacity(game, nation_id);

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let total_workers = nation.economy.labor.total_workers();

    // Scale max workers with province count, min floor.
    // Wealthy nations invest in workforce growth.
    let workers_per_province: u32 = if nation.economy.treasury > wealthy_threshold {
        workers_per_province_wealthy
    } else {
        workers_per_province_base
    };
    let max_workers =
        (nation.province_count() as u32 * workers_per_province).max(min_workers_floor);

    let desired = max_workers.saturating_sub(total_workers);
    nation.economy.pending_immigration = desired.min(max_queue);
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

/// AI trains untrained workers and promotes trained workers to expert.
///
/// Thresholds are Lua-configurable per personality:
/// - `worker_train_threshold`: train when untrained > threshold (default 1)
/// - `worker_promote_threshold`: promote when trained > threshold (default 2)
#[allow(unused_labels, unused_variables)] // labeled blocks + personality used only with cfg(feature = "lua")
pub(crate) fn ai_train_and_promote_workers(game: &mut GameState, nation_id: NationId) {
    let personality = super::common::get_personality(game, nation_id);

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

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let untrained = nation.economy.labor.untrained;
    let has_paper = nation.material_amount(MaterialType::Paper) > 0;

    // Train one untrained worker if above threshold
    if untrained > train_threshold {
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return;
        };
        // Consume paper if available (training requires paper)
        if has_paper {
            nation.consume_material(MaterialType::Paper, 1);
        }
        nation.economy.labor.train_worker();
        if has_paper {
            game.transient.pending_ai_material_outflows.push((
                nation_id,
                MaterialType::Paper,
                crate::economy::ledger::ResourceOut::ConstructionConsumed,
                1,
            ));
        }
    }

    // Re-read state after potential training
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Promote one trained worker to expert if above threshold
    if nation.economy.labor.trained > promote_threshold {
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return;
        };
        nation.economy.labor.promote_worker();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::test_game_with_ai;
    use crate::ai::run_ai_turns;
    use crate::economy::civilians::{Civilian, CivilianType};
    use crate::hex::HexCoord;
    use crate::map::UnitId;

    // ── Worker recruitment ───────────────────────────────────

    #[test]
    fn ai_recruits_workers_when_workforce_is_small() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // New canning recipe: 1 grain + 1 fruit + 1 (fish OR livestock) → 1 canned food.
        ai.add_resource(ResourceType::Grain, 5);
        ai.add_resource(ResourceType::Fruit, 5);
        ai.add_resource(ResourceType::Fish, 5);
        // Starts with 0 workers

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.labor.total_workers(),
            1,
            "AI should recruit 1 worker when workforce < 5 and food available"
        );
    }

    #[test]
    fn ai_does_not_recruit_when_workforce_at_five() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 5;
        ai.add_resource(ResourceType::Grain, 5);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.labor.total_workers(),
            5,
            "AI should not recruit when it already has 5 workers"
        );
        assert_eq!(
            ai.resource_amount(ResourceType::Grain),
            5,
            "Grain should be unchanged"
        );
    }

    #[test]
    fn ai_does_not_recruit_without_food() {
        let mut game = test_game_with_ai();
        // AI has 0 grain, 0 workers

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.labor.total_workers(),
            0,
            "AI should not recruit without food"
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
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 0;
        ai.economy.labor.trained = 0;
        ai.economy.labor.expert = 0;
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
    fn ai_does_not_queue_immigration_without_clothing() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 0;
        ai.economy.labor.trained = 0;
        ai.economy.labor.expert = 0;
        for i in 60..=63 {
            ai.add_province(ProvinceId(i));
        }
        ai.add_material(MaterialType::CannedFood, 3);

        ai_recruit_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.economy.pending_immigration, 0);
    }

    #[test]
    fn ai_can_queue_immigration_from_projected_food_processing_output() {
        let mut game = test_game_with_ai();
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

    // ── AI worker training/promotion tests ──────────────────

    #[test]
    fn ai_trains_worker_when_many_untrained() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 5; // > 3 threshold
        ai.economy.labor.trained = 0;
        ai.add_material(MaterialType::Paper, 2);

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.labor.untrained, 4,
            "Should have trained 1 untrained worker"
        );
        assert_eq!(ai.economy.labor.trained, 1, "Should have 1 trained worker");
        assert_eq!(
            ai.material_amount(MaterialType::Paper),
            1,
            "Should consume 1 paper for training"
        );
    }

    #[test]
    fn ai_does_not_train_when_at_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 1; // at default threshold (1), not above
        ai.economy.labor.trained = 0;
        ai.add_material(MaterialType::Paper, 2);

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.labor.untrained, 1,
            "Should not train when untrained <= threshold"
        );
        assert_eq!(ai.economy.labor.trained, 0);
        assert_eq!(
            ai.material_amount(MaterialType::Paper),
            2,
            "Paper should be unchanged"
        );
    }

    #[test]
    fn ai_promotes_worker_when_many_trained() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 0;
        ai.economy.labor.trained = 5; // > 3 threshold
        ai.economy.labor.expert = 0;

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.labor.trained, 4,
            "Should have promoted 1 trained worker"
        );
        assert_eq!(ai.economy.labor.expert, 1, "Should have 1 expert worker");
    }

    #[test]
    fn ai_does_not_promote_when_at_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 0;
        ai.economy.labor.trained = 2; // at default promote threshold (2)
        ai.economy.labor.expert = 0;

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.labor.trained, 2,
            "Should not promote when trained <= threshold"
        );
        assert_eq!(ai.economy.labor.expert, 0);
    }

    #[test]
    fn ai_trains_without_paper_available() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 5;
        ai.economy.labor.trained = 0;
        // No paper available

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.labor.untrained, 4,
            "Should still train even without paper"
        );
        assert_eq!(ai.economy.labor.trained, 1);
    }

    #[test]
    fn ai_trains_and_promotes_in_same_turn() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.untrained = 5;
        ai.economy.labor.trained = 4; // will be 5 after training
        ai.economy.labor.expert = 0;
        ai.add_material(MaterialType::Paper, 1);

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.economy.labor.untrained, 4, "Trained 1 untrained");
        // trained: was 4, +1 from training = 5, -1 from promotion = 4
        assert_eq!(
            ai.economy.labor.trained, 4,
            "Net trained stays same (trained+1, promoted-1)"
        );
        assert_eq!(ai.economy.labor.expert, 1, "Promoted 1 to expert");
    }
}
