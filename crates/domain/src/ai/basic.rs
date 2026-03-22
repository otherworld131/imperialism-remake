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
        3 => AiPersonality::Balanced,
        4 => AiPersonality::Diplomatic,
        5 => AiPersonality::Aggressive,
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

    // Shuffle AI nation processing order to prevent first-mover advantage
    let mut ai_ids = ai_nation_ids.clone();
    let turn_seed = game.turn.0 as usize;
    for i in (1..ai_ids.len()).rev() {
        let j = (turn_seed.wrapping_mul(i + 7)) % (i + 1);
        ai_ids.swap(i, j);
    }

    for nation_id in &ai_ids {
        ai_research_tech(game, *nation_id, current_year, &mut actions);
        ai_manage_economy(game, *nation_id);
        ai_build_map_infrastructure(game, *nation_id);
        ai_manage_resources(game, *nation_id, &mut actions);
        ai_recruit_workers(game, *nation_id);
        ai_manage_civilians(game, *nation_id);
        ai_build_military(game, *nation_id, &mut actions);
        ai_trade(game, *nation_id);
        ai_build_transport_proactive(game, *nation_id);
        ai_build_consulates(game, *nation_id);
        ai_manage_diplomacy(game, *nation_id, &mut actions);
        ai_pre_election_strategy(game, *nation_id, &mut actions);
        ai_build_merchant_ships(game, *nation_id);
        ai_build_warships(game, *nation_id);
        ai_naval_strategy(game, *nation_id, &mut actions);
        ai_military_strategy(game, *nation_id, &mut actions);
        ai_tactical_decisions(game, *nation_id, &mut actions);
        ai_train_and_promote_workers(game, *nation_id);
    }

    ai_declare_wars(game, &ai_ids, &mut actions);

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
    let available = game
        .game_data
        .tech_tree
        .available_techs(&researched, current_year);
    if available.is_empty() {
        return;
    }

    // Build a personality-ranked list of candidate techs, then pick from the
    // top few using a per-nation pseudo-random seed so different nations
    // (and different games) produce varied research paths.
    let candidates: Vec<_> = match personality {
        AiPersonality::Economic => {
            // Prefer the most expensive techs (long-term investment)
            let mut v = available.clone();
            v.sort_by(|a, b| b.cost.cents().cmp(&a.cost.cents()));
            v
        }
        AiPersonality::Aggressive => {
            // Prefer military techs sorted cheapest-first, then non-military cheapest-first
            let mut military: Vec<_> = available
                .iter()
                .filter(|t| is_military_tech(&t.effects))
                .cloned()
                .collect();
            let mut other: Vec<_> = available
                .iter()
                .filter(|t| !is_military_tech(&t.effects))
                .cloned()
                .collect();
            military.sort_by_key(|t| t.cost.cents());
            other.sort_by_key(|t| t.cost.cents());
            military.extend(other);
            military
        }
        AiPersonality::Diplomatic => {
            // Prefer economic/trade techs sorted cheapest-first, then others
            let mut econ: Vec<_> = available
                .iter()
                .filter(|t| is_economic_tech(&t.effects))
                .cloned()
                .collect();
            let mut other: Vec<_> = available
                .iter()
                .filter(|t| !is_economic_tech(&t.effects))
                .cloned()
                .collect();
            econ.sort_by_key(|t| t.cost.cents());
            other.sort_by_key(|t| t.cost.cents());
            econ.extend(other);
            econ
        }
        AiPersonality::Balanced => {
            // Cheapest first
            let mut v = available.clone();
            v.sort_by_key(|t| t.cost.cents());
            v
        }
    };

    if candidates.is_empty() {
        return;
    }

    // Collect all candidate data as owned values so we can release the borrow on game
    let all_candidates: Vec<(TechId, Money, String)> = candidates
        .iter()
        .map(|t| (t.id, t.cost, t.name.clone()))
        .collect();

    // Pick from the top candidates using a deterministic per-nation seed
    // so that each nation gets a different research path each game.
    let top_n = all_candidates.len().min(3);
    let seed = (game.turn.0 as usize).wrapping_mul(nation_id.0 as usize + 7) % top_n;
    let (tech_id, tech_cost, ref tech_name) = all_candidates[seed];

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
        let entry_text = format!("{} researched {}", nation_name, tech_name);
        // Deduplicate: only push if this exact text doesn't already exist for this turn
        if !game
            .history
            .iter()
            .any(|(t, text)| *t == turn && text == &entry_text)
        {
            game.history.push((turn, entry_text));
        }
        return; // Successfully researched
    }

    // Second pass: if we couldn't afford the preferred tech and treasury is high,
    // try ANY available tech (cheapest first) to avoid hoarding cash.
    let treasury = match game.get_nation(nation_id) {
        Some(n) => n.treasury,
        None => return,
    };
    if treasury > Money::dollars(10_000) {
        let mut fallback_candidates = all_candidates;
        fallback_candidates.sort_by_key(|(_, cost, _)| cost.cents());
        for (cand_id, cand_cost, cand_name) in &fallback_candidates {
            let nation = match game.get_nation_mut(nation_id) {
                Some(n) => n,
                None => return,
            };
            if let Some(remaining) = nation.treasury.checked_sub(*cand_cost) {
                nation.treasury = remaining;
                nation.research_tech(*cand_id);
                let nation_name = nation.name.clone();
                actions.push(format!(
                    "Scientists in {} have discovered {}!",
                    nation_name, cand_name
                ));
                let turn = game.turn;
                let entry_text = format!("{} researched {}", nation_name, cand_name);
                if !game
                    .history
                    .iter()
                    .any(|(t, text)| *t == turn && text == &entry_text)
                {
                    game.history.push((turn, entry_text));
                }
                return;
            }
        }
    }
}

/// Build mills and factories when the nation has the required materials.
fn ai_build_infrastructure(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Build mills if the nation doesn't have them.
    // First mill of each type is free (bootstrap) — this prevents the chicken-and-egg
    // problem where mills require Lumber+Steel that can only be produced by mills.
    // This mirrors the original Imperialism where nations had basic industry from the start.
    let mill_types = [
        BuildingType::LumberMill,
        BuildingType::SteelMill,
        BuildingType::TextileMill,
    ];
    for mill_type in mill_types {
        if !nation.has_building(mill_type) {
            // First mill is free (bootstrap) — no material cost
            nation.buildings.push(Building::new(mill_type, 2));
        }
    }

    // Build factories: first one of each type is free (bootstrap), same as mills
    let mill_factory_pairs = [
        (BuildingType::LumberMill, BuildingType::FurnitureFactory),
        (BuildingType::SteelMill, BuildingType::HardwareFactory),
        (BuildingType::TextileMill, BuildingType::ClothingFactory),
    ];
    for (mill, factory) in mill_factory_pairs {
        if nation.has_building(mill) && !nation.has_building(factory) {
            nation.buildings.push(Building::new(factory, 1));
        }
    }
}

/// AI builds map infrastructure: depots and railroads to connect provinces.
///
/// Strategy: Build a depot on the capital province first, then build depots on
/// adjacent provinces, and railroads to link them. This allows resource flow.
fn ai_build_map_infrastructure(game: &mut GameState, nation_id: NationId) {
    use crate::map::infrastructure::{build_depot, build_railroad};

    let treasury = match game.get_nation(nation_id) {
        Some(n) => n.treasury,
        None => return,
    };

    // Need at least $3,000 to afford a depot + some railroads
    if treasury < Money::dollars(3000) {
        return;
    }

    // Get nation's province IDs and capital province
    let capital_province_id = match game.get_nation(nation_id) {
        Some(n) => n.capital_province_id,
        None => return,
    };

    let province_ids: Vec<ProvinceId> = match game.get_nation(nation_id) {
        Some(n) => n.province_ids.clone(),
        None => return,
    };

    // Step 1: Build depot on capital province if it doesn't have one
    let capital_tiles: Vec<crate::hex::HexCoord> = game
        .get_province(capital_province_id)
        .map(|p| p.tiles.clone())
        .unwrap_or_default();

    let capital_has_depot = capital_tiles.iter().any(|coord| {
        game.hex_map
            .get_tile(*coord)
            .is_some_and(|t| t.infrastructure.has_depot)
    });

    if !capital_has_depot {
        if let Some(&tile_coord) = capital_tiles.first()
            && let Ok(cost) = build_depot(&mut game.hex_map, tile_coord)
            && let Some(nation) = game.get_nation_mut(nation_id)
        {
            nation.treasury -= cost;
        }
        return; // One major action per turn
    }

    // Step 2: Build railroads on capital province tiles that don't have them
    let mut spent = Money::dollars(0);
    let spend_limit = Money::dollars(2000);
    for &tile_coord in &capital_tiles {
        if spent >= spend_limit {
            break;
        }
        let needs_rr = game
            .hex_map
            .get_tile(tile_coord)
            .is_some_and(|t| !t.infrastructure.has_railroad);
        if needs_rr && let Ok(cost) = build_railroad(&mut game.hex_map, tile_coord) {
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.treasury -= cost;
            }
            spent += cost;
        }
    }
    if spent > Money::dollars(0) {
        return; // Spent this turn on railroads
    }

    // Step 3: Build depots on adjacent provinces + railroads to connect
    for &pid in &province_ids {
        if pid == capital_province_id {
            continue;
        }

        let prov_tiles: Vec<crate::hex::HexCoord> = game
            .get_province(pid)
            .map(|p| p.tiles.clone())
            .unwrap_or_default();

        let has_depot = prov_tiles.iter().any(|coord| {
            game.hex_map
                .get_tile(*coord)
                .is_some_and(|t| t.infrastructure.has_depot)
        });

        if !has_depot {
            if let Some(&tile_coord) = prov_tiles.first()
                && game
                    .get_nation(nation_id)
                    .is_some_and(|n| n.treasury >= Money::dollars(2000))
                && let Ok(cost) = build_depot(&mut game.hex_map, tile_coord)
                && let Some(nation) = game.get_nation_mut(nation_id)
            {
                nation.treasury -= cost;
            }
            return; // One depot per turn
        }

        // Build railroads on this province's tiles to extend the network
        for &tile_coord in &prov_tiles {
            let can_afford = game
                .get_nation(nation_id)
                .is_some_and(|n| n.treasury >= Money::dollars(200));
            if !can_afford {
                break;
            }
            let needs_rr = game
                .hex_map
                .get_tile(tile_coord)
                .is_some_and(|t| !t.infrastructure.has_railroad);
            if needs_rr && let Ok(cost) = build_railroad(&mut game.hex_map, tile_coord) {
                if let Some(nation) = game.get_nation_mut(nation_id) {
                    nation.treasury -= cost;
                }
                return; // One railroad per province per turn to spread cost
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

    // Scale max workers with province count (2 per province, min 5)
    // Wealthy nations invest in workforce growth (3 per province)
    let workers_per_province: u32 = if nation.treasury > Money::dollars(20_000) {
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
    let turn_number = game.turn.0;
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let army_count = nation.army.len();
    let treasury = nation.treasury;
    let capital = nation.capital_province_id;
    let nation_name = nation.name.clone();

    // Deterministic per-nation seed for unit-type variety
    let variety_seed = (turn_number as usize).wrapping_mul(nation_id.0 as usize + 7);

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
        // Tier 1: pick from basic unit types with personality bias + variety
        let tier1_options: &[ArmyUnitType] = match personality {
            AiPersonality::Aggressive => &[
                ArmyUnitType::Regulars,
                ArmyUnitType::Regulars,
                ArmyUnitType::Grenadiers,
            ],
            AiPersonality::Diplomatic => &[
                ArmyUnitType::Regulars,
                ArmyUnitType::Regulars,
                ArmyUnitType::Regulars,
            ],
            AiPersonality::Economic => &[
                ArmyUnitType::Regulars,
                ArmyUnitType::Regulars,
                ArmyUnitType::Grenadiers,
            ],
            AiPersonality::Balanced => &[
                ArmyUnitType::Regulars,
                ArmyUnitType::Grenadiers,
                ArmyUnitType::Regulars,
            ],
        };
        let unit_type = tier1_options[variety_seed % tier1_options.len()];
        let cost = match unit_type {
            ArmyUnitType::Grenadiers => Money::dollars(1000),
            _ => Money::dollars(500),
        };
        if treasury > tier1_treasury + cost {
            nation.treasury -= cost;
            let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
            nation.army.push(unit);
            actions.push(format!(
                "{} has been expanding its military forces",
                nation_name
            ));
        }
    } else if army_count < tier2_max && treasury > tier2_treasury {
        // Tier 2: mix of grenadiers and artillery with personality + variety
        let tier2_options: &[ArmyUnitType] = match personality {
            AiPersonality::Aggressive => &[
                ArmyUnitType::LightArtillery,
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
            ],
            AiPersonality::Diplomatic => &[
                ArmyUnitType::Grenadiers,
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
            ],
            AiPersonality::Economic => &[
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
                ArmyUnitType::Grenadiers,
            ],
            AiPersonality::Balanced => &[
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
                ArmyUnitType::Grenadiers,
            ],
        };
        let unit_type = tier2_options[variety_seed % tier2_options.len()];
        let build_cost = if unit_type == ArmyUnitType::LightArtillery {
            Money::dollars(2000)
        } else {
            Money::dollars(1000)
        };
        if treasury > tier2_treasury + build_cost {
            nation.treasury -= build_cost;
            let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
            nation.army.push(unit);
            actions.push(format!(
                "{} has been expanding its military forces",
                nation_name
            ));
        }
    } else if army_count >= tier2_max && treasury > tier3_treasury {
        // Tier 3: advanced units with some variety
        // Cap total army size to prevent runaway military buildup
        let tier3_max = match personality {
            AiPersonality::Aggressive => 15,
            AiPersonality::Diplomatic => 8,
            AiPersonality::Economic => 10,
            AiPersonality::Balanced => 12,
        };
        if army_count < tier3_max {
            let tier3_options: &[ArmyUnitType] = match personality {
                AiPersonality::Aggressive => &[
                    ArmyUnitType::LightArtillery,
                    ArmyUnitType::LightArtillery,
                    ArmyUnitType::Grenadiers,
                ],
                _ => &[
                    ArmyUnitType::LightArtillery,
                    ArmyUnitType::Grenadiers,
                    ArmyUnitType::LightArtillery,
                ],
            };
            // Build up to 2 units per turn when treasury is very high (> $20,000)
            let units_to_build = if treasury > Money::dollars(20_000) {
                2
            } else {
                1
            };
            for i in 0..units_to_build {
                if nation.army.len() >= tier3_max {
                    break;
                }
                let unit_type = tier3_options[(variety_seed.wrapping_add(i)) % tier3_options.len()];
                let cost = if unit_type == ArmyUnitType::LightArtillery {
                    Money::dollars(2000)
                } else {
                    Money::dollars(1000)
                };
                if let Some(remaining) = nation.treasury.checked_sub(cost) {
                    nation.treasury = remaining;
                    let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
                    nation.army.push(unit);
                    if i == 0 {
                        actions.push(format!(
                            "{} has been expanding its military forces",
                            nation_name
                        ));
                    }
                } else {
                    break;
                }
            }
        }
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

    // Collect minor nation IDs, their capitals, names, and tile counts.
    // Skip minor nations that have been fully conquered: check both
    // province_ids (ownership tracking) and actual province ownership.
    let minor_nations: Vec<(NationId, ProvinceId, String, usize)> = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .filter(|n| {
            // Must still actually own at least one province
            game.provinces.iter().any(|p| p.owner == n.id)
        })
        .map(|n| {
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
            AiPersonality::Aggressive => (25u32, 5),
            AiPersonality::Diplomatic => (40, 8),
            AiPersonality::Economic => (30, 5),
            AiPersonality::Balanced => (25, 4),
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

        // AI with low standing (<30) avoids declaring wars to limit diplomatic damage
        let standing = game.diplomacy.get_standing(ai_id);
        if standing < 30 {
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

        // Skip candidates with 0 tiles (already fully conquered)
        candidates.retain(|c| c.3 > 0);

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

        // Find a province actually owned by the target to attack
        let attack_province = game
            .provinces
            .iter()
            .find(|p| p.owner == target_id)
            .map(|p| p.id)
            .unwrap_or(target_capital);

        // Skip if no province is actually owned by the target
        if game.provinces.iter().all(|p| p.owner != target_id) {
            continue;
        }

        let attacker_name = game
            .get_nation(ai_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        game.diplomacy.declare_war(ai_id, target_id);
        game.pending_attacks.push((ai_id, attack_province));
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

    // Phase 2: Great Power vs Great Power wars
    // Aggressive AIs will target weaker Great Powers when no minor targets remain
    // and they have military superiority.
    let remaining_minors: usize = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power() && !n.province_ids.is_empty())
        .count();

    if remaining_minors <= 2 && turn_number > 40 {
        for &ai_id in ai_nation_ids {
            let personality = get_personality(game, ai_id);

            // Aggressive, Balanced, and Economic AIs consider GP wars
            let gp_war_interval = match personality {
                AiPersonality::Aggressive => 30u32,
                AiPersonality::Economic => 50,
                AiPersonality::Balanced => 60,
                _ => continue,
            };

            if !turn_number.is_multiple_of(gp_war_interval) {
                continue;
            }

            let ai_army = game.get_nation(ai_id).map(|n| n.army.len()).unwrap_or(0);
            if ai_army < 4 {
                continue;
            }

            let ai_provinces = game
                .get_nation(ai_id)
                .map(|n| n.province_count())
                .unwrap_or(0);

            // Find weakest GP that is not allied, not already at war with us, not human
            let mut gp_targets: Vec<(NationId, usize, ProvinceId)> = game
                .nations
                .iter()
                .filter(|n| {
                    n.is_great_power()
                        && n.id != ai_id
                        && n.id != game.human_player_nation
                        && !game
                            .diplomacy
                            .get_relation(ai_id, n.id)
                            .is_some_and(|r| r.at_war)
                        && !game.diplomacy.has_treaty(
                            ai_id,
                            n.id,
                            crate::events::TreatyType::Alliance,
                        )
                })
                .map(|n| (n.id, n.province_count(), n.capital_province_id))
                .collect();

            // Target the GP with fewest provinces (weakest)
            gp_targets.sort_by_key(|&(_, p, _)| p);

            if let Some(&(target_id, target_provinces, target_capital)) = gp_targets.first() {
                // Only attack if we have more territory
                if ai_provinces > target_provinces + 2 {
                    // Find the weakest province of the target GP (fewest tiles)
                    // rather than always attacking the capital which is often
                    // the most heavily defended.
                    let attack_province = game
                        .provinces
                        .iter()
                        .filter(|p| p.owner == target_id)
                        .min_by_key(|p| p.tiles.len())
                        .map(|p| p.id)
                        .unwrap_or(target_capital);

                    let attacker_name = game
                        .get_nation(ai_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let target_name = game
                        .get_nation(target_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();

                    game.diplomacy.declare_war(ai_id, target_id);
                    game.pending_attacks.push((ai_id, attack_province));
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
        }
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
    // Smarter targeting: prefer provinces with fewer defenders, valuable resources,
    // and avoid attacking when outnumbered.
    if !enemies.is_empty() && army_size >= 4 {
        // Score each enemy province — lower score = better target
        let mut candidates: Vec<(ProvinceId, i32)> = Vec::new();
        for &enemy_id in &enemies {
            let enemy_is_gp = game
                .get_nation(enemy_id)
                .map(|n| n.is_great_power())
                .unwrap_or(false);
            // Count enemy army units in each province
            let enemy_army: Vec<(ProvinceId, usize)> = {
                let mut counts: Vec<(ProvinceId, usize)> = Vec::new();
                if let Some(en) = game.get_nation(enemy_id) {
                    for unit in &en.army {
                        if let Some(entry) = counts.iter_mut().find(|(p, _)| *p == unit.position) {
                            entry.1 += 1;
                        } else {
                            counts.push((unit.position, 1));
                        }
                    }
                }
                counts
            };

            for prov in &game.provinces {
                if prov.owner == enemy_id {
                    let tile_count = prov.tiles.len();
                    // Use actual garrison count from the province
                    let garrison_size = prov.garrison_count as usize;
                    // Estimated defender strength: garrison + stationed army
                    let stationed = enemy_army
                        .iter()
                        .find(|(p, _)| *p == prov.id)
                        .map(|(_, c)| *c)
                        .unwrap_or(0);
                    let total_defenders = garrison_size + stationed;

                    // For GP enemies, be more aggressive: attack if we have at
                    // least 2/3 of their defenders (wars stagnate otherwise
                    // because neither side ever attacks).
                    // For minor nations, keep the conservative check.
                    let dominated = if enemy_is_gp {
                        // Allow attacking GP provinces when we have a reasonable
                        // force, even if not strictly outnumbering defenders.
                        total_defenders > army_size + army_size / 2
                    } else {
                        total_defenders > army_size
                    };
                    if dominated {
                        continue;
                    }

                    // Score: fewer tiles = weaker (lower score = better)
                    // Bonus: check for valuable terrain (mountains/hills may have
                    // mineral deposits worth targeting)
                    let mut score = tile_count as i32 + stationed as i32 * 3;

                    // Penalize terrain defense (mountains are hard to attack)
                    let capital_terrain = game
                        .hex_map
                        .get_tile(prov.capital_tile)
                        .map(|t| t.terrain());
                    if let Some(terrain) = capital_terrain {
                        match terrain {
                            TerrainType::Mountain => score += 5,
                            TerrainType::FertileHills | TerrainType::BarrenHills => score += 2,
                            _ => {}
                        }
                    }

                    // Bonus for provinces with many tiles (valuable resources)
                    // but not so much that it outweighs defense difficulty
                    if tile_count >= 4 {
                        score -= 1; // Slightly prefer larger provinces (more valuable)
                    }

                    // Prefer attacking GP enemies (they are higher-value targets
                    // and wars should not stagnate)
                    if enemy_is_gp {
                        score -= 3;
                    }

                    candidates.push((prov.id, score));
                }
            }
        }

        // Sort by score ascending (best target first)
        candidates.sort_by_key(|&(_, score)| score);

        if let Some(&(target_prov, _)) = candidates.first() {
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
        // Skip nations that have been fully conquered (0 provinces)
        let minor_nations: Vec<(NationId, ProvinceId, usize)> = game
            .nations
            .iter()
            .filter(|n| !n.is_great_power())
            .filter(|n| game.provinces.iter().any(|p| p.owner == n.id))
            .map(|n| {
                let total_tiles: usize = game
                    .provinces
                    .iter()
                    .filter(|p| p.owner == n.id)
                    .map(|p| p.tiles.len())
                    .sum();
                (n.id, n.capital_province_id, total_tiles)
            })
            .filter(|(mn_id, _, tiles)| {
                *tiles > 0
                    && !game
                        .diplomacy
                        .get_relation(nation_id, *mn_id)
                        .map(|r| r.at_war)
                        .unwrap_or(false)
            })
            .collect();

        // Sort by tile count ascending to find weakest
        let mut sorted = minor_nations;
        sorted.sort_by_key(|&(_, _, tiles)| tiles);

        if let Some(&(target_id, _, _)) = sorted.first() {
            // Find a province actually owned by the target
            let attack_province = match game.provinces.iter().find(|p| p.owner == target_id) {
                Some(p) => p.id,
                None => return, // Target has no provinces left
            };
            let target_name = game
                .get_nation(target_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            game.diplomacy.declare_war(nation_id, target_id);
            game.pending_attacks.push((nation_id, attack_province));
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
                            .game_data
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

/// Manage AI resources: sell excess finished goods when treasury is low.
///
/// When the AI's treasury drops below $3,000, it sells excess finished goods
/// (Furniture, Hardware, Clothing) for cash. Each finished good is valued at
/// a fixed price: Furniture $200, Hardware $250, Clothing $200.
/// The AI keeps at least 2 of each good in reserve.
pub fn ai_manage_resources(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Only sell goods when treasury is low
    if nation.treasury >= Money::dollars(3000) {
        return;
    }

    let nation_name = nation.name.clone();

    // Define goods to sell and their prices
    let goods_prices: [(GoodsType, i64); 3] = [
        (GoodsType::Furniture, 200),
        (GoodsType::Hardware, 250),
        (GoodsType::Clothing, 200),
    ];

    let mut total_revenue = Money::ZERO;

    for (goods_type, price_per_unit) in &goods_prices {
        let amount = match game.get_nation(nation_id) {
            Some(n) => n.goods_amount(*goods_type),
            None => return,
        };
        // Keep at least 2 in reserve
        if amount <= 2 {
            continue;
        }
        let excess = amount - 2;
        let revenue = Money::dollars(*price_per_unit) * excess as i64;

        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_goods(*goods_type, excess);
        nation.treasury += revenue;
        total_revenue += revenue;
    }

    if total_revenue > Money::ZERO {
        actions.push(format!(
            "{} sold excess goods for ${}",
            nation_name,
            total_revenue.as_dollars()
        ));
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

    // When treasury is very high, expand existing mills/factories even without
    // surplus resources — invest in future capacity growth.
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    if nation.treasury > Money::dollars(15_000) {
        let expandable: Vec<BuildingType> = nation
            .buildings
            .iter()
            .filter(|b| {
                matches!(
                    b.building_type,
                    BuildingType::LumberMill | BuildingType::SteelMill | BuildingType::TextileMill
                ) && b.pending_capacity == 0
            })
            .map(|b| b.building_type)
            .collect();

        for bt in expandable {
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

    // Don't sell resources when already sitting on a large treasury —
    // keep the materials for building ships, units, and infrastructure instead.
    if nation.treasury > Money::dollars(20_000) {
        return;
    }

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

    // Build freight cars if we have fewer than needed (scale with province count)
    let target_cars = (nation.province_count() as u32).max(2);
    if nation.transport.freight_cars >= target_cars {
        return;
    }

    // Build up to 2 freight cars per turn (cost: 1 lumber + 1 steel each)
    let cars_to_build = (target_cars - nation.transport.freight_cars).min(2);
    let lumber_available = nation.material_amount(MaterialType::Lumber);
    let steel_available = nation.material_amount(MaterialType::Steel);
    let affordable = cars_to_build.min(lumber_available).min(steel_available);

    if affordable > 0 {
        nation.consume_material(MaterialType::Lumber, affordable);
        nation.consume_material(MaterialType::Steel, affordable);
        nation.transport.build_freight_cars(affordable);
    }
}

/// Proactive transport building: build freight cars when transport capacity
/// is insufficient for current resource production.
///
/// Checks total resources in the warehouse against freight car capacity.
/// If warehouse resources exceed capacity, builds additional freight cars
/// (up to 2 per turn) when materials are available.
fn ai_build_transport_proactive(game: &mut GameState, nation_id: NationId) {
    // First, use the basic logic to build initial cars if none exist
    ai_build_transport(game, nation_id);

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Calculate total resources in warehouse
    let total_resources: u32 = nation.warehouse.values().sum();
    let capacity = nation.transport.total_capacity();

    // If resources exceed capacity, we need more freight cars
    if total_resources <= capacity {
        return;
    }

    // Build additional freight cars (1 lumber + 1 steel each, up to 2 per turn)
    let cars_to_build = 2u32;
    let lumber_available = nation.material_amount(MaterialType::Lumber);
    let steel_available = nation.material_amount(MaterialType::Steel);
    let affordable = cars_to_build.min(lumber_available).min(steel_available);

    if affordable > 0 {
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_material(MaterialType::Lumber, affordable);
        nation.consume_material(MaterialType::Steel, affordable);
        nation.transport.build_freight_cars(affordable);
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
    // Auto-make peace with any nation that has 0 provinces left.
    // There is nothing left to fight over, so continuing a war is pointless.
    {
        let war_targets: Vec<NationId> = game
            .nations
            .iter()
            .filter(|n| {
                n.id != nation_id
                    && n.province_ids.is_empty()
                    && game
                        .diplomacy
                        .get_relation(nation_id, n.id)
                        .is_some_and(|r| r.at_war)
            })
            .map(|n| n.id)
            .collect();

        for target_id in war_targets {
            // Skip peace if we have pending attacks against provinces owned by this nation
            let has_pending_attack = game.pending_attacks.iter().any(|(attacker, prov_id)| {
                *attacker == nation_id
                    && game
                        .get_province(*prov_id)
                        .is_some_and(|p| p.owner == target_id)
            });
            if has_pending_attack {
                continue;
            }
            game.diplomacy.make_peace(nation_id, target_id);
            let nation_name = game
                .get_nation(nation_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let target_name = game
                .get_nation(target_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            actions.push(format!(
                "{} made peace with {} (no provinces remaining)",
                nation_name, target_name
            ));
        }
    }

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
                let pact_text = format!(
                    "{} signed a non-aggression pact with {}",
                    nation_name, mn_name
                );
                actions.push(pact_text.clone());
                let turn = game.turn;
                if !game
                    .history
                    .iter()
                    .any(|(t, text)| *t == turn && text == &pact_text)
                {
                    game.history.push((turn, pact_text));
                }
            }
        }
    }

    // Phase 2: Propose alliances with other Great Powers (Diplomatic personality only)
    // Wait until turn 10+ so diplomatic history develops, and limit to max 2 alliances
    if propose_alliance_chance && turn_number >= 10 {
        // Count existing alliances to cap at 2
        let existing_alliances: usize = game
            .nations
            .iter()
            .filter(|n| {
                n.is_great_power()
                    && n.id != nation_id
                    && game.diplomacy.has_treaty(
                        nation_id,
                        n.id,
                        crate::events::TreatyType::Alliance,
                    )
            })
            .count();

        if existing_alliances >= 2 {
            // Already have max alliances, skip
        } else {
            let gp_ids: Vec<NationId> = game
                .nations
                .iter()
                .filter(|n| {
                    n.is_great_power() && n.id != nation_id && n.id != game.human_player_nation
                })
                .map(|n| n.id)
                .collect();

            let mut alliances_formed = existing_alliances;
            for gp_id in gp_ids {
                // Re-check cap inside loop to prevent forming more than 2 total
                if alliances_formed >= 2 {
                    break;
                }

                let at_war = game
                    .diplomacy
                    .get_relation(nation_id, gp_id)
                    .is_some_and(|r| r.at_war);
                if at_war {
                    continue;
                }

                let already_allied = game.diplomacy.has_treaty(
                    nation_id,
                    gp_id,
                    crate::events::TreatyType::Alliance,
                );
                if already_allied {
                    continue;
                }

                // Skip nations with low standing (<50) — AI is less likely to accept treaties
                let partner_standing = game.diplomacy.get_standing(gp_id);
                if partner_standing < 50 {
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
                    alliances_formed += 1;
                    let nation_name = game
                        .get_nation(nation_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let gp_name = game
                        .get_nation(gp_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let alliance_text =
                        format!("{} and {} have formed an alliance!", nation_name, gp_name);
                    actions.push(alliance_text.clone());
                    let turn = game.turn;
                    if !game
                        .history
                        .iter()
                        .any(|(t, text)| *t == turn && text == &alliance_text)
                    {
                        game.history.push((turn, alliance_text));
                    }
                }
            }
        } // end else (existing_alliances < 2)
    }

    // Phase 3: Send cash grants to Minor Nations with embassies
    // Wealthy AIs send much larger grants to burn excess treasury.
    if grant_amount > 0
        && grant_every_n_turns > 0
        && turn_number.is_multiple_of(grant_every_n_turns)
    {
        let treasury_val = game
            .get_nation(nation_id)
            .map(|n| n.treasury.as_dollars())
            .unwrap_or(0);
        let wealth_multiplier = if treasury_val > 100_000 {
            20 // Very wealthy: grant 20x more
        } else if treasury_val > 50_000 {
            10
        } else if treasury_val > 20_000 {
            5
        } else {
            1
        };
        let ai_standing = game.diplomacy.get_standing(nation_id);
        let adjusted_grant = if ai_standing > 80 {
            (grant_amount + grant_amount / 2) * wealth_multiplier
        } else {
            grant_amount * wealth_multiplier
        };
        let grant = Money::dollars(adjusted_grant);
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

/// AI pre-election strategy: when approaching a decade election (within 4 turns),
/// send larger cash grants to Minor Nation partners to boost relationship scores,
/// and build more consulates/embassies to expand influence.
///
/// - **Diplomatic** personality: sends double grants and builds embassies aggressively
/// - **Balanced/Economic**: sends standard grants
/// - **Aggressive**: does nothing extra
pub fn ai_pre_election_strategy(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<String>,
) {
    // Only activate within 4 turns of a decade election
    if !game.turn.is_near_decade_election(4) {
        return;
    }

    let personality = get_personality(game, nation_id);

    // Aggressive nations don't care about elections
    if personality == AiPersonality::Aggressive {
        return;
    }

    let grant_amount = match personality {
        AiPersonality::Diplomatic => Money::dollars(1000),
        _ => Money::dollars(500),
    };

    // Send grants to all MNs with embassies to boost relationship before the vote
    let minor_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| n.id)
        .collect();

    for mn_id in &minor_ids {
        let has_embassy = game
            .diplomacy
            .get_relation(nation_id, *mn_id)
            .is_some_and(|r| r.has_embassy);
        if !has_embassy {
            continue;
        }

        let can_afford = game
            .get_nation(nation_id)
            .is_some_and(|n| n.treasury.checked_sub(grant_amount).is_some());
        if !can_afford {
            break;
        }

        game.diplomacy.send_grant(nation_id, *mn_id, grant_amount);
        if let Some(nation) = game.get_nation_mut(nation_id) {
            nation.treasury -= grant_amount;
        }
    }

    // All personalities try to build embassies with MNs that have consulates,
    // but with different treasury thresholds based on personality.
    let embassy_treasury_threshold = match personality {
        AiPersonality::Diplomatic => Money::dollars(5000),
        AiPersonality::Balanced | AiPersonality::Economic => Money::dollars(10_000),
        AiPersonality::Aggressive => Money::dollars(15_000),
    };
    let embassy_cost = Money::dollars(5000);
    let treasury_ok = game
        .get_nation(nation_id)
        .is_some_and(|n| n.treasury >= embassy_treasury_threshold);
    if treasury_ok {
        for mn_id in &minor_ids {
            let has_consulate_no_embassy = game
                .diplomacy
                .get_relation(nation_id, *mn_id)
                .is_some_and(|r| r.has_consulate && !r.has_embassy);
            if !has_consulate_no_embassy {
                continue;
            }

            let can_afford = game
                .get_nation(nation_id)
                .is_some_and(|n| n.treasury.checked_sub(embassy_cost).is_some());
            if !can_afford {
                break;
            }

            if game.diplomacy.build_embassy(nation_id, *mn_id).is_ok() {
                if let Some(nation) = game.get_nation_mut(nation_id) {
                    nation.treasury -= embassy_cost;
                }
                let nation_name = game
                    .get_nation(nation_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let mn_name = game
                    .get_nation(*mn_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                actions.push(format!(
                    "{} built an embassy in {} ahead of the election",
                    nation_name, mn_name
                ));
            }
        }
    }
}

/// Minor Nation bonus trade: when a MN has an embassy with a GP and relationship > 50,
/// it offers 1 extra of each resource type the MN has.
///
/// This function should be called during the trade phase to add bonus resources
/// for preferred trade partners.
pub fn minor_nation_bonus_trade(game: &mut GameState) {
    // Collect MN/GP pairs that qualify
    let mut bonus_pairs: Vec<(NationId, NationId)> = Vec::new();

    let minor_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| n.id)
        .collect();
    let gp_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    for &mn_id in &minor_ids {
        for &gp_id in &gp_ids {
            let qualifies = game
                .diplomacy
                .get_relation(mn_id, gp_id)
                .is_some_and(|r| r.has_embassy && r.score > 50);
            if qualifies {
                bonus_pairs.push((mn_id, gp_id));
            }
        }
    }

    // For each qualifying pair, give 1 bonus of each resource type the GP has
    // (simulating the MN offering bonus trade goods to preferred partners)
    for (_, gp_id) in bonus_pairs {
        let resources = [
            ResourceType::Timber,
            ResourceType::Iron,
            ResourceType::Coal,
            ResourceType::Grain,
            ResourceType::Cotton,
            ResourceType::Wool,
        ];
        if let Some(gp) = game.get_nation_mut(gp_id) {
            for resource in &resources {
                gp.add_resource(*resource, 1);
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

    // Wealthy nations invest in larger navies
    let max_warships: usize = if nation.treasury > Money::dollars(8_000) {
        match personality {
            AiPersonality::Aggressive => 6,
            _ => 4,
        }
    } else {
        match personality {
            AiPersonality::Aggressive => 4,
            _ => 2,
        }
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

    let treasury = nation.treasury;

    // Ship cap depends on personality; wealthy nations always aim for 5
    let max_ships: usize = if treasury > Money::dollars(5_000) {
        5
    } else {
        match personality {
            AiPersonality::Economic => 3,
            _ => 1,
        }
    };

    // For non-Economic with low treasury, only build if cargo capacity is 0
    if personality != AiPersonality::Economic
        && treasury <= Money::dollars(5_000)
        && nation.total_cargo_capacity() > 0
    {
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

/// AI naval strategy: build warships when outmatched, plan blockades, evaluate
/// beachhead viability for coastal attacks.
///
/// - If at war and enemy has more naval firepower: try to build additional warships
/// - If at war and AI has naval superiority: report blockade capability
/// - Estimate enemy strength (provinces × 4 for garrison + known army size)
/// - Prefer coastal attack targets when AI has naval superiority
pub fn ai_naval_strategy(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let our_naval_fp = nation.total_naval_firepower();
    let nation_name = nation.name.clone();

    // Find enemies we are at war with
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

    if enemies.is_empty() {
        return;
    }

    // Calculate max enemy naval firepower
    let max_enemy_naval_fp: u32 = enemies
        .iter()
        .filter_map(|&eid| game.get_nation(eid))
        .map(|n| n.total_naval_firepower())
        .max()
        .unwrap_or(0);

    // If enemy has more naval firepower: try to build more warships
    if max_enemy_naval_fp > our_naval_fp {
        // Build additional warships beyond normal cap
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };

        let fabric_have = nation.material_amount(MaterialType::Fabric);
        let lumber_have = nation.material_amount(MaterialType::Lumber);
        let arms_have = nation.material_amount(MaterialType::Arms);
        let steel_have = nation.material_amount(MaterialType::Steel);

        // Try producing arms from steel if needed
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

        if fabric_have >= 2 && lumber_have >= 5 && arms_have >= 2 {
            let uid = next_unit_id();
            let ship = Ship::new(uid, ShipType::Frigate, nation_id);
            let nation = game.get_nation_mut(nation_id).unwrap();
            nation.consume_material(MaterialType::Fabric, 2);
            nation.consume_material(MaterialType::Lumber, 5);
            nation.consume_material(MaterialType::Arms, 2);
            nation.warships.push(ship);
            actions.push(format!(
                "{} is building warships to counter enemy naval superiority",
                nation_name
            ));
        }
        return; // Focus on shipbuilding when outmatched
    }

    // If AI has naval superiority, announce blockade capability
    if our_naval_fp > 0 && our_naval_fp > max_enemy_naval_fp {
        // Blockade is applied automatically by the game engine.
        // AI reconnaissance: estimate enemy forces
        for &enemy_id in &enemies {
            let enemy_provinces = game
                .provinces
                .iter()
                .filter(|p| p.owner == enemy_id)
                .count();
            let enemy_army_size = game.get_nation(enemy_id).map(|n| n.army.len()).unwrap_or(0);
            let estimated_enemy_strength = enemy_provinces * 4 + enemy_army_size;

            // If AI has army superiority and naval superiority, prefer coastal targets
            let our_army_size = game
                .get_nation(nation_id)
                .map(|n| n.army.len())
                .unwrap_or(0);

            if our_army_size >= 4 && our_army_size > estimated_enemy_strength / 2 {
                // Look for coastal enemy provinces to prioritize in attacks
                // (The actual attack queueing happens in ai_military_strategy;
                // this just adds a headline for the report)
                let enemy_has_coastal = game
                    .provinces
                    .iter()
                    .any(|p| p.owner == enemy_id && p.is_coastal());

                if enemy_has_coastal {
                    actions.push(format!(
                        "{} is preparing amphibious operations against the enemy coast",
                        nation_name
                    ));
                }
            }
        }
    }
}

/// AI tactical combat decisions: build forts, move units to threatened provinces,
/// and propose peace after prolonged losing wars.
///
/// - **Fort building**: If treasury > $5,000 and a border province exists (adjacent
///   to an enemy-owned province), build a fort on the capital tile of that province.
///   Aggressive AI builds forts on offensive staging provinces. Diplomatic AI builds
///   forts on the capital for defense.
///
/// - **Move units to threatened provinces**: If a province borders an enemy and has
///   no stationed army units, move one unit there from the capital.
///
/// - **Retreat from losing wars**: If at war for 20+ turns and has lost provinces
///   (owns fewer than started with), propose peace. Diplomatic AI: 10 turns.
///   Aggressive AI: 30 turns.
pub fn ai_tactical_decisions(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let personality = get_personality(game, nation_id);

    // Phase 1: Build forts on border provinces
    ai_build_forts(game, nation_id, personality, actions);

    // Phase 2: Move units to threatened (undefended border) provinces
    ai_move_units_to_threatened(game, nation_id);

    // Phase 3: Propose peace after prolonged losing war
    ai_propose_peace(game, nation_id, personality, actions);
}

/// Build a fort on a border province's capital tile if the AI can afford it.
///
/// A "border province" is one that has tiles adjacent to tiles belonging to a
/// province owned by a nation the AI is at war with.
///
/// - Aggressive AI: picks the province closest to the enemy (offensive staging)
/// - Diplomatic AI: always forts the national capital
/// - Others: pick the first border province found
fn ai_build_forts(
    game: &mut GameState,
    nation_id: NationId,
    personality: AiPersonality,
    actions: &mut Vec<String>,
) {
    use crate::map::infrastructure::build_fort;

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Need treasury > $5,000 to build a fort (level 1 costs $5,000)
    if nation.treasury <= Money::dollars(5000) {
        return;
    }

    // Find enemies we are at war with
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

    if enemies.is_empty() {
        return;
    }

    let nation = game.get_nation(nation_id).unwrap();
    let capital_province_id = nation.capital_province_id;
    let nation_name = nation.name.clone();
    let owned_provinces: Vec<ProvinceId> = nation.province_ids.clone();

    // Collect enemy-owned tiles for adjacency check
    let enemy_province_ids: Vec<ProvinceId> = game
        .provinces
        .iter()
        .filter(|p| enemies.contains(&p.owner))
        .map(|p| p.id)
        .collect();

    // Find which of our provinces border enemy territory
    let mut border_provinces: Vec<ProvinceId> = Vec::new();
    for &pid in &owned_provinces {
        if let Some(prov) = game.get_province(pid) {
            let is_border = prov.tiles.iter().any(|&tile_coord| {
                tile_coord.neighbors().iter().any(|neighbor| {
                    game.hex_map
                        .get_tile(*neighbor)
                        .and_then(|t| t.province_id)
                        .is_some_and(|npid| enemy_province_ids.contains(&npid))
                })
            });
            if is_border {
                border_provinces.push(pid);
            }
        }
    }

    if border_provinces.is_empty() {
        return;
    }

    // Choose which province to fort based on personality
    let target_province = match personality {
        AiPersonality::Diplomatic => {
            // Fort the capital for defense
            if owned_provinces.contains(&capital_province_id) {
                capital_province_id
            } else {
                border_provinces[0]
            }
        }
        AiPersonality::Aggressive => {
            // Fort the border province (offensive staging)
            border_provinces[0]
        }
        _ => {
            // Default: first border province
            border_provinces[0]
        }
    };

    // Get the capital tile of that province
    let fort_coord = match game.get_province(target_province) {
        Some(p) => p.capital_tile,
        None => return,
    };

    // Check if there's already a fort at max level
    let current_level = game
        .hex_map
        .get_tile(fort_coord)
        .map(|t| t.infrastructure.fort_level)
        .unwrap_or(0);
    if current_level >= 3 {
        return;
    }

    // Build the fort
    let new_level = current_level + 1;
    let cost = match crate::map::infrastructure::fort_cost(new_level) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Can we afford it?
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    if nation.treasury.checked_sub(cost).is_none() {
        return;
    }

    if build_fort(&mut game.hex_map, fort_coord).is_ok() {
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.treasury -= cost;
        actions.push(format!("{} has fortified its borders", nation_name));
    }
}

/// Move units to threatened provinces: provinces that border enemy territory
/// but have no army units stationed there.
fn ai_move_units_to_threatened(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Find enemies
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

    if enemies.is_empty() {
        return;
    }

    let owned_provinces: Vec<ProvinceId> = nation.province_ids.clone();

    // Collect enemy province IDs
    let enemy_province_ids: Vec<ProvinceId> = game
        .provinces
        .iter()
        .filter(|p| enemies.contains(&p.owner))
        .map(|p| p.id)
        .collect();

    // Find threatened provinces: border enemy, have no units stationed
    let nation = game.get_nation(nation_id).unwrap();
    let mut threatened: Vec<ProvinceId> = Vec::new();

    for &pid in &owned_provinces {
        // Check if any unit is stationed in this province
        let has_unit = nation.army.iter().any(|u| u.position == pid);
        if has_unit {
            continue;
        }

        // Check if this province borders enemy territory
        if let Some(prov) = game.get_province(pid) {
            let borders_enemy = prov.tiles.iter().any(|&tile_coord| {
                tile_coord.neighbors().iter().any(|neighbor| {
                    game.hex_map
                        .get_tile(*neighbor)
                        .and_then(|t| t.province_id)
                        .is_some_and(|npid| enemy_province_ids.contains(&npid))
                })
            });
            if borders_enemy {
                threatened.push(pid);
            }
        }
    }

    // For each threatened province, try to move a unit from the capital
    // (or any non-threatened province with units)
    for target_pid in threatened {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };

        // Find an available unit (not already being moved this turn, stationed
        // in a non-threatened province)
        let unit_idx = nation.army.iter().position(|u| {
            u.position != target_pid && !game.pending_moves.iter().any(|(_, uid, _)| *uid == u.id)
        });

        if let Some(idx) = unit_idx {
            let unit_id = nation.army[idx].id;
            game.pending_moves.push((nation_id, unit_id, target_pid));
        }
    }
}

/// If AI has been at war for a prolonged time and is losing (lost provinces),
/// propose peace.
///
/// War duration thresholds by personality:
/// - Diplomatic: 10 turns
/// - Balanced/Economic: 20 turns
/// - Aggressive: 30 turns
///
/// Province-loss-based retreat:
/// - If AI has lost >50% of its starting provinces, accept peace immediately
/// - Diplomatic AI retreats at 30% loss
fn ai_propose_peace(
    game: &mut GameState,
    nation_id: NationId,
    personality: AiPersonality,
    actions: &mut Vec<String>,
) {
    let turn_number = game.turn.0;

    let peace_threshold = match personality {
        AiPersonality::Diplomatic => 10u32,
        AiPersonality::Aggressive => 30,
        _ => 20,
    };

    // Province-loss threshold for immediate peace (fraction of starting provinces lost)
    let loss_threshold: f64 = match personality {
        AiPersonality::Diplomatic => 0.30,
        _ => 0.50,
    };

    // Find enemies we are at war with
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

    if enemies.is_empty() {
        return;
    }

    let nation_name = game
        .get_nation(nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    let current_provinces = game
        .get_nation(nation_id)
        .map(|n| n.province_ids.len())
        .unwrap_or(0);

    // Pre-compute enemy names for efficient history scanning
    let enemies_with_names: Vec<(NationId, String)> = enemies
        .iter()
        .map(|&eid| {
            let name = game
                .get_nation(eid)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            (eid, name)
        })
        .collect();

    // Pre-compute search patterns (avoid re-creating strings in the loop)
    let loss_pattern = format!("from {}", nation_name);

    // Single pass over history to count lost provinces and find war start turns
    let mut provinces_lost_count = 0usize;
    let mut war_starts: Vec<(NationId, u32)> = Vec::new(); // (enemy_id, turn)
    for (turn_entry, desc) in &game.history {
        if desc.contains("conquered") && desc.contains(&loss_pattern) {
            provinces_lost_count += 1;
        }
        if desc.contains("declared war") && desc.contains(&nation_name) {
            for (enemy_id, enemy_name) in &enemies_with_names {
                if desc.contains(enemy_name.as_str()) {
                    war_starts.push((*enemy_id, turn_entry.0));
                }
            }
        }
    }

    let estimated_starting = current_provinces + provinces_lost_count;

    for (enemy_id, enemy_name) in &enemies_with_names {
        // Skip peace if we have pending attacks against provinces owned by this enemy
        let has_pending_attack = game.pending_attacks.iter().any(|(attacker, prov_id)| {
            *attacker == nation_id
                && game
                    .get_province(*prov_id)
                    .is_some_and(|p| p.owner == *enemy_id)
        });
        if has_pending_attack {
            continue;
        }

        // Immediate peace if province loss exceeds threshold
        if estimated_starting > 0 {
            let loss_ratio = provinces_lost_count as f64 / estimated_starting as f64;
            if loss_ratio >= loss_threshold {
                game.diplomacy.make_peace(nation_id, *enemy_id);
                actions.push(format!(
                    "{} has sued for peace with {} (heavy losses)",
                    nation_name, enemy_name
                ));
                let turn = game.turn;
                game.history.push((
                    turn,
                    format!("{} made peace with {}", nation_name, enemy_name),
                ));
                continue;
            }
        }

        // Look up war start turn from pre-computed data
        let war_start_turn = war_starts
            .iter()
            .filter(|(eid, _)| *eid == *enemy_id)
            .map(|(_, t)| *t)
            .min();

        let war_duration = match war_start_turn {
            Some(start) => turn_number.saturating_sub(start),
            None => 0,
        };

        if war_duration < peace_threshold {
            continue;
        }

        // Simple heuristic: if AI has 1 or fewer provinces, definitely losing
        // or if the enemy has more provinces than us
        let enemy_provinces = game
            .get_nation(*enemy_id)
            .map(|n| n.province_ids.len())
            .unwrap_or(0);

        let is_losing = current_provinces <= 1 || enemy_provinces > current_provinces;

        if is_losing {
            game.diplomacy.make_peace(nation_id, *enemy_id);
            actions.push(format!(
                "{} has sued for peace with {}",
                nation_name, enemy_name
            ));
            let turn = game.turn;
            game.history.push((
                turn,
                format!("{} made peace with {}", nation_name, enemy_name),
            ));
        }
    }
}

/// AI trains untrained workers and promotes trained workers to expert.
///
/// - If AI has > 3 untrained workers, train one per turn (requires 1 paper if available)
/// - If AI has > 3 trained workers, promote one to expert per turn
fn ai_train_and_promote_workers(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let untrained = nation.labor.untrained;
    let has_paper = nation.material_amount(MaterialType::Paper) > 0;

    // Train one untrained worker if we have > 3 untrained
    if untrained > 3 {
        let nation = game.get_nation_mut(nation_id).unwrap();
        // Consume paper if available (training requires paper)
        if has_paper {
            nation.consume_material(MaterialType::Paper, 1);
        }
        nation.labor.train_worker();
    }

    // Re-read state after potential training
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Promote one trained worker to expert if we have > 3 trained
    if nation.labor.trained > 3 {
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.labor.promote_worker();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::diplomacy::DiplomacyState;
    use crate::hex::HexCoord;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};

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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
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
    fn ai_bootstraps_mills_and_factories() {
        let mut game = test_game_with_ai();
        // AI has no materials at all

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // First mills and factories are free (bootstrap)
        assert!(
            ai.has_building(BuildingType::LumberMill),
            "AI should bootstrap first LumberMill for free"
        );
        assert!(
            ai.has_building(BuildingType::SteelMill),
            "AI should bootstrap first SteelMill for free"
        );
        assert!(
            ai.has_building(BuildingType::FurnitureFactory),
            "AI should bootstrap first FurnitureFactory for free"
        );
        assert!(
            ai.has_building(BuildingType::ClothingFactory),
            "AI should bootstrap first ClothingFactory for free"
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
    fn ai_builds_unit_when_army_has_3_units() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(15000);
        ai.ai_personality = Some(AiPersonality::Balanced);
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
        assert!(
            ai.army.len() >= 4,
            "AI should have built at least a 4th unit, has {}",
            ai.army.len()
        );
        assert!(
            ai.treasury < Money::dollars(15000),
            "Treasury should be reduced after building a unit"
        );
    }

    #[test]
    fn ai_builds_advanced_unit_when_army_large() {
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
        // With variety, the unit type varies by personality and seed
        let unit_type = ai.army[5].unit_type;
        assert!(
            matches!(
                unit_type,
                ArmyUnitType::LightArtillery
                    | ArmyUnitType::StandardArtillery
                    | ArmyUnitType::SiegeArtillery
                    | ArmyUnitType::Grenadiers
            ),
            "6th unit should be a tier-3 type, got {:?}",
            unit_type
        );
        assert!(
            ai.treasury < Money::dollars(12000),
            "Treasury should be reduced after building a unit"
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
    fn ai_declares_war_on_turn_25() {
        let mut game = test_game_with_ai_and_minor();
        // Set to turn 25 (divisible by 25, the Balanced war interval)
        game.turn = TurnNumber::new(25);
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
    fn ai_does_not_declare_war_on_non_multiple_of_25() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(15);

        run_ai_turns(&mut game);

        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        // Either no relation exists, or it's not at war
        let at_war = rel.map(|r| r.at_war).unwrap_or(false);
        assert!(
            !at_war,
            "AI should not declare war on non-multiple-of-25 turns"
        );
        assert!(
            game.pending_attacks.is_empty(),
            "No attacks should be pending"
        );
    }

    #[test]
    fn ai_does_not_redeclare_war_on_existing_enemy() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(25);

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
    fn ai_scales_freight_cars_with_provinces() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.transport.build_freight_cars(1); // start with 1 car
        // Give plenty of materials (some may be consumed by economy/infra building)
        ai.add_material(MaterialType::Lumber, 20);
        ai.add_material(MaterialType::Steel, 20);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // With 1 province, target = max(1*2, 5) = 5, so AI builds more
        // (up to 2 per turn, from 1 → 3)
        assert!(
            ai.transport.freight_cars > 1,
            "AI should build more freight cars to meet target (has {})",
            ai.transport.freight_cars
        );
    }

    #[test]
    fn ai_builds_depot_on_capital() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // The test AI's capital tile is at (3,3) — verify it exists
        let ai = game.get_nation(ai_id).unwrap();
        let cap_province = game.get_province(ai.capital_province_id).unwrap();
        let cap_tile = cap_province.tiles[0];

        // If the tile doesn't exist in the map, skip (test map too small)
        if game.hex_map.get_tile(cap_tile).is_none() {
            // Still verify the function doesn't panic on missing tiles
            ai_build_map_infrastructure(&mut game, ai_id);
            return;
        }

        assert!(
            !game
                .hex_map
                .get_tile(cap_tile)
                .unwrap()
                .infrastructure
                .has_depot,
            "No depot initially"
        );

        ai_build_map_infrastructure(&mut game, ai_id);

        // After one call, should have built a depot on capital
        assert!(
            game.hex_map
                .get_tile(cap_tile)
                .unwrap()
                .infrastructure
                .has_depot,
            "AI should build depot on capital tile"
        );

        // Treasury should have decreased by $2,000
        let ai = game.get_nation(ai_id).unwrap();
        assert_eq!(ai.treasury, Money::dollars(8000));
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
        assert_eq!(personality_for_nation_index(3), AiPersonality::Balanced);
        assert_eq!(personality_for_nation_index(4), AiPersonality::Diplomatic);
        assert_eq!(personality_for_nation_index(5), AiPersonality::Aggressive);
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
    fn aggressive_ai_declares_war_on_turn_25() {
        let mut game = test_game_with_ai_and_minor();
        // Set Aggressive personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Aggressive);
        // Give AI 5 army units (Aggressive threshold is 5)
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        // Turn 25: Aggressive declares (every 25 turns)
        game.turn = TurnNumber::new(25);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        assert!(
            rel.is_some() && rel.unwrap().at_war,
            "Aggressive AI should declare war on turn 25"
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
        ai.treasury = Money::dollars(3_000); // below $5K threshold: cap is 3
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
        ai.treasury = Money::dollars(3_000); // below $5K threshold: cap is 1
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
        ai.treasury = Money::dollars(5_000); // below $8K threshold: cap is 4
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
        ai.treasury = Money::dollars(5_000); // below $8K threshold: cap is 2
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

        // Advance turn to 10+ so alliance proposals are allowed
        game.turn = TurnNumber(10);

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        // Diplomatic AI should propose alliance with ThirdPower (not human player)
        assert!(
            game.diplomacy
                .has_treaty(ai_id, NationId(4), crate::events::TreatyType::Alliance),
            "Diplomatic AI should propose alliance with non-threatening GP"
        );
    }

    // ── AI resource management tests ──────────────────────────────

    #[test]
    fn ai_sells_excess_goods_when_treasury_low() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Set treasury below $3,000 threshold
        game.get_nation_mut(ai_id).unwrap().treasury = Money::dollars(1000);

        // Give AI excess goods
        game.get_nation_mut(ai_id)
            .unwrap()
            .add_goods(GoodsType::Furniture, 5); // 5 - 2 reserve = 3 to sell
        game.get_nation_mut(ai_id)
            .unwrap()
            .add_goods(GoodsType::Hardware, 4); // 4 - 2 reserve = 2 to sell
        game.get_nation_mut(ai_id)
            .unwrap()
            .add_goods(GoodsType::Clothing, 1); // below reserve, won't sell

        let mut actions = Vec::new();
        ai_manage_resources(&mut game, ai_id, &mut actions);

        let ai = game.get_nation(ai_id).unwrap();

        // Should have sold 3 Furniture @ $200 = $600
        // and 2 Hardware @ $250 = $500
        // Total revenue: $1,100
        assert_eq!(
            ai.goods_amount(GoodsType::Furniture),
            2,
            "Should keep 2 Furniture"
        );
        assert_eq!(
            ai.goods_amount(GoodsType::Hardware),
            2,
            "Should keep 2 Hardware"
        );
        assert_eq!(
            ai.goods_amount(GoodsType::Clothing),
            1,
            "Should not sell Clothing below reserve"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(2100), // 1000 + 600 + 500
            "Treasury should increase by goods revenue"
        );
        assert!(
            actions.iter().any(|a| a.contains("sold excess goods")),
            "Should report selling goods"
        );
    }

    #[test]
    fn ai_does_not_sell_goods_when_treasury_sufficient() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Treasury above threshold
        game.get_nation_mut(ai_id).unwrap().treasury = Money::dollars(5000);

        // Give AI excess goods
        game.get_nation_mut(ai_id)
            .unwrap()
            .add_goods(GoodsType::Furniture, 10);

        let mut actions = Vec::new();
        ai_manage_resources(&mut game, ai_id, &mut actions);

        let ai = game.get_nation(ai_id).unwrap();
        assert_eq!(
            ai.goods_amount(GoodsType::Furniture),
            10,
            "Should not sell goods when treasury is sufficient"
        );
        assert!(actions.is_empty(), "No action should be reported");
    }

    #[test]
    fn ai_builds_transport_proactively_when_overflow() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Give AI some resources that exceed transport capacity
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.add_resource(ResourceType::Timber, 20);
        ai.add_resource(ResourceType::Coal, 10);
        // Give materials for building freight cars
        ai.add_material(MaterialType::Lumber, 4);
        ai.add_material(MaterialType::Steel, 4);
        // No freight cars initially

        ai_build_transport_proactive(&mut game, ai_id);

        let ai = game.get_nation(ai_id).unwrap();
        // Should have built freight cars: first the basic (2), then proactive (up to 2 more)
        assert!(
            ai.transport.freight_cars >= 2,
            "AI should build freight cars proactively, got {}",
            ai.transport.freight_cars
        );
    }

    // ── Build scripts existence tests ────────────────────────

    #[test]
    fn build_scripts_exist_and_are_executable() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let scripts = [
            "scripts/build.sh",
            "scripts/test.sh",
            "scripts/check.sh",
            "scripts/pre-commit",
        ];

        // Find the workspace root by going up from the crate dir
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();

        for script in &scripts {
            let path = workspace_root.join(script);
            assert!(
                path.exists(),
                "Script {} should exist at {:?}",
                script,
                path
            );

            let metadata = fs::metadata(&path).unwrap();
            let mode = metadata.permissions().mode();
            assert!(
                mode & 0o111 != 0,
                "Script {} should be executable (mode: {:o})",
                script,
                mode
            );
        }
    }

    // ── AI fort building tests ──────────────────────────────

    /// Build a game state with two adjacent provinces for border tests.
    fn test_game_with_adjacent_provinces() -> GameState {
        let mut hex_map = HexMap::new(20, 20);

        // AI province tiles: (0,0) and (1,0)
        let ai_tile1 = HexCoord::new(0, 0);
        let ai_tile2 = HexCoord::new(1, 0);
        hex_map.set_tile(
            ai_tile1,
            crate::map::tile::Tile::with_province(TerrainType::Farm, ProvinceId(2)),
        );
        hex_map.set_tile(
            ai_tile2,
            crate::map::tile::Tile::with_province(TerrainType::Farm, ProvinceId(2)),
        );

        // Enemy province tile: (2,0) — adjacent to (1,0)
        let enemy_tile = HexCoord::new(2, 0);
        hex_map.set_tile(
            enemy_tile,
            crate::map::tile::Tile::with_province(TerrainType::Farm, ProvinceId(3)),
        );

        // Human province tile
        let human_tile = HexCoord::new(5, 5);
        hex_map.set_tile(
            human_tile,
            crate::map::tile::Tile::with_province(TerrainType::Farm, ProvinceId(1)),
        );

        let province1 = Province::new(
            ProvinceId(1),
            "Human Land".to_string(),
            NationId(1),
            human_tile,
            vec![human_tile],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            ai_tile1,
            vec![ai_tile1, ai_tile2],
            4,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Enemy Land".to_string(),
            NationId(3),
            enemy_tile,
            vec![enemy_tile],
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
        ai_nation.treasury = Money::dollars(20000);
        ai_nation.ai_personality = Some(AiPersonality::Balanced);
        // Pre-populate with 4 civilians
        for i in 0..4 {
            ai_nation.civilians.push(Civilian::new(
                UnitId(10000 + i),
                CivilianType::Farmer,
                NationId(2),
            ));
        }

        let enemy_nation = Nation::new(
            NationId(3),
            "EnemyLand".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(3),
        );

        let mut diplomacy = DiplomacyState::new();
        // Declare war between AI and enemy
        diplomacy.declare_war(NationId(2), NationId(3));

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2, province3],
            nations: vec![human_nation, ai_nation, enemy_nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        }
    }

    #[test]
    fn ai_builds_fort_on_border_province() {
        let mut game = test_game_with_adjacent_provinces();

        let mut actions = Vec::new();
        ai_build_forts(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        // Check that a fort was built on the AI province's capital tile
        let ai_capital_tile = HexCoord::new(0, 0);
        let tile = game.hex_map.get_tile(ai_capital_tile).unwrap();
        assert!(
            tile.infrastructure.has_fort,
            "AI should build a fort on border province capital tile"
        );
        assert_eq!(tile.infrastructure.fort_level, 1, "Fort should be level 1");

        // Treasury should be reduced by $5,000
        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.treasury,
            Money::dollars(15000),
            "Treasury should be reduced by $5,000 for fort"
        );

        assert!(
            actions.iter().any(|a| a.contains("fortified")),
            "Should report fort building"
        );
    }

    #[test]
    fn ai_does_not_build_fort_when_poor() {
        let mut game = test_game_with_adjacent_provinces();
        game.get_nation_mut(NationId(2)).unwrap().treasury = Money::dollars(3000);

        let mut actions = Vec::new();
        ai_build_forts(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        let ai_capital_tile = HexCoord::new(0, 0);
        let tile = game.hex_map.get_tile(ai_capital_tile).unwrap();
        assert!(
            !tile.infrastructure.has_fort,
            "AI should not build fort when too poor"
        );
    }

    #[test]
    fn ai_does_not_build_fort_when_not_at_war() {
        let mut game = test_game_with_adjacent_provinces();
        // Make peace
        game.diplomacy.make_peace(NationId(2), NationId(3));

        let mut actions = Vec::new();
        ai_build_forts(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        let ai_capital_tile = HexCoord::new(0, 0);
        let tile = game.hex_map.get_tile(ai_capital_tile).unwrap();
        assert!(
            !tile.infrastructure.has_fort,
            "AI should not build fort when not at war"
        );
    }

    // ── AI unit movement to threatened provinces ────────────

    #[test]
    fn ai_moves_unit_to_threatened_province() {
        let mut game = test_game_with_adjacent_provinces();

        // Give AI a unit stationed at a non-threatened location (capital)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.army.push(ArmyUnit::new(
            UnitId(9000),
            ArmyUnitType::Regulars,
            NationId(2),
            ProvinceId(2), // stationed in AI province
        ));

        // Add another province for the AI that is NOT a border province
        let safe_tile = HexCoord::new(0, 5);
        game.hex_map.set_tile(
            safe_tile,
            crate::map::tile::Tile::with_province(TerrainType::Farm, ProvinceId(4)),
        );
        let safe_province = Province::new(
            ProvinceId(4),
            "Safe Province".to_string(),
            NationId(2),
            safe_tile,
            vec![safe_tile],
            4,
        );
        game.provinces.push(safe_province);
        game.get_nation_mut(NationId(2))
            .unwrap()
            .add_province(ProvinceId(4));

        // Move the unit to the safe province so it's available to be moved
        game.get_nation_mut(NationId(2)).unwrap().army[0].position = ProvinceId(4);

        ai_move_units_to_threatened(&mut game, NationId(2));

        // Should have a pending move to the border province (ProvinceId(2))
        assert!(
            game.pending_moves
                .iter()
                .any(|(nation, _, dest)| *nation == NationId(2) && *dest == ProvinceId(2)),
            "AI should queue a move to the threatened border province"
        );
    }

    #[test]
    fn ai_does_not_move_units_when_not_at_war() {
        let mut game = test_game_with_adjacent_provinces();
        game.diplomacy.make_peace(NationId(2), NationId(3));

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.army.push(ArmyUnit::new(
            UnitId(9001),
            ArmyUnitType::Regulars,
            NationId(2),
            ProvinceId(2),
        ));

        ai_move_units_to_threatened(&mut game, NationId(2));

        assert!(
            game.pending_moves.is_empty(),
            "No moves should be queued when not at war"
        );
    }

    // ── AI peace proposals ──────────────────────────────────

    #[test]
    fn ai_proposes_peace_after_prolonged_losing_war() {
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(25);

        // Record war declaration in history at turn 1
        game.history.push((
            TurnNumber::new(1),
            "AINation declared war on EnemyLand".to_string(),
        ));

        // Make AI "losing": enemy has more provinces
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(4));
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(5));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        // Should have made peace
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(true);
        assert!(
            !at_war,
            "AI should propose peace after 24 turns of losing war (threshold 20 for Balanced)"
        );
        assert!(
            actions.iter().any(|a| a.contains("sued for peace")),
            "Should report peace proposal"
        );
    }

    #[test]
    fn diplomatic_ai_proposes_peace_earlier() {
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(15);

        game.history.push((
            TurnNumber::new(1),
            "AINation declared war on EnemyLand".to_string(),
        ));

        // Make AI "losing"
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(4));
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(5));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Diplomatic,
            &mut actions,
        );

        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(true);
        assert!(
            !at_war,
            "Diplomatic AI should propose peace after 14 turns (threshold 10)"
        );
    }

    #[test]
    fn aggressive_ai_fights_longer() {
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(25);

        game.history.push((
            TurnNumber::new(1),
            "AINation declared war on EnemyLand".to_string(),
        ));

        // Make AI "losing"
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(4));
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(5));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Aggressive,
            &mut actions,
        );

        // At turn 25 with war starting at turn 1: 24 turns of war < 30 threshold
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            at_war,
            "Aggressive AI should NOT propose peace at 24 turns (threshold is 30)"
        );
    }

    // ── AI worker training/promotion tests ──────────────────

    #[test]
    fn ai_trains_worker_when_many_untrained() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.labor.untrained = 5; // > 3 threshold
        ai.labor.trained = 0;
        ai.add_material(MaterialType::Paper, 2);

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.labor.untrained, 4,
            "Should have trained 1 untrained worker"
        );
        assert_eq!(ai.labor.trained, 1, "Should have 1 trained worker");
        assert_eq!(
            ai.material_amount(MaterialType::Paper),
            1,
            "Should consume 1 paper for training"
        );
    }

    #[test]
    fn ai_does_not_train_when_few_untrained() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.labor.untrained = 3; // at threshold, not above
        ai.labor.trained = 0;
        ai.add_material(MaterialType::Paper, 2);

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.labor.untrained, 3,
            "Should not train when untrained <= 3"
        );
        assert_eq!(ai.labor.trained, 0);
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
        ai.labor.untrained = 0;
        ai.labor.trained = 5; // > 3 threshold
        ai.labor.expert = 0;

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.labor.trained, 4, "Should have promoted 1 trained worker");
        assert_eq!(ai.labor.expert, 1, "Should have 1 expert worker");
    }

    #[test]
    fn ai_does_not_promote_when_few_trained() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.labor.untrained = 0;
        ai.labor.trained = 3; // at threshold
        ai.labor.expert = 0;

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.labor.trained, 3, "Should not promote when trained <= 3");
        assert_eq!(ai.labor.expert, 0);
    }

    #[test]
    fn ai_trains_without_paper_available() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.labor.untrained = 5;
        ai.labor.trained = 0;
        // No paper available

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.labor.untrained, 4,
            "Should still train even without paper"
        );
        assert_eq!(ai.labor.trained, 1);
    }

    #[test]
    fn ai_trains_and_promotes_in_same_turn() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.labor.untrained = 5;
        ai.labor.trained = 4; // will be 5 after training
        ai.labor.expert = 0;
        ai.add_material(MaterialType::Paper, 1);

        ai_train_and_promote_workers(&mut game, NationId(2));

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.labor.untrained, 4, "Trained 1 untrained");
        // trained: was 4, +1 from training = 5, -1 from promotion = 4
        assert_eq!(
            ai.labor.trained, 4,
            "Net trained stays same (trained+1, promoted-1)"
        );
        assert_eq!(ai.labor.expert, 1, "Promoted 1 to expert");
    }

    // ── Pre-election strategy ────────────────────────────────────

    #[test]
    fn ai_pre_election_grants_within_4_turns() {
        let mut game = test_game_with_ai_and_minor();
        // Set turn to 3 turns before an election at 1825 Q1 (turn 41).
        // Turn 38 = 1824 Q2 (within 4 turns of turn 41)
        game.turn = TurnNumber::from_year_quarter(1824, 2);

        // Give AI a Balanced personality (not Aggressive, so it will send grants)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.treasury = Money::dollars(10000);

        // Set up embassy between AI(2) and MN(3)
        game.diplomacy
            .build_consulate(NationId(2), NationId(3))
            .unwrap();
        game.diplomacy
            .build_embassy(NationId(2), NationId(3))
            .unwrap();

        let score_before = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .unwrap()
            .score;

        let mut actions = Vec::new();
        ai_pre_election_strategy(&mut game, NationId(2), &mut actions);

        let score_after = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .unwrap()
            .score;
        assert!(
            score_after > score_before,
            "Pre-election grants should improve relationship: before={}, after={}",
            score_before,
            score_after
        );

        // Treasury should have decreased
        let treasury_after = game.get_nation(NationId(2)).unwrap().treasury;
        assert!(
            treasury_after < Money::dollars(10000),
            "Treasury should decrease from pre-election grants"
        );
    }

    #[test]
    fn ai_pre_election_does_nothing_when_far_from_election() {
        let mut game = test_game_with_ai_and_minor();
        // Set turn to 1820 Q1 — far from any election
        game.turn = TurnNumber::from_year_quarter(1820, 1);

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.treasury = Money::dollars(10000);

        // Set up embassy
        game.diplomacy
            .build_consulate(NationId(2), NationId(3))
            .unwrap();
        game.diplomacy
            .build_embassy(NationId(2), NationId(3))
            .unwrap();

        let treasury_before = game.get_nation(NationId(2)).unwrap().treasury;
        let mut actions = Vec::new();
        ai_pre_election_strategy(&mut game, NationId(2), &mut actions);

        let treasury_after = game.get_nation(NationId(2)).unwrap().treasury;
        assert_eq!(
            treasury_before, treasury_after,
            "Pre-election strategy should do nothing when far from election"
        );
    }

    #[test]
    fn ai_aggressive_ignores_pre_election() {
        let mut game = test_game_with_ai_and_minor();
        // Set turn near election
        game.turn = TurnNumber::from_year_quarter(1824, 2);

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Aggressive);
        ai.treasury = Money::dollars(10000);

        // Set up embassy
        game.diplomacy
            .build_consulate(NationId(2), NationId(3))
            .unwrap();
        game.diplomacy
            .build_embassy(NationId(2), NationId(3))
            .unwrap();

        let treasury_before = game.get_nation(NationId(2)).unwrap().treasury;
        let mut actions = Vec::new();
        ai_pre_election_strategy(&mut game, NationId(2), &mut actions);

        let treasury_after = game.get_nation(NationId(2)).unwrap().treasury;
        assert_eq!(
            treasury_before, treasury_after,
            "Aggressive AI should ignore pre-election strategy"
        );
    }

    // ── AI naval strategy ────────────────────────────────────

    #[test]
    fn ai_naval_strategy_builds_ships_when_outmatched() {
        let mut game = test_game_with_ai_and_minor();

        // Put AI at war with minor nation
        game.diplomacy.declare_war(NationId(2), NationId(3));

        // Give the minor nation 2 warships (more than AI's 0)
        let minor = game.get_nation_mut(NationId(3)).unwrap();
        minor
            .warships
            .push(Ship::new(UnitId(50001), ShipType::Frigate, NationId(3)));
        minor
            .warships
            .push(Ship::new(UnitId(50002), ShipType::Frigate, NationId(3)));

        // Give AI materials to build a warship (2 fabric + 5 lumber + 2 arms)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Arms, 4);
        // Verify AI has no warships initially
        assert_eq!(ai.warship_count(), 0);

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.warship_count(),
            1,
            "AI should build a warship when outmatched at sea"
        );
        assert!(
            actions
                .iter()
                .any(|a| a.contains("warships") || a.contains("naval")),
            "Should report shipbuilding action"
        );
    }

    #[test]
    fn ai_naval_strategy_does_nothing_when_not_at_war() {
        let mut game = test_game_with_ai();
        // Not at war — naval strategy should do nothing
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.add_material(MaterialType::Fabric, 10);
        ai.add_material(MaterialType::Lumber, 20);
        ai.add_material(MaterialType::Arms, 10);

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        assert!(
            actions.is_empty(),
            "Naval strategy should do nothing when not at war"
        );
    }

    // ── Smart attack targeting ───────────────────────────────

    #[test]
    fn ai_targets_weaker_provinces() {
        use crate::hex::HexCoord;
        use crate::map::Province;

        let mut game = test_game_with_ai_and_minor();

        // Add a second minor province with more tiles (stronger garrison estimate)
        let province4 = Province::new(
            ProvinceId(4),
            "Big Minor Province".to_string(),
            NationId(3),
            HexCoord::new(6, 6),
            vec![
                HexCoord::new(6, 6),
                HexCoord::new(7, 6),
                HexCoord::new(6, 7),
                HexCoord::new(7, 7),
                HexCoord::new(8, 6),
            ],
            3,
        );
        game.provinces.push(province4);
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(4));

        // Put AI at war with minor
        game.diplomacy.declare_war(NationId(2), NationId(3));

        // Give AI enough army units
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..6 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(2),
            ));
        }

        let mut actions = Vec::new();
        ai_military_strategy(&mut game, NationId(2), &mut actions);

        // AI should prefer the smaller province (ProvinceId(3) with 1 tile)
        // over the larger one (ProvinceId(4) with 5 tiles)
        let attack = game.pending_attacks.iter().find(|(a, _)| *a == NationId(2));
        assert!(attack.is_some(), "AI should queue an attack");
        let (_, target) = attack.unwrap();
        assert_eq!(
            *target,
            ProvinceId(3),
            "AI should target the smaller/weaker province (1 tile vs 5 tiles)"
        );
    }

    // ── AI accepts peace when losing badly ───────────────────

    #[test]
    fn ai_accepts_peace_when_lost_over_50_percent_provinces() {
        let mut game = test_game_with_ai_and_minor();

        // Give AI multiple provinces, then simulate heavy losses in history
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        // AI has 1 province (ProvinceId(2)) but started with 4
        // Simulate losing 3 provinces in history
        game.history.push((
            TurnNumber::new(5),
            "AINation declared war on MinorLand".to_string(),
        ));
        game.history.push((
            TurnNumber::new(10),
            "HumanNation conquered Province A from AINation".to_string(),
        ));
        game.history.push((
            TurnNumber::new(12),
            "HumanNation conquered Province B from AINation".to_string(),
        ));
        game.history.push((
            TurnNumber::new(14),
            "HumanNation conquered Province C from AINation".to_string(),
        ));

        // Put AI at war with human
        game.diplomacy.declare_war(NationId(2), NationId(1));

        // AI has lost 3 of 4 provinces (75% > 50% threshold)
        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        // AI should sue for peace
        assert!(
            actions
                .iter()
                .any(|a| a.contains("sued for peace") && a.contains("heavy losses")),
            "AI should sue for peace when losing > 50%% of provinces; actions: {:?}",
            actions
        );

        // War should be over
        let rel = game.diplomacy.get_relation(NationId(2), NationId(1));
        assert!(
            rel.is_none() || !rel.unwrap().at_war,
            "Should no longer be at war after suing for peace"
        );
    }

    #[test]
    fn diplomatic_ai_accepts_peace_at_30_percent_loss() {
        let mut game = test_game_with_ai_and_minor();

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Diplomatic);
        // AI has 1 province, simulate losing 1 (so started with 2, lost 50% > 30%)
        game.history.push((
            TurnNumber::new(5),
            "AINation declared war on HumanNation".to_string(),
        ));
        game.history.push((
            TurnNumber::new(10),
            "HumanNation conquered Lost Province from AINation".to_string(),
        ));

        // Put at war
        game.diplomacy.declare_war(NationId(2), NationId(1));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Diplomatic,
            &mut actions,
        );

        // Diplomatic AI should sue for peace at 50% (> 30% threshold)
        assert!(
            actions.iter().any(|a| a.contains("sued for peace")),
            "Diplomatic AI should sue for peace at 50%% loss (threshold=30%%); actions: {:?}",
            actions
        );
    }

    #[test]
    fn ai_does_not_sue_for_peace_when_not_losing() {
        let mut game = test_game_with_ai_and_minor();

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        // AI has not lost any provinces — no conquest history against it

        // Put at war
        game.diplomacy.declare_war(NationId(2), NationId(1));
        game.turn = TurnNumber::new(50); // past any war duration threshold
        game.history.push((
            TurnNumber::new(1),
            "AINation declared war on HumanNation".to_string(),
        ));

        // Give AI more provinces than enemy so it doesn't feel like it's losing
        game.get_nation_mut(NationId(2))
            .unwrap()
            .add_province(ProvinceId(10));
        game.get_nation_mut(NationId(2))
            .unwrap()
            .add_province(ProvinceId(11));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        assert!(
            actions.is_empty(),
            "AI should not sue for peace when not losing; actions: {:?}",
            actions
        );
    }

    // ── Regression: C5 — AI never declares war on Great Powers ──

    #[test]
    fn ai_does_not_declare_war_on_great_powers() {
        let mut game = test_game_with_ai();
        // Turn 20 is a war-consideration turn for Balanced AI
        game.turn = TurnNumber::new(20);

        // Give AI enough army units to meet the threshold (4 for Balanced)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..6 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        // No minor nations exist in this game — only two Great Powers
        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // AI should NOT declare war on the human Great Power
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(1))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            !at_war,
            "AI should never declare war on a Great Power; only minor nations are valid targets"
        );
        assert!(
            game.pending_attacks.is_empty(),
            "No attacks should be pending against a Great Power"
        );
    }

    // ── Regression: H2 — Alliance spam prevention ──────────────

    #[test]
    fn no_alliances_formed_before_turn_10() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Set AI to Diplomatic personality (the only one that proposes alliances)
        game.get_nation_mut(ai_id).unwrap().ai_personality = Some(AiPersonality::Diplomatic);

        // Add a third GP that is AI-controlled
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

        // Initialize GP embassies
        let gp_ids: Vec<NationId> = game
            .nations
            .iter()
            .filter(|n| n.is_great_power())
            .map(|n| n.id)
            .collect();
        game.diplomacy.initialize_great_powers(&gp_ids);

        // Turn 1: alliances should NOT be proposed (turn < 10)
        game.turn = TurnNumber(1);

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        let alliance_count: usize = game
            .nations
            .iter()
            .filter(|n| {
                n.is_great_power()
                    && n.id != ai_id
                    && game
                        .diplomacy
                        .has_treaty(ai_id, n.id, crate::events::TreatyType::Alliance)
            })
            .count();
        assert_eq!(
            alliance_count, 0,
            "No alliances should form before turn 10; got {}",
            alliance_count
        );
    }

    #[test]
    fn alliances_capped_at_two() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Set AI to Diplomatic personality
        game.get_nation_mut(ai_id).unwrap().ai_personality = Some(AiPersonality::Diplomatic);

        // Add multiple AI-controlled Great Powers
        for i in 4..=7 {
            let mut gp = Nation::new(
                NationId(i),
                format!("Power{}", i),
                NationColor::Green,
                NationType::GreatPower,
                ProvinceId(2),
            );
            gp.ai_personality = Some(AiPersonality::Balanced);
            gp.treasury = Money::dollars(10000);
            game.nations.push(gp);
        }

        // Initialize GP embassies
        let gp_ids: Vec<NationId> = game
            .nations
            .iter()
            .filter(|n| n.is_great_power())
            .map(|n| n.id)
            .collect();
        game.diplomacy.initialize_great_powers(&gp_ids);

        // Set turn to 40 (well past the turn 10 threshold)
        game.turn = TurnNumber(40);

        // Run diplomacy multiple times to give it every chance to form alliances
        for _ in 0..5 {
            let mut actions = Vec::new();
            ai_manage_diplomacy(&mut game, ai_id, &mut actions);
        }

        let alliance_count: usize = game
            .nations
            .iter()
            .filter(|n| {
                n.is_great_power()
                    && n.id != ai_id
                    && game
                        .diplomacy
                        .has_treaty(ai_id, n.id, crate::events::TreatyType::Alliance)
            })
            .count();
        assert!(
            alliance_count <= 2,
            "Diplomatic AI should have at most 2 alliances; got {}",
            alliance_count
        );
    }
}
