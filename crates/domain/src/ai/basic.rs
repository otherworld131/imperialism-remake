use crate::economy::buildings::{Building, BuildingType};
use crate::economy::civilians::{Civilian, CivilianType, next_civilian_id};
use crate::economy::trade;
use crate::events::TechId;
use crate::game_state::GameState;
use crate::map::UnitId;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// Global counter for generating unique UnitIds for AI-built army units.
static AI_UNIT_ID_COUNTER: AtomicU32 = AtomicU32::new(2_000_000);

/// Generate a unique UnitId for an AI-built unit.
fn next_unit_id() -> UnitId {
    UnitId(AI_UNIT_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Run AI decisions for all non-human Great Powers.
///
/// Returns a list of notable actions taken by AI nations, suitable for
/// inclusion in the newspaper / turn report.
pub fn run_ai_turns(game: &mut GameState) -> Vec<String> {
    let human_id = game.human_player_nation;
    let current_year = game.turn.year();

    // Collect AI nation IDs
    let ai_nation_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.id != human_id && n.is_great_power())
        .map(|n| n.id)
        .collect();

    let mut actions: Vec<String> = Vec::new();

    for nation_id in &ai_nation_ids {
        ai_research_tech(game, *nation_id, current_year, &mut actions);
        ai_manage_economy(game, *nation_id);
        ai_recruit_workers(game, *nation_id);
        ai_manage_civilians(game, *nation_id);
        ai_build_military(game, *nation_id, &mut actions);
        ai_trade(game, *nation_id);
        ai_build_transport(game, *nation_id);
        ai_military_strategy(game, *nation_id, &mut actions);
    }

    ai_declare_wars(game, &ai_nation_ids, &mut actions);

    actions
}

/// Pick the cheapest available tech and research it if the nation can afford it.
fn ai_research_tech(
    game: &mut GameState,
    nation_id: NationId,
    current_year: u32,
    actions: &mut Vec<String>,
) {
    // Gather the nation's researched techs
    let researched: Vec<TechId> = match game.get_nation(nation_id) {
        Some(n) => n.researched_techs.clone(),
        None => return,
    };

    // Find available techs and pick the cheapest
    let available = game.tech_tree.available_techs(&researched, current_year);
    let cheapest = available.iter().min_by_key(|t| t.cost.cents());
    let (tech_id, tech_cost, tech_name) = match cheapest {
        Some(tech) => (tech.id, tech.cost, tech.name.clone()),
        None => return,
    };

    // Check if the nation can afford it
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };
    if let Some(remaining) = nation.treasury.checked_sub(tech_cost) {
        nation.treasury = remaining;
        nation.research_tech(tech_id);
        let nation_name = nation.name.clone();
        actions.push(format!(
            "Scientists in {} have discovered {}!",
            nation_name, tech_name
        ));
        let turn = game.turn;
        game.history
            .push((turn, format!("{} researched {}", nation_name, tech_name)));
    }
}

/// Build mills and factories when the nation has the required materials.
fn ai_build_infrastructure(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Build mills if the nation doesn't have them and has materials (1 lumber + 1 steel each)
    let mill_types = [
        BuildingType::LumberMill,
        BuildingType::SteelMill,
        BuildingType::TextileMill,
    ];
    for mill_type in mill_types {
        if !nation.has_building(mill_type) {
            let has_lumber = nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0)
                >= 1;
            let has_steel = nation
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0)
                >= 1;
            if has_lumber && has_steel {
                *nation.materials.entry(MaterialType::Lumber).or_insert(0) -= 1;
                *nation.materials.entry(MaterialType::Steel).or_insert(0) -= 1;
                nation.buildings.push(Building::new(mill_type, 2));
            }
        }
    }

    // Build factories if the nation has the corresponding mill but not the factory
    let mill_factory_pairs = [
        (BuildingType::LumberMill, BuildingType::FurnitureFactory),
        (BuildingType::SteelMill, BuildingType::HardwareFactory),
        (BuildingType::TextileMill, BuildingType::ClothingFactory),
    ];
    for (mill, factory) in mill_factory_pairs {
        if nation.has_building(mill) && !nation.has_building(factory) {
            let has_lumber = nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0)
                >= 1;
            let has_steel = nation
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0)
                >= 1;
            if has_lumber && has_steel {
                *nation.materials.entry(MaterialType::Lumber).or_insert(0) -= 1;
                *nation.materials.entry(MaterialType::Steel).or_insert(0) -= 1;
                nation.buildings.push(Building::new(factory, 1));
            }
        }
    }
}

/// Recruit a worker if the nation has fewer than 5 total and has surplus food.
///
/// AI only recruits if total food (grain + fruit + livestock) exceeds total workers
/// (i.e., there is a surplus to feed the new worker next turn).
/// AI also processes food first if it has a FoodProcessing building and raw food.
fn ai_recruit_workers(game: &mut GameState, nation_id: NationId) {
    // First, process food if possible
    ai_process_food(game, nation_id);

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let total_workers = nation.labor.total_workers();
    let grain = nation.resource_amount(ResourceType::Grain);
    let fruit = nation.resource_amount(ResourceType::Fruit);
    let livestock = nation.resource_amount(ResourceType::Livestock);
    let total_food = grain + fruit + livestock;

    // Only recruit if workforce is small AND there is surplus food
    if total_workers < 5 && total_food > total_workers {
        // Consume 1 grain (or fruit/livestock) to recruit
        if nation.resource_amount(ResourceType::Grain) > 0 {
            nation.remove_resource(ResourceType::Grain, 1);
        } else if nation.resource_amount(ResourceType::Fruit) > 0 {
            nation.remove_resource(ResourceType::Fruit, 1);
        } else if nation.resource_amount(ResourceType::Livestock) > 0 {
            nation.remove_resource(ResourceType::Livestock, 1);
        }
        nation.labor.recruit_immigrant();
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
        .buildings
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
    let workers = nation.labor.total_workers();
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
    let _ = remaining - livestock_used;

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
fn ai_manage_civilians(game: &mut GameState, nation_id: NationId) {
    // Phase 1: Hire civilians
    ai_hire_civilians(game, nation_id);

    // Phase 2: Deploy idle civilians
    ai_deploy_civilians(game, nation_id);
}

/// Hire new civilian units if the nation can afford them.
fn ai_hire_civilians(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let civilian_count = nation.civilians.len();
    let treasury = nation.treasury;

    // Rule 1: If < 2 civilians and treasury > $1,000, hire a Farmer
    if civilian_count < 2 && treasury > Money::dollars(1000) {
        let cost = CivilianType::Farmer.creation_cost();
        nation.treasury -= cost;
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

        let cost = civ_type.creation_cost();
        if let Some(remaining) = nation.treasury.checked_sub(cost) {
            nation.treasury = remaining;
            let civilian = Civilian::new(next_civilian_id(), civ_type, nation_id);
            nation.civilians.push(civilian);
        }
    }
}

/// Deploy idle civilians to improvable tiles in the nation's provinces.
fn ai_deploy_civilians(game: &mut GameState, nation_id: NationId) {
    // Collect province IDs owned by this nation
    let province_ids: Vec<ProvinceId> = match game.get_nation(nation_id) {
        Some(n) => n.province_ids.clone(),
        None => return,
    };

    // Find all improvable tiles across the nation's provinces.
    // Each entry: (coord, terrain, improvement_level, max_level, has_civilian_assigned)
    let mut improvable_tiles: Vec<(crate::hex::HexCoord, TerrainType, u8, u8, bool)> = Vec::new();
    for &pid in &province_ids {
        for (coord, tile) in game.hex_map.tiles_in_province(pid) {
            let terrain = tile.terrain();
            let max_level = terrain.max_improvement_level();
            if terrain.is_improvable() && tile.improvement_level() < max_level {
                let has_assigned = tile.assigned_civilian.is_some();
                improvable_tiles.push((
                    coord,
                    terrain,
                    tile.improvement_level(),
                    max_level,
                    has_assigned,
                ));
            }
        }
    }

    // Get idle civilian indices and their types
    let idle_civilians: Vec<(usize, CivilianType)> = match game.get_nation(nation_id) {
        Some(n) => n
            .civilians
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.working && c.turns_remaining == 0)
            .map(|(i, c)| (i, c.civilian_type))
            .collect(),
        None => return,
    };

    // For each idle civilian, try to find a matching tile
    for (civ_idx, civ_type) in idle_civilians {
        // Find the best tile: matching terrain, not already assigned, lowest improvement level first
        let best_tile = improvable_tiles
            .iter()
            .enumerate()
            .filter(|(_, (_, terrain, _, _, has_assigned))| {
                !has_assigned && civ_type.can_improve(*terrain)
            })
            .min_by_key(|(_, (_, _, improvement, _, _))| *improvement);

        if let Some((tile_idx, &(coord, _, _, _, _))) = best_tile {
            // Mark the tile as assigned in our working list
            improvable_tiles[tile_idx].4 = true;

            // Deploy the civilian and start work
            let nation = game.get_nation_mut(nation_id).unwrap();
            let civilian_id = nation.civilians[civ_idx].id;
            nation.civilians[civ_idx].deploy(coord);
            nation.civilians[civ_idx].start_work(2);

            // Mark the tile on the map
            if let Some(tile) = game.hex_map.get_tile_mut(coord) {
                tile.assigned_civilian = Some(civilian_id);
            }
        }
    }
}

/// Build military units when the nation has sufficient treasury.
///
/// - If nation has < 3 army units AND treasury > $2,000, build a Regulars unit ($500)
/// - If nation has < 5 army units AND treasury > $5,000, build a Grenadiers unit ($1,000)
/// - If nation has >= 5 army units AND treasury > $10,000, build Light Artillery ($2,000)
fn ai_build_military(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let army_count = nation.army.len();
    let treasury = nation.treasury;
    let capital = nation.capital_province_id;
    let nation_name = nation.name.clone();

    if army_count < 3 && treasury > Money::dollars(2000) {
        let cost = Money::dollars(500);
        nation.treasury -= cost;
        let unit = ArmyUnit::new(next_unit_id(), ArmyUnitType::Regulars, nation_id, capital);
        nation.army.push(unit);
        actions.push(format!(
            "{} has been expanding its military forces",
            nation_name
        ));
    } else if army_count < 5 && treasury > Money::dollars(5000) {
        let cost = Money::dollars(1000);
        nation.treasury -= cost;
        let unit = ArmyUnit::new(next_unit_id(), ArmyUnitType::Grenadiers, nation_id, capital);
        nation.army.push(unit);
        actions.push(format!(
            "{} has been expanding its military forces",
            nation_name
        ));
    } else if army_count >= 5 && treasury > Money::dollars(10000) {
        let cost = Money::dollars(2000);
        nation.treasury -= cost;
        let unit = ArmyUnit::new(
            next_unit_id(),
            ArmyUnitType::LightArtillery,
            nation_id,
            capital,
        );
        nation.army.push(unit);
        actions.push(format!(
            "{} has been expanding its military forces",
            nation_name
        ));
    }
}

/// Every ~20 turns, each AI Great Power considers declaring war on a Minor Nation.
///
/// Smarter targeting:
/// - Prefer Minor Nations with more tiles (more valuable provinces)
/// - Only attack if army size >= 4 units (enough to beat a garrison)
/// - Don't declare war on nations that another AI is already at war with (avoid dogpiling)
fn ai_declare_wars(game: &mut GameState, ai_nation_ids: &[NationId], actions: &mut Vec<String>) {
    if !game.turn.0.is_multiple_of(20) {
        return;
    }

    // Collect minor nation IDs, their capitals, names, and tile counts
    let minor_nations: Vec<(NationId, ProvinceId, String, usize)> = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| {
            // Count total tiles across all provinces owned by this minor
            let total_tiles: usize = game
                .provinces
                .iter()
                .filter(|p| p.owner == n.id)
                .map(|p| p.tiles.len())
                .sum();
            (n.id, n.capital_province_id, n.name.clone(), total_tiles)
        })
        .collect();

    if minor_nations.is_empty() {
        return;
    }

    // Track which minors are being targeted this round to avoid dogpiling
    let mut targeted_this_round: Vec<NationId> = Vec::new();

    // Also check which minors are already at war with any AI
    let already_targeted: Vec<NationId> = minor_nations
        .iter()
        .filter(|(mn_id, _, _, _)| {
            ai_nation_ids.iter().any(|&ai_id| {
                game.diplomacy
                    .get_relation(ai_id, *mn_id)
                    .map(|r| r.at_war)
                    .unwrap_or(false)
            })
        })
        .map(|(mn_id, _, _, _)| *mn_id)
        .collect();

    for &ai_id in ai_nation_ids {
        // Only attack if AI has >= 4 army units
        let army_size = game.get_nation(ai_id).map(|n| n.army.len()).unwrap_or(0);
        if army_size < 4 {
            continue;
        }

        // Find best target: not already at war, not dogpiled, most tiles (most valuable)
        let mut candidates: Vec<_> = minor_nations
            .iter()
            .filter(|(mn_id, _, _, _)| {
                // Not already at war with this AI
                let at_war = game
                    .diplomacy
                    .get_relation(ai_id, *mn_id)
                    .map(|r| r.at_war)
                    .unwrap_or(false);
                // Not already targeted by another AI (anti-dogpile)
                let dogpiled =
                    already_targeted.contains(mn_id) || targeted_this_round.contains(mn_id);
                !at_war && !dogpiled
            })
            .collect();

        // Sort by tile count descending (most valuable first)
        candidates.sort_by(|a, b| b.3.cmp(&a.3));

        // Use pseudo-random seed to add some variety: pick from top 3 candidates
        if candidates.is_empty() {
            continue;
        }
        let seed = (game.turn.0 as usize).wrapping_add(ai_id.0 as usize);
        let pick_range = candidates.len().min(3);
        let target_index = seed % pick_range;
        let (target_id, target_capital, ref target_name, _) = *candidates[target_index];

        let attacker_name = game
            .get_nation(ai_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        game.diplomacy.declare_war(ai_id, target_id);
        game.pending_attacks.push((ai_id, target_capital));
        targeted_this_round.push(target_id);
        actions.push(format!(
            "{} has declared war on {}!",
            attacker_name, target_name
        ));
        let turn = game.turn;
        game.history.push((
            turn,
            format!("{} declared war on {}", attacker_name, target_name),
        ));
    }
}

/// Strategic military decisions for an AI nation.
///
/// - If at war and has >= 4 army units, queue an attack on the enemy's weakest province
/// - If not at war and has >= 6 army units and turn > 40, consider declaring war on a weak Minor Nation
/// - Upgrade units when tech allows (call unit_type.upgrade_to(), check if prereq tech is researched)
fn ai_military_strategy(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    // Phase 1: Upgrade units if possible
    ai_upgrade_units(game, nation_id);

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let army_size = nation.army.len();
    let nation_name = nation.name.clone();

    // Find nations we are at war with
    let enemies: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.id != nation_id)
        .filter(|n| {
            game.diplomacy
                .get_relation(nation_id, n.id)
                .map(|r| r.at_war)
                .unwrap_or(false)
        })
        .map(|n| n.id)
        .collect();

    // If at war and has >= 4 army units, attack enemy's weakest province
    if !enemies.is_empty() && army_size >= 4 {
        // Find the weakest enemy province (fewest tiles)
        let mut best_target: Option<(ProvinceId, usize)> = None;
        for &enemy_id in &enemies {
            for prov in &game.provinces {
                if prov.owner == enemy_id {
                    let tile_count = prov.tiles.len();
                    if best_target.is_none() || tile_count < best_target.unwrap().1 {
                        best_target = Some((prov.id, tile_count));
                    }
                }
            }
        }

        if let Some((target_prov, _)) = best_target {
            // Only queue if not already pending
            let already_pending = game
                .pending_attacks
                .iter()
                .any(|(a, p)| *a == nation_id && *p == target_prov);
            if !already_pending {
                game.pending_attacks.push((nation_id, target_prov));
            }
        }
    }

    // If not at war and has >= 6 army units and turn > 40, consider proactive war
    if enemies.is_empty() && army_size >= 6 && game.turn.0 > 40 {
        // Find weakest Minor Nation (fewest total tiles, not at war with anyone)
        let minor_nations: Vec<(NationId, ProvinceId, usize)> = game
            .nations
            .iter()
            .filter(|n| !n.is_great_power())
            .map(|n| {
                let total_tiles: usize = game
                    .provinces
                    .iter()
                    .filter(|p| p.owner == n.id)
                    .map(|p| p.tiles.len())
                    .sum();
                (n.id, n.capital_province_id, total_tiles)
            })
            .filter(|(mn_id, _, _)| {
                !game
                    .diplomacy
                    .get_relation(nation_id, *mn_id)
                    .map(|r| r.at_war)
                    .unwrap_or(false)
            })
            .collect();

        // Sort by tile count ascending to find weakest
        let mut sorted = minor_nations;
        sorted.sort_by_key(|&(_, _, tiles)| tiles);

        if let Some(&(target_id, target_capital, _)) = sorted.first() {
            let target_name = game
                .get_nation(target_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            game.diplomacy.declare_war(nation_id, target_id);
            game.pending_attacks.push((nation_id, target_capital));
            actions.push(format!(
                "{} has declared war on {}!",
                nation_name, target_name
            ));
            let turn = game.turn;
            game.history.push((
                turn,
                format!("{} declared war on {}", nation_name, target_name),
            ));
        }
    }
}

/// Upgrade units when tech prerequisites have been researched.
fn ai_upgrade_units(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let researched = nation.researched_techs.clone();

    // Collect upgrade info: (index, new_type)
    let upgrades: Vec<(usize, ArmyUnitType)> = nation
        .army
        .iter()
        .enumerate()
        .filter_map(|(i, unit)| {
            unit.unit_type.upgrade_to().and_then(|new_type| {
                let prereq = new_type.stats().prerequisite_tech;
                match prereq {
                    // If the upgrade requires a tech, check it's researched
                    Some(ref tech_name) => {
                        let has_tech = game
                            .tech_tree
                            .all_techs()
                            .iter()
                            .any(|t| t.name == *tech_name && researched.contains(&t.id));
                        if has_tech { Some((i, new_type)) } else { None }
                    }
                    // No tech prereq: always upgrade
                    None => Some((i, new_type)),
                }
            })
        })
        .collect();

    // Apply upgrades
    if let Some(nation) = game.get_nation_mut(nation_id) {
        for (idx, new_type) in upgrades {
            if idx < nation.army.len() {
                nation.army[idx].unit_type = new_type;
            }
        }
    }
}

/// Consolidate AI economic decisions.
///
/// - If AI has no mills and has lumber+steel materials: build a LumberMill
/// - If AI has mills producing materials, build corresponding factories
/// - Expand mills when capacity is maxed (if resources > capacity * 2)
fn ai_manage_economy(game: &mut GameState, nation_id: NationId) {
    // Build infrastructure handles mills and factories
    ai_build_infrastructure(game, nation_id);

    // Expand mills when input resources exceed capacity * 2
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let expansions_needed: Vec<BuildingType> = nation
        .buildings
        .iter()
        .filter_map(|b| {
            let input_resources = match b.building_type {
                BuildingType::LumberMill => nation.resource_amount(ResourceType::Timber),
                BuildingType::SteelMill => {
                    nation.resource_amount(ResourceType::Coal)
                        + nation.resource_amount(ResourceType::Iron)
                }
                BuildingType::TextileMill => {
                    nation.resource_amount(ResourceType::Cotton)
                        + nation.resource_amount(ResourceType::Wool)
                }
                _ => return None,
            };
            if input_resources > b.effective_capacity() * 2 && b.pending_capacity == 0 {
                Some(b.building_type)
            } else {
                None
            }
        })
        .collect();

    for bt in expansions_needed {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };
        let has_lumber = nation.material_amount(MaterialType::Lumber) >= 1;
        let has_steel = nation.material_amount(MaterialType::Steel) >= 1;
        if has_lumber && has_steel {
            let nation = game.get_nation_mut(nation_id).unwrap();
            nation.consume_material(MaterialType::Lumber, 1);
            nation.consume_material(MaterialType::Steel, 1);
            if let Some(building) = nation.get_building_mut(bt) {
                building.start_expansion(1);
            }
        }
    }
}

/// Sell excess tradeable resources on the market for cash.
///
/// For each tradeable resource the AI has more than 10 of, sell the excess
/// at base_price and add proceeds to the treasury.
fn ai_trade(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Check all tradeable resource types for surplus
    let tradeable_resources = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Oil,
    ];

    for resource in tradeable_resources {
        let amount = nation.resource_amount(resource);
        if amount > 10 {
            let excess = amount - 10;
            let price = trade::base_price(resource);
            if price != Money::ZERO {
                let revenue = price * excess as i64;
                nation.remove_resource(resource, excess);
                nation.treasury += revenue;
            }
        }
    }
}

/// Build freight cars if the nation has none and has the required materials.
///
/// Cost per freight car: 1 lumber + 1 steel (labor requirement simplified away).
/// Builds 2 freight cars if possible.
fn ai_build_transport(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    if nation.transport.freight_cars > 0 {
        return;
    }

    // Need 2 lumber + 2 steel for 2 freight cars
    let has_lumber = nation.material_amount(MaterialType::Lumber) >= 2;
    let has_steel = nation.material_amount(MaterialType::Steel) >= 2;

    if has_lumber && has_steel {
        nation.consume_material(MaterialType::Lumber, 2);
        nation.consume_material(MaterialType::Steel, 2);
        nation.transport.build_freight_cars(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplomacy::DiplomacyState;
    use crate::hex::HexCoord;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};
    use crate::tech::TechTree;

    /// Build a game state with a human nation and one AI great power.
    fn test_game_with_ai() -> GameState {
        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Human Land".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            HexCoord::new(3, 3),
            vec![HexCoord::new(3, 3)],
            4,
        );

        let mut human_nation = Nation::new(
            NationId(1),
            "HumanNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        human_nation.treasury = Money::dollars(10000);

        let mut ai_nation = Nation::new(
            NationId(2),
            "AINation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        ai_nation.treasury = Money::dollars(10000);
        // Pre-populate with 4 civilians so AI does not hire more during tests
        for i in 0..4 {
            ai_nation.civilians.push(Civilian::new(
                UnitId(10000 + i),
                CivilianType::Farmer,
                NationId(2),
            ));
        }

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2],
            nations: vec![human_nation, ai_nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            history: Vec::new(),
        }
    }

    /// Build a game state that includes a minor nation for war tests.
    fn test_game_with_ai_and_minor() -> GameState {
        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Human Land".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            HexCoord::new(3, 3),
            vec![HexCoord::new(3, 3)],
            4,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Minor Capital".to_string(),
            NationId(3),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            3,
        );

        let mut human_nation = Nation::new(
            NationId(1),
            "HumanNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        human_nation.treasury = Money::dollars(10000);

        let mut ai_nation = Nation::new(
            NationId(2),
            "AINation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        ai_nation.treasury = Money::dollars(10000);
        // Pre-populate with 4 civilians so AI does not hire more during tests
        for i in 0..4 {
            ai_nation.civilians.push(Civilian::new(
                UnitId(10000 + i),
                CivilianType::Farmer,
                NationId(2),
            ));
        }

        let minor_nation = Nation::new(
            NationId(3),
            "MinorLand".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(3),
        );

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2, province3],
            nations: vec![human_nation, ai_nation, minor_nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            history: Vec::new(),
        }
    }

    // ── Tech research ─────────────────────────────────────────

    #[test]
    fn ai_researches_cheapest_available_tech() {
        let mut game = test_game_with_ai();
        // At 1815, two free techs are available (cost $0):
        // "High Pressure Steam Engine" (ID 1) and "Seed Drill" (ID 2)
        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have researched at least one of the two free techs
        assert!(
            ai.has_researched(TechId(1)) || ai.has_researched(TechId(2)),
            "AI should research a free tech"
        );
        // Treasury reduced by $500 for building a Regulars unit (AI has < 3 army, > $2000)
        assert_eq!(ai.treasury, Money::dollars(9500));
    }

    #[test]
    fn ai_does_not_spend_more_than_it_has() {
        let mut game = test_game_with_ai();
        // Pre-research the free techs so only paid techs remain
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.research_tech(TechId(1));
        ai.research_tech(TechId(2));
        // Set treasury to $500 (less than the cheapest paid tech at $1,000)
        ai.treasury = Money::dollars(500);

        // Move to year 1816 so Cotton Gin ($1,000) becomes available
        game.turn = TurnNumber::from_year_quarter(1816, 1);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should NOT have researched Cotton Gin since it can't afford it
        assert!(
            !ai.has_researched(TechId(3)),
            "AI should not research techs it cannot afford"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(500),
            "Treasury should be unchanged"
        );
    }

    // ── Infrastructure building ──────────────────────────────

    #[test]
    fn ai_builds_mill_when_it_has_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give the AI lumber and steel materials
        *ai.materials.entry(MaterialType::Lumber).or_insert(0) = 3;
        *ai.materials.entry(MaterialType::Steel).or_insert(0) = 3;

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have built a LumberMill (first in the loop)
        assert!(
            ai.has_building(BuildingType::LumberMill),
            "AI should build a LumberMill when it has lumber + steel materials"
        );
    }

    #[test]
    fn ai_builds_factory_when_it_has_mill_and_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give the AI all three mills already so it won't spend materials on them
        ai.buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        ai.buildings.push(Building::new(BuildingType::SteelMill, 2));
        ai.buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        // Give materials for factory construction
        *ai.materials.entry(MaterialType::Lumber).or_insert(0) = 2;
        *ai.materials.entry(MaterialType::Steel).or_insert(0) = 2;

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.has_building(BuildingType::FurnitureFactory),
            "AI should build a FurnitureFactory when it has a LumberMill and materials"
        );
    }

    #[test]
    fn ai_does_not_build_without_materials() {
        let mut game = test_game_with_ai();
        // AI has no materials at all

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            !ai.has_building(BuildingType::LumberMill),
            "AI should not build buildings without materials"
        );
        assert!(
            !ai.has_building(BuildingType::SteelMill),
            "AI should not build buildings without materials"
        );
    }

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
            ai.labor.total_workers(),
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
        ai.labor.untrained = 5;
        ai.add_resource(ResourceType::Grain, 5);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.labor.total_workers(),
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
            ai.labor.total_workers(),
            0,
            "AI should not recruit without food"
        );
    }

    // ── Human player not affected ────────────────────────────

    #[test]
    fn ai_does_not_touch_human_player() {
        let mut game = test_game_with_ai();
        let human = game.get_nation_mut(NationId(1)).unwrap();
        let original_treasury = human.treasury;
        let original_techs = human.researched_techs.len();

        run_ai_turns(&mut game);

        let human = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            human.treasury, original_treasury,
            "Human player should not be affected by AI turns"
        );
        assert_eq!(
            human.researched_techs.len(),
            original_techs,
            "Human player techs should not change"
        );
    }

    // ── Military building ────────────────────────────────────

    #[test]
    fn ai_builds_regulars_when_army_small_and_can_afford() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(3000);
        // AI starts with 0 army units

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.army.len(), 1, "AI should build 1 Regulars unit");
        assert_eq!(ai.army[0].unit_type, ArmyUnitType::Regulars);
        assert_eq!(ai.army[0].owner, NationId(2));
        assert_eq!(ai.army[0].position, ProvinceId(2)); // capital
        assert_eq!(
            ai.treasury,
            Money::dollars(2500),
            "Treasury should be reduced by $500"
        );
    }

    #[test]
    fn ai_does_not_build_military_when_poor() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(1000); // < $2,000 threshold

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.army.is_empty(),
            "AI should not build army units when treasury <= $2,000"
        );
    }

    #[test]
    fn ai_builds_grenadiers_when_army_has_3_units() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(6000);
        // Give AI 3 existing army units
        for i in 0..3 {
            ai.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.army.len(), 4, "AI should have built a 4th unit");
        assert_eq!(
            ai.army[3].unit_type,
            ArmyUnitType::Grenadiers,
            "4th unit should be Grenadiers"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(5000),
            "Treasury should be reduced by $1,000"
        );
    }

    #[test]
    fn ai_builds_light_artillery_when_army_large() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(12000);
        // Give AI 5 existing army units
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.army.len(), 6, "AI should have built a 6th unit");
        assert_eq!(
            ai.army[5].unit_type,
            ArmyUnitType::LightArtillery,
            "6th unit should be Light Artillery"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(10000),
            "Treasury should be reduced by $2,000"
        );
    }

    #[test]
    fn ai_military_units_have_unique_ids() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(50000);

        // Run multiple turns to build several units
        let mut actions = Vec::new();
        for _ in 0..5 {
            ai_build_military(&mut game, NationId(2), &mut actions);
        }

        let ai = game.get_nation(NationId(2)).unwrap();
        let ids: Vec<UnitId> = ai.army.iter().map(|u| u.id).collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "AI army units must have unique IDs");
            }
        }
    }

    // ── War declaration ──────────────────────────────────────

    #[test]
    fn ai_declares_war_on_turn_20() {
        let mut game = test_game_with_ai_and_minor();
        // Set to turn 20 (divisible by 20)
        game.turn = TurnNumber::new(20);
        // Give AI enough army units to meet the >= 4 threshold for war declaration
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..4 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        run_ai_turns(&mut game);

        // AI should have declared war on the minor nation
        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        assert!(rel.is_some(), "Relation between AI and minor should exist");
        assert!(
            rel.unwrap().at_war,
            "AI should be at war with the minor nation"
        );
        // Should have queued a pending attack on the minor's capital
        assert!(
            game.pending_attacks
                .iter()
                .any(|(attacker, target)| *attacker == NationId(2) && *target == ProvinceId(3)),
            "AI should queue an attack on the minor's capital"
        );
    }

    #[test]
    fn ai_does_not_declare_war_on_non_multiple_of_20() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(15);

        run_ai_turns(&mut game);

        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        // Either no relation exists, or it's not at war
        let at_war = rel.map(|r| r.at_war).unwrap_or(false);
        assert!(
            !at_war,
            "AI should not declare war on non-multiple-of-20 turns"
        );
        assert!(
            game.pending_attacks.is_empty(),
            "No attacks should be pending"
        );
    }

    #[test]
    fn ai_does_not_redeclare_war_on_existing_enemy() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(20);

        // Pre-set war
        game.diplomacy.declare_war(NationId(2), NationId(3));

        run_ai_turns(&mut game);

        // Should not have queued a duplicate pending attack
        let attack_count = game
            .pending_attacks
            .iter()
            .filter(|(a, _)| *a == NationId(2))
            .count();
        assert_eq!(
            attack_count, 0,
            "AI should not queue attack if already at war"
        );
    }

    // ── Trade ────────────────────────────────────────────────

    #[test]
    fn ai_sells_excess_resources() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(1000);
        // Give AI 15 timber (surplus over 10 threshold)
        ai.add_resource(ResourceType::Timber, 15);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have sold 5 timber at $50 each = $250
        assert_eq!(
            ai.resource_amount(ResourceType::Timber),
            10,
            "AI should sell down to 10 timber"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(1250),
            "Treasury should increase by $250 from selling 5 timber at $50"
        );
    }

    #[test]
    fn ai_does_not_sell_resources_below_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(1000);
        ai.add_resource(ResourceType::Timber, 8); // below threshold of 10

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.resource_amount(ResourceType::Timber),
            8,
            "AI should not sell resources at or below 10"
        );
    }

    #[test]
    fn ai_sells_multiple_excess_resources() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(0);
        ai.add_resource(ResourceType::Timber, 15); // 5 excess at $50 = $250
        ai.add_resource(ResourceType::Coal, 20); // 10 excess at $75 = $750

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.resource_amount(ResourceType::Timber), 10);
        assert_eq!(ai.resource_amount(ResourceType::Coal), 10);
        assert_eq!(
            ai.treasury,
            Money::dollars(1000),
            "Treasury should increase by $250 + $750 = $1000"
        );
    }

    #[test]
    fn ai_does_not_sell_non_tradeable_grain() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(0);
        ai.add_resource(ResourceType::Grain, 20); // grain is not in the tradeable list

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Grain has base_price $0, and is not in the tradeable_resources list in ai_trade
        // so it should remain untouched. Worker recruitment may consume 1 grain.
        // But with 0 workers and < 5 workers, 1 grain consumed for recruitment.
        assert_eq!(
            ai.resource_amount(ResourceType::Grain),
            19,
            "Only 1 grain consumed for worker recruitment, none sold"
        );
    }

    // ── Transport building ───────────────────────────────────

    #[test]
    fn ai_builds_freight_cars_when_it_has_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give all buildings so infrastructure doesn't consume materials
        ai.buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        ai.buildings.push(Building::new(BuildingType::SteelMill, 2));
        ai.buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        ai.buildings
            .push(Building::new(BuildingType::FurnitureFactory, 1));
        ai.buildings
            .push(Building::new(BuildingType::HardwareFactory, 1));
        ai.buildings
            .push(Building::new(BuildingType::ClothingFactory, 1));
        ai.add_material(MaterialType::Lumber, 5);
        ai.add_material(MaterialType::Steel, 5);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.transport.freight_cars, 2,
            "AI should build 2 freight cars"
        );
        // Should have consumed 2 lumber + 2 steel
        assert_eq!(ai.material_amount(MaterialType::Lumber), 3);
        assert_eq!(ai.material_amount(MaterialType::Steel), 3);
    }

    #[test]
    fn ai_does_not_build_freight_cars_without_materials() {
        let mut game = test_game_with_ai();
        // AI has no materials

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.transport.freight_cars, 0,
            "AI should not build freight cars without materials"
        );
    }

    #[test]
    fn ai_does_not_build_freight_cars_if_already_has_some() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.transport.build_freight_cars(1); // already has cars
        ai.add_material(MaterialType::Lumber, 5);
        ai.add_material(MaterialType::Steel, 5);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.transport.freight_cars, 1,
            "AI should not build more freight cars when it already has some"
        );
        // Materials should be untouched (except if used by infrastructure building)
    }

    // ── Civilian management ─────────────────────────────────

    #[test]
    fn ai_hires_farmer_when_few_civilians_and_can_afford() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(5000);
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
            ai.treasury,
            Money::dollars(4900),
            "Treasury should be reduced by $100 (Farmer cost)"
        );
    }

    #[test]
    fn ai_does_not_hire_civilian_when_too_poor() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(500); // Below $1,000 threshold
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
        ai.treasury = Money::dollars(5000);
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
        ai.treasury = Money::dollars(5000);
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
        assert_eq!(ai.treasury, Money::dollars(3500), "Miner costs $1,500");
    }

    #[test]
    fn ai_deploys_idle_civilian_to_improvable_tile() {
        let mut game = test_game_with_ai();

        // Set up a Farm tile in AI's province
        let farm_coord = HexCoord::new(3, 3);
        let tile = crate::map::tile::Tile::with_province(TerrainType::Farm, ProvinceId(2));
        game.hex_map.set_tile(farm_coord, tile);

        // Give AI a Farmer civilian (idle), clear pre-populated ones
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(500); // Not enough for hiring
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

        // Set up a Farm tile at max improvement
        let farm_coord = HexCoord::new(3, 3);
        let mut tile = crate::map::tile::Tile::with_province(TerrainType::Farm, ProvinceId(2));
        tile.set_improvement_level(3); // max for Farm
        game.hex_map.set_tile(farm_coord, tile);

        // Give AI a Farmer civilian (idle), clear pre-populated ones
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(500);
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
}
