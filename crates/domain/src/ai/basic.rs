use crate::economy::buildings::{Building, BuildingType};
use crate::economy::civilians::{Civilian, CivilianType, next_civilian_id};
use crate::economy::trade;
use crate::events::TechId;
use crate::game_state::GameState;
use crate::map::UnitId;
use crate::military::ships::{Ship, ShipType};
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::tech::tree::TechEffect;
use crate::types::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// AI personality types that affect decision-making priorities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AiPersonality {
    /// Prioritizes military, declares wars early.
    Aggressive,
    /// Prioritizes trade and alliances, avoids war.
    Diplomatic,
    /// Prioritizes production and tech investment.
    Economic,
    /// Default: adapts to circumstances.
    Balanced,
}

impl std::fmt::Display for AiPersonality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiPersonality::Aggressive => write!(f, "Aggressive"),
            AiPersonality::Diplomatic => write!(f, "Diplomatic"),
            AiPersonality::Economic => write!(f, "Economic"),
            AiPersonality::Balanced => write!(f, "Balanced"),
        }
    }
}

/// Returns the default AI personality for a Great Power based on nation index.
///
/// - Index 0 (Deneb): Balanced (usually human-controlled)
/// - Index 1 (Devron): Aggressive
/// - Index 2 (Haxaco): Economic
/// - Index 3 (Kem): Aggressive
/// - Index 4 (Ordune): Diplomatic
/// - Index 5 (Patagon): Economic
/// - Index 6 (Zimm): Balanced
pub fn personality_for_nation_index(index: usize) -> AiPersonality {
    match index {
        0 => AiPersonality::Balanced,
        1 => AiPersonality::Aggressive,
        2 => AiPersonality::Economic,
        3 => AiPersonality::Aggressive,
        4 => AiPersonality::Diplomatic,
        5 => AiPersonality::Economic,
        6 => AiPersonality::Balanced,
        _ => AiPersonality::Balanced,
    }
}

/// Global counter for generating unique UnitIds for AI-built army units.
static AI_UNIT_ID_COUNTER: AtomicU32 = AtomicU32::new(2_000_000);

/// Generate a unique UnitId for an AI-built unit.
fn next_unit_id() -> UnitId {
    UnitId(AI_UNIT_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Get the AI personality for a nation, defaulting to Balanced.
fn get_personality(game: &GameState, nation_id: NationId) -> AiPersonality {
    game.get_nation(nation_id)
        .and_then(|n| n.ai_personality)
        .unwrap_or(AiPersonality::Balanced)
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
        ai_build_consulates(game, *nation_id);
        ai_manage_diplomacy(game, *nation_id, &mut actions);
        ai_build_merchant_ships(game, *nation_id);
        ai_build_warships(game, *nation_id);
        ai_military_strategy(game, *nation_id, &mut actions);
    }

    ai_declare_wars(game, &ai_nation_ids, &mut actions);

    actions
}

/// Returns true if a technology has military effects (upgrades units, unlocks military units/ships).
fn is_military_tech(effects: &[TechEffect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            TechEffect::UpgradeUnit { .. } | TechEffect::UnlockUnit(_) | TechEffect::UnlockShip(_)
        )
    })
}

/// Returns true if a technology has economic effects (buildings, terrain, infrastructure, civilians).
fn is_economic_tech(effects: &[TechEffect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            TechEffect::UnlockBuilding(_)
                | TechEffect::EnableTerrainImprovement { .. }
                | TechEffect::EnableInfrastructure(_)
                | TechEffect::EnableCivilian(_)
        )
    })
}

/// Pick a tech based on personality and research it if the nation can afford it.
///
/// - **Economic**: prefer the most expensive available tech (invest in the future)
/// - **Aggressive**: prefer military techs (unit upgrades, unit unlocks, ship unlocks)
/// - **Diplomatic**: prefer economic/trade techs (buildings, terrain, infrastructure)
/// - **Balanced**: pick the cheapest available tech (current behavior)
fn ai_research_tech(
    game: &mut GameState,
    nation_id: NationId,
    current_year: u32,
    actions: &mut Vec<String>,
) {
    let personality = get_personality(game, nation_id);

    // Gather the nation's researched techs
    let researched: Vec<TechId> = match game.get_nation(nation_id) {
        Some(n) => n.researched_techs.clone(),
        None => return,
    };

    // Find available techs
    let available = game.tech_tree.available_techs(&researched, current_year);
    if available.is_empty() {
        return;
    }

    // Select tech based on personality
    let chosen = match personality {
        AiPersonality::Economic => {
            // Prefer the most expensive tech (long-term investment)
            available.iter().max_by_key(|t| t.cost.cents())
        }
        AiPersonality::Aggressive => {
            // Prefer military techs, fallback to cheapest
            let military: Vec<_> = available
                .iter()
                .filter(|t| is_military_tech(&t.effects))
                .collect();
            if military.is_empty() {
                available.iter().min_by_key(|t| t.cost.cents())
            } else {
                military.into_iter().min_by_key(|t| t.cost.cents())
            }
        }
        AiPersonality::Diplomatic => {
            // Prefer economic/trade techs, fallback to cheapest
            let econ: Vec<_> = available
                .iter()
                .filter(|t| is_economic_tech(&t.effects))
                .collect();
            if econ.is_empty() {
                available.iter().min_by_key(|t| t.cost.cents())
            } else {
                econ.into_iter().min_by_key(|t| t.cost.cents())
            }
        }
        AiPersonality::Balanced => {
            // Pick the cheapest available tech
            available.iter().min_by_key(|t| t.cost.cents())
        }
    };

    let (tech_id, tech_cost, tech_name) = match chosen {
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
/// Personality affects thresholds and unit preferences:
///
/// - **Aggressive**: lower thresholds, prefer artillery
/// - **Diplomatic**: higher thresholds, fewer units
/// - **Economic**: moderate thresholds
/// - **Balanced**: default behavior
fn ai_build_military(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let personality = get_personality(game, nation_id);
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let army_count = nation.army.len();
    let treasury = nation.treasury;
    let capital = nation.capital_province_id;
    let nation_name = nation.name.clone();

    // Thresholds vary by personality
    let (tier1_max, tier1_treasury, tier2_max, tier2_treasury, tier3_treasury) = match personality {
        AiPersonality::Aggressive => (
            4,
            Money::dollars(1500),
            7,
            Money::dollars(3000),
            Money::dollars(6000),
        ),
        AiPersonality::Diplomatic => (
            2,
            Money::dollars(3000),
            4,
            Money::dollars(8000),
            Money::dollars(15000),
        ),
        AiPersonality::Economic => (
            3,
            Money::dollars(2500),
            5,
            Money::dollars(6000),
            Money::dollars(12000),
        ),
        AiPersonality::Balanced => (
            3,
            Money::dollars(2000),
            5,
            Money::dollars(5000),
            Money::dollars(10000),
        ),
    };

    if army_count < tier1_max && treasury > tier1_treasury {
        let cost = Money::dollars(500);
        nation.treasury -= cost;
        let unit = ArmyUnit::new(next_unit_id(), ArmyUnitType::Regulars, nation_id, capital);
        nation.army.push(unit);
        actions.push(format!(
            "{} has been expanding its military forces",
            nation_name
        ));
    } else if army_count < tier2_max && treasury > tier2_treasury {
        let cost = Money::dollars(1000);
        nation.treasury -= cost;
        // Aggressive personality prefers artillery over grenadiers when possible
        let unit_type = if personality == AiPersonality::Aggressive && army_count >= 3 {
            ArmyUnitType::LightArtillery
        } else {
            ArmyUnitType::Grenadiers
        };
        let build_cost = if unit_type == ArmyUnitType::LightArtillery {
            Money::dollars(2000)
        } else {
            Money::dollars(1000)
        };
        // Re-check if we can afford the artillery cost
        if unit_type == ArmyUnitType::LightArtillery {
            // Need to undo the grenadier cost and pay artillery cost
            nation.treasury += cost; // undo
            if nation.treasury > tier2_treasury {
                nation.treasury -= build_cost;
                let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
                nation.army.push(unit);
                actions.push(format!(
                    "{} has been expanding its military forces",
                    nation_name
                ));
            }
        } else {
            let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
            nation.army.push(unit);
            actions.push(format!(
                "{} has been expanding its military forces",
                nation_name
            ));
        }
    } else if army_count >= tier2_max && treasury > tier3_treasury {
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

/// Periodically, each AI Great Power considers declaring war on a Minor Nation.
/// Frequency and army threshold depend on personality:
///
/// - **Aggressive**: every 15 turns, needs >= 3 units
/// - **Diplomatic**: every 40 turns, needs >= 8 units
/// - **Economic**: every 30 turns, needs >= 5 units
/// - **Balanced**: every 20 turns, needs >= 4 units
fn ai_declare_wars(game: &mut GameState, ai_nation_ids: &[NationId], actions: &mut Vec<String>) {
    let turn_number = game.turn.0;

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
        let personality = get_personality(game, ai_id);

        // War frequency and army threshold depend on personality
        let (war_interval, army_threshold) = match personality {
            AiPersonality::Aggressive => (15u32, 3),
            AiPersonality::Diplomatic => (40, 8),
            AiPersonality::Economic => (30, 5),
            AiPersonality::Balanced => (20, 4),
        };

        // Check if this is a war-consideration turn for this AI
        if !turn_number.is_multiple_of(war_interval) {
            continue;
        }

        // Only attack if AI has enough army units
        let army_size = game.get_nation(ai_id).map(|n| n.army.len()).unwrap_or(0);
        if army_size < army_threshold {
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
/// - Expand mills when capacity is maxed (if resources > capacity * threshold)
/// - **Economic** personality: expand more aggressively (threshold multiplier 1 instead of 2)
fn ai_manage_economy(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);

    // Build infrastructure handles mills and factories
    ai_build_infrastructure(game, nation_id);

    // Economic personality expands more aggressively
    let expansion_threshold_multiplier: u32 = match personality {
        AiPersonality::Economic => 1,
        _ => 2,
    };

    // Expand mills when input resources exceed capacity * threshold
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
            if input_resources > b.effective_capacity() * expansion_threshold_multiplier
                && b.pending_capacity == 0
            {
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

/// AI builds trade consulates with Minor Nations.
///
/// - Requires treasury > $2,000 and $500 per consulate
/// - **Diplomatic** personality: build up to 4 consulates per turn
/// - Others: build up to 2 consulates per turn
fn ai_build_consulates(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);
    let cost = Money::dollars(500);
    let treasury_threshold = Money::dollars(2000);

    let max_per_turn = match personality {
        AiPersonality::Diplomatic => 4,
        _ => 2,
    };

    // Check treasury threshold
    let treasury = match game.get_nation(nation_id) {
        Some(n) => n.treasury,
        None => return,
    };
    if treasury < treasury_threshold {
        return;
    }

    // Gather minor nation IDs
    let minor_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| n.id)
        .collect();

    let mut built = 0;
    for mn_id in minor_ids {
        if built >= max_per_turn {
            break;
        }

        let treasury = match game.get_nation(nation_id) {
            Some(n) => n.treasury,
            None => return,
        };
        if treasury.checked_sub(cost).is_none() {
            break;
        }

        // Check if consulate already exists
        let already_has = game
            .diplomacy
            .get_relation(nation_id, mn_id)
            .is_some_and(|r| r.has_consulate);
        if already_has {
            continue;
        }

        // Build consulate
        if game.diplomacy.build_consulate(nation_id, mn_id).is_ok() {
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.treasury -= cost;
            }
            built += 1;
        }
    }
}

/// AI manages diplomatic relations: proposes treaties, sends grants.
///
/// - **Diplomatic**: proposes pacts with all Minor Nations it has embassies with,
///   proposes alliances with non-threatening GPs, sends grants to MNs with embassies.
/// - **Aggressive**: rarely proposes treaties, breaks alliances more easily.
/// - **Economic**: proposes pacts for trade security, sends grants.
/// - **All AI**: send small grants ($500) to Minor Nations with embassies to improve relations.
pub fn ai_manage_diplomacy(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let personality = get_personality(game, nation_id);
    let turn_number = game.turn.0;

    // Determine behavior parameters based on personality
    let (propose_pact_chance, propose_alliance_chance, grant_amount, grant_every_n_turns) =
        match personality {
            AiPersonality::Diplomatic => (true, true, 500i64, 4u32),
            AiPersonality::Economic => (true, false, 500, 6),
            AiPersonality::Aggressive => (false, false, 0, 0),
            AiPersonality::Balanced => (true, false, 500, 8),
        };

    // Phase 1: Propose non-aggression pacts with Minor Nations that have embassies
    if propose_pact_chance {
        let minor_ids: Vec<NationId> = game
            .nations
            .iter()
            .filter(|n| !n.is_great_power())
            .map(|n| n.id)
            .collect();

        for mn_id in minor_ids {
            let has_embassy = game
                .diplomacy
                .get_relation(nation_id, mn_id)
                .is_some_and(|r| r.has_embassy);
            if !has_embassy {
                continue;
            }

            let already_has_pact = game.diplomacy.has_treaty(
                nation_id,
                mn_id,
                crate::events::TreatyType::NonAggressionPact,
            );
            if already_has_pact {
                continue;
            }

            if game.diplomacy.propose_pact(nation_id, mn_id).is_ok() {
                let nation_name = game
                    .get_nation(nation_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let mn_name = game
                    .get_nation(mn_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                actions.push(format!(
                    "{} signed a non-aggression pact with {}",
                    nation_name, mn_name
                ));
            }
        }
    }

    // Phase 2: Propose alliances with other Great Powers (Diplomatic personality only)
    if propose_alliance_chance {
        let gp_ids: Vec<NationId> = game
            .nations
            .iter()
            .filter(|n| n.is_great_power() && n.id != nation_id && n.id != game.human_player_nation)
            .map(|n| n.id)
            .collect();

        for gp_id in gp_ids {
            let at_war = game
                .diplomacy
                .get_relation(nation_id, gp_id)
                .is_some_and(|r| r.at_war);
            if at_war {
                continue;
            }

            let already_allied =
                game.diplomacy
                    .has_treaty(nation_id, gp_id, crate::events::TreatyType::Alliance);
            if already_allied {
                continue;
            }

            // Only propose if score is positive (non-threatening)
            let score = game
                .diplomacy
                .get_relation(nation_id, gp_id)
                .map(|r| r.score)
                .unwrap_or(0);
            if score < 0 {
                continue;
            }

            if game.diplomacy.propose_alliance(nation_id, gp_id).is_ok() {
                let nation_name = game
                    .get_nation(nation_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let gp_name = game
                    .get_nation(gp_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                actions.push(format!(
                    "{} and {} have formed an alliance!",
                    nation_name, gp_name
                ));
            }
        }
    }

    // Phase 3: Send cash grants to Minor Nations with embassies
    if grant_amount > 0
        && grant_every_n_turns > 0
        && turn_number.is_multiple_of(grant_every_n_turns)
    {
        let grant = Money::dollars(grant_amount);
        let minor_ids: Vec<NationId> = game
            .nations
            .iter()
            .filter(|n| !n.is_great_power())
            .map(|n| n.id)
            .collect();

        for mn_id in minor_ids {
            let has_embassy = game
                .diplomacy
                .get_relation(nation_id, mn_id)
                .is_some_and(|r| r.has_embassy);
            if !has_embassy {
                continue;
            }

            let can_afford = game
                .get_nation(nation_id)
                .is_some_and(|n| n.treasury.checked_sub(grant).is_some());
            if !can_afford {
                break;
            }

            game.diplomacy.send_grant(nation_id, mn_id, grant);
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.treasury -= grant;
            }
        }
    }
}

/// AI builds merchant ships when it has the materials and too few ships.
///
/// - **Economic** personality: build up to 3 ships total
/// - Others: build if cargo capacity is 0
///
/// AI builds warships if it has fewer than the threshold and has the required materials.
///
/// - If AI has < 2 warships and has fabric + lumber + arms materials, build a Frigate.
/// - Aggressive AI builds up to 4 warships.
/// - If AI has steel but no arms, it produces arms from steel first.
fn ai_build_warships(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let max_warships: usize = match personality {
        AiPersonality::Aggressive => 4,
        _ => 2,
    };

    if nation.warship_count() >= max_warships {
        return;
    }

    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);
    let arms_have = nation.material_amount(MaterialType::Arms);
    let steel_have = nation.material_amount(MaterialType::Steel);

    // If we have the fabric and lumber but need arms, produce arms from steel
    if fabric_have >= 2 && lumber_have >= 5 && arms_have < 2 && steel_have > 0 {
        let arms_needed = 2 - arms_have;
        let arms_to_produce = arms_needed.min(steel_have);
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_material(MaterialType::Steel, arms_to_produce);
        nation.add_material(MaterialType::Arms, arms_to_produce);
    }

    // Re-check after possible arms production
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);
    let arms_have = nation.material_amount(MaterialType::Arms);

    // Try to build a Frigate: 2 fabric + 5 lumber + 2 arms
    if fabric_have >= 2 && lumber_have >= 5 && arms_have >= 2 {
        let uid = next_unit_id();
        let ship = Ship::new(uid, ShipType::Frigate, nation_id);
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_material(MaterialType::Fabric, 2);
        nation.consume_material(MaterialType::Lumber, 5);
        nation.consume_material(MaterialType::Arms, 2);
        nation.warships.push(ship);
    }
}

fn ai_build_merchant_ships(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Ship cap depends on personality
    let max_ships: usize = match personality {
        AiPersonality::Economic => 3,
        _ => 1,
    };

    // For non-Economic, only build if cargo capacity is 0
    if personality != AiPersonality::Economic && nation.total_cargo_capacity() > 0 {
        return;
    }

    if nation.merchant_ship_count() >= max_ships {
        return;
    }

    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);

    // Try to build Trader (2 fabric + 4 lumber)
    if fabric_have >= 2 && lumber_have >= 4 {
        let uid = next_unit_id();
        let ship = Ship::new(uid, ShipType::Trader, nation_id);
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_material(MaterialType::Fabric, 2);
        nation.consume_material(MaterialType::Lumber, 4);
        nation.merchant_fleet.push(ship);
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
            pending_moves: Vec::new(),
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
            pending_moves: Vec::new(),
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

    // ── Personality assignment ────────────────────────────────

    #[test]
    fn personality_assignment_is_deterministic() {
        assert_eq!(personality_for_nation_index(0), AiPersonality::Balanced);
        assert_eq!(personality_for_nation_index(1), AiPersonality::Aggressive);
        assert_eq!(personality_for_nation_index(2), AiPersonality::Economic);
        assert_eq!(personality_for_nation_index(3), AiPersonality::Aggressive);
        assert_eq!(personality_for_nation_index(4), AiPersonality::Diplomatic);
        assert_eq!(personality_for_nation_index(5), AiPersonality::Economic);
        assert_eq!(personality_for_nation_index(6), AiPersonality::Balanced);
        // Out-of-range defaults to Balanced
        assert_eq!(personality_for_nation_index(99), AiPersonality::Balanced);
    }

    #[test]
    fn personality_display_format() {
        assert_eq!(format!("{}", AiPersonality::Aggressive), "Aggressive");
        assert_eq!(format!("{}", AiPersonality::Diplomatic), "Diplomatic");
        assert_eq!(format!("{}", AiPersonality::Economic), "Economic");
        assert_eq!(format!("{}", AiPersonality::Balanced), "Balanced");
    }

    #[test]
    fn new_game_assigns_ai_personalities() {
        let gs = crate::game_state::new_game("test", Difficulty::Normal, 0);
        // Human player (index 0, Deneb) should have no personality
        let human = gs.get_nation(gs.human_player_nation).unwrap();
        assert_eq!(
            human.ai_personality, None,
            "Human player should not have an AI personality"
        );

        // Other Great Powers should have personalities
        for nation in gs.great_powers() {
            if nation.id == gs.human_player_nation {
                continue;
            }
            assert!(
                nation.ai_personality.is_some(),
                "AI Great Power {} should have a personality",
                nation.name
            );
        }
    }

    // ── Personality affects tech choice ──────────────────────

    #[test]
    fn economic_ai_prefers_expensive_tech() {
        let mut game = test_game_with_ai();
        // Set Economic personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Economic);
        ai.treasury = Money::dollars(50000);

        // At year 1821, multiple techs with different costs are available
        game.turn = TurnNumber::from_year_quarter(1821, 1);

        // Pre-research the free techs so only paid techs remain
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.research_tech(TechId(1));
        ai.research_tech(TechId(2));

        let mut actions = Vec::new();
        ai_research_tech(&mut game, NationId(2), 1821, &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Economic should pick the most expensive available tech
        // At 1821: Iron Railroad Bridge ($1,500), Feed Grasses ($1,500),
        // Square-Set Timbering ($1,500), Streamlined Hulls ($1,500)
        // All cost $1,500 so any of them is valid (they're equally expensive)
        assert!(
            ai.researched_techs.len() > 2,
            "Economic AI should have researched a tech"
        );
    }

    #[test]
    fn aggressive_ai_prefers_military_tech() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Aggressive);
        ai.treasury = Money::dollars(100000);

        // Set year to 1841 when Breech-Loading Rifles (military) is available
        game.turn = TurnNumber::from_year_quarter(1841, 1);

        // Pre-research Bessemer Converter (prerequisite for Breech-Loading Rifles)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.research_tech(TechId(1));
        ai.research_tech(TechId(2));
        ai.research_tech(TechId(11)); // Bessemer Converter

        let mut actions = Vec::new();
        ai_research_tech(&mut game, NationId(2), 1841, &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have picked Breech-Loading Rifles (TechId 13, military)
        // or Rifled Artillery (TechId 14, also military)
        let has_military = ai.has_researched(TechId(13)) || ai.has_researched(TechId(14));
        assert!(
            has_military,
            "Aggressive AI should prefer military techs (Breech-Loading Rifles or Rifled Artillery)"
        );
    }

    #[test]
    fn balanced_ai_picks_cheapest_tech() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.treasury = Money::dollars(50000);

        // At 1815, two free techs (ID 1 and 2) are available
        let mut actions = Vec::new();
        ai_research_tech(&mut game, NationId(2), 1815, &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should pick one of the free techs
        assert!(
            ai.has_researched(TechId(1)) || ai.has_researched(TechId(2)),
            "Balanced AI should pick the cheapest (free) tech"
        );
    }

    // ── Personality affects war declaration ──────────────────

    #[test]
    fn aggressive_ai_declares_war_on_turn_15() {
        let mut game = test_game_with_ai_and_minor();
        // Set Aggressive personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Aggressive);
        // Give AI 3 army units (Aggressive threshold is 3)
        for i in 0..3 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        // Turn 15: Aggressive declares (every 15 turns), Balanced would not (every 20)
        game.turn = TurnNumber::new(15);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        assert!(
            rel.is_some() && rel.unwrap().at_war,
            "Aggressive AI should declare war on turn 15"
        );
    }

    #[test]
    fn diplomatic_ai_does_not_declare_war_on_turn_20() {
        let mut game = test_game_with_ai_and_minor();
        // Set Diplomatic personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Diplomatic);
        // Give AI 5 army units (enough for Balanced, not enough for Diplomatic which needs 8)
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        game.turn = TurnNumber::new(20);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Turn 20 is not a multiple of 40, so Diplomatic AI should not declare war
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            !at_war,
            "Diplomatic AI should not declare war on turn 20 (interval is 40)"
        );
    }

    // ── Consulate building ──────────────────────────────────

    #[test]
    fn diplomatic_ai_builds_more_consulates() {
        let mut game = test_game_with_ai_and_minor();
        // Add more minor nations for the test
        let province4 = Province::new(
            ProvinceId(4),
            "Minor Capital 2".to_string(),
            NationId(4),
            HexCoord::new(7, 7),
            vec![HexCoord::new(7, 7)],
            3,
        );
        let province5 = Province::new(
            ProvinceId(5),
            "Minor Capital 3".to_string(),
            NationId(5),
            HexCoord::new(8, 8),
            vec![HexCoord::new(8, 8)],
            3,
        );
        let province6 = Province::new(
            ProvinceId(6),
            "Minor Capital 4".to_string(),
            NationId(6),
            HexCoord::new(9, 9),
            vec![HexCoord::new(9, 9)],
            3,
        );
        let province7 = Province::new(
            ProvinceId(7),
            "Minor Capital 5".to_string(),
            NationId(7),
            HexCoord::new(4, 4),
            vec![HexCoord::new(4, 4)],
            3,
        );
        game.provinces.push(province4);
        game.provinces.push(province5);
        game.provinces.push(province6);
        game.provinces.push(province7);
        game.nations.push(Nation::new(
            NationId(4),
            "Minor2".to_string(),
            NationColor::Brown,
            NationType::MinorNation,
            ProvinceId(4),
        ));
        game.nations.push(Nation::new(
            NationId(5),
            "Minor3".to_string(),
            NationColor::Pink,
            NationType::MinorNation,
            ProvinceId(5),
        ));
        game.nations.push(Nation::new(
            NationId(6),
            "Minor4".to_string(),
            NationColor::Teal,
            NationType::MinorNation,
            ProvinceId(6),
        ));
        game.nations.push(Nation::new(
            NationId(7),
            "Minor5".to_string(),
            NationColor::Olive,
            NationType::MinorNation,
            ProvinceId(7),
        ));

        // Set Diplomatic personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Diplomatic);
        ai.treasury = Money::dollars(10000);

        ai_build_consulates(&mut game, NationId(2));

        // Count consulates built
        let consulate_count = [
            NationId(3),
            NationId(4),
            NationId(5),
            NationId(6),
            NationId(7),
        ]
        .iter()
        .filter(|&&mn_id| {
            game.diplomacy
                .get_relation(NationId(2), mn_id)
                .is_some_and(|r| r.has_consulate)
        })
        .count();

        assert_eq!(
            consulate_count, 4,
            "Diplomatic AI should build up to 4 consulates"
        );
    }

    #[test]
    fn balanced_ai_builds_fewer_consulates() {
        let mut game = test_game_with_ai_and_minor();
        // Add more minor nations
        game.provinces.push(Province::new(
            ProvinceId(4),
            "Minor2 Capital".to_string(),
            NationId(4),
            HexCoord::new(7, 7),
            vec![HexCoord::new(7, 7)],
            3,
        ));
        game.provinces.push(Province::new(
            ProvinceId(5),
            "Minor3 Capital".to_string(),
            NationId(5),
            HexCoord::new(8, 8),
            vec![HexCoord::new(8, 8)],
            3,
        ));
        game.provinces.push(Province::new(
            ProvinceId(6),
            "Minor4 Capital".to_string(),
            NationId(6),
            HexCoord::new(9, 9),
            vec![HexCoord::new(9, 9)],
            3,
        ));
        game.nations.push(Nation::new(
            NationId(4),
            "Minor2".to_string(),
            NationColor::Brown,
            NationType::MinorNation,
            ProvinceId(4),
        ));
        game.nations.push(Nation::new(
            NationId(5),
            "Minor3".to_string(),
            NationColor::Pink,
            NationType::MinorNation,
            ProvinceId(5),
        ));
        game.nations.push(Nation::new(
            NationId(6),
            "Minor4".to_string(),
            NationColor::Teal,
            NationType::MinorNation,
            ProvinceId(6),
        ));

        // Set Balanced personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.treasury = Money::dollars(10000);

        ai_build_consulates(&mut game, NationId(2));

        // Count consulates built
        let consulate_count = [NationId(3), NationId(4), NationId(5), NationId(6)]
            .iter()
            .filter(|&&mn_id| {
                game.diplomacy
                    .get_relation(NationId(2), mn_id)
                    .is_some_and(|r| r.has_consulate)
            })
            .count();

        assert_eq!(
            consulate_count, 2,
            "Balanced AI should build up to 2 consulates"
        );
    }

    #[test]
    fn ai_does_not_build_consulates_when_poor() {
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.treasury = Money::dollars(1000); // Below $2,000 threshold

        ai_build_consulates(&mut game, NationId(2));

        let has_consulate = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .is_some_and(|r| r.has_consulate);
        assert!(
            !has_consulate,
            "AI should not build consulates when treasury < $2,000"
        );
    }

    // ── Merchant ship building ──────────────────────────────

    #[test]
    fn economic_ai_builds_merchant_ships() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Economic);
        ai.add_material(MaterialType::Fabric, 10);
        ai.add_material(MaterialType::Lumber, 20);

        // Build ships up to 3 for Economic personality
        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            1,
            "Should build 1 ship per call"
        );

        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            2,
        );

        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            3,
        );

        // Should not build more than 3
        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            3,
            "Economic AI should cap at 3 ships"
        );
    }

    #[test]
    fn balanced_ai_only_builds_one_merchant_ship() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 10);
        ai.add_material(MaterialType::Lumber, 20);

        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            1,
        );

        // Should not build more (has cargo capacity > 0)
        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            1,
            "Balanced AI should only build 1 ship (has cargo capacity)"
        );
    }

    // ── Warship building ──────────────────────────────────────

    #[test]
    fn ai_builds_warship_with_arms() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Arms, 4);

        ai_build_warships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            1,
            "AI should build a warship when it has sufficient materials"
        );
    }

    #[test]
    fn ai_produces_arms_from_steel_for_warships() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Steel, 5);
        // No arms at all

        ai_build_warships(&mut game, NationId(2));
        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.warship_count(),
            1,
            "AI should produce arms from steel and build a warship"
        );
        // Steel should be consumed: 2 for arms production
        assert_eq!(ai.material_amount(MaterialType::Steel), 3);
    }

    #[test]
    fn ai_does_not_build_warship_without_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        // No materials at all

        ai_build_warships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            0,
            "AI should not build warships without materials"
        );
    }

    #[test]
    fn aggressive_ai_builds_up_to_four_warships() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Aggressive);
        ai.add_material(MaterialType::Fabric, 20);
        ai.add_material(MaterialType::Lumber, 40);
        ai.add_material(MaterialType::Arms, 20);

        for _ in 0..4 {
            ai_build_warships(&mut game, NationId(2));
        }
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            4,
            "Aggressive AI should build up to 4 warships"
        );

        // Should not build a 5th
        ai_build_warships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            4,
            "Aggressive AI should cap at 4 warships"
        );
    }

    #[test]
    fn balanced_ai_caps_at_two_warships() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 20);
        ai.add_material(MaterialType::Lumber, 40);
        ai.add_material(MaterialType::Arms, 20);

        for _ in 0..3 {
            ai_build_warships(&mut game, NationId(2));
        }
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            2,
            "Balanced AI should cap at 2 warships"
        );
    }

    #[test]
    fn ai_produces_partial_arms_from_steel() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Arms, 1); // have 1, need 2
        ai.add_material(MaterialType::Steel, 1); // can produce 1 more

        ai_build_warships(&mut game, NationId(2));
        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.warship_count(),
            1,
            "AI should produce 1 arms from steel to supplement existing 1 arms"
        );
        assert_eq!(ai.material_amount(MaterialType::Steel), 0);
    }

    #[test]
    fn ai_does_not_produce_arms_when_no_steel() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        // No arms and no steel

        ai_build_warships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            0,
            "AI should not build warship without arms or steel"
        );
    }

    // ── AI diplomacy tests ────────────────────────────────────────

    #[test]
    fn diplomatic_ai_proposes_pacts_with_embassy_nations() {
        let mut game = test_game_with_ai_and_minor();
        let ai_id = NationId(2);
        let mn_id = NationId(3);

        // Set AI to Diplomatic personality
        game.get_nation_mut(ai_id).unwrap().ai_personality = Some(AiPersonality::Diplomatic);

        // Build consulate and embassy for the AI with the minor nation
        game.diplomacy.build_consulate(ai_id, mn_id).unwrap();
        game.diplomacy.build_embassy(ai_id, mn_id).unwrap();

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        // Diplomatic AI should propose a pact
        assert!(
            game.diplomacy
                .has_treaty(ai_id, mn_id, crate::events::TreatyType::NonAggressionPact),
            "Diplomatic AI should propose pact with Minor Nation it has embassy with"
        );
        assert!(
            actions.iter().any(|a| a.contains("non-aggression pact")),
            "Should report pact in actions"
        );
    }

    #[test]
    fn aggressive_ai_does_not_propose_treaties() {
        let mut game = test_game_with_ai_and_minor();
        let ai_id = NationId(2);
        let mn_id = NationId(3);

        // Set AI to Aggressive personality
        game.get_nation_mut(ai_id).unwrap().ai_personality = Some(AiPersonality::Aggressive);

        // Build consulate and embassy
        game.diplomacy.build_consulate(ai_id, mn_id).unwrap();
        game.diplomacy.build_embassy(ai_id, mn_id).unwrap();

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        // Aggressive AI should NOT propose pacts
        assert!(
            !game
                .diplomacy
                .has_treaty(ai_id, mn_id, crate::events::TreatyType::NonAggressionPact),
            "Aggressive AI should not propose pacts"
        );
        assert!(
            actions.is_empty(),
            "Aggressive AI should not take diplomatic actions"
        );
    }

    #[test]
    fn diplomatic_ai_sends_grants() {
        let mut game = test_game_with_ai_and_minor();
        let ai_id = NationId(2);
        let mn_id = NationId(3);

        // Set AI to Diplomatic personality
        game.get_nation_mut(ai_id).unwrap().ai_personality = Some(AiPersonality::Diplomatic);
        // Set turn to multiple of 4 so Diplomatic AI sends grants
        game.turn = TurnNumber::new(4);

        // Build consulate and embassy
        game.diplomacy.build_consulate(ai_id, mn_id).unwrap();
        game.diplomacy.build_embassy(ai_id, mn_id).unwrap();

        let score_before = game.diplomacy.get_relation(ai_id, mn_id).unwrap().score;

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        let score_after = game.diplomacy.get_relation(ai_id, mn_id).unwrap().score;

        // Score should have improved (pact gives +10, grant gives +5)
        assert!(
            score_after > score_before,
            "AI grant should improve relationship score (before: {}, after: {})",
            score_before,
            score_after
        );

        // Treasury should have decreased by $500 for the grant
        assert!(
            game.get_nation(ai_id).unwrap().treasury < Money::dollars(10000),
            "AI treasury should decrease after sending grant"
        );
    }

    #[test]
    fn diplomatic_ai_proposes_alliances_with_gps() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Set AI to Diplomatic personality
        game.get_nation_mut(ai_id).unwrap().ai_personality = Some(AiPersonality::Diplomatic);

        // Add a third GP that is AI-controlled (non-human, non-current-AI)
        let mut gp3 = Nation::new(
            NationId(4),
            "ThirdPower".to_string(),
            NationColor::Green,
            NationType::GreatPower,
            ProvinceId(2),
        );
        gp3.ai_personality = Some(AiPersonality::Balanced);
        gp3.treasury = Money::dollars(10000);
        game.nations.push(gp3);

        // Initialize GP embassies (so they have embassies with each other)
        let gp_ids: Vec<NationId> = game
            .nations
            .iter()
            .filter(|n| n.is_great_power())
            .map(|n| n.id)
            .collect();
        game.diplomacy.initialize_great_powers(&gp_ids);

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        // Diplomatic AI should propose alliance with ThirdPower (not human player)
        assert!(
            game.diplomacy
                .has_treaty(ai_id, NationId(4), crate::events::TreatyType::Alliance),
            "Diplomatic AI should propose alliance with non-threatening GP"
        );
    }
}
