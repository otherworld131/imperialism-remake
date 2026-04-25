use crate::economy::buildings::BuildingType;
use crate::economy::civilians::CivilianType;
#[cfg(test)]
use crate::economy::civilians::{Civilian, next_civilian_id};
use crate::game_state::GameState;
use crate::types::*;

/// AI only recruits if total food (grain + fruit + livestock) exceeds total workers
/// (i.e., there is a surplus to feed the new worker next turn).
/// AI also processes food first if it has a FoodProcessing building and raw food.
pub(crate) fn ai_recruit_workers(game: &mut GameState, nation_id: NationId) {
    // First, process food if possible
    ai_process_food(game, nation_id);

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let total_workers = nation.economy.labor.total_workers();
    let grain = nation.resource_amount(ResourceType::Grain);
    let fruit = nation.resource_amount(ResourceType::Fruit);
    let livestock = nation.resource_amount(ResourceType::Livestock);
    let total_food = grain + fruit + livestock;

    // Scale max workers with province count (2 per province, min 5)
    // Wealthy nations invest in workforce growth (3 per province)
    let workers_per_province: u32 = if nation.economy.treasury > Money::dollars(20_000) {
        3
    } else {
        2
    };
    let max_workers = (nation.province_count() as u32 * workers_per_province).max(5);

    // Only recruit if workforce is below target AND there is surplus food
    if total_workers < max_workers && total_food > total_workers {
        // Consume 1 grain (or fruit/livestock) to recruit
        if nation.resource_amount(ResourceType::Grain) > 0 {
            nation.remove_resource(ResourceType::Grain, 1);
        } else if nation.resource_amount(ResourceType::Fruit) > 0 {
            nation.remove_resource(ResourceType::Fruit, 1);
        } else if nation.resource_amount(ResourceType::Livestock) > 0 {
            nation.remove_resource(ResourceType::Livestock, 1);
        }
        nation.economy.labor.recruit_immigrant();
    }
}

/// AI processes food: if the nation has a FoodProcessing building and raw food,
/// convert raw food to canned food (2 raw -> 1 canned).
fn ai_process_food(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let food_processing_cap = nation
        .economy.buildings
        .iter()
        .find(|b| b.building_type == BuildingType::FoodProcessing)
        .map(|b| b.effective_capacity())
        .unwrap_or(0);

    if food_processing_cap == 0 {
        return;
    }

    let grain = nation.resource_amount(ResourceType::Grain);
    let fruit = nation.resource_amount(ResourceType::Fruit);
    let livestock = nation.resource_amount(ResourceType::Livestock);
    let total_raw = grain + fruit + livestock;

    // Only process if we have excess food beyond worker needs
    let workers = nation.economy.labor.total_workers();
    if total_raw <= workers {
        return; // Don't process food we need to eat
    }

    let available_for_processing = total_raw - workers;
    if available_for_processing < 2 {
        return;
    }

    let raw_limited = available_for_processing / 2;
    let units = food_processing_cap.min(raw_limited);

    if units == 0 {
        return;
    }

    // Consume grain first, then fruit, then livestock
    let mut remaining = units * 2;
    let grain_used = grain.min(remaining);
    remaining -= grain_used;
    let fruit_used = fruit.min(remaining);
    remaining -= fruit_used;
    let livestock_used = livestock.min(remaining);

    if grain_used > 0 {
        nation.remove_resource(ResourceType::Grain, grain_used);
    }
    if fruit_used > 0 {
        nation.remove_resource(ResourceType::Fruit, fruit_used);
    }
    if livestock_used > 0 {
        nation.remove_resource(ResourceType::Livestock, livestock_used);
    }
    nation.add_material(MaterialType::CannedFood, units);
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
    let cfg = game.game_data.game_config.clone();
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let civilian_count = nation.civilians.len();
    let treasury = nation.economy.treasury;

    // Rule 1: If < 2 civilians and treasury > $1,000, hire a Farmer
    if civilian_count < 2 && treasury > Money::dollars(1000) {
        let cost = CivilianType::Farmer.creation_cost(&cfg);
        nation.economy.treasury -= cost;
        let farmer = Civilian::new(next_civilian_id(), CivilianType::Farmer, nation_id);
        nation.civilians.push(farmer);
        return; // Only hire one per turn
    }

    // Rule 2: If < 4 civilians and treasury > $2,000, hire Forester or Miner
    if civilian_count < 4 && treasury > Money::dollars(2000) {
        // Prefer Forester (cheaper) unless we already have one
        let has_forester = nation
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
            nation.civilians.push(civilian);
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

    // Precompute the set of tiles currently harvesting to the capital. We
    // prefer deploying civilians onto these (their improvements turn into
    // real yield), but we still allow speculative improvement of other tiles
    // if nothing in `collectable` is available — the AI's rail planner is
    // expected to catch up and connect them within a few turns.
    let collectable: std::collections::HashSet<crate::hex::HexCoord> = {
        if game.get_nation(nation_id).is_none() {
            return;
        }
        let connected = super::super::turn::connected_provinces(game, nation_id);
        let owned_provinces: Vec<&crate::map::Province> = game
            .provinces
            .iter()
            .filter(|p| p.owner == nation_id)
            .collect();
        crate::map::infrastructure::collectable_hexes(&game.hex_map, &owned_provinces, &connected)
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
    for &pid in &province_ids {
        for (coord, tile) in game.hex_map.tiles_in_province(pid) {
            let terrain = tile.terrain();
            let resource = tile.resource_deposit();
            let max_level = resource.map(|r| r.max_improvement_level()).unwrap_or(0);
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
    for (civ_idx, civ_type) in idle_civilians {
        let best_tile = improvable_tiles
            .iter()
            .enumerate()
            .filter(|(_, (_, terrain, resource, _, _, has_assigned))| {
                !has_assigned && civ_type.can_improve(*terrain, *resource)
            })
            .min_by_key(|(_, (coord, _, _, improvement, _, _))| {
                // `false` (0) sorts before `true` (1), so collectable tiles
                // come first; within each bucket, lowest improvement_level wins.
                (!collectable.contains(coord), *improvement)
            });

        if let Some((tile_idx, &(coord, _, _, _, _, _))) = best_tile {
            // Mark the tile as assigned in our working list
            improvable_tiles[tile_idx].5 = true;

            // Deploy the civilian and start work
            let Some(nation) = game.get_nation_mut(nation_id) else {
                return;
            };
            let civilian_id = nation.civilians[civ_idx].id;
            // Clear the old tile's slot before redeploying so stale IDs don't
            // block the engineer from building railroad on previously-worked hexes.
            if let Some(old_pos) = nation.civilians[civ_idx].position
                && let Some(old_tile) = game.hex_map.get_tile_mut(old_pos)
                && old_tile.assigned_civilian == Some(civilian_id)
            {
                old_tile.assigned_civilian = None;
            }
            let Some(nation) = game.get_nation_mut(nation_id) else {
                return;
            };
            nation.civilians[civ_idx].deploy(coord);
            nation.civilians[civ_idx].start_work(2);

            // Mark the tile on the map
            if let Some(tile) = game.hex_map.get_tile_mut(coord) {
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
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));

    let train_threshold: u32 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.worker_train_threshold) {
            break 'val v;
        }
        1
    };
    let promote_threshold: u32 = 'val: {
        #[cfg(feature = "lua")]
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
        ai.add_resource(ResourceType::Grain, 5);
        // Starts with 0 workers

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.labor.total_workers(),
            1,
            "AI should recruit 1 worker when workforce < 5 and food available"
        );
        assert_eq!(
            ai.resource_amount(ResourceType::Grain),
            4,
            "AI should consume 1 grain to recruit"
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
        ai.civilians.clear(); // Start with 0 civilians

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.civilians.len(),
            1,
            "AI should hire 1 Farmer when it has < 2 civilians"
        );
        assert_eq!(ai.civilians[0].civilian_type, CivilianType::Farmer);
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
        ai.civilians.clear(); // Start with 0 civilians

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.civilians.is_empty(),
            "AI should not hire civilians when treasury <= $1,000"
        );
    }

    #[test]
    fn ai_hires_forester_when_has_two_civilians_and_enough_money() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(5000);
        ai.civilians.clear();
        // Give AI 2 existing civilians (both farmers)
        ai.civilians.push(Civilian::new(
            UnitId(900),
            CivilianType::Farmer,
            NationId(2),
        ));
        ai.civilians.push(Civilian::new(
            UnitId(901),
            CivilianType::Farmer,
            NationId(2),
        ));

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.civilians.len(),
            3,
            "AI should hire a 3rd civilian (Forester)"
        );
        assert_eq!(
            ai.civilians[2].civilian_type,
            CivilianType::Forester,
            "3rd civilian should be a Forester"
        );
    }

    #[test]
    fn ai_hires_miner_when_already_has_forester() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(5000);
        ai.civilians.clear();
        // Give AI 2 existing civilians including a forester
        ai.civilians.push(Civilian::new(
            UnitId(900),
            CivilianType::Farmer,
            NationId(2),
        ));
        ai.civilians.push(Civilian::new(
            UnitId(901),
            CivilianType::Forester,
            NationId(2),
        ));

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.civilians.len(), 3);
        assert_eq!(
            ai.civilians[2].civilian_type,
            CivilianType::Miner,
            "Should hire Miner when Forester already exists"
        );
        assert_eq!(ai.economy.treasury, Money::dollars(3500), "Miner costs $1,500");
    }

    #[test]
    fn ai_deploys_idle_civilian_to_improvable_tile() {
        let mut game = test_game_with_ai();

        // Set up a Grassland tile with Grain in AI's province
        let farm_coord = HexCoord::new(3, 3);
        let mut tile = crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        tile.set_resource(ResourceType::Grain);
        game.hex_map.set_tile(farm_coord, tile);

        // Give AI a Farmer civilian (idle), clear pre-populated ones
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(500); // Not enough for hiring
        ai.civilians.clear();
        ai.civilians.push(Civilian::new(
            UnitId(950),
            CivilianType::Farmer,
            NationId(2),
        ));

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.civilians.len(), 1, "Should still have 1 civilian");
        assert!(ai.civilians[0].working, "Civilian should be working");
        assert_eq!(
            ai.civilians[0].position,
            Some(farm_coord),
            "Civilian should be deployed to the farm tile"
        );

        // Check that the tile has the civilian assigned
        let tile = game.hex_map.get_tile(farm_coord).unwrap();
        assert_eq!(
            tile.assigned_civilian,
            Some(UnitId(950)),
            "Tile should have the civilian assigned"
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
        game.hex_map.set_tile(farm_coord, tile);

        // Give AI a Farmer civilian (idle), clear pre-populated ones
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(500);
        ai.civilians.clear();
        ai.civilians.push(Civilian::new(
            UnitId(960),
            CivilianType::Farmer,
            NationId(2),
        ));

        ai_manage_civilians(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        // Civilian should stay idle because no improvable tiles
        assert!(
            !ai.civilians[0].working,
            "Civilian should remain idle when no improvable tiles exist"
        );
        assert_eq!(
            ai.civilians[0].position, None,
            "Civilian should not be deployed"
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
        assert_eq!(ai.economy.labor.trained, 4, "Should have promoted 1 trained worker");
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
