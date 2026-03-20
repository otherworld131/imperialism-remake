use crate::ai::run_ai_turns;
use crate::economy::buildings::BuildingType;
use crate::economy::civilians::CivilianType;
use crate::economy::production::{
    ProductionChain, calculate_factory_production, calculate_mill_production,
};
use crate::economy::trade::{self, TradeTransaction};
use crate::events::*;
use crate::game_state::GameState;
use crate::map::SettlementLevel;
use crate::military::combat::{BattleResult, CombatForce, create_garrison, resolve_battle};
use crate::military::naval::{NavalBattleResult, calculate_blockade_effect, resolve_naval_battle};
use crate::turn::scoring::{CouncilVoteResult, calculate_score, run_council_vote};
use crate::types::*;

/// Result of processing one turn.
#[derive(Debug)]
pub struct TurnReport {
    pub turn: TurnNumber,
    pub year: u32,
    pub quarter: u32,
    pub events: Vec<DomainEvent>,
    pub resource_production: Vec<(NationId, ResourceType, u32)>,
    pub gold_income: Vec<(NationId, Money)>,
    pub maintenance_costs: Vec<(NationId, Money)>,
    pub production_output: Vec<(NationId, String, u32)>,
    pub food_consumed: Vec<(NationId, u32)>,
    pub starvation: Vec<(NationId, u32)>,
    pub newspaper_headlines: Vec<String>,
    pub techs_available: Vec<(NationId, Vec<String>)>,
    pub council_vote: Option<CouncilVoteResult>,
    pub trade_transactions: Vec<TradeTransaction>,
    pub battles: Vec<BattleResult>,
    /// Naval battles resolved this turn.
    pub naval_battles: Vec<NavalBattleResult>,
    /// Scores for all Great Powers: (nation_id, nation_name, total_score).
    pub scores: Vec<(NationId, String, u32)>,
    /// Summary of notable actions taken by AI nations this turn.
    pub ai_actions: Vec<String>,
    /// Descriptions of completed civilian work this turn.
    pub civilian_completions: Vec<(NationId, String)>,
    /// Resources lost to insufficient transport capacity: (nation, resource, amount lost).
    pub transport_overflow: Vec<(NationId, ResourceType, u32)>,
    /// Workers recruited via immigration this turn: (nation, count).
    pub immigration: Vec<(NationId, u32)>,
    /// Settlement upgrades that happened this turn: (province_id, new_level_name).
    pub settlement_upgrades: Vec<(ProvinceId, String)>,
    /// Trade balance: (nation_id, total_spent, total_earned).
    pub trade_balance: Vec<(NationId, Money, Money)>,
}

impl TurnReport {
    /// Format a compact summary line suitable for CLI display.
    ///
    /// Example: `Turn 21 (1820 Q1) | Treasury: $8,500 | Workers: 5 | Army: 3 | Provinces: 9 | Score: 1,230 (#2)`
    pub fn format_summary_line(&self, game: &GameState) -> String {
        let player = match game.get_nation(game.human_player_nation) {
            Some(n) => n,
            None => return format!("Turn {} ({})", self.turn.0, self.turn),
        };

        let treasury = player.treasury.as_dollars();
        let workers = player.labor.total_workers();
        let army = player.army.len();
        let provinces = player.province_count();

        // Find score and rank from the scores list
        let (score, rank) = self
            .scores
            .iter()
            .enumerate()
            .find(|(_, (nid, _, _))| *nid == game.human_player_nation)
            .map(|(i, (_, _, s))| (*s, i + 1))
            .unwrap_or((0, 0));

        format!(
            "Turn {} ({}) | Treasury: ${} | Workers: {} | Army: {} | Provinces: {} | Score: {} (#{})",
            self.turn.0,
            self.turn,
            format_number_with_commas(treasury),
            workers,
            army,
            provinces,
            format_number_with_commas(score as i64),
            rank
        )
    }
}

/// Format a number with comma separators (e.g. 8500 -> "8,500").
fn format_number_with_commas(n: i64) -> String {
    let negative = n < 0;
    let s = n.unsigned_abs().to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    if negative {
        result.push('-');
    }
    result.chars().rev().collect()
}

/// Process one turn of the game.
pub fn process_turn(game: &mut GameState) -> TurnReport {
    let turn = game.turn;
    let mut report = TurnReport {
        turn,
        year: turn.year(),
        quarter: turn.quarter(),
        events: Vec::new(),
        resource_production: Vec::new(),
        gold_income: Vec::new(),
        maintenance_costs: Vec::new(),
        production_output: Vec::new(),
        food_consumed: Vec::new(),
        starvation: Vec::new(),
        newspaper_headlines: Vec::new(),
        techs_available: Vec::new(),
        council_vote: None,
        trade_transactions: Vec::new(),
        battles: Vec::new(),
        naval_battles: Vec::new(),
        scores: Vec::new(),
        ai_actions: Vec::new(),
        civilian_completions: Vec::new(),
        transport_overflow: Vec::new(),
        immigration: Vec::new(),
        settlement_upgrades: Vec::new(),
        trade_balance: Vec::new(),
    };

    // 0. AI decisions for computer-controlled Great Powers
    let ai_actions = run_ai_turns(game);
    report.ai_actions = ai_actions;

    // 0b. Resolve civilian actions (tick working civilians, apply improvements)
    resolve_civilian_actions(game, &mut report);

    // 1. Resource production: gather yields from all owned tiles
    collect_resources(game, &mut report);

    // 1b. Transport resolution: cap resources delivered by freight car capacity
    resolve_transport(game, &mut report);

    // 2. Gold/Gems -> money conversion
    convert_monetary_resources(game, &mut report);

    // 3. Run production chains (mills then factories)
    run_production(game, &mut report);

    // 3b. Trade session: Minor Nations sell resources to Great Powers
    resolve_trade_session(game, &mut report);

    // 4. Tick buildings (process expansion timers)
    tick_buildings(game);

    // 4b. Food processing: convert raw food to canned food
    process_food(game, &mut report);

    // 5. Food consumption
    food_consumption(game, &mut report);

    // 5b. Immigration: auto-recruit workers if nation has surplus food and materials
    resolve_immigration(game, &mut report);

    // 6. Maintenance costs (placeholder)
    apply_maintenance(game, &mut report);

    // 7. Resolve combat (pending attacks)
    resolve_combat(game, &mut report);

    // 7b. Resolve naval combat (warship engagements between nations at war)
    resolve_naval_combat(game, &mut report);

    // 7c. Apply blockade effects (reduce trade cargo for blockaded nations)
    apply_blockade_effects(game, &mut report);

    // 8. Report available techs
    report_available_techs(game, &mut report);

    // 9. Council of Governors vote (at decade boundaries)
    check_council_vote(game, &mut report);

    // 10. Calculate and store scores for all Great Powers
    calculate_scores(game, &mut report);

    // 10b. Update settlement progression for connected provinces
    update_settlements(game, &mut report);

    // 11. Generate newspaper
    generate_newspaper(game, &mut report);

    // 12. Advance turn
    report
        .events
        .push(DomainEvent::TurnEnded(TurnEnded { turn }));
    game.advance_turn();
    report
        .events
        .push(DomainEvent::TurnStarted(TurnStarted { turn: game.turn }));

    report
}

/// Collect resource yields from all tiles owned by each nation.
///
/// For each nation, iterates through their provinces, looks up tiles in the hex map,
/// calculates yields, and adds resources to the nation's warehouse.
fn collect_resources(game: &mut GameState, report: &mut TurnReport) {
    // Phase 1: collect production data using immutable borrows
    let mut production_data: Vec<(NationId, ResourceType, u32)> = Vec::new();
    for province in &game.provinces {
        for tile_coord in &province.tiles {
            if let Some(tile) = game.hex_map.get_tile(*tile_coord)
                && let Some(yield_amount) = tile.calculate_yield()
            {
                production_data.push((
                    province.owner,
                    yield_amount.resource,
                    yield_amount.quantity,
                ));
            }
        }
    }

    // Phase 2: apply to nations using mutable borrows
    for (nation_id, resource, amount) in &production_data {
        if let Some(nation) = game.nations.iter_mut().find(|n| n.id == *nation_id) {
            nation.add_resource(*resource, *amount);
        }
    }

    // Record in report
    report.resource_production.extend(production_data);
}

/// Resolve transport: cap resources delivered *this turn* based on freight car capacity.
///
/// Works on the resources collected this turn (from `report.resource_production`), not on the
/// total warehouse. Resources already in the warehouse from prior turns are unaffected.
///
/// For each nation:
/// - If freight cars == 0: only resources from the capital province tiles are delivered.
///   Resources from non-capital provinces are "left in the field" and subtracted.
/// - If freight cars > 0: total resources delivered this turn are capped at freight car capacity.
///   Excess resources are removed from warehouse.
/// - Resources lost are tracked in `report.transport_overflow`.
fn resolve_transport(game: &mut GameState, report: &mut TurnReport) {
    // Gather per-nation resource production from this turn
    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };

        let capacity = nation.transport.total_capacity();

        // Aggregate this turn's resource production for this nation
        let mut produced_this_turn: Vec<(ResourceType, u32)> = Vec::new();
        for (nid, resource, amount) in &report.resource_production {
            if *nid == nation_id && *amount > 0 {
                if let Some(entry) = produced_this_turn.iter_mut().find(|(r, _)| *r == *resource) {
                    entry.1 += amount;
                } else {
                    produced_this_turn.push((*resource, *amount));
                }
            }
        }

        let total_produced: u32 = produced_this_turn.iter().map(|(_, q)| q).sum();
        if total_produced == 0 {
            continue;
        }

        if capacity == 0 {
            // Zero freight cars: only capital province resources are delivered.
            // Calculate how many resources came from capital province tiles.
            let capital_province_id = nation.capital_province_id;
            let capital_tile_count = game
                .provinces
                .iter()
                .find(|p| p.id == capital_province_id)
                .map(|p| p.tiles.len() as u32)
                .unwrap_or(0);

            // Keep resources up to capital_tile_count (those are adjacent to capital).
            let keep = capital_tile_count.min(total_produced);
            if total_produced > keep {
                let overflow = total_produced - keep;
                let nation = game.nations.iter_mut().find(|n| n.id == nation_id).unwrap();
                let mut remaining_to_remove = overflow;

                for (resource, produced) in &produced_this_turn {
                    if remaining_to_remove == 0 {
                        break;
                    }
                    let current_in_warehouse = nation.resource_amount(*resource);
                    let removable = (*produced)
                        .min(current_in_warehouse)
                        .min(remaining_to_remove);
                    if removable > 0 {
                        nation.remove_resource(*resource, removable);
                        report
                            .transport_overflow
                            .push((nation_id, *resource, removable));
                        remaining_to_remove -= removable;
                    }
                }
            }
        } else if total_produced > capacity {
            // Has freight cars but this turn's production exceeds capacity.
            let overflow = total_produced - capacity;

            let nation = game.nations.iter_mut().find(|n| n.id == nation_id).unwrap();
            let mut remaining_to_remove = overflow;

            for (resource, produced) in &produced_this_turn {
                if remaining_to_remove == 0 {
                    break;
                }
                let current_in_warehouse = nation.resource_amount(*resource);
                let removable = (*produced)
                    .min(current_in_warehouse)
                    .min(remaining_to_remove);
                if removable > 0 {
                    nation.remove_resource(*resource, removable);
                    report
                        .transport_overflow
                        .push((nation_id, *resource, removable));
                    remaining_to_remove -= removable;
                }
            }
        }
    }
}

/// Resolve immigration for all nations.
///
/// For each nation, if they have the required materials (1 CannedFood + 1 Clothing + 1 Furniture)
/// in their goods/materials warehouses, auto-recruit 1 untrained worker.
/// Limit: max 1 immigrant per 4 provinces (or per 3 if Capitol building is expanded beyond level 1).
/// Only recruits if the nation has a food surplus (total food > total workers).
fn resolve_immigration(game: &mut GameState, report: &mut TurnReport) {
    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Only Great Powers get immigration
        if !nation.is_great_power() {
            continue;
        }

        // Check food surplus: total raw food > total workers
        let grain = nation.resource_amount(ResourceType::Grain);
        let fruit = nation.resource_amount(ResourceType::Fruit);
        let livestock = nation.resource_amount(ResourceType::Livestock);
        let total_food = grain + fruit + livestock;
        let total_workers = nation.labor.total_workers();

        if total_food <= total_workers {
            continue; // no food surplus
        }

        // Immigration limit: 1 per 4 provinces, or 1 per 3 if Capitol expanded
        let capitol_expanded = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::Capitol)
            .map(|b| b.effective_capacity() > 1)
            .unwrap_or(false);

        let provinces_per_immigrant = if capitol_expanded { 3 } else { 4 };
        let province_count = nation.province_count() as u32;
        let max_immigrants = if province_count == 0 {
            0
        } else {
            province_count / provinces_per_immigrant
        };

        if max_immigrants == 0 {
            continue;
        }

        // Check if nation has required materials: 1 CannedFood + 1 Clothing + 1 Furniture
        let has_canned_food = nation.material_amount(MaterialType::CannedFood) >= 1;
        let has_clothing = nation.goods_amount(GoodsType::Clothing) >= 1;
        let has_furniture = nation.goods_amount(GoodsType::Furniture) >= 1;

        if !has_canned_food || !has_clothing || !has_furniture {
            continue;
        }

        // Recruit immigrants (up to max_immigrants, consuming 1 set of materials per immigrant)
        let mut recruited = 0;
        let nation = game.nations.iter_mut().find(|n| n.id == nation_id).unwrap();

        for _ in 0..max_immigrants {
            // Check materials for each immigrant
            let can_food = nation.material_amount(MaterialType::CannedFood) >= 1;
            let can_clothing = nation.goods_amount(GoodsType::Clothing) >= 1;
            let can_furniture = nation.goods_amount(GoodsType::Furniture) >= 1;

            if !can_food || !can_clothing || !can_furniture {
                break;
            }

            nation.consume_material(MaterialType::CannedFood, 1);
            nation.consume_goods(GoodsType::Clothing, 1);
            nation.consume_goods(GoodsType::Furniture, 1);
            nation.labor.recruit_immigrant();
            recruited += 1;
        }

        if recruited > 0 {
            report.immigration.push((nation_id, recruited));
        }
    }
}

/// Update settlement progression for connected provinces.
///
/// For each province connected to its nation's capital via depot/port:
/// - If the province just became connected, start the industrialization countdown (6 turns).
/// - Tick down the industrialization counter each turn.
/// - When the countdown reaches 0 and the settlement is a Hamlet, upgrade to Village.
fn update_settlements(game: &mut GameState, report: &mut TurnReport) {
    // Collect province IDs and their owner for processing
    let province_data: Vec<(ProvinceId, NationId)> =
        game.provinces.iter().map(|p| (p.id, p.owner)).collect();

    for (province_id, owner_id) in &province_data {
        let province = match game.provinces.iter().find(|p| p.id == *province_id) {
            Some(p) => p,
            None => continue,
        };

        // Skip if this is the nation's capital province (already developed)
        let is_capital = game
            .nations
            .iter()
            .find(|n| n.id == *owner_id)
            .map(|n| n.capital_province_id == *province_id)
            .unwrap_or(false);

        if is_capital {
            continue;
        }

        if province.connected_to_capital {
            match province.industrialization_turns_remaining {
                None => {
                    // Just connected or already industrialized; if still Hamlet, start countdown
                    if province.settlement_level == SettlementLevel::Hamlet {
                        let prov = game
                            .provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();
                        prov.industrialization_turns_remaining = Some(6);
                    }
                }
                Some(remaining) => {
                    if remaining <= 1 {
                        // Countdown complete: upgrade settlement
                        let prov = game
                            .provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();

                        if prov.settlement_level == SettlementLevel::Hamlet {
                            prov.settlement_level = SettlementLevel::Village;
                            prov.industrialization_turns_remaining = None;

                            let headline = format!("{} has grown into a Village!", prov.name);
                            report.newspaper_headlines.push(headline.clone());
                            report
                                .settlement_upgrades
                                .push((*province_id, "Village".to_string()));
                        } else {
                            prov.industrialization_turns_remaining = None;
                        }
                    } else {
                        // Tick down
                        let prov = game
                            .provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();
                        prov.industrialization_turns_remaining = Some(remaining - 1);
                    }
                }
            }
        }
    }
}

/// Resolve civilian work actions for all nations.
///
/// For each nation, ticks all working civilians. When work completes:
/// - Farmer/Rancher/Forester/Miner/Driller: improve the tile (increment improvement_level)
/// - Prospector: reveal the tile's hidden resource deposit (simplified: always reveals coal/iron/oil)
fn resolve_civilian_actions(game: &mut GameState, report: &mut TurnReport) {
    // Phase 1: collect completed work info using immutable borrows on nations,
    // then apply tile mutations separately.
    struct CompletedWork {
        nation_id: NationId,
        civilian_type: CivilianType,
        position: crate::hex::HexCoord,
        description: String,
    }

    let mut completed: Vec<CompletedWork> = Vec::new();

    for nation in &mut game.nations {
        for civilian in &mut nation.civilians {
            if !civilian.working {
                continue;
            }
            let just_finished = civilian.tick();
            if just_finished && let Some(pos) = civilian.position {
                let desc = format!(
                    "{} completed work at ({}, {})",
                    civilian.civilian_type, pos.q, pos.r
                );
                completed.push(CompletedWork {
                    nation_id: nation.id,
                    civilian_type: civilian.civilian_type,
                    position: pos,
                    description: desc,
                });
            }
        }
    }

    // Phase 2: apply tile improvements
    for work in &completed {
        if let Some(tile) = game.hex_map.get_tile_mut(work.position) {
            match work.civilian_type {
                CivilianType::Farmer
                | CivilianType::Rancher
                | CivilianType::Forester
                | CivilianType::Miner
                | CivilianType::Driller => {
                    tile.improve();
                }
                CivilianType::Prospector => {
                    // Reveal a resource deposit based on terrain
                    if tile.terrain().requires_prospecting() && tile.resource_deposit().is_none() {
                        let deposit = match tile.terrain() {
                            TerrainType::BarrenHills | TerrainType::Mountain => ResourceType::Coal,
                            TerrainType::Swamp | TerrainType::Desert | TerrainType::Tundra => {
                                ResourceType::Oil
                            }
                            _ => ResourceType::Coal,
                        };
                        tile.reveal_deposit(deposit);
                    }
                }
                CivilianType::Engineer => {
                    // Engineers build infrastructure, not tile improvements — handled separately
                }
            }
        }
        report
            .civilian_completions
            .push((work.nation_id, work.description.clone()));
    }
}

/// Convert monetary resources (Gold, Gems) into treasury money.
///
/// Gold: each unit = $500
/// Gems: each unit = $1,000
fn convert_monetary_resources(game: &mut GameState, report: &mut TurnReport) {
    for nation in &mut game.nations {
        let gold_amount = nation.resource_amount(ResourceType::Gold);
        let gems_amount = nation.resource_amount(ResourceType::Gems);

        let mut income = Money::ZERO;

        if gold_amount > 0 {
            let gold_value = Money::dollars(gold_amount as i64 * 500);
            income += gold_value;
            nation.remove_resource(ResourceType::Gold, gold_amount);
        }

        if gems_amount > 0 {
            let gems_value = Money::dollars(gems_amount as i64 * 1000);
            income += gems_value;
            nation.remove_resource(ResourceType::Gems, gems_amount);
        }

        if income != Money::ZERO {
            nation.treasury += income;
            report.gold_income.push((nation.id, income));
        }
    }
}

/// Run production chains: mills convert resources to materials, factories convert materials to goods.
///
/// Labor is simplified for now: assumes sufficient labor (constraints added later).
fn run_production(game: &mut GameState, report: &mut TurnReport) {
    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Gather current resource inventory as slices
        let resources: Vec<(ResourceType, u32)> =
            nation.warehouse.iter().map(|(r, q)| (*r, *q)).collect();
        let available_labor = u32::MAX; // simplified: assume sufficient labor

        // ── Mills: resources → materials ──

        // Timber chain: LumberMill
        let lumber_mill_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let timber_result = if lumber_mill_cap > 0 {
            Some(calculate_mill_production(
                ProductionChain::Timber,
                &resources,
                lumber_mill_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Metal chain: SteelMill
        let steel_mill_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::SteelMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let metal_result = if steel_mill_cap > 0 {
            Some(calculate_mill_production(
                ProductionChain::Metal,
                &resources,
                steel_mill_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Textile chain: TextileMill
        let textile_mill_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::TextileMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let textile_result = if textile_mill_cap > 0 {
            Some(calculate_mill_production(
                ProductionChain::Textile,
                &resources,
                textile_mill_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Apply mill results: consume resources, produce materials
        let nation = game.nations.iter_mut().find(|n| n.id == nation_id).unwrap();

        // Collect newly produced materials to feed into factories
        let mut new_materials: Vec<(MaterialType, u32)> = Vec::new();

        for result in [&timber_result, &metal_result, &textile_result]
            .into_iter()
            .flatten()
        {
            // Consume resources
            for (resource, amount) in &result.resources_consumed {
                if *amount > 0 {
                    nation.remove_resource(*resource, *amount);
                }
            }
            // Produce materials
            for (material, amount) in &result.materials_produced {
                if *amount > 0 {
                    *nation.materials.entry(*material).or_insert(0) += *amount;
                    new_materials.push((*material, *amount));
                    report
                        .production_output
                        .push((nation_id, format!("{:?}", material), *amount));
                }
            }
        }

        // ── Factories: materials → goods ──

        // Build the current materials inventory for factory input
        let materials_inventory: Vec<(MaterialType, u32)> =
            nation.materials.iter().map(|(m, q)| (*m, *q)).collect();

        // Furniture: LumberMill output → FurnitureFactory
        let furniture_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::FurnitureFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let furniture_result = if furniture_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Timber,
                &materials_inventory,
                furniture_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Hardware: SteelMill output → HardwareFactory
        let hardware_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::HardwareFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let hardware_result = if hardware_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Metal,
                &materials_inventory,
                hardware_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Clothing: TextileMill output → ClothingFactory
        let clothing_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::ClothingFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let clothing_result = if clothing_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Textile,
                &materials_inventory,
                clothing_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Apply factory results: consume materials, produce goods
        for result in [&furniture_result, &hardware_result, &clothing_result]
            .into_iter()
            .flatten()
        {
            // Consume materials
            for (material, amount) in &result.materials_consumed {
                if *amount > 0 {
                    let entry = nation.materials.entry(*material).or_insert(0);
                    *entry = entry.saturating_sub(*amount);
                }
            }
            // Produce goods
            for (good, amount) in &result.goods_produced {
                if *amount > 0 {
                    *nation.goods.entry(*good).or_insert(0) += *amount;
                    report
                        .production_output
                        .push((nation_id, format!("{:?}", good), *amount));
                }
            }
        }
    }
}

/// Tick all buildings for all nations, advancing expansion timers.
fn tick_buildings(game: &mut GameState) {
    for nation in &mut game.nations {
        for building in &mut nation.buildings {
            building.tick();
        }
    }
}

/// Process food: convert raw food into canned food using FoodProcessing buildings.
///
/// If a nation has a FoodProcessing building and raw food (grain/fruit/livestock),
/// convert up to (building capacity) units: 2 raw food -> 1 canned food.
/// Prioritize grain for canning.
fn process_food(game: &mut GameState, report: &mut TurnReport) {
    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };

        let food_processing_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::FoodProcessing)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        if food_processing_cap == 0 {
            continue;
        }

        let grain = nation.resource_amount(ResourceType::Grain);
        let fruit = nation.resource_amount(ResourceType::Fruit);
        let livestock = nation.resource_amount(ResourceType::Livestock);
        let total_raw_food = grain + fruit + livestock;

        if total_raw_food < 2 {
            continue;
        }

        // Maximum units we can produce: limited by capacity and raw food
        let raw_food_limited = total_raw_food / 2;
        let units_to_produce = food_processing_cap.min(raw_food_limited);

        if units_to_produce == 0 {
            continue;
        }

        // Consume raw food: prioritize grain, then fruit, then livestock
        let mut remaining_to_consume = units_to_produce * 2;

        let grain_used = grain.min(remaining_to_consume);
        remaining_to_consume -= grain_used;

        let fruit_used = fruit.min(remaining_to_consume);
        remaining_to_consume -= fruit_used;

        let livestock_used = livestock.min(remaining_to_consume);
        // remaining_to_consume should be 0 now

        let nation = game.nations.iter_mut().find(|n| n.id == nation_id).unwrap();
        if grain_used > 0 {
            nation.remove_resource(ResourceType::Grain, grain_used);
        }
        if fruit_used > 0 {
            nation.remove_resource(ResourceType::Fruit, fruit_used);
        }
        if livestock_used > 0 {
            nation.remove_resource(ResourceType::Livestock, livestock_used);
        }

        nation.add_material(MaterialType::CannedFood, units_to_produce);
        let _ = remaining_to_consume;

        report
            .production_output
            .push((nation_id, "CannedFood".to_string(), units_to_produce));
    }
}

/// Consume food for each nation based on population.
///
/// Each worker (untrained + trained + expert) needs 1 food per turn.
/// Food priority: Grain first (preferred by 50%+), then Fruit, then Livestock.
/// If not enough food: 1 worker dies per missing food unit, up to 2 max per turn.
fn food_consumption(game: &mut GameState, report: &mut TurnReport) {
    for nation in &mut game.nations {
        let population = nation.labor.total_workers();
        if population == 0 {
            continue;
        }

        let grain = nation.resource_amount(ResourceType::Grain);
        let fruit = nation.resource_amount(ResourceType::Fruit);
        let livestock = nation.resource_amount(ResourceType::Livestock);
        let total_food = grain + fruit + livestock;

        let food_needed = population;
        let food_to_consume = food_needed.min(total_food);

        // Consume food in priority order: Grain first, then Fruit, then Livestock
        // Grain is preferred by 50%+ of population
        let mut remaining = food_to_consume;

        let grain_consumed = grain.min(remaining);
        if grain_consumed > 0 {
            nation.remove_resource(ResourceType::Grain, grain_consumed);
        }
        remaining -= grain_consumed;

        let fruit_consumed = fruit.min(remaining);
        if fruit_consumed > 0 {
            nation.remove_resource(ResourceType::Fruit, fruit_consumed);
        }
        remaining -= fruit_consumed;

        let livestock_consumed = livestock.min(remaining);
        if livestock_consumed > 0 {
            nation.remove_resource(ResourceType::Livestock, livestock_consumed);
        }

        if food_to_consume > 0 {
            report.food_consumed.push((nation.id, food_to_consume));
        }

        // Starvation: workers die if not enough food
        if total_food < food_needed {
            let deficit = food_needed - total_food;
            let workers_lost = deficit.min(2); // cap at 2 per turn

            let mut actual_lost = 0;
            for _ in 0..workers_lost {
                if nation.labor.remove_worker() {
                    actual_lost += 1;
                }
            }
            if actual_lost > 0 {
                report.starvation.push((nation.id, actual_lost));
            }
        }
    }
}

/// Resolve a trade session: generate offers from Minor Nations, use smart bids
/// (respecting consulate requirements and cargo capacity), resolve trades, and apply
/// the resulting transactions.
fn resolve_trade_session(game: &mut GameState, report: &mut TurnReport) {
    // 1. Generate offers from Minor Nations
    let offers = trade::generate_minor_nation_offers(&game.nations, &game.provinces, &game.hex_map);

    if offers.is_empty() {
        return;
    }

    // 2. Generate smart bids for all Great Powers
    let mut all_bids = Vec::new();

    let gp_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    for gp_id in &gp_ids {
        if let Some(nation) = game.get_nation(*gp_id) {
            let cargo_capacity = nation.total_cargo_capacity();
            let bids = trade::generate_smart_bids(nation, &offers, &game.diplomacy, cargo_capacity);
            all_bids.extend(bids);
        }
    }

    if all_bids.is_empty() {
        return;
    }

    // 3. Resolve trades
    let transactions = trade::resolve_trades(&offers, &all_bids);

    // 4. Apply transactions
    for txn in &transactions {
        // Buyer pays money and receives resources
        if let Some(buyer) = game.get_nation_mut(txn.buyer) {
            buyer.treasury -= txn.total_cost;
            buyer.add_resource(txn.resource, txn.quantity);
        }
        // Seller gets money
        if let Some(seller) = game.get_nation_mut(txn.seller) {
            seller.treasury += txn.total_cost;
        }
    }

    // 5. Record trade balance per nation
    let mut spent: std::collections::HashMap<NationId, Money> = std::collections::HashMap::new();
    let mut earned: std::collections::HashMap<NationId, Money> = std::collections::HashMap::new();
    for txn in &transactions {
        *spent.entry(txn.buyer).or_insert(Money::ZERO) += txn.total_cost;
        *earned.entry(txn.seller).or_insert(Money::ZERO) += txn.total_cost;
    }
    let all_ids: std::collections::HashSet<NationId> =
        spent.keys().chain(earned.keys()).copied().collect();
    for nid in all_ids {
        report.trade_balance.push((
            nid,
            *spent.get(&nid).unwrap_or(&Money::ZERO),
            *earned.get(&nid).unwrap_or(&Money::ZERO),
        ));
    }

    // 6. Record in report
    report.trade_transactions = transactions;
}

/// Apply maintenance costs for army units.
fn apply_maintenance(game: &mut GameState, _report: &mut TurnReport) {
    for nation in &mut game.nations {
        let total_cost: Money = nation
            .army
            .iter()
            .map(|u| u.maintenance_cost())
            .fold(Money::ZERO, |acc, c| acc + c);
        if total_cost != Money::ZERO {
            nation.treasury -= total_cost;
        }
    }
}

/// Resolve combat for all pending attacks.
///
/// For each pending attack:
/// 1. Create attacker CombatForce from the attacking nation's army units
/// 2. Create defender CombatForce from garrison (based on province owner type)
/// 3. Call resolve_battle()
/// 4. If attacker wins: change province owner, record ProvinceConquered event, add headline
/// 5. Clear pending_attacks after processing
fn resolve_combat(game: &mut GameState, report: &mut TurnReport) {
    let attacks: Vec<(NationId, ProvinceId)> = game.pending_attacks.drain(..).collect();

    for (attacker_id, province_id) in attacks {
        // Look up province owner
        let defender_id = match game.get_province(province_id) {
            Some(p) => p.owner,
            None => continue,
        };

        // Get defender nation type for garrison creation
        let defender_type = match game.get_nation(defender_id) {
            Some(n) => n.nation_type,
            None => continue,
        };

        // Create attacker force from nation's army
        let attacker_units: Vec<_> = match game.get_nation(attacker_id) {
            Some(n) => n.army.clone(),
            None => continue,
        };

        if attacker_units.is_empty() {
            continue;
        }

        let attacker_force = CombatForce {
            nation: attacker_id,
            units: attacker_units,
        };

        // Create defender force from garrison
        let mut garrison = create_garrison(defender_type);
        for unit in &mut garrison {
            unit.owner = defender_id;
            unit.position = province_id;
        }
        let defender_force = CombatForce {
            nation: defender_id,
            units: garrison,
        };

        // Get terrain and fort level from the province's capital tile
        let (battle_terrain, battle_fort_level) = game
            .get_province(province_id)
            .and_then(|prov| {
                game.hex_map.get_tile(prov.capital_tile).map(|tile| {
                    let terrain = tile.terrain();
                    let fort_level = if tile.infrastructure.has_fort {
                        tile.infrastructure.fort_level
                    } else {
                        0
                    };
                    (Some(terrain), fort_level)
                })
            })
            .unwrap_or((None, 0));

        let result = resolve_battle(
            &attacker_force,
            &defender_force,
            province_id,
            battle_terrain,
            battle_fort_level,
        );

        // Update attacker's surviving army
        if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
            attacker_nation.army = result.attacker_survivors.clone();
        }

        if result.attacker_won {
            // Change province owner
            if let Some(province) = game.get_province_mut(province_id) {
                province.owner = attacker_id;
            }

            // Update nation province lists
            if let Some(defender_nation) = game.get_nation_mut(defender_id) {
                defender_nation
                    .province_ids
                    .retain(|pid| *pid != province_id);
            }
            if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
                attacker_nation.add_province(province_id);
            }

            // Record event
            report
                .events
                .push(DomainEvent::ProvinceConquered(ProvinceConquered {
                    province: province_id,
                    new_owner: attacker_id,
                }));

            let atk_name = game
                .get_nation(attacker_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let def_name_conquest = game
                .get_nation(defender_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let prov_name = game
                .get_province(province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            report.newspaper_headlines.push(format!(
                "BREAKING: {} conquers {} from {}!",
                atk_name, prov_name, def_name_conquest
            ));

            // Record history event
            game.history.push((
                game.turn,
                format!(
                    "{} conquered {} from {}",
                    atk_name, prov_name, def_name_conquest
                ),
            ));

            // Check if the defender has been eliminated (lost all provinces)
            let defender_eliminated = game
                .get_nation(defender_id)
                .is_some_and(|n| n.is_great_power() && n.province_ids.is_empty());
            if defender_eliminated {
                report
                    .newspaper_headlines
                    .push(format!("{} has been eliminated!", def_name_conquest));
            }
        } else {
            let def_name = game
                .get_nation(defender_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let prov_name = game
                .get_province(province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            report
                .newspaper_headlines
                .push(format!("{} repels attack on {}!", def_name, prov_name));
        }

        report.battles.push(result);
    }
}

/// Resolve naval combat between nations at war that both have warships.
///
/// For each pair of nations at war where both sides have warships, resolve
/// a naval battle. The winner keeps surviving ships, the loser loses destroyed ships.
fn resolve_naval_combat(game: &mut GameState, report: &mut TurnReport) {
    // Collect all pairs of nations at war where both have warships
    let gp_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    let mut battles_to_resolve: Vec<(NationId, NationId)> = Vec::new();

    for i in 0..gp_ids.len() {
        for j in (i + 1)..gp_ids.len() {
            let a = gp_ids[i];
            let b = gp_ids[j];

            // Check if at war
            let at_war = game.diplomacy.get_relation(a, b).is_some_and(|r| r.at_war);

            if !at_war {
                continue;
            }

            // Check if both have warships
            let a_has_warships = game.get_nation(a).is_some_and(|n| !n.warships.is_empty());
            let b_has_warships = game.get_nation(b).is_some_and(|n| !n.warships.is_empty());

            if a_has_warships && b_has_warships {
                battles_to_resolve.push((a, b));
            }
        }
    }

    for (attacker_id, defender_id) in battles_to_resolve {
        let atk_ships = match game.get_nation(attacker_id) {
            Some(n) => n.warships.clone(),
            None => continue,
        };
        let def_ships = match game.get_nation(defender_id) {
            Some(n) => n.warships.clone(),
            None => continue,
        };

        let result = resolve_naval_battle(&atk_ships, &def_ships, attacker_id, defender_id);

        let atk_name = game
            .get_nation(attacker_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let def_name = game
            .get_nation(defender_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        // Update surviving fleets
        if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
            attacker_nation.warships = result.attacker_survivors.clone();
        }
        if let Some(defender_nation) = game.get_nation_mut(defender_id) {
            defender_nation.warships = result.defender_survivors.clone();
        }

        // Add headline
        if result.attacker_won {
            report.newspaper_headlines.push(format!(
                "NAVAL VICTORY: {} defeats {} fleet! ({} ships sunk)",
                atk_name,
                def_name,
                result.defender_ships_lost.len()
            ));
        } else {
            report.newspaper_headlines.push(format!(
                "NAVAL VICTORY: {} defeats {} fleet! ({} ships sunk)",
                def_name,
                atk_name,
                result.attacker_ships_lost.len()
            ));
        }

        report.naval_battles.push(result);
    }
}

/// Apply blockade effects: reduce effective trade cargo capacity for nations
/// whose enemies have warships.
///
/// This is a simplified model: for each nation at war with an enemy that has
/// warships, the nation's effective cargo for trade is reduced. We apply this
/// by recording it for reference (the actual trade resolution already happened,
/// but the blockade effect adds a newspaper headline).
fn apply_blockade_effects(game: &GameState, report: &mut TurnReport) {
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

        // Sum up enemy warship counts
        let mut enemy_warship_count: u32 = 0;
        for &other_id in &gp_ids {
            if other_id == nation_id {
                continue;
            }
            let at_war = game
                .diplomacy
                .get_relation(nation_id, other_id)
                .is_some_and(|r| r.at_war);
            if at_war && let Some(other) = game.get_nation(other_id) {
                enemy_warship_count += other.warship_count() as u32;
            }
        }

        if enemy_warship_count > 0 {
            let effective_cargo = calculate_blockade_effect(cargo, enemy_warship_count);
            let blocked = cargo - effective_cargo;
            if blocked > 0 {
                let nation_name = &nation.name;
                report.newspaper_headlines.push(format!(
                    "BLOCKADE: {} merchant fleet loses {} cargo capacity to enemy warships",
                    nation_name, blocked
                ));
            }
        }
    }
}

/// Report which technologies are available for research by the human player.
fn report_available_techs(game: &GameState, report: &mut TurnReport) {
    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return,
    };
    let available = game
        .tech_tree
        .available_techs(&nation.researched_techs, game.turn.year());
    let tech_names: Vec<String> = available.iter().map(|t| t.name.clone()).collect();
    if !tech_names.is_empty() {
        report
            .techs_available
            .push((game.human_player_nation, tech_names));
    }
}

/// Generate newspaper headlines for the turn report.
///
/// Gathers notable events from the turn: AI actions (tech research, military
/// buildup, war declarations), trade activity, and adds period-appropriate
/// flavor headlines that rotate based on the turn number.
fn generate_newspaper(game: &GameState, report: &mut TurnReport) {
    let year = game.turn.year();
    let quarter = game.turn.quarter();

    report
        .newspaper_headlines
        .push(format!("The Imperial Times - {year} Q{quarter}"));

    // AI actions (tech research, military buildup, war declarations)
    for action in &report.ai_actions {
        report.newspaper_headlines.push(action.clone());
    }

    // Trade activity headline for the human player
    if !report.trade_transactions.is_empty()
        && let Some(human_nation) = game.get_nation(game.human_player_nation)
    {
        let human_traded = report
            .trade_transactions
            .iter()
            .any(|txn| txn.buyer == game.human_player_nation);
        if human_traded {
            report.newspaper_headlines.push(format!(
                "Trade flourishes between {} and its partners",
                human_nation.name
            ));
        }
    }

    if let Some(human_nation) = game.get_nation(game.human_player_nation) {
        report
            .newspaper_headlines
            .push(format!("The {} empire grows stronger", human_nation.name));
    }

    if game.turn.is_decade_election() {
        report
            .newspaper_headlines
            .push("Council of Governors to convene!".to_string());
    }

    // Period-appropriate flavor headlines that rotate based on turn number
    let flavor_headlines = [
        "Railroad expansion continues across the continent",
        "Industrial production reaches new heights",
        "Diplomatic tensions simmer between the Great Powers",
        "Colonial ambitions drive the Great Powers forward",
        "New trade routes open promising opportunities",
        "The age of progress marches ever onward",
        "Rumors of unrest in the frontier provinces",
        "Great exhibitions showcase industrial might",
    ];
    let flavor_index = (game.turn.0 as usize) % flavor_headlines.len();
    report
        .newspaper_headlines
        .push(flavor_headlines[flavor_index].to_string());
}

/// Calculate scores for all Great Powers and store them in the report.
fn calculate_scores(game: &GameState, report: &mut TurnReport) {
    let mut scores: Vec<(NationId, String, u32)> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| {
            let s = calculate_score(n);
            (n.id, n.name.clone(), s.total)
        })
        .collect();
    scores.sort_by(|a, b| b.2.cmp(&a.2));
    report.scores = scores;
}

fn check_council_vote(game: &GameState, report: &mut TurnReport) {
    if !game.turn.is_decade_election() {
        return;
    }

    let is_final = game.turn.is_game_end();
    let result = run_council_vote(&game.nations, &game.provinces, is_final);

    if let Some(winner_id) = result.winner {
        if let Some(winner) = game.get_nation(winner_id) {
            report.newspaper_headlines.push(format!(
                "BREAKING: {} wins the Council of Governors with {} of {} votes!",
                winner.name,
                result
                    .votes
                    .iter()
                    .find(|(id, _)| *id == winner_id)
                    .map(|(_, v)| *v)
                    .unwrap_or(0),
                result.total_governors
            ));
        }
    } else {
        report.newspaper_headlines.push(format!(
            "Council of Governors: No nation achieves the required {} vote majority.",
            result.majority_threshold
        ));
    }

    report.council_vote = Some(result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplomacy::DiplomacyState;
    use crate::economy::buildings::Building;
    use crate::hex::HexCoord;
    use crate::map::tile::Tile;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};
    use crate::tech::TechTree;

    /// Build a minimal GameState for testing the turn processor.
    fn test_game_state() -> GameState {
        let coord_farm = HexCoord::new(0, 0);
        let coord_forest = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);

        // A farm tile (produces 1 Grain at level 0)
        let farm_tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        hex_map.set_tile(coord_farm, farm_tile);

        // A scrub forest tile (produces 1 Timber always)
        let forest_tile = Tile::with_province(TerrainType::ScrubForest, ProvinceId(1));
        hex_map.set_tile(coord_forest, forest_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Homeland".to_string(),
            NationId(1),
            coord_farm,
            vec![coord_farm, coord_forest],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.treasury = Money::dollars(1000);

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1],
            nations: vec![nation1],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            history: Vec::new(),
        }
    }

    /// Build a game state with a gold mine for testing monetary conversion.
    fn test_game_state_with_gold() -> GameState {
        let coord_gold = HexCoord::new(0, 0);

        let mut hex_map = HexMap::new(10, 10);

        // A mountain tile with gold deposit at improvement level 1 (produces 1 Gold)
        let mut gold_tile = Tile::with_province(TerrainType::Mountain, ProvinceId(1));
        gold_tile.reveal_deposit(ResourceType::Gold);
        gold_tile.set_improvement_level(1);
        hex_map.set_tile(coord_gold, gold_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Gold Province".to_string(),
            NationId(1),
            coord_gold,
            vec![coord_gold],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "GoldNation".to_string(),
            NationColor::Yellow,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.treasury = Money::dollars(2000);

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1],
            nations: vec![nation1],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            history: Vec::new(),
        }
    }

    // ── Turn advancement ──────────────────────────────────────

    #[test]
    fn process_turn_advances_turn_number() {
        let mut game = test_game_state();
        assert_eq!(game.turn, TurnNumber::new(1));

        let report = process_turn(&mut game);

        assert_eq!(report.turn, TurnNumber::new(1)); // report reflects the turn that was processed
        assert_eq!(game.turn, TurnNumber::new(2)); // game has advanced
    }

    // ── Resource collection ───────────────────────────────────

    #[test]
    fn resource_collection_gathers_from_owned_tiles() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        // Should have collected Grain (from Farm) and Timber (from ScrubForest)
        let grain_produced: u32 = report
            .resource_production
            .iter()
            .filter(|(_, r, _)| *r == ResourceType::Grain)
            .map(|(_, _, q)| q)
            .sum();
        let timber_produced: u32 = report
            .resource_production
            .iter()
            .filter(|(_, r, _)| *r == ResourceType::Timber)
            .map(|(_, _, q)| q)
            .sum();

        assert_eq!(grain_produced, 1); // Farm at level 0 = 1 Grain
        assert_eq!(timber_produced, 1); // ScrubForest = 1 Timber

        // Verify the nation's warehouse was updated
        // With 0 workers, no food is consumed
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 1);
        assert_eq!(nation.resource_amount(ResourceType::Timber), 1);
    }

    // ── Gold conversion ───────────────────────────────────────

    #[test]
    fn gold_converts_to_money() {
        let mut game = test_game_state_with_gold();
        let initial_treasury = game.get_nation(NationId(1)).unwrap().treasury;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        // 1 Gold collected => $500 added to treasury
        assert_eq!(nation.treasury, initial_treasury + Money::dollars(500));

        // Gold should have been removed from warehouse
        assert_eq!(nation.resource_amount(ResourceType::Gold), 0);

        // Report should record the income
        assert!(!report.gold_income.is_empty());
        let (_, income) = report.gold_income[0];
        assert_eq!(income, Money::dollars(500));
    }

    #[test]
    fn gems_convert_to_money() {
        let mut game = test_game_state();
        // Manually add gems to the nation's warehouse
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Gems, 3);

        let initial_treasury = game.get_nation(NationId(1)).unwrap().treasury;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        // 3 Gems => $3,000
        assert_eq!(nation.treasury, initial_treasury + Money::dollars(3000));
        assert_eq!(nation.resource_amount(ResourceType::Gems), 0);
        assert!(!report.gold_income.is_empty());
    }

    // ── Newspaper generation ──────────────────────────────────

    #[test]
    fn newspaper_is_generated() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        assert!(!report.newspaper_headlines.is_empty());
        assert!(report.newspaper_headlines[0].contains("The Imperial Times"));
        assert!(report.newspaper_headlines[0].contains("1815"));
        assert!(report.newspaper_headlines[0].contains("Q1"));
    }

    #[test]
    fn newspaper_includes_human_nation() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        let has_empire_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.contains("Testlandia"));
        assert!(has_empire_headline);
    }

    #[test]
    fn newspaper_includes_election_headline() {
        let mut game = test_game_state();
        // Set to 1825 Q1 which is a decade election year
        game.turn = TurnNumber::from_year_quarter(1825, 1);

        let report = process_turn(&mut game);

        let has_election = report
            .newspaper_headlines
            .iter()
            .any(|h| h.contains("Council of Governors"));
        assert!(has_election);
    }

    // ── Multiple turns in sequence ────────────────────────────

    #[test]
    fn multiple_turns_can_be_processed() {
        let mut game = test_game_state();

        for expected_turn in 1..=5 {
            assert_eq!(game.turn, TurnNumber::new(expected_turn));
            let report = process_turn(&mut game);
            assert_eq!(report.turn, TurnNumber::new(expected_turn));
        }
        assert_eq!(game.turn, TurnNumber::new(6));

        // After 5 turns, the nation should have accumulated resources
        // With 0 workers, no food is consumed
        // Grain: 1 gathered per turn, not consumed = 5
        // Timber: 1 gathered per turn, not consumed = 5
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 5);
        assert_eq!(nation.resource_amount(ResourceType::Timber), 5);
    }

    // ── Turn events ───────────────────────────────────────────

    #[test]
    fn turn_events_include_ended_and_started() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        let has_ended = report.events.iter().any(|e| {
            matches!(e, DomainEvent::TurnEnded(TurnEnded { turn }) if *turn == TurnNumber::new(1))
        });
        let has_started = report.events.iter().any(|e| {
            matches!(e, DomainEvent::TurnStarted(TurnStarted { turn }) if *turn == TurnNumber::new(2))
        });

        assert!(has_ended);
        assert!(has_started);
    }

    // ── Edge case: no tiles produce nothing ───────────────────

    #[test]
    fn empty_map_produces_nothing() {
        let hex_map = HexMap::new(10, 10);
        let province = Province::new(
            ProvinceId(1),
            "Empty".to_string(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)], // tile exists in province but not in hex_map
            4,
        );
        let mut nation = Nation::new(
            NationId(1),
            "EmptyNation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.treasury = Money::dollars(500);

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            history: Vec::new(),
        };

        let report = process_turn(&mut game);

        assert!(report.resource_production.is_empty());
        assert!(report.gold_income.is_empty());

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.treasury, Money::dollars(500)); // unchanged
    }

    // ── Gold + Gems combined ──────────────────────────────────

    #[test]
    fn gold_and_gems_both_convert() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Gold, 2);
        nation.add_resource(ResourceType::Gems, 1);
        let initial = nation.treasury;

        let _report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // 2 Gold = $1,000, 1 Gems = $1,000 => $2,000 total
        assert_eq!(nation.treasury, initial + Money::dollars(2000));
        assert_eq!(nation.resource_amount(ResourceType::Gold), 0);
        assert_eq!(nation.resource_amount(ResourceType::Gems), 0);
    }

    // ── Production pipeline ───────────────────────────────────

    /// Helper: build a game state with a nation that has buildings and resources.
    fn test_game_state_with_production() -> GameState {
        use crate::economy::buildings::{Building, BuildingType};

        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province = Province::new(
            ProvinceId(1),
            "Industrial".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );

        let mut nation = Nation::new(
            NationId(1),
            "FactoryNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.treasury = Money::dollars(5000);

        // Add mills and factories
        nation
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        nation
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        nation
            .buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        nation
            .buildings
            .push(Building::new(BuildingType::FurnitureFactory, 1));
        nation
            .buildings
            .push(Building::new(BuildingType::HardwareFactory, 1));
        nation
            .buildings
            .push(Building::new(BuildingType::ClothingFactory, 1));

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            history: Vec::new(),
        }
    }

    #[test]
    fn lumber_mill_produces_lumber_from_timber() {
        let mut game = test_game_state_with_production();
        // Add timber to warehouse (need 2 per lumber unit)
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Timber, 6);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill capacity 2, 6 timber / 2 per unit = 3, limited by capacity = 2 lumber produced
        // Then FurnitureFactory cap 1 consumes 2 lumber → 1 furniture
        // Net lumber: 2 - 2 = 0
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0),
            0
        );
        // 6 - 4 consumed = 2 timber remaining
        assert_eq!(nation.resource_amount(ResourceType::Timber), 2);
        // Furniture produced by factory
        assert_eq!(
            nation
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            1
        );

        // Report should show lumber was produced by the mill
        let lumber_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Lumber")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(lumber_output, 2);
    }

    #[test]
    fn steel_mill_produces_steel_from_coal_and_iron() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Coal, 5);
        nation.add_resource(ResourceType::Iron, 3);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill capacity 2, min(5, 3) = 3 limited by capacity = 2 steel produced
        // Then HardwareFactory cap 1 consumes 2 steel → 1 hardware
        // Net steel: 2 - 2 = 0
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0),
            0
        );
        // 5-2=3 coal, 3-2=1 iron remaining
        assert_eq!(nation.resource_amount(ResourceType::Coal), 3);
        assert_eq!(nation.resource_amount(ResourceType::Iron), 1);
        // Hardware produced
        assert_eq!(
            nation.goods.get(&GoodsType::Hardware).copied().unwrap_or(0),
            1
        );

        let steel_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Steel")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(steel_output, 2);
    }

    #[test]
    fn textile_mill_produces_fabric_from_cotton() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Cotton, 4);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill capacity 2, 4 cotton / 2 per unit = 2 fabric produced
        // Then ClothingFactory cap 1 consumes 2 fabric → 1 clothing
        // Net fabric: 2 - 2 = 0
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Fabric)
                .copied()
                .unwrap_or(0),
            0
        );
        assert_eq!(nation.resource_amount(ResourceType::Cotton), 0);
        // Clothing produced
        assert_eq!(
            nation.goods.get(&GoodsType::Clothing).copied().unwrap_or(0),
            1
        );

        let fabric_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Fabric")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(fabric_output, 2);
    }

    #[test]
    fn furniture_factory_produces_from_lumber() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Pre-stock lumber (bypassing mill)
        *nation.materials.entry(MaterialType::Lumber).or_insert(0) = 4;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Factory capacity 1, 4 lumber / 2 per unit = 2, limited by capacity = 1
        assert_eq!(
            nation
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            1
        );
        // 4 - 2 consumed = 2 lumber remaining
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0),
            2
        );

        let furniture_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Furniture")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(furniture_output, 1);
    }

    #[test]
    fn hardware_factory_produces_from_steel() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        *nation.materials.entry(MaterialType::Steel).or_insert(0) = 4;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Factory capacity 1, 4 steel / 2 = 2, limited by capacity = 1
        assert_eq!(
            nation.goods.get(&GoodsType::Hardware).copied().unwrap_or(0),
            1
        );
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0),
            2
        );

        let hardware_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Hardware")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(hardware_output, 1);
    }

    #[test]
    fn clothing_factory_produces_from_fabric() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        *nation.materials.entry(MaterialType::Fabric).or_insert(0) = 6;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Factory capacity 1, 6 fabric / 2 = 3, limited by capacity = 1
        assert_eq!(
            nation.goods.get(&GoodsType::Clothing).copied().unwrap_or(0),
            1
        );
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Fabric)
                .copied()
                .unwrap_or(0),
            4
        );

        let clothing_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Clothing")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(clothing_output, 1);
    }

    #[test]
    fn full_timber_chain_mill_then_factory() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Add 8 timber: mill produces 2 lumber (cap 2), then factory makes 1 furniture (cap 1)
        nation.add_resource(ResourceType::Timber, 8);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill: 8 timber, cap 2 → 2 lumber produced, 4 timber consumed, 4 remain
        // Factory: 2 lumber available, cap 1 → 1 furniture, 2 lumber consumed, 0 lumber remain
        assert_eq!(nation.resource_amount(ResourceType::Timber), 4);
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0),
            0
        );
        assert_eq!(
            nation
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            1
        );

        // Report should have both lumber and furniture entries
        let has_lumber = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Lumber" && *q > 0);
        let has_furniture = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Furniture" && *q > 0);
        assert!(has_lumber);
        assert!(has_furniture);
    }

    #[test]
    fn no_production_without_buildings() {
        let mut game = test_game_state(); // no buildings
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Timber, 10);

        let report = process_turn(&mut game);

        // No production output since there are no mills/factories
        let production_for_nation: Vec<_> = report
            .production_output
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(1))
            .collect();
        assert!(production_for_nation.is_empty());
        // Timber should still be there
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Timber), 11); // 10 + 1 from forest tile
    }

    #[test]
    fn no_production_without_resources() {
        let mut game = test_game_state_with_production();
        // No resources added; nation has buildings but nothing to process

        let report = process_turn(&mut game);

        let production_for_nation: Vec<_> = report
            .production_output
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(1))
            .collect();
        assert!(production_for_nation.is_empty());
    }

    // ── Food consumption ──────────────────────────────────────

    #[test]
    fn food_consumption_eats_per_worker() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Grain, 10);
        nation.labor.untrained = 3; // 3 workers need 3 food

        let report = process_turn(&mut game);

        // Started with 10, gained 1 from farm = 11, consumed 3 (1 per worker) = 8
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 8);

        let consumed: u32 = report
            .food_consumed
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(consumed, 3);
    }

    #[test]
    fn food_consumption_uses_fruit_and_livestock() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.labor.untrained = 5; // 5 workers need 5 food
        nation.add_resource(ResourceType::Grain, 2);
        nation.add_resource(ResourceType::Fruit, 2);
        nation.add_resource(ResourceType::Livestock, 3);

        let report = process_turn(&mut game);

        // Consume grain first (2), then fruit (2), then livestock (1) = 5 total
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 0);
        assert_eq!(nation.resource_amount(ResourceType::Fruit), 0);
        assert_eq!(nation.resource_amount(ResourceType::Livestock), 2);

        let consumed: u32 = report
            .food_consumed
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(consumed, 5);
    }

    #[test]
    fn food_consumption_with_no_workers() {
        let mut game = test_game_state_with_production();
        // No workers, no food consumed

        let report = process_turn(&mut game);

        let consumed: u32 = report
            .food_consumed
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(consumed, 0);
    }

    #[test]
    fn food_consumption_starvation_kills_workers() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.labor.untrained = 5; // 5 workers need 5 food
        nation.add_resource(ResourceType::Grain, 2); // only 2 food available

        let report = process_turn(&mut game);

        // Deficit = 5 - 2 = 3, capped at 2 deaths per turn
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.labor.total_workers(), 3); // 5 - 2 = 3

        let starved: u32 = report
            .starvation
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(starved, 2);
    }

    #[test]
    fn food_consumption_starvation_capped_at_two() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.labor.untrained = 10; // 10 workers need 10 food
        // No food at all -> deficit 10, but cap at 2

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.labor.total_workers(), 8); // 10 - 2 = 8

        let starved: u32 = report
            .starvation
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(starved, 2);
    }

    #[test]
    fn food_processing_converts_raw_food_to_canned() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Add a FoodProcessing building with capacity 3
        nation
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 3));
        nation.add_resource(ResourceType::Grain, 8);

        let report = process_turn(&mut game);

        // FoodProcessing: cap 3, 8 grain / 2 = 4 limited by cap = 3 canned food
        // Consumes 6 grain, leaves 2
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 2);
        assert_eq!(nation.material_amount(MaterialType::CannedFood), 3);

        let canned_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "CannedFood")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(canned_output, 3);
    }

    // ── Building tick ─────────────────────────────────────────

    #[test]
    fn tick_buildings_advances_expansion() {
        use crate::economy::buildings::BuildingType;

        let mut game = test_game_state_with_production();
        // Start expanding the lumber mill
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation
            .get_building_mut(BuildingType::LumberMill)
            .unwrap()
            .start_expansion(3);

        // After 1 turn, expansion countdown should go from 2 to 1
        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .unwrap();
        assert_eq!(mill.turns_until_upgrade, 1);
        assert_eq!(mill.capacity, 2); // not yet applied

        // After 2nd turn, expansion should complete
        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .unwrap();
        assert_eq!(mill.turns_until_upgrade, 0);
        assert_eq!(mill.capacity, 5); // 2 + 3
    }

    // ── Production accumulates over multiple turns ────────────

    #[test]
    fn production_accumulates_over_turns() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Timber, 20);

        // Run 3 turns
        for _ in 0..3 {
            process_turn(&mut game);
        }

        let nation = game.get_nation(NationId(1)).unwrap();
        // Each turn: mill cap 2 → 2 lumber, factory cap 1 → 1 furniture (consumes 2 lumber)
        // Net lumber per turn: 2 - 2 = 0 remaining (factory consumes what mill produces)
        // Net furniture per turn: 1
        // Timber consumed: 4 per turn = 12 total, 20 - 12 = 8 remaining
        assert_eq!(nation.resource_amount(ResourceType::Timber), 8);
        assert_eq!(
            nation
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            3
        );
    }

    // ── Tech reporting ────────────────────────────────────────

    #[test]
    fn turn_report_includes_available_techs() {
        let mut game = test_game_state();
        // At turn 1, year 1815: should have 2 techs available
        let report = process_turn(&mut game);
        assert!(!report.techs_available.is_empty());
        let (nation_id, techs) = &report.techs_available[0];
        assert_eq!(*nation_id, NationId(1));
        assert!(techs.contains(&"High Pressure Steam Engine".to_string()));
        assert!(techs.contains(&"Seed Drill".to_string()));
    }

    #[test]
    fn turn_report_excludes_researched_techs() {
        let mut game = test_game_state();
        // Research both 1815 techs
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.research_tech(crate::events::TechId(1));
        nation.research_tech(crate::events::TechId(2));

        let report = process_turn(&mut game);
        // No techs should be available at 1815 after researching both
        assert!(
            report.techs_available.is_empty(),
            "No techs should be available after researching all 1815 techs"
        );
    }

    // ── Transport resolution ──────────────────────────────────

    #[test]
    fn transport_no_overflow_when_capacity_exceeds_production() {
        let mut game = test_game_state();
        // Give nation freight cars: capacity 10, production is 2 (1 Grain + 1 Timber)
        game.get_nation_mut(NationId(1))
            .unwrap()
            .transport
            .build_freight_cars(10);

        let report = process_turn(&mut game);

        assert!(
            report.transport_overflow.is_empty(),
            "No overflow when capacity >= production"
        );
        // Resources should be fully delivered
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 1);
        assert_eq!(nation.resource_amount(ResourceType::Timber), 1);
    }

    #[test]
    fn transport_overflow_when_capacity_insufficient() {
        // Build a game state with many tiles producing resources
        let mut hex_map = HexMap::new(10, 10);
        let mut tiles = Vec::new();
        for i in 0..6 {
            let coord = HexCoord::new(i, 0);
            let tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
            hex_map.set_tile(coord, tile);
            tiles.push(coord);
        }

        let province = Province::new(
            ProvinceId(1),
            "BigProvince".to_string(),
            NationId(1),
            HexCoord::new(0, 0),
            tiles,
            4,
        );

        let mut nation = Nation::new(
            NationId(1),
            "TransportTest".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.treasury = Money::dollars(5000);
        // Give only 3 freight cars, but 6 farms produce 6 Grain
        nation.transport.build_freight_cars(3);

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            history: Vec::new(),
        };

        let report = process_turn(&mut game);

        // 6 grain produced, capacity 3 -> 3 overflow
        let total_overflow: u32 = report
            .transport_overflow
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(1))
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(total_overflow, 3);

        // Should have 3 grain remaining
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 3);
    }

    #[test]
    fn transport_zero_cars_keeps_capital_province_resources() {
        let mut game = test_game_state();
        // Default: 0 freight cars, 2 tiles in capital province (Farm + ScrubForest)
        // Produces 2 resources total, capital tile count = 2, so all should be kept

        let report = process_turn(&mut game);

        assert!(
            report.transport_overflow.is_empty(),
            "With 0 freight cars but only capital province tiles, no overflow"
        );
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 1);
        assert_eq!(nation.resource_amount(ResourceType::Timber), 1);
    }

    // ── Immigration ──────────────────────────────────────────

    #[test]
    fn immigration_requires_materials_and_food_surplus() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Give sufficient materials for immigration
        nation.add_material(MaterialType::CannedFood, 2);
        nation.add_goods(GoodsType::Clothing, 2);
        nation.add_goods(GoodsType::Furniture, 2);

        // Give food surplus
        nation.add_resource(ResourceType::Grain, 20);

        // Start with 0 workers so food surplus is guaranteed
        nation.labor.untrained = 0;

        // Give enough provinces: need at least 4 provinces for 1 immigrant
        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        let report = process_turn(&mut game);

        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(
            immigration, 1,
            "Should recruit 1 immigrant with 4+ provinces"
        );

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.labor.untrained, 1, "Should have 1 untrained worker");
    }

    #[test]
    fn immigration_does_not_happen_without_food_surplus() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Materials present
        nation.add_material(MaterialType::CannedFood, 2);
        nation.add_goods(GoodsType::Clothing, 2);
        nation.add_goods(GoodsType::Furniture, 2);

        // Workers need food: 5 workers, only 4 food -> no surplus
        nation.labor.untrained = 5;
        nation.add_resource(ResourceType::Grain, 4);

        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        let report = process_turn(&mut game);

        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(immigration, 0, "No immigration without food surplus");
    }

    #[test]
    fn immigration_does_not_happen_without_materials() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Plenty of food, no materials
        nation.add_resource(ResourceType::Grain, 20);

        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        let report = process_turn(&mut game);

        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(immigration, 0, "No immigration without materials");
    }

    #[test]
    fn immigration_limited_by_province_count() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Plenty of everything
        nation.add_material(MaterialType::CannedFood, 10);
        nation.add_goods(GoodsType::Clothing, 10);
        nation.add_goods(GoodsType::Furniture, 10);
        nation.add_resource(ResourceType::Grain, 50);
        nation.labor.untrained = 0;

        // With only 1 province, max immigrants = 1/4 = 0
        // (already has 1 province from construction)

        let report = process_turn(&mut game);

        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(immigration, 0, "No immigration with fewer than 4 provinces");
    }

    // ── Building tick / expansion ──────────────────────────────

    #[test]
    fn building_expansion_completes_after_two_turns_in_pipeline() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Start expanding the SteelMill from capacity 2 by adding 3
        nation
            .get_building_mut(BuildingType::SteelMill)
            .unwrap()
            .start_expansion(3);

        // Verify initial state
        let mill = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::SteelMill)
            .unwrap();
        assert_eq!(mill.capacity, 2);
        assert_eq!(mill.turns_until_upgrade, 2);

        // Turn 1
        process_turn(&mut game);
        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::SteelMill)
            .unwrap();
        assert_eq!(mill.capacity, 2);
        assert_eq!(mill.turns_until_upgrade, 1);

        // Turn 2 - expansion completes
        process_turn(&mut game);
        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::SteelMill)
            .unwrap();
        assert_eq!(mill.capacity, 5); // 2 + 3
        assert_eq!(mill.turns_until_upgrade, 0);
        assert_eq!(mill.pending_capacity, 0);
    }

    // ── Settlement progression ──────────────────────────────────

    #[test]
    fn settlement_upgrades_after_six_turns_connected() {
        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(3, 3);

        let hex_map = HexMap::new(10, 10);

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );

        let mut province_remote = Province::new(
            ProvinceId(2),
            "Remote Land".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        // Mark as connected to capital
        province_remote.connected_to_capital = true;
        // Settlement starts as Hamlet

        let mut nation = Nation::new(
            NationId(1),
            "SettlementNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province_capital, province_remote],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            history: Vec::new(),
        };

        // Process 7 turns: 1 turn to start countdown (set to 6), then 6 turns to count down
        for _ in 0..7 {
            process_turn(&mut game);
        }

        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            crate::map::SettlementLevel::Village,
            "Province should have upgraded to Village after 6 turns connected"
        );
    }

    #[test]
    fn settlement_does_not_upgrade_if_not_connected() {
        let coord = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(3, 3);
        let hex_map = HexMap::new(10, 10);

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );

        let province_disconnected = Province::new(
            ProvinceId(2),
            "Disconnected".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        // connected_to_capital defaults to false

        let mut nation = Nation::new(
            NationId(1),
            "TestNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province_capital, province_disconnected],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            history: Vec::new(),
        };

        for _ in 0..10 {
            process_turn(&mut game);
        }

        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            crate::map::SettlementLevel::Hamlet,
            "Disconnected province should not be upgraded"
        );
    }

    // ── Summary line formatting ──────────────────────────────

    #[test]
    fn format_summary_line_contains_key_info() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.treasury = Money::dollars(8500);
        nation.labor.untrained = 5;
        // Add enough food so workers don't starve
        nation.add_resource(ResourceType::Grain, 10);

        let report = process_turn(&mut game);
        let summary = report.format_summary_line(&game);

        assert!(
            summary.contains("Turn 1"),
            "Summary should contain turn number"
        );
        assert!(
            summary.contains("Treasury: $8,500"),
            "Summary should contain formatted treasury: {}",
            summary
        );
        assert!(
            summary.contains("Workers: 5"),
            "Summary should contain worker count: {}",
            summary
        );
        assert!(
            summary.contains("Score:"),
            "Summary should contain score: {}",
            summary
        );
    }

    #[test]
    fn format_number_with_commas_works() {
        assert_eq!(super::format_number_with_commas(0), "0");
        assert_eq!(super::format_number_with_commas(999), "999");
        assert_eq!(super::format_number_with_commas(1000), "1,000");
        assert_eq!(super::format_number_with_commas(1234567), "1,234,567");
        assert_eq!(super::format_number_with_commas(-500), "-500");
        assert_eq!(super::format_number_with_commas(-1234), "-1,234");
    }
}
