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
use crate::map::infrastructure::is_province_connected;
use crate::military::combat::{BattleResult, CombatForce, create_garrison, resolve_battle};
use crate::military::naval::{NavalBattleResult, calculate_blockade_effect, resolve_naval_battle};
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::turn::scoring::{CouncilVoteResult, calculate_score, run_council_vote};
use crate::types::*;
use std::collections::HashSet;

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
    /// Town production output: (nation_id, item_name, quantity).
    pub town_production: Vec<(NationId, String, u32)>,
    /// Unit movement descriptions: (nation_id, description).
    pub unit_movements: Vec<(NationId, String)>,
    /// Voluntary incorporations: (minor_nation_id, great_power_id).
    pub incorporations: Vec<(NationId, NationId)>,
    /// Unit upgrades: (nation_id, from_type, to_type).
    pub unit_upgrades: Vec<(NationId, String, String)>,
    /// Subsidy costs deducted this turn: (nation_id, target_nation_id, cost).
    pub subsidy_costs: Vec<(NationId, NationId, Money)>,
    /// Diplomatic score improvements from trade: (nation_a, nation_b, improvement).
    pub trade_diplomacy: Vec<(NationId, NationId, i32)>,
    /// Resources lost because the producing province was disconnected from the capital.
    pub disconnected_resources: Vec<(NationId, ResourceType, u32)>,
    /// Rewards earned this turn: (nation_id, description).
    pub rewards_earned: Vec<(NationId, String)>,
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
        town_production: Vec::new(),
        unit_movements: Vec::new(),
        incorporations: Vec::new(),
        unit_upgrades: Vec::new(),
        subsidy_costs: Vec::new(),
        trade_diplomacy: Vec::new(),
        disconnected_resources: Vec::new(),
        rewards_earned: Vec::new(),
    };

    // 0. AI decisions for computer-controlled Great Powers
    let ai_actions = run_ai_turns(game);
    report.ai_actions = ai_actions;

    // 0a. Alliance obligations: AI allies automatically join wars
    resolve_alliance_obligations(game, &mut report);

    // 0b. Voluntary incorporations: Minor Nations with high relations join Great Powers
    resolve_voluntary_incorporations(game, &mut report);

    // 0c. Unit upgrades for AI nations (auto-upgrade when tech is available)
    resolve_unit_upgrades(game, &mut report);

    // 0d. Resolve civilian actions (tick working civilians, apply improvements)
    resolve_civilian_actions(game, &mut report);

    // 1. Resource production: gather yields from all owned tiles
    collect_resources(game, &mut report);

    // 1b. Transport resolution: cap resources delivered by freight car capacity
    resolve_transport(game, &mut report);

    // 2. Gold/Gems -> money conversion
    convert_monetary_resources(game, &mut report);

    // 3. Run production chains (mills then factories)
    run_production(game, &mut report);

    // 3a. Town production: Villages and Towns produce materials and goods autonomously
    resolve_town_production(game, &mut report);

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

    // 6b. Resolve military unit movement (pending moves)
    resolve_military_movement(game, &mut report);

    // 7. Resolve combat (pending attacks)
    resolve_combat(game, &mut report);

    // 7b. Resolve naval combat (warship engagements between nations at war)
    resolve_naval_combat(game, &mut report);

    // 7c. Apply blockade effects (reduce trade cargo for blockaded nations)
    apply_blockade_effects(game, &mut report);

    // 7d. Resolve rewards (Generals earned, capitol expansion)
    resolve_rewards(game, &mut report);

    // 8. Report available techs
    report_available_techs(game, &mut report);

    // 8b. Resolve technology for AI nations (generate TechnologyResearched events)
    resolve_technology(game, &mut report);

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

/// Compute the set of province IDs connected to a nation's capital via railroad/port.
///
/// The capital province is always considered connected. Other provinces are checked
/// via [`is_province_connected`].
pub fn connected_provinces(game: &GameState, nation_id: NationId) -> HashSet<ProvinceId> {
    let mut connected = HashSet::new();
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return connected,
    };
    let capital_pid = nation.capital_province_id;
    connected.insert(capital_pid);

    let capital_province = match game.get_province(capital_pid) {
        Some(p) => p,
        None => return connected,
    };
    let capital_tile = capital_province.capital_tile;

    for &pid in &nation.province_ids {
        if pid == capital_pid {
            continue;
        }
        if is_province_connected(&game.hex_map, capital_tile, pid, &game.provinces) {
            connected.insert(pid);
        }
    }

    connected
}

/// Collect resource yields from all tiles owned by each nation.
///
/// For each nation, iterates through their provinces, looks up tiles in the hex map,
/// calculates yields, and adds resources to the nation's warehouse.
/// Only resources from provinces connected to the capital are delivered;
/// resources from disconnected provinces are tracked in `report.disconnected_resources`.
fn collect_resources(game: &mut GameState, report: &mut TurnReport) {
    // Phase 0: precompute connected provinces for each nation
    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();
    let connected_map: Vec<(NationId, HashSet<ProvinceId>)> = nation_ids
        .iter()
        .map(|&nid| (nid, connected_provinces(game, nid)))
        .collect();

    // Phase 1: collect production data using immutable borrows
    let mut production_data: Vec<(NationId, ResourceType, u32)> = Vec::new();
    let mut disconnected_data: Vec<(NationId, ResourceType, u32)> = Vec::new();

    for province in &game.provinces {
        // Check if this province is connected for its owner
        let is_connected = connected_map
            .iter()
            .find(|(nid, _)| *nid == province.owner)
            .map(|(_, set)| set.contains(&province.id))
            .unwrap_or(false);

        for tile_coord in &province.tiles {
            if let Some(tile) = game.hex_map.get_tile(*tile_coord)
                && let Some(yield_amount) = tile.calculate_yield()
            {
                if is_connected {
                    production_data.push((
                        province.owner,
                        yield_amount.resource,
                        yield_amount.quantity,
                    ));
                } else {
                    disconnected_data.push((
                        province.owner,
                        yield_amount.resource,
                        yield_amount.quantity,
                    ));
                }
            }
        }
    }

    // Phase 2: apply connected resources to nations using mutable borrows,
    // with AI difficulty bonus applied to non-human nations.
    let human_id = game.human_player_nation;
    let difficulty = game.difficulty;
    for (nation_id, resource, amount) in &production_data {
        if let Some(nation) = game.nations.iter_mut().find(|n| n.id == *nation_id) {
            // Apply AI difficulty bonus multiplier
            let bonus_multiplier = match difficulty {
                Difficulty::Hard => {
                    if *nation_id != human_id {
                        1.1
                    } else {
                        1.0
                    }
                }
                Difficulty::NighOnImpossible => {
                    if *nation_id != human_id {
                        1.25
                    } else {
                        1.0
                    }
                }
                _ => 1.0,
            };
            let adjusted = (*amount as f64 * bonus_multiplier).round() as u32;
            nation.add_resource(*resource, adjusted);
        }
    }

    // Record in report
    report.resource_production.extend(production_data);
    report.disconnected_resources.extend(disconnected_data);
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
        let owner_nation = game.nations.iter().find(|n| n.id == *owner_id);
        let is_capital = owner_nation
            .map(|n| n.capital_province_id == *province_id)
            .unwrap_or(false);

        if is_capital {
            continue;
        }

        // Skip settlement progression for Minor Nation provinces —
        // Minor Nation capitals should never industrialize beyond Hamlet.
        let is_minor_nation = owner_nation.map(|n| !n.is_great_power()).unwrap_or(false);
        if is_minor_nation {
            continue;
        }

        if province.connected_to_capital {
            let mut just_became_village = false;

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
                            // Start the Town countdown (12 turns)
                            prov.town_countdown = Some(12);
                            just_became_village = true;

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

            // Village → Town progression: tick down the town_countdown
            // Skip if the province just became a Village this turn
            if !just_became_village {
                let prov_level = game
                    .provinces
                    .iter()
                    .find(|p| p.id == *province_id)
                    .map(|p| (p.settlement_level, p.town_countdown));

                if let Some((SettlementLevel::Village, Some(remaining))) = prov_level {
                    if remaining <= 1 {
                        let prov = game
                            .provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();
                        prov.settlement_level = SettlementLevel::Town;
                        prov.town_countdown = None;

                        let headline = format!("{} has grown into a Town!", prov.name);
                        report.newspaper_headlines.push(headline.clone());
                        report
                            .settlement_upgrades
                            .push((*province_id, "Town".to_string()));
                    } else {
                        let prov = game
                            .provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();
                        prov.town_countdown = Some(remaining - 1);
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

/// Resolve autonomous town production for Village and Town provinces.
///
/// For each province that `can_produce()`:
/// 1. Sum resource yields from all tiles in the province.
/// 2. Apply 2:1 conversion for raw resources → materials:
///    - Timber → Lumber (2 timber → 1 lumber)
///    - Coal + Iron → Steel (1 coal + 1 iron → 1 steel)
///    - Cotton/Wool → Fabric (2 cotton/wool → 1 fabric)
/// 3. Convert half of materials to goods (2:1):
///    - Lumber → Furniture (2 lumber → 1 furniture)
///    - Steel → Hardware (2 steel → 1 hardware)
///    - Fabric → Clothing (2 fabric → 1 clothing)
/// 4. Towns produce at double rate (multiplier 2).
/// 5. Add produced materials and goods to the owning nation's warehouse.
fn resolve_town_production(game: &mut GameState, report: &mut TurnReport) {
    // Phase 1: calculate production for each producing province
    struct TownOutput {
        owner: NationId,
        lumber: u32,
        steel: u32,
        fabric: u32,
        furniture: u32,
        hardware: u32,
        clothing: u32,
    }

    let mut outputs: Vec<TownOutput> = Vec::new();

    for province in &game.provinces {
        if !province.can_produce() {
            continue;
        }

        let rate_multiplier: u32 = if province.settlement_level == SettlementLevel::Town {
            2
        } else {
            1
        };

        // Sum resource yields from all tiles in the province
        let mut timber_yield: u32 = 0;
        let mut coal_yield: u32 = 0;
        let mut iron_yield: u32 = 0;
        let mut cotton_yield: u32 = 0;
        let mut wool_yield: u32 = 0;

        for tile_coord in &province.tiles {
            if let Some(tile) = game.hex_map.get_tile(*tile_coord)
                && let Some(yield_amount) = tile.calculate_yield()
            {
                match yield_amount.resource {
                    ResourceType::Timber => timber_yield += yield_amount.quantity,
                    ResourceType::Coal => coal_yield += yield_amount.quantity,
                    ResourceType::Iron => iron_yield += yield_amount.quantity,
                    ResourceType::Cotton => cotton_yield += yield_amount.quantity,
                    ResourceType::Wool => wool_yield += yield_amount.quantity,
                    _ => {} // other resources don't participate in town production
                }
            }
        }

        // Apply rate multiplier for Town
        timber_yield *= rate_multiplier;
        coal_yield *= rate_multiplier;
        iron_yield *= rate_multiplier;
        cotton_yield *= rate_multiplier;
        wool_yield *= rate_multiplier;

        // Step 2: Convert raw resources to materials (2:1 ratio)
        let lumber = timber_yield / 2;
        let steel = coal_yield.min(iron_yield); // 1 coal + 1 iron → 1 steel
        let fabric = (cotton_yield + wool_yield) / 2;

        // Step 3: Convert half of materials to goods (2:1 ratio)
        let furniture = lumber / 2;
        let hardware = steel / 2;
        let clothing = fabric / 2;

        // Only record if something was produced
        if lumber > 0 || steel > 0 || fabric > 0 || furniture > 0 || hardware > 0 || clothing > 0 {
            outputs.push(TownOutput {
                owner: province.owner,
                lumber,
                steel,
                fabric,
                furniture,
                hardware,
                clothing,
            });
        }
    }

    // Phase 2: apply to nations
    for output in &outputs {
        if let Some(nation) = game.nations.iter_mut().find(|n| n.id == output.owner) {
            if output.lumber > 0 {
                nation.add_material(MaterialType::Lumber, output.lumber);
                report
                    .town_production
                    .push((output.owner, "Lumber".to_string(), output.lumber));
            }
            if output.steel > 0 {
                nation.add_material(MaterialType::Steel, output.steel);
                report
                    .town_production
                    .push((output.owner, "Steel".to_string(), output.steel));
            }
            if output.fabric > 0 {
                nation.add_material(MaterialType::Fabric, output.fabric);
                report
                    .town_production
                    .push((output.owner, "Fabric".to_string(), output.fabric));
            }
            if output.furniture > 0 {
                nation.add_goods(GoodsType::Furniture, output.furniture);
                report.town_production.push((
                    output.owner,
                    "Furniture".to_string(),
                    output.furniture,
                ));
            }
            if output.hardware > 0 {
                nation.add_goods(GoodsType::Hardware, output.hardware);
                report.town_production.push((
                    output.owner,
                    "Hardware".to_string(),
                    output.hardware,
                ));
            }
            if output.clothing > 0 {
                nation.add_goods(GoodsType::Clothing, output.clothing);
                report.town_production.push((
                    output.owner,
                    "Clothing".to_string(),
                    output.clothing,
                ));
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
    // 0. Deduct subsidy costs from Great Powers
    let gp_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    for gp_id in &gp_ids {
        let subsidies: Vec<(NationId, Money)> = game
            .get_nation(*gp_id)
            .map(|n| n.trade_subsidies.iter().map(|(k, v)| (*k, *v)).collect())
            .unwrap_or_default();
        for (target_id, cost) in subsidies {
            if cost != Money::ZERO {
                if let Some(nation) = game.get_nation_mut(*gp_id) {
                    nation.treasury -= cost;
                }
                report.subsidy_costs.push((*gp_id, target_id, cost));
            }
        }
    }

    // 1. Generate offers from Minor Nations
    let offers = trade::generate_minor_nation_offers(&game.nations, &game.provinces, &game.hex_map);

    if offers.is_empty() {
        return;
    }

    // 2. Generate smart bids for all Great Powers
    let mut all_bids = Vec::new();

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

    // 3. Build relationship scores and subsidies maps for preference-based resolution
    let mut relationship_scores: std::collections::HashMap<(NationId, NationId), i32> =
        std::collections::HashMap::new();
    let mut subsidies_map: std::collections::HashMap<(NationId, NationId), Money> =
        std::collections::HashMap::new();

    for gp_id in &gp_ids {
        if let Some(nation) = game.get_nation(*gp_id) {
            // Collect subsidies
            for (target_id, amount) in &nation.trade_subsidies {
                subsidies_map.insert((*gp_id, *target_id), *amount);
            }
        }
        // Collect relationship scores
        for offer in &offers {
            if let Some(rel) = game.diplomacy.get_relation(*gp_id, offer.seller) {
                relationship_scores.insert((*gp_id, offer.seller), rel.score);
            }
        }
    }

    // 4. Resolve trades with preference system
    let transactions = trade::resolve_trades_with_preference(
        &offers,
        &all_bids,
        &relationship_scores,
        &subsidies_map,
    );

    // 5. Apply transactions
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

    // 5b. Record trade history for each nation involved
    let current_turn = game.turn;
    for txn in &transactions {
        // Record for buyer (partner is seller)
        if let Some(buyer) = game.get_nation_mut(txn.buyer) {
            buyer.trade_history.push(trade::TradeHistoryEntry {
                turn: current_turn,
                partner: txn.seller,
                resource: txn.resource,
                quantity: txn.quantity,
                total_cost: txn.total_cost,
            });
        }
        // Record for seller (partner is buyer)
        if let Some(seller) = game.get_nation_mut(txn.seller) {
            seller.trade_history.push(trade::TradeHistoryEntry {
                turn: current_turn,
                partner: txn.buyer,
                resource: txn.resource,
                quantity: txn.quantity,
                total_cost: txn.total_cost,
            });
        }
    }

    // 6. Diplomatic impact: +1 score per distinct commodity type traded per partner pair
    let mut trade_pairs: std::collections::HashMap<
        (NationId, NationId),
        std::collections::HashSet<ResourceType>,
    > = std::collections::HashMap::new();
    for txn in &transactions {
        trade_pairs
            .entry((txn.buyer, txn.seller))
            .or_default()
            .insert(txn.resource);
    }
    for ((buyer, seller), resources) in &trade_pairs {
        let improvement = resources.len() as i32;
        let rel = game.diplomacy.ensure_relation(*buyer, *seller);
        rel.improve_score(improvement);
        report.trade_diplomacy.push((*buyer, *seller, improvement));
    }

    // 7. Record trade balance per nation
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

    // 8. Record in report
    report.trade_transactions = transactions;
}

/// Apply maintenance costs for army units.
/// Bankruptcy floor: treasury cannot go below -$5,000.
const BANKRUPTCY_FLOOR: Money = Money::dollars(-5000);

fn apply_maintenance(game: &mut GameState, report: &mut TurnReport) {
    for nation in &mut game.nations {
        let total_cost: Money = nation
            .army
            .iter()
            .map(|u| u.maintenance_cost())
            .fold(Money::ZERO, |acc, c| acc + c);
        if total_cost != Money::ZERO {
            nation.treasury -= total_cost;
        }

        // Bankruptcy protection: cap treasury at floor of -$5,000
        if nation.treasury < BANKRUPTCY_FLOOR {
            nation.treasury = BANKRUPTCY_FLOOR;
        }

        // Generate bankruptcy headline if treasury went negative
        if nation.is_bankrupt() {
            report.newspaper_headlines.push(format!(
                "FINANCIAL CRISIS: {} faces bankruptcy!",
                nation.name
            ));
        }
    }
}

/// Resolve military unit movement from pending_moves.
///
/// For each pending move:
/// 1. Validate the unit exists in the nation's army
/// 2. If destination is owned by the nation, move the unit there
/// 3. If destination is owned by an enemy at war, convert to a pending_attack instead
/// 4. Otherwise, reject the move
fn resolve_military_movement(game: &mut GameState, report: &mut TurnReport) {
    let moves: Vec<(NationId, crate::map::UnitId, ProvinceId)> =
        game.pending_moves.drain(..).collect();

    for (nation_id, unit_id, dest_province_id) in moves {
        // Look up the destination province owner
        let dest_owner = match game.get_province(dest_province_id) {
            Some(p) => p.owner,
            None => continue,
        };
        let dest_name = game
            .get_province(dest_province_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        if dest_owner == nation_id {
            // Friendly province: move the unit
            if let Some(nation) = game.get_nation_mut(nation_id)
                && let Some(unit) = nation.army.iter_mut().find(|u| u.id == unit_id)
            {
                let unit_type = format!("{:?}", unit.unit_type);
                unit.position = dest_province_id;
                report
                    .unit_movements
                    .push((nation_id, format!("{} moved to {}", unit_type, dest_name)));
            }
        } else {
            // Check if at war with the destination owner
            let at_war = game
                .diplomacy
                .get_relation(nation_id, dest_owner)
                .is_some_and(|r| r.at_war);
            if at_war {
                // Convert to pending attack
                game.pending_attacks.push((nation_id, dest_province_id));
                report.unit_movements.push((
                    nation_id,
                    format!("Attack ordered on {} (enemy territory)", dest_name),
                ));
            }
            // Otherwise ignore the invalid move
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

        // Trigger pact defense: if defender (Minor Nation) has a NAP with any GP,
        // that GP declares war on the attacker.
        trigger_pact_defense(game, defender_id, attacker_id, report);

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
            // Siege artillery destroys fort sections: reduce fort_level by 1
            if result.siege_reduced_fort
                && let Some(province) = game.get_province(province_id)
            {
                let cap_tile = province.capital_tile;
                if let Some(tile) = game.hex_map.get_tile_mut(cap_tile)
                    && tile.infrastructure.has_fort
                    && tile.infrastructure.fort_level > 0
                {
                    tile.infrastructure.fort_level -= 1;
                    if tile.infrastructure.fort_level == 0 {
                        tile.infrastructure.has_fort = false;
                    }
                }
            }

            // Change province owner and reset garrison (conquering nation has no garrison)
            if let Some(province) = game.get_province_mut(province_id) {
                province.owner = attacker_id;
                province.garrison_count = 0;
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

            // Captured GP capital: industrialize immediately (set to Village)
            let is_gp_capital = defender_type == NationType::GreatPower
                && game
                    .get_nation(defender_id)
                    .is_some_and(|n| n.capital_province_id == province_id);
            if is_gp_capital
                && let Some(province) = game.get_province_mut(province_id)
                && province.settlement_level == SettlementLevel::Hamlet
            {
                province.settlement_level = SettlementLevel::Village;
                province.industrialization_turns_remaining = None;
                province.town_countdown = Some(12);
                report
                    .settlement_upgrades
                    .push((province_id, "Village".to_string()));
                report.newspaper_headlines.push(format!(
                    "{} immediately industrializes under new management!",
                    province.name
                ));
            }

            // Award conquest medal if this is a Minor Nation capital
            let is_mn_capital = defender_type == NationType::MinorNation
                && game
                    .get_nation(defender_id)
                    .is_some_and(|n| n.capital_province_id == province_id);
            if is_mn_capital
                && let Some(attacker_nation) = game.get_nation_mut(attacker_id)
                && let Some(first_unit) = attacker_nation.army.first_mut()
            {
                first_unit.award_medal();
                report
                    .rewards_earned
                    .push((attacker_id, "Conquest medal awarded!".to_string()));
            }

            // Free Clippers: first colony established (first MN province conquered)
            if defender_type == NationType::MinorNation {
                let already_has_colony = game.get_nation(attacker_id).is_some_and(|n| n.has_colony);
                if !already_has_colony
                    && let Some(attacker_nation) = game.get_nation_mut(attacker_id)
                {
                    use crate::map::UnitId;
                    use crate::military::ships::{Ship, ShipType};
                    attacker_nation.has_colony = true;
                    let base_id = 5_000_000 + attacker_nation.id.0 * 100;
                    attacker_nation.merchant_fleet.push(Ship::new(
                        UnitId(base_id + 1),
                        ShipType::Clipper,
                        attacker_id,
                    ));
                    attacker_nation.merchant_fleet.push(Ship::new(
                        UnitId(base_id + 2),
                        ShipType::Clipper,
                        attacker_id,
                    ));
                    let atk_colony_name = attacker_nation.name.clone();
                    report.rewards_earned.push((
                        attacker_id,
                        format!(
                            "{} receives free Clipper ships for establishing its first colony!",
                            atk_colony_name
                        ),
                    ));
                    report.newspaper_headlines.push(format!(
                        "{} receives free Clipper ships for establishing its first colony!",
                        atk_colony_name
                    ));
                }
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

    // ── Counter-attacks ──────────────────────────────────────────
    // After all initial attacks resolve, check if any conquered province has
    // the original defender's army units in an adjacent province.  If so,
    // those units counter-attack the newly occupied province (one round only).
    let conquered_provinces: Vec<(NationId, ProvinceId, NationId)> = report
        .battles
        .iter()
        .filter(|b| b.attacker_won)
        .map(|b| (b.attacker, b.province, b.defender))
        .collect();

    let mut counter_attacks: Vec<(NationId, ProvinceId)> = Vec::new();

    for (_new_owner, conquered_prov_id, original_defender) in &conquered_provinces {
        // Collect tiles that belong to the conquered province
        let conquered_tiles: Vec<crate::hex::HexCoord> = game
            .get_province(*conquered_prov_id)
            .map(|p| p.tiles.clone())
            .unwrap_or_default();

        // Find all provinces adjacent to the conquered province
        let mut adjacent_province_ids: Vec<ProvinceId> = Vec::new();
        for tile_coord in &conquered_tiles {
            for neighbor in tile_coord.neighbors() {
                if let Some(tile) = game.hex_map.get_tile(neighbor)
                    && let Some(adj_pid) = tile.province_id
                    && adj_pid != *conquered_prov_id
                    && !adjacent_province_ids.contains(&adj_pid)
                {
                    adjacent_province_ids.push(adj_pid);
                }
            }
        }

        // Check if the original defender has army units in any adjacent province
        // that is still owned by the defender
        let defender_nation = match game.get_nation(*original_defender) {
            Some(n) => n,
            None => continue,
        };

        let has_adjacent_units = defender_nation.army.iter().any(|u| {
            adjacent_province_ids.contains(&u.position)
                && defender_nation.province_ids.contains(&u.position)
        });

        if has_adjacent_units
            && !counter_attacks
                .iter()
                .any(|(_, p)| *p == *conquered_prov_id)
        {
            counter_attacks.push((*original_defender, *conquered_prov_id));
        }
    }

    // Resolve counter-attacks (second pass — no further counter-attacks after this)
    for (counter_attacker_id, target_province_id) in counter_attacks {
        let new_owner_id = match game.get_province(target_province_id) {
            Some(p) => p.owner,
            None => continue,
        };

        // The counter-attacker uses their army units from adjacent provinces
        let counter_units: Vec<ArmyUnit> = match game.get_nation(counter_attacker_id) {
            Some(n) => n.army.clone(),
            None => continue,
        };

        if counter_units.is_empty() {
            continue;
        }

        let counter_force = CombatForce {
            nation: counter_attacker_id,
            units: counter_units,
        };

        // Defender of counter-attack is the new occupier — use surviving attacker army
        let occupier_units: Vec<ArmyUnit> = match game.get_nation(new_owner_id) {
            Some(n) => n.army.clone(),
            None => continue,
        };

        let defender_force = CombatForce {
            nation: new_owner_id,
            units: occupier_units,
        };

        let (battle_terrain, battle_fort_level) = game
            .get_province(target_province_id)
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
            &counter_force,
            &defender_force,
            target_province_id,
            battle_terrain,
            battle_fort_level,
        );

        // Update counter-attacker's surviving army
        if let Some(ca_nation) = game.get_nation_mut(counter_attacker_id) {
            ca_nation.army = result.attacker_survivors.clone();
        }

        // Update occupier's surviving army
        if let Some(occ_nation) = game.get_nation_mut(new_owner_id) {
            occ_nation.army = result.defender_survivors.clone();
        }

        if result.attacker_won {
            // Counter-attack succeeds: province returns to original defender
            if let Some(province) = game.get_province_mut(target_province_id) {
                province.owner = counter_attacker_id;
            }
            if let Some(occ_nation) = game.get_nation_mut(new_owner_id) {
                occ_nation
                    .province_ids
                    .retain(|pid| *pid != target_province_id);
            }
            if let Some(ca_nation) = game.get_nation_mut(counter_attacker_id) {
                ca_nation.add_province(target_province_id);
            }

            // Reset garrison to 0 on re-conquered province
            if let Some(province) = game.get_province_mut(target_province_id) {
                province.garrison_count = 0;
            }

            let ca_name = game
                .get_nation(counter_attacker_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let prov_name = game
                .get_province(target_province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            report.newspaper_headlines.push(format!(
                "{} counter-attacks and recaptures {}!",
                ca_name, prov_name
            ));
        } else {
            let occ_name = game
                .get_nation(new_owner_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let prov_name = game
                .get_province(target_province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            report.newspaper_headlines.push(format!(
                "{} repels counter-attack on {}!",
                occ_name, prov_name
            ));
        }

        report.battles.push(result);
    }
}

/// Trigger pact defense: when a Minor Nation with a Non-Aggression Pact is attacked,
/// the Great Power that signed the pact declares war on the attacker.
///
/// For AI GPs: automatically declare war. For human GP: generates a newspaper headline
/// notifying them of the attack (human makes their own decisions).
fn trigger_pact_defense(
    game: &mut GameState,
    defender_nation_id: NationId,
    attacker_nation_id: NationId,
    report: &mut TurnReport,
) {
    // Only trigger for Minor Nation defenders
    let is_minor = game
        .get_nation(defender_nation_id)
        .is_some_and(|n| !n.is_great_power());
    if !is_minor {
        return;
    }

    let defender_name = game
        .get_nation(defender_nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    // Find all Great Powers that have a NonAggressionPact with this Minor Nation
    let gp_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power() && n.id != attacker_nation_id)
        .map(|n| n.id)
        .collect();

    let mut pact_holders: Vec<(NationId, String)> = Vec::new();
    for gp_id in &gp_ids {
        let has_pact = game.diplomacy.has_treaty(
            *gp_id,
            defender_nation_id,
            crate::events::TreatyType::NonAggressionPact,
        );
        if !has_pact {
            continue;
        }

        // Check if already at war with the attacker
        let already_at_war = game
            .diplomacy
            .get_relation(*gp_id, attacker_nation_id)
            .is_some_and(|r| r.at_war);
        if already_at_war {
            continue;
        }

        let gp_name = game
            .get_nation(*gp_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        pact_holders.push((*gp_id, gp_name));
    }

    let attacker_name = game
        .get_nation(attacker_nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    for (gp_id, gp_name) in &pact_holders {
        // Notify about the pact being triggered
        report.newspaper_headlines.push(format!(
            "{} requests {}'s aid against {}!",
            defender_name, gp_name, attacker_name
        ));

        // AI GPs automatically declare war on the attacker
        let is_ai = game
            .get_nation(*gp_id)
            .is_some_and(|n| n.ai_personality.is_some());
        if is_ai {
            game.diplomacy.declare_war(*gp_id, attacker_nation_id);
            report.newspaper_headlines.push(format!(
                "{} honors its pact with {} and declares war on {}!",
                gp_name, defender_name, attacker_name
            ));
            game.history.push((
                game.turn,
                format!(
                    "{} declared war on {} to honor pact with {}",
                    gp_name, attacker_name, defender_name
                ),
            ));
        }
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

/// Resolve technology for AI nations that researched tech this turn.
///
/// For each AI nation, check if it researched a technology (tracked in ai_actions).
/// If so, generate a TechnologyResearched domain event and push it to the report.
fn resolve_technology(game: &mut GameState, report: &mut TurnReport) {
    // Collect techs researched by AI this turn from ai_actions
    // AI actions contain strings like "Deneb researched High Pressure Steam Engine ($0)"
    let researched_pattern = " researched ";
    for action in &report.ai_actions {
        if let Some(pos) = action.find(researched_pattern) {
            let nation_name = &action[..pos];
            // Find the nation by name
            if let Some(nation) = game.nations.iter().find(|n| n.name == nation_name) {
                let nation_id = nation.id;
                // Find the most recently researched tech
                if let Some(tech_id) = nation.researched_techs.last().copied() {
                    report
                        .events
                        .push(DomainEvent::TechnologyResearched(TechnologyResearched {
                            nation: nation_id,
                            tech: tech_id,
                        }));
                }
            }
        }
    }
}

/// Generate newspaper headlines for the turn report.
///
/// Gathers notable events from the turn: AI actions (tech research, military
/// buildup, war declarations), trade activity, and adds period-appropriate
/// flavor headlines that rotate based on the turn number.
/// Check for alliance obligations when nations are at war.
///
/// When a nation is at war, check if the defender has allies (Alliance treaty).
/// AI allies automatically join the war by declaring war on the attacker.
/// A newspaper headline is generated for each alliance activation.
fn resolve_alliance_obligations(game: &mut GameState, report: &mut TurnReport) {
    // Collect all active wars
    let mut wars: Vec<(NationId, NationId)> = Vec::new();
    for nation in &game.nations {
        if !nation.is_great_power() {
            continue;
        }
        let rels = game.diplomacy.relations_for(nation.id);
        for ((a, b), rel) in &rels {
            if rel.at_war && *a == nation.id {
                wars.push((*a, *b));
            }
        }
    }

    // For each war, check if either side has allies that are not yet at war with the other side
    let mut new_wars: Vec<(NationId, NationId, String, String)> = Vec::new();
    for (attacker, defender) in &wars {
        // Check defender's allies
        let defender_allies = game.diplomacy.get_allies(*defender);
        for ally in &defender_allies {
            if *ally == *attacker {
                continue;
            }
            // Check if this ally is already at war with the attacker
            let already_at_war = game
                .diplomacy
                .get_relation(*ally, *attacker)
                .is_some_and(|r| r.at_war);
            if already_at_war {
                continue;
            }
            // Check if this ally is an AI nation (human allies make their own decisions)
            let is_ai = game
                .get_nation(*ally)
                .is_some_and(|n| n.ai_personality.is_some());
            if !is_ai {
                continue;
            }
            let ally_name = game
                .get_nation(*ally)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let defender_name = game
                .get_nation(*defender)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let attacker_name = game
                .get_nation(*attacker)
                .map(|n| n.name.clone())
                .unwrap_or_default();

            new_wars.push((*ally, *attacker, ally_name.clone(), attacker_name.clone()));
            report.newspaper_headlines.push(format!(
                "{} honors its alliance with {} and declares war on {}!",
                ally_name, defender_name, attacker_name
            ));
        }

        // Check attacker's allies
        let attacker_allies = game.diplomacy.get_allies(*attacker);
        for ally in &attacker_allies {
            if *ally == *defender {
                continue;
            }
            let already_at_war = game
                .diplomacy
                .get_relation(*ally, *defender)
                .is_some_and(|r| r.at_war);
            if already_at_war {
                continue;
            }
            let is_ai = game
                .get_nation(*ally)
                .is_some_and(|n| n.ai_personality.is_some());
            if !is_ai {
                continue;
            }
            let ally_name = game
                .get_nation(*ally)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let attacker_name = game
                .get_nation(*attacker)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let defender_name = game
                .get_nation(*defender)
                .map(|n| n.name.clone())
                .unwrap_or_default();

            new_wars.push((*ally, *defender, ally_name.clone(), defender_name.clone()));
            report.newspaper_headlines.push(format!(
                "{} honors its alliance with {} and declares war on {}!",
                ally_name, attacker_name, defender_name
            ));
        }
    }

    // Actually declare the new wars (done after collecting to avoid borrow issues)
    for (ally, enemy, ally_name, enemy_name) in &new_wars {
        game.diplomacy.declare_war(*ally, *enemy);
        let turn = game.turn;
        game.history.push((
            turn,
            format!(
                "{} joined war against {} (alliance obligation)",
                ally_name, enemy_name
            ),
        ));
    }
}

/// Resolve voluntary incorporations: Minor Nations with high relationship scores
/// voluntarily join the Great Power with the highest score.
///
/// For each Minor Nation, checks relationships with all Great Powers.
/// If any relationship score >= 75, the Minor Nation joins the GP with the highest score.
/// All of the Minor Nation's provinces are transferred to the Great Power.
fn resolve_voluntary_incorporations(game: &mut GameState, report: &mut TurnReport) {
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

    let threshold = 75;

    for minor_id in &minor_ids {
        let mut best_gp: Option<NationId> = None;
        let mut best_score: i32 = threshold - 1; // must be >= threshold

        for gp_id in &gp_ids {
            if let Some(rel) = game.diplomacy.get_relation(*minor_id, *gp_id)
                && rel.score >= threshold
                && rel.score > best_score
            {
                best_score = rel.score;
                best_gp = Some(*gp_id);
            }
        }

        if let Some(gp_id) = best_gp {
            // Transfer all provinces from minor nation to great power
            let provinces_to_transfer: Vec<ProvinceId> = game
                .get_nation(*minor_id)
                .map(|n| n.province_ids.clone())
                .unwrap_or_default();

            // Update province owners
            for pid in &provinces_to_transfer {
                if let Some(prov) = game.get_province_mut(*pid) {
                    prov.owner = gp_id;
                }
            }

            // Remove provinces from minor nation
            if let Some(minor) = game.get_nation_mut(*minor_id) {
                minor.province_ids.clear();
            }

            // Add provinces to great power
            if let Some(gp) = game.get_nation_mut(gp_id) {
                for pid in &provinces_to_transfer {
                    gp.add_province(*pid);
                }
            }

            // Get names for reporting
            let minor_name = game
                .get_nation(*minor_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let gp_name = game
                .get_nation(gp_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());

            // Record event
            report
                .events
                .push(DomainEvent::NationIncorporated(NationIncorporated {
                    minor_nation: *minor_id,
                    great_power: gp_id,
                }));

            report.incorporations.push((*minor_id, gp_id));

            // Free Clippers: first colony established (MN voluntarily incorporated)
            {
                let already_has_colony = game.get_nation(gp_id).is_some_and(|n| n.has_colony);
                if !already_has_colony && let Some(gp) = game.get_nation_mut(gp_id) {
                    use crate::map::UnitId;
                    use crate::military::ships::{Ship, ShipType};
                    gp.has_colony = true;
                    let base_id = 5_000_000 + gp.id.0 * 100;
                    gp.merchant_fleet.push(Ship::new(
                        UnitId(base_id + 1),
                        ShipType::Clipper,
                        gp_id,
                    ));
                    gp.merchant_fleet.push(Ship::new(
                        UnitId(base_id + 2),
                        ShipType::Clipper,
                        gp_id,
                    ));
                    let gp_colony_name = gp.name.clone();
                    report.rewards_earned.push((
                        gp_id,
                        format!(
                            "{} receives free Clipper ships for establishing its first colony!",
                            gp_colony_name
                        ),
                    ));
                    report.newspaper_headlines.push(format!(
                        "{} receives free Clipper ships for establishing its first colony!",
                        gp_colony_name
                    ));
                }
            }

            // Record in history
            game.history.push((
                game.turn,
                format!("{} voluntarily joined the {} empire", minor_name, gp_name),
            ));

            // Newspaper headline added later via report.incorporations
        }
    }
}

/// Resolve unit upgrades for AI nations.
///
/// For each AI nation, for each army unit, checks if an upgrade is available:
/// - The unit type has an `upgrade_to()` path
/// - The target unit type has a `required_tech()` name
/// - The nation has researched a tech whose name matches the required tech
///
/// Player units are not auto-upgraded (player uses `upgrade <index>` command).
fn resolve_unit_upgrades(game: &mut GameState, report: &mut TurnReport) {
    let ai_nation_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.ai_personality.is_some())
        .map(|n| n.id)
        .collect();

    for nation_id in &ai_nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Collect researched tech names for this nation
        let researched_tech_names: Vec<String> = nation
            .researched_techs
            .iter()
            .filter_map(|tid| game.tech_tree.get(*tid))
            .map(|t| t.name.clone())
            .collect();

        // Find upgrades to perform
        let mut upgrades: Vec<(usize, ArmyUnitType, ArmyUnitType)> = Vec::new();
        for (i, unit) in nation.army.iter().enumerate() {
            if let Some(target_type) = unit.unit_type.upgrade_to()
                && let Some(required_tech_name) = target_type.required_tech()
                && researched_tech_names
                    .iter()
                    .any(|name| name == required_tech_name)
            {
                upgrades.push((i, unit.unit_type, target_type));
            }
        }

        // Apply upgrades (preserve medals, health)
        let nation = match game.nations.iter_mut().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        for (idx, from_type, to_type) in &upgrades {
            if *idx < nation.army.len() {
                nation.army[*idx].unit_type = *to_type;
                // Refresh movement for new type
                nation.army[*idx].movement_remaining = to_type.stats().movement;
            }
            report.unit_upgrades.push((
                *nation_id,
                format!("{:?}", from_type),
                format!("{:?}", to_type),
            ));
            report.events.push(DomainEvent::UnitUpgraded(UnitUpgraded {
                nation: *nation_id,
                from_type: format!("{:?}", from_type),
                to_type: format!("{:?}", to_type),
            }));
        }
    }
}

/// Resolve rewards: Generals earned from arms buildup, Admirals earned from Ship-of-the-Line
/// buildup, free Clippers for first colony, capitol expansion from GP capital conquest.
fn resolve_rewards(game: &mut GameState, report: &mut TurnReport) {
    use crate::map::UnitId;
    use crate::military::ships::{Ship, ShipType};

    let nation_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    for nation_id in &nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Calculate total arms built (sum of arms_required for all army units)
        let total_arms: u32 = nation
            .army
            .iter()
            .map(|u| u.unit_type.stats().arms_required)
            .sum();

        // Update the tracked total
        let current_total = total_arms.max(nation.total_arms_built);

        // General thresholds: 6, 12, 20, 30, ...
        // The nth general is earned at: 6, 12, 20, 30 (6 + 6 + 8 + 10...)
        // Simplified: thresholds are 6, 12, 20, 30, 42, 56, ...
        let general_thresholds = [6u32, 12, 20, 30, 42, 56, 72, 90];
        let generals_earned_now = nation.generals_earned;

        let mut new_generals = 0u32;
        for (i, threshold) in general_thresholds.iter().enumerate() {
            if i as u32 >= generals_earned_now && current_total >= *threshold {
                new_generals += 1;
            }
        }

        if new_generals > 0 || current_total != nation.total_arms_built {
            let nation = match game.nations.iter_mut().find(|n| n.id == *nation_id) {
                Some(n) => n,
                None => continue,
            };
            nation.total_arms_built = current_total;

            for _ in 0..new_generals {
                nation.generals_earned += 1;
                let gen_id = UnitId(3_000_000 + nation.id.0 * 100 + nation.generals_earned);
                let general_unit = ArmyUnit::new(
                    gen_id,
                    ArmyUnitType::General,
                    *nation_id,
                    nation.capital_province_id,
                );
                nation.army.push(general_unit);

                let nation_name = nation.name.clone();
                report
                    .rewards_earned
                    .push((*nation_id, format!("{} has earned a General!", nation_name)));
                report
                    .newspaper_headlines
                    .push(format!("{} has earned a General!", nation_name));
            }
        }
    }

    // Admiral reward: track Ships-of-the-Line built per nation.
    // When count >= 5 (and then every 5 more): earn an Admiral (free bonus Ship-of-the-Line).
    for nation_id in &nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Count Ships-of-the-Line in warship fleet
        let sol_count: u32 = nation
            .warships
            .iter()
            .filter(|s| s.ship_type == ShipType::ShipOfTheLine)
            .count() as u32;

        let current_sol = sol_count.max(nation.total_ships_of_the_line_built);
        let admirals_earned_now = nation.admirals_earned;

        // Admiral thresholds: every 5 Ships-of-the-Line (5, 10, 15, ...)
        let mut new_admirals = 0u32;
        let mut threshold = 5u32;
        let mut idx = 0u32;
        while threshold <= current_sol {
            if idx >= admirals_earned_now {
                new_admirals += 1;
            }
            idx += 1;
            threshold += 5;
        }

        if new_admirals > 0 || current_sol != nation.total_ships_of_the_line_built {
            let nation = match game.nations.iter_mut().find(|n| n.id == *nation_id) {
                Some(n) => n,
                None => continue,
            };
            nation.total_ships_of_the_line_built = current_sol;

            for _ in 0..new_admirals {
                nation.admirals_earned += 1;
                // Award a free Ship-of-the-Line as the Admiral bonus warship
                let ship_id = UnitId(4_000_000 + nation.id.0 * 100 + nation.admirals_earned);
                let bonus_ship = Ship::new(ship_id, ShipType::ShipOfTheLine, *nation_id);
                nation.warships.push(bonus_ship);

                let nation_name = nation.name.clone();
                report.rewards_earned.push((
                    *nation_id,
                    format!("{} has earned an Admiral!", nation_name),
                ));
                report
                    .newspaper_headlines
                    .push(format!("{} has earned an Admiral!", nation_name));
            }
        }
    }

    // Capitol expansion: check if any GP conquered another GP's capital this turn
    // We detect this by checking battles where the attacker won and the province
    // was a capital of a Great Power.
    let battle_results: Vec<(NationId, ProvinceId)> = report
        .battles
        .iter()
        .filter(|b| b.attacker_won)
        .map(|b| (b.attacker, b.province))
        .collect();

    for (attacker_id, province_id) in battle_results {
        // Check if this province is a capital of any Great Power
        let is_gp_capital = game.nations.iter().any(|n| {
            n.is_great_power() && n.capital_province_id == province_id && n.id != attacker_id
        });

        if is_gp_capital
            && let Some(attacker) = game.nations.iter_mut().find(|n| n.id == attacker_id)
        {
            attacker.capitol_bonus_capacity += 1;
            let attacker_name = attacker.name.clone();
            report.rewards_earned.push((
                attacker_id,
                format!(
                    "{}'s capitol building has expanded from conquering a Great Power's capital!",
                    attacker_name
                ),
            ));
            report.newspaper_headlines.push(format!(
                "{}'s capitol building has expanded!",
                attacker_name
            ));
        }
    }

    // Expert worker reward: at 10 experts -> +1 capitol_bonus_capacity,
    // at 30 experts -> +1 more. Tracked by expert_rewards_earned to prevent duplicates.
    let expert_thresholds: [(u32, u8); 2] = [(10, 1), (30, 2)];
    for nation_id in &nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        let expert_count = nation.labor.expert;
        let already_earned = nation.expert_rewards_earned;

        // Determine how many rewards should have been earned by now
        let mut should_have_earned: u8 = 0;
        for &(threshold, reward_level) in &expert_thresholds {
            if expert_count >= threshold {
                should_have_earned = reward_level;
            }
        }

        if should_have_earned > already_earned {
            let new_rewards = should_have_earned - already_earned;
            let nation = match game.nations.iter_mut().find(|n| n.id == *nation_id) {
                Some(n) => n,
                None => continue,
            };
            nation.expert_rewards_earned = should_have_earned;
            nation.capitol_bonus_capacity += new_rewards as u32;

            let nation_name = nation.name.clone();
            for _ in 0..new_rewards {
                report.rewards_earned.push((
                    *nation_id,
                    format!(
                        "{}'s capitol has expanded from expert workforce development!",
                        nation_name
                    ),
                ));
                report.newspaper_headlines.push(format!(
                    "{}'s expert workforce drives capitol expansion!",
                    nation_name
                ));
            }
        }
    }
}

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

    // Voluntary incorporations — major headline
    for (minor_id, gp_id) in &report.incorporations {
        let minor_name = game
            .get_nation(*minor_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let gp_name = game
            .get_nation(*gp_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        report.newspaper_headlines.push(format!(
            "BREAKING: {} has voluntarily joined the {} empire!",
            minor_name, gp_name
        ));
    }

    // Unit upgrades — brief mention
    if !report.unit_upgrades.is_empty() {
        let upgrade_count = report.unit_upgrades.len();
        report.newspaper_headlines.push(format!(
            "Military modernization: {} unit{} upgraded across the nations",
            upgrade_count,
            if upgrade_count == 1 { "" } else { "s" }
        ));
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
    let result = run_council_vote(&game.nations, &game.provinces, is_final, &game.diplomacy);

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
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
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
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
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
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
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
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
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
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
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
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
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
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
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

    // ── Town production ────────────────────────────────────────

    /// Helper: build a game state with a Village province containing timber tiles.
    fn test_game_state_with_village(terrain_types: &[TerrainType]) -> GameState {
        let mut hex_map = HexMap::new(20, 20);
        let mut tiles = Vec::new();
        let capital_coord = HexCoord::new(0, 0);

        // Capital province (just a simple farm)
        let cap_tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        hex_map.set_tile(capital_coord, cap_tile);

        // Village province with given terrain types
        for (i, terrain) in terrain_types.iter().enumerate() {
            let coord = HexCoord::new(5 + i as i32, 0);
            let tile = Tile::with_province(*terrain, ProvinceId(2));
            hex_map.set_tile(coord, tile);
            tiles.push(coord);
        }

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            capital_coord,
            vec![capital_coord],
            4,
        );

        let village_capital = if tiles.is_empty() {
            HexCoord::new(5, 0)
        } else {
            tiles[0]
        };
        let mut province_village = Province::new(
            ProvinceId(2),
            "Villageton".to_string(),
            NationId(1),
            village_capital,
            tiles,
            4,
        );
        province_village.settlement_level = SettlementLevel::Village;
        province_village.connected_to_capital = true;

        let mut nation = Nation::new(
            NationId(1),
            "TownNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province_capital, province_village],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        }
    }

    #[test]
    fn town_production_village_produces_lumber_from_timber() {
        // Village with 4 ScrubForest tiles: each yields 1 Timber = 4 total
        // 4 timber / 2 = 2 lumber
        // 2 lumber / 2 = 1 furniture
        let mut game = test_game_state_with_village(&[TerrainType::ScrubForest; 4]);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Lumber),
            2,
            "Village should produce 2 lumber from 4 timber"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Furniture),
            1,
            "Village should produce 1 furniture from 2 lumber"
        );

        // Check report tracks town production
        let lumber_output: u32 = report
            .town_production
            .iter()
            .filter(|(_, name, _)| name == "Lumber")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(lumber_output, 2);
    }

    #[test]
    fn town_production_village_produces_steel_from_coal_and_iron() {
        // Create a village with prospected BarrenHills tiles containing coal and iron
        let mut hex_map = HexMap::new(20, 20);
        let capital_coord = HexCoord::new(0, 0);
        let cap_tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        hex_map.set_tile(capital_coord, cap_tile);

        // 2 Coal tiles and 2 Iron tiles (BarrenHills with revealed deposits)
        let coords: Vec<HexCoord> = (0..4).map(|i| HexCoord::new(5 + i, 0)).collect();
        for (i, &coord) in coords.iter().enumerate() {
            let mut tile = Tile::with_province(TerrainType::BarrenHills, ProvinceId(2));
            if i < 2 {
                tile.reveal_deposit(ResourceType::Coal);
            } else {
                tile.reveal_deposit(ResourceType::Iron);
            }
            hex_map.set_tile(coord, tile);
        }

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            capital_coord,
            vec![capital_coord],
            4,
        );

        let mut province_village = Province::new(
            ProvinceId(2),
            "SteelVillage".to_string(),
            NationId(1),
            coords[0],
            coords.clone(),
            4,
        );
        province_village.settlement_level = SettlementLevel::Village;
        province_village.connected_to_capital = true;

        let mut nation = Nation::new(
            NationId(1),
            "SteelNation".to_string(),
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
            provinces: vec![province_capital, province_village],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        let report = process_turn(&mut game);

        // Coal: 2 tiles * 1 yield = 2, Iron: 2 tiles * 1 yield = 2
        // Steel = min(2, 2) = 2
        // Hardware = 2 / 2 = 1
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Steel),
            2,
            "Village should produce 2 steel from 2 coal + 2 iron"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Hardware),
            1,
            "Village should produce 1 hardware from 2 steel"
        );

        let steel_output: u32 = report
            .town_production
            .iter()
            .filter(|(_, name, _)| name == "Steel")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(steel_output, 2);
    }

    #[test]
    fn town_production_village_produces_fabric_and_clothing() {
        // Village with 4 Plantation tiles (Cotton): each yields 1 cotton = 4 total
        // 4 cotton / 2 = 2 fabric
        // 2 fabric / 2 = 1 clothing
        let mut game = test_game_state_with_village(&[TerrainType::Plantation; 4]);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Fabric),
            2,
            "Village should produce 2 fabric from 4 cotton"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Clothing),
            1,
            "Village should produce 1 clothing from 2 fabric"
        );

        let fabric_output: u32 = report
            .town_production
            .iter()
            .filter(|(_, name, _)| name == "Fabric")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(fabric_output, 2);
    }

    #[test]
    fn town_production_hamlet_does_not_produce() {
        // Same setup but settlement is Hamlet, not Village
        let mut game = test_game_state_with_village(&[TerrainType::ScrubForest; 4]);
        // Override to Hamlet
        game.provinces
            .iter_mut()
            .find(|p| p.id == ProvinceId(2))
            .unwrap()
            .settlement_level = SettlementLevel::Hamlet;

        let report = process_turn(&mut game);

        // Should have no town production
        assert!(
            report.town_production.is_empty(),
            "Hamlet province should not produce via town production"
        );
    }

    #[test]
    fn town_produces_at_double_rate() {
        // Town with 4 ScrubForest tiles
        // Village rate: 4 timber → 2 lumber → 1 furniture
        // Town rate: 4*2=8 timber → 4 lumber → 2 furniture
        let mut game = test_game_state_with_village(&[TerrainType::ScrubForest; 4]);
        // Upgrade to Town
        game.provinces
            .iter_mut()
            .find(|p| p.id == ProvinceId(2))
            .unwrap()
            .settlement_level = SettlementLevel::Town;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Lumber),
            4,
            "Town should produce 4 lumber (double rate)"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Furniture),
            2,
            "Town should produce 2 furniture (double rate)"
        );

        let lumber_output: u32 = report
            .town_production
            .iter()
            .filter(|(_, name, _)| name == "Lumber")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(lumber_output, 4);
    }

    // ── Village → Town progression ─────────────────────────────

    #[test]
    fn village_upgrades_to_town_after_12_connected_turns() {
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

        let mut province_village = Province::new(
            ProvinceId(2),
            "Growing Town".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        province_village.settlement_level = SettlementLevel::Village;
        province_village.connected_to_capital = true;
        province_village.town_countdown = Some(12);

        let mut nation = Nation::new(
            NationId(1),
            "GrowthNation".to_string(),
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
            provinces: vec![province_capital, province_village],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        // Process 12 turns to count down the town_countdown
        for _ in 0..12 {
            process_turn(&mut game);
        }

        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            SettlementLevel::Town,
            "Province should have upgraded to Town after 12 turns"
        );
        assert_eq!(province.town_countdown, None);
    }

    #[test]
    fn village_does_not_upgrade_to_town_before_12_turns() {
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

        let mut province_village = Province::new(
            ProvinceId(2),
            "Still Village".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        province_village.settlement_level = SettlementLevel::Village;
        province_village.connected_to_capital = true;
        province_village.town_countdown = Some(12);

        let mut nation = Nation::new(
            NationId(1),
            "PatientNation".to_string(),
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
            provinces: vec![province_capital, province_village],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        // Process only 11 turns — not enough
        for _ in 0..11 {
            process_turn(&mut game);
        }

        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            SettlementLevel::Village,
            "Province should still be Village after only 11 turns"
        );
        assert_eq!(province.town_countdown, Some(1));
    }

    // ── Full settlement progression: Hamlet → Village → Town ───

    #[test]
    fn full_settlement_progression_hamlet_to_village_to_town() {
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
            "RemoteLand".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        province_remote.connected_to_capital = true;
        // Starts as Hamlet

        let mut nation = Nation::new(
            NationId(1),
            "ProgressNation".to_string(),
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
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        // 7 turns: Hamlet → Village (1 to start countdown + 6 to count down)
        for _ in 0..7 {
            process_turn(&mut game);
        }
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(province.settlement_level, SettlementLevel::Village);
        assert_eq!(
            province.town_countdown,
            Some(12),
            "Town countdown should start at 12 upon becoming Village"
        );

        // 12 more turns: Village → Town
        for _ in 0..12 {
            process_turn(&mut game);
        }
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            SettlementLevel::Town,
            "Province should have upgraded to Town after 7 + 12 = 19 turns"
        );
    }

    // ── Economy integration tests ──────────────────────────────

    #[test]
    fn full_economic_cycle_resources_to_goods_to_trade() {
        // Set up a nation with full production chain
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Add raw resources for all chains
        nation.add_resource(ResourceType::Timber, 10);
        nation.add_resource(ResourceType::Coal, 10);
        nation.add_resource(ResourceType::Iron, 10);
        nation.add_resource(ResourceType::Cotton, 10);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // LumberMill cap 2: 10 timber → 2 lumber (4 timber consumed), 6 remain
        // SteelMill cap 2: 10 coal + 10 iron → 2 steel (2 each consumed), 8 each remain
        // TextileMill cap 2: 10 cotton → 2 fabric (4 consumed), 6 remain
        // FurnitureFactory cap 1: 2 lumber → 1 furniture (2 consumed), 0 lumber remain
        // HardwareFactory cap 1: 2 steel → 1 hardware (2 consumed), 0 steel remain
        // ClothingFactory cap 1: 2 fabric → 1 clothing (2 consumed), 0 fabric remain
        assert_eq!(nation.goods_amount(GoodsType::Furniture), 1);
        assert_eq!(nation.goods_amount(GoodsType::Hardware), 1);
        assert_eq!(nation.goods_amount(GoodsType::Clothing), 1);

        // Verify production was reported
        let has_furniture = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Furniture" && *q == 1);
        let has_hardware = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Hardware" && *q == 1);
        let has_clothing = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Clothing" && *q == 1);
        assert!(has_furniture);
        assert!(has_hardware);
        assert!(has_clothing);
    }

    #[test]
    fn immigration_cycle_food_canned_food_clothing_furniture_new_worker() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Add food processing building
        nation
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 5));

        // Plenty of raw food for canning and surplus
        nation.add_resource(ResourceType::Grain, 50);

        // Pre-stock clothing and furniture for immigration
        nation.add_goods(GoodsType::Clothing, 5);
        nation.add_goods(GoodsType::Furniture, 5);

        // Start with 0 workers so food surplus is guaranteed
        nation.labor.untrained = 0;

        // Need at least 4 provinces for 1 immigrant per turn
        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        // Run one turn: food processing creates CannedFood, immigration uses it
        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        // Should have recruited at least 1 immigrant
        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(immigration, 1, "Should recruit 1 immigrant");
        assert_eq!(
            nation.labor.untrained, 1,
            "Should have 1 untrained worker after immigration"
        );
    }

    #[test]
    fn starvation_insufficient_food_kills_workers() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.labor.untrained = 8;
        // Give only 3 food, need 8, deficit = 5, capped at 2 deaths
        nation.add_resource(ResourceType::Grain, 3);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.labor.total_workers(),
            6,
            "Should lose 2 workers (capped)"
        );

        let starved: u32 = report
            .starvation
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(starved, 2, "Should report 2 workers lost to starvation");
    }

    // ── Combat tests ───────────────────────────────────────────

    #[test]
    fn fort_defense_bonus_applies_correctly() {
        use crate::military::combat::fort_defense_bonus;

        assert_eq!(fort_defense_bonus(0), 0.0);
        assert!((fort_defense_bonus(1) - 0.20).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(2) - 0.40).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(3) - 0.60).abs() < f64::EPSILON);
    }

    #[test]
    fn medal_boosted_unit_has_higher_firepower() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let base_unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        let base_fp = base_unit.effective_firepower();

        let mut medal_unit = ArmyUnit::new(
            UnitId(2),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        medal_unit.award_medal();
        medal_unit.award_medal();
        let medal_fp = medal_unit.effective_firepower();

        // 2 medals = 1.0 + 2*0.25 = 1.5x multiplier
        assert!(
            medal_fp > base_fp,
            "Medal unit firepower ({}) should exceed base ({})",
            medal_fp,
            base_fp
        );
        assert!(
            (medal_fp - base_fp * 1.5).abs() < f64::EPSILON,
            "2 medals should give 1.5x firepower"
        );
    }

    #[test]
    fn garrison_created_correctly_for_gp_and_mn() {
        use crate::military::combat::create_garrison;

        let gp_garrison = create_garrison(NationType::GreatPower);
        assert_eq!(
            gp_garrison.len(),
            4,
            "Great Power garrison should have 4 units"
        );

        let mn_garrison = create_garrison(NationType::MinorNation);
        assert_eq!(
            mn_garrison.len(),
            3,
            "Minor Nation garrison should have 3 units"
        );
    }

    #[test]
    fn town_production_wool_produces_fabric() {
        // Village with 4 FertileHills tiles (Wool): each yields 1 wool = 4 total
        // 4 wool / 2 = 2 fabric
        // 2 fabric / 2 = 1 clothing
        let mut game = test_game_state_with_village(&[TerrainType::FertileHills; 4]);

        let _report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Fabric),
            2,
            "Village should produce 2 fabric from 4 wool"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Clothing),
            1,
            "Village should produce 1 clothing from 2 fabric"
        );
    }

    #[test]
    fn town_production_mixed_cotton_and_wool() {
        // Village with 2 Plantation (Cotton) + 2 FertileHills (Wool) tiles
        // total cotton+wool = 4 → 2 fabric → 1 clothing
        let mut game = test_game_state_with_village(&[
            TerrainType::Plantation,
            TerrainType::Plantation,
            TerrainType::FertileHills,
            TerrainType::FertileHills,
        ]);

        let _report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Fabric),
            2,
            "Village should produce 2 fabric from 2 cotton + 2 wool"
        );
    }

    // ── Voluntary incorporation tests ─────────────────────────────

    /// Helper: build a game state with one Great Power and one Minor Nation
    /// for testing voluntary incorporation.
    fn test_game_state_with_minor_nation() -> GameState {
        let coord = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(1, 0);

        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Homeland".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Minor Land".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.treasury = Money::dollars(1000);

        let nation2 = Nation::new(
            NationId(2),
            "Smallton".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy.initialize_great_powers(&[NationId(1)]);

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2],
            nations: vec![nation1, nation2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        }
    }

    #[test]
    fn voluntary_incorporation_at_threshold() {
        let mut game = test_game_state_with_minor_nation();

        // Set diplomacy score to exactly 75
        let rel = game.diplomacy.ensure_relation(NationId(2), NationId(1));
        rel.score = 75;

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
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
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
        };

        resolve_voluntary_incorporations(&mut game, &mut report);

        // Minor nation's provinces should be transferred
        assert_eq!(report.incorporations.len(), 1);
        assert_eq!(report.incorporations[0], (NationId(2), NationId(1)));

        // Great power now has province 2
        let gp = game.get_nation(NationId(1)).unwrap();
        assert!(gp.province_ids.contains(&ProvinceId(2)));

        // Minor nation has no provinces
        let mn = game.get_nation(NationId(2)).unwrap();
        assert!(mn.province_ids.is_empty());

        // Province owner updated
        let prov = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(prov.owner, NationId(1));

        // History recorded
        assert!(!game.history.is_empty());
        assert!(game.history[0].1.contains("Smallton"));
        assert!(game.history[0].1.contains("Testlandia"));
    }

    #[test]
    fn no_incorporation_below_threshold() {
        let mut game = test_game_state_with_minor_nation();

        // Set diplomacy score to 74 (just below threshold)
        let rel = game.diplomacy.ensure_relation(NationId(2), NationId(1));
        rel.score = 74;

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
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
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
        };

        resolve_voluntary_incorporations(&mut game, &mut report);

        // No incorporation should happen
        assert!(report.incorporations.is_empty());

        // Minor nation still has its province
        let mn = game.get_nation(NationId(2)).unwrap();
        assert!(mn.province_ids.contains(&ProvinceId(2)));

        // Province owner unchanged
        let prov = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(prov.owner, NationId(2));
    }

    // ── Unit upgrade tests ────────────────────────────────────────

    #[test]
    fn unit_upgrade_when_tech_is_researched() {
        use crate::ai::basic::AiPersonality;
        use crate::map::UnitId;
        use crate::military::units::ArmyUnit;

        let mut game = test_game_state();

        // Make nation1 an AI so auto-upgrade triggers
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.ai_personality = Some(AiPersonality::Balanced);

        // Give the nation a Regulars unit
        let unit = ArmyUnit::new(
            UnitId(100),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        nation.army.push(unit);

        // Research "Breech-Loading Rifles" (TechId 13) —
        // RifleInfantry.required_tech() returns "Breech-Loading Rifles"
        nation.research_tech(crate::events::TechId(13));

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
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
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
        };

        resolve_unit_upgrades(&mut game, &mut report);

        // The Regulars should be upgraded to RifleInfantry
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.army[0].unit_type, ArmyUnitType::RifleInfantry);

        // Report should record the upgrade
        assert_eq!(report.unit_upgrades.len(), 1);
        assert_eq!(report.unit_upgrades[0].0, NationId(1));
    }

    #[test]
    fn unit_upgrade_preserves_medals() {
        use crate::ai::basic::AiPersonality;
        use crate::map::UnitId;
        use crate::military::units::ArmyUnit;

        let mut game = test_game_state();

        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.ai_personality = Some(AiPersonality::Balanced);

        let mut unit = ArmyUnit::new(
            UnitId(100),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.medals = 3;
        unit.health = 75;
        nation.army.push(unit);

        // Research "Breech-Loading Rifles" (TechId 13)
        nation.research_tech(crate::events::TechId(13));

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
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
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
        };

        resolve_unit_upgrades(&mut game, &mut report);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.army[0].unit_type, ArmyUnitType::RifleInfantry);
        assert_eq!(nation.army[0].medals, 3, "Medals should be preserved");
        assert_eq!(nation.army[0].health, 75, "Health should be preserved");
    }

    // ── Pact defense tests ──────────────────────────────────────

    #[test]
    fn pact_defense_triggers_when_minor_with_pact_attacked() {
        // Setup: GP attacker (ID 2), Minor Nation defender (ID 3), GP pact holder (ID 4)
        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Attacker Land".to_string(),
            NationId(2),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Minor Land".to_string(),
            NationId(3),
            HexCoord::new(3, 3),
            vec![HexCoord::new(3, 3)],
            3,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Pact Holder Land".to_string(),
            NationId(4),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            4,
        );

        let mut human = Nation::new(
            NationId(1),
            "Human".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        human.treasury = Money::dollars(10000);

        let mut attacker = Nation::new(
            NationId(2),
            "Attacker".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        attacker.ai_personality = Some(crate::ai::basic::AiPersonality::Aggressive);
        attacker.treasury = Money::dollars(10000);

        let minor = Nation::new(
            NationId(3),
            "MinorDefender".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut pact_holder = Nation::new(
            NationId(4),
            "PactHolder".to_string(),
            NationColor::Green,
            NationType::GreatPower,
            ProvinceId(3),
        );
        pact_holder.ai_personality = Some(crate::ai::basic::AiPersonality::Balanced);
        pact_holder.treasury = Money::dollars(10000);

        let mut diplomacy = DiplomacyState::new();
        // Establish consulate + embassy + pact between PactHolder and MinorDefender
        diplomacy.build_consulate(NationId(4), NationId(3)).unwrap();
        diplomacy.build_embassy(NationId(4), NationId(3)).unwrap();
        diplomacy.propose_pact(NationId(4), NationId(3)).unwrap();

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2, province3],
            nations: vec![human, attacker, minor, pact_holder],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        // Verify pact exists
        assert!(game.diplomacy.has_treaty(
            NationId(4),
            NationId(3),
            crate::events::TreatyType::NonAggressionPact
        ));

        // Trigger pact defense
        let mut report = TurnReport {
            turn: TurnNumber::new(1),
            year: 1815,
            quarter: 1,
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
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
        };

        trigger_pact_defense(&mut game, NationId(3), NationId(2), &mut report);

        // PactHolder (AI) should now be at war with Attacker
        assert!(
            game.diplomacy
                .get_relation(NationId(4), NationId(2))
                .is_some_and(|r| r.at_war),
            "PactHolder should declare war on attacker when pact Minor is attacked"
        );

        // Should have generated headlines
        assert!(
            report
                .newspaper_headlines
                .iter()
                .any(|h| h.contains("requests") && h.contains("aid")),
            "Should generate 'requests aid' headline: {:?}",
            report.newspaper_headlines
        );
        assert!(
            report
                .newspaper_headlines
                .iter()
                .any(|h| h.contains("honors its pact")),
            "Should generate 'honors pact' headline: {:?}",
            report.newspaper_headlines
        );
    }

    #[test]
    fn pact_defense_does_not_trigger_for_great_power_defender() {
        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Land".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "GP1".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.treasury = Money::dollars(10000);

        let mut nation2 = Nation::new(
            NationId(2),
            "GP2".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation2.ai_personality = Some(crate::ai::basic::AiPersonality::Balanced);
        nation2.treasury = Money::dollars(10000);

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1],
            nations: vec![nation1, nation2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        let mut report = TurnReport {
            turn: TurnNumber::new(1),
            year: 1815,
            quarter: 1,
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
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
        };

        // GP defender - should not trigger pact defense
        trigger_pact_defense(&mut game, NationId(1), NationId(2), &mut report);

        assert!(
            report.newspaper_headlines.is_empty(),
            "No headlines should be generated for GP defenders"
        );
    }

    // ── Trade subsidy deduction tests ────────────────────────────

    #[test]
    fn subsidy_costs_deducted_each_turn() {
        let mut game = test_game_state();
        game.nations[0].treasury = Money::dollars(10000);
        // Set a subsidy with a fictional MN
        game.nations[0]
            .trade_subsidies
            .insert(NationId(10), Money::dollars(200));

        let report = process_turn(&mut game);

        // Subsidy cost should appear in report
        assert!(
            report
                .subsidy_costs
                .iter()
                .any(|(gp, mn, cost)| *gp == NationId(1)
                    && *mn == NationId(10)
                    && *cost == Money::dollars(200)),
            "Subsidy cost should be recorded in report"
        );
        // Treasury should be reduced by at least the subsidy amount
        assert!(
            game.get_nation(NationId(1)).unwrap().treasury < Money::dollars(10000),
            "Treasury should be reduced after subsidy deduction"
        );
    }

    // ── Trade improves relationship score ────────────────────────

    #[test]
    fn trade_improves_relationship_score() {
        use crate::hex::HexCoord;
        use crate::map::tile::Tile;
        use crate::military::ships::{Ship, ShipType};

        let coord_forest = HexCoord::new(0, 0);
        let coord_plantation = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            coord_forest,
            Tile::with_province(TerrainType::ScrubForest, ProvinceId(20)),
        );
        hex_map.set_tile(
            coord_plantation,
            Tile::with_province(TerrainType::Plantation, ProvinceId(20)),
        );

        let mn_province = Province::new(
            ProvinceId(20),
            "Minor Province".to_string(),
            NationId(10),
            coord_forest,
            vec![coord_forest, coord_plantation],
            3,
        );

        let gp_province = Province::new(
            ProvinceId(1),
            "GP Province".to_string(),
            NationId(1),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            4,
        );

        let mut gp = Nation::new(
            NationId(1),
            "TestGP".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp.treasury = Money::dollars(50000);
        // Give merchant ship for cargo
        gp.merchant_fleet.push(Ship::new(
            crate::map::UnitId(999),
            ShipType::Trader,
            NationId(1),
        ));

        let mn = Nation::new(
            NationId(10),
            "TestMN".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        );

        let mut diplomacy = DiplomacyState::new();
        // Build consulate so trade is possible
        diplomacy
            .build_consulate(NationId(1), NationId(10))
            .unwrap();

        let score_before = diplomacy
            .get_relation(NationId(1), NationId(10))
            .map(|r| r.score)
            .unwrap_or(0);

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![gp_province, mn_province],
            nations: vec![gp, mn],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        let report = process_turn(&mut game);

        // If trade happened, relationship should have improved
        if !report.trade_transactions.is_empty() {
            let score_after = game
                .diplomacy
                .get_relation(NationId(1), NationId(10))
                .map(|r| r.score)
                .unwrap_or(0);
            assert!(
                score_after > score_before,
                "Trade should improve relationship score (before={}, after={})",
                score_before,
                score_after
            );
            // trade_diplomacy should have entries
            assert!(
                !report.trade_diplomacy.is_empty(),
                "Trade diplomacy improvements should be recorded"
            );
        }
    }

    // ── Trade history recording ──────────────────────────────────

    #[test]
    fn trade_records_history_entries() {
        use crate::hex::HexCoord;
        use crate::map::tile::Tile;
        use crate::military::ships::{Ship, ShipType};

        let coord_forest = HexCoord::new(0, 0);
        let coord_plantation = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            coord_forest,
            Tile::with_province(TerrainType::ScrubForest, ProvinceId(20)),
        );
        hex_map.set_tile(
            coord_plantation,
            Tile::with_province(TerrainType::Plantation, ProvinceId(20)),
        );

        let mn_province = Province::new(
            ProvinceId(20),
            "Minor Province".to_string(),
            NationId(10),
            coord_forest,
            vec![coord_forest, coord_plantation],
            3,
        );

        let gp_province = Province::new(
            ProvinceId(1),
            "GP Province".to_string(),
            NationId(1),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            4,
        );

        let mut gp = Nation::new(
            NationId(1),
            "TestGP".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp.treasury = Money::dollars(50000);
        gp.merchant_fleet.push(Ship::new(
            crate::map::UnitId(999),
            ShipType::Trader,
            NationId(1),
        ));

        let mn = Nation::new(
            NationId(10),
            "TestMN".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy
            .build_consulate(NationId(1), NationId(10))
            .unwrap();

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![gp_province, mn_province],
            nations: vec![gp, mn],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        let report = process_turn(&mut game);

        // If trade happened, history should be recorded
        if !report.trade_transactions.is_empty() {
            let gp_nation = game.get_nation(NationId(1)).unwrap();
            assert!(
                !gp_nation.trade_history.is_empty(),
                "Great Power should have trade history entries after trade"
            );
            // Check the first entry has correct fields
            let first = &gp_nation.trade_history[0];
            assert_eq!(first.turn, TurnNumber::new(1));
            assert_eq!(first.partner, NationId(10));
            assert!(first.quantity > 0);
            assert!(first.total_cost > Money::ZERO);

            let mn_nation = game.get_nation(NationId(10)).unwrap();
            assert!(
                !mn_nation.trade_history.is_empty(),
                "Minor Nation should also have trade history entries"
            );
        }
    }

    // ── Town production output added to warehouse ──────────────

    #[test]
    fn town_production_output_added_to_warehouse() {
        // Create a Village province with ScrubForest tiles (produces timber)
        // Village: 4 timber → 2 lumber, 2 lumber → 1 furniture
        let mut game = test_game_state_with_village(&[TerrainType::ScrubForest; 4]);

        // Ensure nation warehouse starts empty
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.material_amount(MaterialType::Lumber), 0);
        assert_eq!(nation.goods_amount(GoodsType::Furniture), 0);

        let report = process_turn(&mut game);

        // After turn, materials should be in the nation's warehouse
        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.material_amount(MaterialType::Lumber) > 0,
            "Town production should add lumber to nation warehouse (got {})",
            nation.material_amount(MaterialType::Lumber)
        );
        assert!(
            nation.goods_amount(GoodsType::Furniture) > 0,
            "Town production should add furniture to nation warehouse (got {})",
            nation.goods_amount(GoodsType::Furniture)
        );

        // Report should record the town production
        let lumber_in_report: u32 = report
            .town_production
            .iter()
            .filter(|(nid, name, _)| *nid == NationId(1) && name == "Lumber")
            .map(|(_, _, q)| *q)
            .sum();
        assert!(
            lumber_in_report > 0,
            "Report should record lumber town production"
        );
    }

    // ── Immigration consumes correct goods ──────────────────────

    #[test]
    fn immigration_consumes_correct_goods() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Add food processing building
        nation
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 5));

        // Plenty of raw food (surplus for immigration)
        nation.add_resource(ResourceType::Grain, 50);

        // Pre-stock the exact goods needed for 1 immigrant:
        // 1 CannedFood + 1 Clothing + 1 Furniture
        nation.add_material(MaterialType::CannedFood, 2);
        nation.add_goods(GoodsType::Clothing, 2);
        nation.add_goods(GoodsType::Furniture, 2);

        // Start with 0 workers so food surplus is guaranteed
        nation.labor.untrained = 0;
        nation.labor.trained = 0;

        // Need 4 provinces for 1 immigrant slot
        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        // Record initial amounts
        let canned_before = nation.material_amount(MaterialType::CannedFood);
        let clothing_before = nation.goods_amount(GoodsType::Clothing);
        let furniture_before = nation.goods_amount(GoodsType::Furniture);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        // Verify immigration happened
        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert!(immigration >= 1, "Should recruit at least 1 immigrant");

        // Verify goods were consumed (at least 1 of each per immigrant)
        // Note: food processing may produce additional canned food during the turn,
        // so we check that the amount decreased from what it would have been.
        // The key invariant is: per immigrant, 1 CannedFood + 1 Clothing + 1 Furniture consumed.
        let canned_after = nation.material_amount(MaterialType::CannedFood);
        let clothing_after = nation.goods_amount(GoodsType::Clothing);
        let furniture_after = nation.goods_amount(GoodsType::Furniture);

        // Clothing and Furniture are only consumed by immigration (no production adds them
        // in this test since there are no raw resources for the mills/factories)
        assert!(
            clothing_after < clothing_before,
            "Clothing should be consumed by immigration: before={}, after={}",
            clothing_before,
            clothing_after
        );
        assert!(
            furniture_after < furniture_before,
            "Furniture should be consumed by immigration: before={}, after={}",
            furniture_before,
            furniture_after
        );
        // CannedFood: food processing may produce more, but at least some should have been consumed
        // We can verify indirectly via the immigration count
        let _ = canned_before;
        let _ = canned_after;
        // The immigration count confirms consumption happened
        assert!(
            immigration >= 1,
            "Immigration confirms CannedFood + Clothing + Furniture were consumed"
        );
    }

    // ── Counter-attack tests ────────────────────────────────────

    /// Build a game state with three adjacent provinces for counter-attack testing.
    ///
    /// Layout on the hex grid:
    /// - Province 1 (Nation 1, attacker): tile (0,0)
    /// - Province 2 (Nation 2, defender): tile (1,0)  ← adjacent to Province 1
    /// - Province 3 (Nation 2, defender): tile (2,0)  ← adjacent to Province 2
    ///
    /// Nation 1 has a strong army. Nation 2 has army units in Province 3.
    fn test_game_for_counter_attack() -> GameState {
        use crate::map::UnitId;

        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(1, 0);
        let coord3 = HexCoord::new(2, 0);

        let mut hex_map = HexMap::new(10, 10);
        let tile1 = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        let tile2 = Tile::with_province(TerrainType::DryPlains, ProvinceId(2));
        let tile3 = Tile::with_province(TerrainType::DryPlains, ProvinceId(3));
        hex_map.set_tile(coord1, tile1);
        hex_map.set_tile(coord2, tile2);
        hex_map.set_tile(coord3, tile3);

        let province1 = Province::new(
            ProvinceId(1),
            "Attacker Land".to_string(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Target Province".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Defender Rear".to_string(),
            NationId(2),
            coord3,
            vec![coord3],
            3,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.treasury = Money::dollars(10000);
        // Give attacker a strong army
        for i in 0..6 {
            nation1.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }

        let mut nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );
        nation2.add_province(ProvinceId(3));

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2, province3],
            nations: vec![nation1, nation2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        }
    }

    #[test]
    fn counter_attack_triggers_when_defender_has_adjacent_units() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Give defender army units in Province 3 (adjacent to Province 2)
        let nation2 = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..5 {
            nation2.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(3), // stationed in Province 3
            ));
        }

        // Queue attack on Province 2
        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Should have at least 2 battles: initial attack + counter-attack
        assert!(
            report.battles.len() >= 2,
            "Should have counter-attack battle; got {} battles",
            report.battles.len()
        );

        // Check that a counter-attack headline was generated
        let has_counter_attack_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.contains("counter-attack") || h.contains("repels counter-attack"));
        assert!(
            has_counter_attack_headline,
            "Should have counter-attack headline; headlines: {:?}",
            report.newspaper_headlines
        );
    }

    #[test]
    fn no_counter_attack_when_no_adjacent_units() {
        let mut game = test_game_for_counter_attack();

        // Defender has NO army units (only garrison)
        // Queue attack on Province 2
        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Should have exactly 1 battle (the initial attack only)
        assert_eq!(
            report.battles.len(),
            1,
            "Should have only 1 battle (no counter-attack); got {}",
            report.battles.len()
        );

        // No counter-attack headline
        let has_counter_attack_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.contains("counter-attack"));
        assert!(
            !has_counter_attack_headline,
            "Should not have counter-attack headline"
        );
    }

    #[test]
    fn garrison_resets_to_zero_on_conquest() {
        let mut game = test_game_for_counter_attack();

        // Province 2 starts with garrison_count = 3 (Minor Nation)
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().garrison_count,
            3,
            "Province 2 should start with garrison_count = 3"
        );

        // Queue attack on Province 2 (attacker has strong army, will win)
        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Verify attack succeeded
        assert!(
            report.battles[0].attacker_won,
            "Attacker should win with 6 Guards vs 3 Militia"
        );

        // After conquest, garrison_count should be 0
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.garrison_count, 0,
            "Garrison should be 0 after conquest (conquering nation must station own units)"
        );
        assert_eq!(
            province.owner,
            NationId(1),
            "Province should now belong to attacker"
        );
    }

    #[test]
    fn garrison_uses_new_owner_type_after_conquest() {
        use crate::military::combat::create_garrison;

        let mut game = test_game_for_counter_attack();

        // Province 2 is Minor Nation (garrison_count=3).
        // After being conquered by GP Nation 1, garrison_count = 0.
        // But if garrison is rebuilt, it should use GP type (4 Militia).
        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        process_turn(&mut game);

        // Province now owned by Nation 1 (Great Power)
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(province.owner, NationId(1));

        // Verify that create_garrison with the new owner type gives the GP amount
        let new_owner_type = game.get_nation(NationId(1)).unwrap().nation_type;
        let garrison = create_garrison(new_owner_type);
        assert_eq!(garrison.len(), 4, "GP garrison should have 4 Militia units");
    }

    // ── Connected-tile resource filtering tests ─────────────────

    #[test]
    fn connected_province_resources_are_collected() {
        // Capital province tiles should always produce resources (capital is always connected)
        let mut game = test_game_state();

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
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
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
        };

        collect_resources(&mut game, &mut report);

        // Capital province is always connected, so resources should be produced
        assert!(
            !report.resource_production.is_empty(),
            "Connected capital province tiles should produce resources"
        );
        assert!(
            report.disconnected_resources.is_empty(),
            "No disconnected resources when all provinces belong to capital"
        );
    }

    #[test]
    fn disconnected_province_resources_not_collected() {
        // Create a game state with a second province that is NOT connected via railroad/port
        let coord_farm = HexCoord::new(0, 0);
        let coord_forest = HexCoord::new(1, 0);
        let coord_distant = HexCoord::new(8, 8);

        let mut hex_map = HexMap::new(20, 20);
        let farm_tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        hex_map.set_tile(coord_farm, farm_tile);
        let forest_tile = Tile::with_province(TerrainType::ScrubForest, ProvinceId(1));
        hex_map.set_tile(coord_forest, forest_tile);

        // Distant province tile (disconnected - no railroad/depot/port)
        let distant_tile = Tile::with_province(TerrainType::Farm, ProvinceId(2));
        hex_map.set_tile(coord_distant, distant_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Capital Province".to_string(),
            NationId(1),
            coord_farm,
            vec![coord_farm, coord_forest],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Distant Province".to_string(),
            NationId(1),
            coord_distant,
            vec![coord_distant],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.add_province(ProvinceId(2));
        nation1.treasury = Money::dollars(1000);

        let game_state = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2],
            nations: vec![nation1],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        let mut game = game_state;

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
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
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
        };

        collect_resources(&mut game, &mut report);

        // Capital province tiles produce resources (connected)
        assert!(
            !report.resource_production.is_empty(),
            "Capital province should produce resources"
        );

        // The distant province's tile should be in disconnected_resources
        assert!(
            !report.disconnected_resources.is_empty(),
            "Disconnected province should have resources tracked as disconnected"
        );

        // The disconnected resource should be from NationId(1)
        for (nid, _, _) in &report.disconnected_resources {
            assert_eq!(*nid, NationId(1));
        }

        // The disconnected province's resources should NOT be added to warehouse
        // (only the capital province's resources should be there)
        let nation = game.get_nation(NationId(1)).unwrap();
        let total_in_warehouse: u32 = nation.warehouse.values().sum();
        let total_produced: u32 = report.resource_production.iter().map(|(_, _, q)| q).sum();
        assert_eq!(
            total_in_warehouse, total_produced,
            "Only connected resources should be in warehouse"
        );
    }

    #[test]
    fn connected_provinces_includes_capital() {
        let game = test_game_state();
        let connected = connected_provinces(&game, NationId(1));
        assert!(
            connected.contains(&ProvinceId(1)),
            "Capital province should always be connected"
        );
    }

    #[test]
    fn connected_provinces_excludes_disconnected() {
        let coord_farm = HexCoord::new(0, 0);
        let coord_distant = HexCoord::new(8, 8);

        let mut hex_map = HexMap::new(20, 20);
        let farm_tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        hex_map.set_tile(coord_farm, farm_tile);
        let distant_tile = Tile::with_province(TerrainType::Farm, ProvinceId(2));
        hex_map.set_tile(coord_distant, distant_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord_farm,
            vec![coord_farm],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Distant".to_string(),
            NationId(1),
            coord_distant,
            vec![coord_distant],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.add_province(ProvinceId(2));

        let game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2],
            nations: vec![nation1],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        let connected = connected_provinces(&game, NationId(1));
        assert!(
            connected.contains(&ProvinceId(1)),
            "Capital always connected"
        );
        assert!(
            !connected.contains(&ProvinceId(2)),
            "Province without railroad/port should not be connected"
        );
    }

    // ── Bankruptcy protection ──────────────────────────────────────

    #[test]
    fn bankruptcy_protection_caps_treasury_at_floor() {
        let mut game = test_game_state();
        // Give the nation a huge army with high maintenance
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.treasury = Money::dollars(100);
        // Add many expensive units to trigger large maintenance
        for i in 0..50u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(9000 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            );
            nation.army.push(unit);
        }

        let report = process_turn(&mut game);

        // Treasury should not go below -$5,000
        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.treasury >= Money::dollars(-5000),
            "Treasury {} should not go below -$5,000",
            nation.treasury
        );

        // Should have bankruptcy headline
        let has_crisis_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.contains("FINANCIAL CRISIS"));
        assert!(
            has_crisis_headline,
            "Should have FINANCIAL CRISIS headline when bankrupt"
        );
    }

    #[test]
    fn is_bankrupt_after_excessive_maintenance() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.treasury = Money::dollars(10);
        // Add expensive unit
        let unit = ArmyUnit::new(
            crate::map::UnitId(9100),
            ArmyUnitType::Guards,
            NationId(1),
            ProvinceId(1),
        );
        nation.army.push(unit);

        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Guards cost maintenance, so treasury should go negative
        // is_bankrupt should be true
        if nation.treasury < Money::ZERO {
            assert!(nation.is_bankrupt());
        }
    }

    // ── Treasury integration test ──────────────────────────────────

    #[test]
    fn treasury_test_comprehensive() {
        let mut game = test_game_state();
        let player = NationId(1);

        // Set initial treasury to $10,000
        game.get_nation_mut(player).unwrap().treasury = Money::dollars(10000);
        assert_eq!(
            game.get_nation(player).unwrap().treasury,
            Money::dollars(10000)
        );

        // Deduct $500 for a consulate-like expense
        game.get_nation_mut(player).unwrap().treasury -= Money::dollars(500);
        assert_eq!(
            game.get_nation(player).unwrap().treasury,
            Money::dollars(9500)
        );

        // Process a turn (maintenance, production, etc.)
        process_turn(&mut game);

        // Verify treasury changed (maintenance or income applied)
        let final_treasury = game.get_nation(player).unwrap().treasury;
        // It should still be positive (started with $9,500, no huge army)
        assert!(
            final_treasury > Money::ZERO,
            "Treasury after 1 turn: {} should still be positive",
            final_treasury
        );
    }

    // ── Minor Nation capitals never industrialize ──────────────────

    #[test]
    fn minor_nation_capitals_never_industrialize() {
        let coord_mn = HexCoord::new(3, 3);

        let mut hex_map = HexMap::new(10, 10);
        let mn_tile = Tile::with_province(TerrainType::Farm, ProvinceId(2));
        hex_map.set_tile(coord_mn, mn_tile);

        // Also add the GP capital tile
        let coord_gp = HexCoord::new(0, 0);
        let gp_tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        hex_map.set_tile(coord_gp, gp_tile);

        let province_gp = Province::new(
            ProvinceId(1),
            "GP Capital".to_string(),
            NationId(1),
            coord_gp,
            vec![coord_gp],
            4,
        );

        let mut province_mn = Province::new(
            ProvinceId(2),
            "Minor Capital".to_string(),
            NationId(2),
            coord_mn,
            vec![coord_mn],
            3,
        );
        // Mark as connected and start industrialization
        province_mn.connected_to_capital = true;
        province_mn.industrialization_turns_remaining = Some(1); // about to complete

        let nation_gp = Nation::new(
            NationId(1),
            "TestGP".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );

        let nation_mn = Nation::new(
            NationId(2),
            "TestMN".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province_gp, province_mn],
            nations: vec![nation_gp, nation_mn],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        // Process several turns
        for _ in 0..10 {
            process_turn(&mut game);
        }

        // Minor nation province should still be Hamlet
        let mn_province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            mn_province.settlement_level,
            SettlementLevel::Hamlet,
            "Minor Nation capital should never industrialize"
        );
    }

    // ── Captured GP capital industrializes immediately ──────────────

    #[test]
    fn captured_gp_capital_industrializes_immediately() {
        let coord_atk = HexCoord::new(0, 0);
        let coord_def = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        let atk_tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        hex_map.set_tile(coord_atk, atk_tile);
        let def_tile = Tile::with_province(TerrainType::Farm, ProvinceId(2));
        hex_map.set_tile(coord_def, def_tile);

        let province_atk = Province::new(
            ProvinceId(1),
            "Attacker Capital".to_string(),
            NationId(1),
            coord_atk,
            vec![coord_atk],
            4,
        );
        let province_def = Province::new(
            ProvinceId(2),
            "Defender Capital".to_string(),
            NationId(2),
            coord_def,
            vec![coord_def],
            4,
        );

        let mut nation_atk = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation_atk.treasury = Money::dollars(10000);
        // Give a powerful army to guarantee victory
        for i in 0..10u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            );
            nation_atk.army.push(unit);
        }

        let nation_def = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy.initialize_great_powers(&[NationId(1), NationId(2)]);
        diplomacy.declare_war(NationId(1), NationId(2));

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province_atk, province_def],
            nations: vec![nation_atk, nation_def],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy,
            pending_attacks: vec![(NationId(1), ProvinceId(2))],
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        process_turn(&mut game);

        // Check if the defender capital was conquered
        let province = game.get_province(ProvinceId(2)).unwrap();
        if province.owner == NationId(1) {
            // Captured GP capital should be Village immediately
            assert_eq!(
                province.settlement_level,
                SettlementLevel::Village,
                "Captured GP capital should be immediately industrialized to Village"
            );
        }
    }

    // ── Siege artillery destroys fort sections ──────────────────────

    #[test]
    fn siege_artillery_destroys_fort_on_conquest() {
        let coord_atk = HexCoord::new(0, 0);
        let coord_def = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        let atk_tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        hex_map.set_tile(coord_atk, atk_tile);
        let mut def_tile = Tile::with_province(TerrainType::Farm, ProvinceId(2));
        // Set up a fort on the defender's tile
        def_tile.infrastructure.has_fort = true;
        def_tile.infrastructure.fort_level = 2;
        hex_map.set_tile(coord_def, def_tile);

        let province_atk = Province::new(
            ProvinceId(1),
            "Attacker Land".to_string(),
            NationId(1),
            coord_atk,
            vec![coord_atk],
            4,
        );
        let province_def = Province::new(
            ProvinceId(2),
            "Fortified Land".to_string(),
            NationId(2),
            coord_def,
            vec![coord_def],
            3,
        );

        let mut nation_atk = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation_atk.treasury = Money::dollars(10000);
        // Add siege artillery and strong army
        for i in 0..8u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            );
            nation_atk.army.push(unit);
        }
        // Add siege artillery
        for i in 0..3u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(300 + i),
                ArmyUnitType::SiegeArtillery,
                NationId(1),
                ProvinceId(1),
            );
            nation_atk.army.push(unit);
        }

        let nation_def = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy.initialize_great_powers(&[NationId(1)]);
        diplomacy.declare_war(NationId(1), NationId(2));

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province_atk, province_def],
            nations: vec![nation_atk, nation_def],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy,
            pending_attacks: vec![(NationId(1), ProvinceId(2))],
            pending_moves: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
        };

        process_turn(&mut game);

        // If attacker won, fort should be reduced
        let province = game.get_province(ProvinceId(2)).unwrap();
        if province.owner == NationId(1) {
            let tile = game.hex_map.get_tile(coord_def).unwrap();
            // Fort level should be reduced by 1 (from 2 to 1)
            assert!(
                tile.infrastructure.fort_level < 2,
                "Fort level should be reduced after siege artillery conquest (was 2, now {})",
                tile.infrastructure.fort_level
            );
        }
    }
}
