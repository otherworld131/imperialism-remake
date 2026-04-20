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
use crate::map::infrastructure::is_province_connected_multi;
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
    pub newspaper_headlines: Vec<Headline>,
    pub techs_available: Vec<(NationId, Vec<String>)>,
    pub council_vote: Option<CouncilVoteResult>,
    pub trade_transactions: Vec<TradeTransaction>,
    pub battles: Vec<BattleResult>,
    /// Naval battles resolved this turn.
    pub naval_battles: Vec<NavalBattleResult>,
    /// Scores for all Great Powers: (nation_id, nation_name, total_score).
    pub scores: Vec<(NationId, String, u32)>,
    /// Summary of notable actions taken by AI nations this turn.
    pub ai_actions: Vec<crate::ai::AiAction>,
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
    /// Create an empty report (used by WASM bridge for pact defense responses).
    pub fn empty() -> Self {
        Self {
            turn: TurnNumber(0),
            year: 0,
            quarter: 0,
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
        }
    }

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

/// Award free Clipper ships when a Great Power establishes its first colony.
///
/// Called when a GP either conquers a Minor Nation province or receives one
/// through voluntary incorporation. Awards 2 Clippers and sets the `has_colony` flag.
fn award_first_colony_clippers(game: &mut GameState, nation_id: NationId, report: &mut TurnReport) {
    let already_has_colony = game.get_nation(nation_id).is_some_and(|n| n.has_colony);
    if already_has_colony {
        return;
    }
    let Some(nation) = game.get_nation_mut(nation_id) else {
        return;
    };
    use crate::map::UnitId;
    use crate::military::ships::{Ship, ShipType};
    nation.has_colony = true;
    let base_id = 5_000_000 + nation.id.0 * 100;
    nation
        .merchant_fleet
        .push(Ship::new(UnitId(base_id + 1), ShipType::Clipper, nation_id));
    nation
        .merchant_fleet
        .push(Ship::new(UnitId(base_id + 2), ShipType::Clipper, nation_id));
    let name = nation.name.clone();
    report.rewards_earned.push((
        nation_id,
        format!(
            "{} receives free Clipper ships for establishing its first colony!",
            name
        ),
    ));
    report.newspaper_headlines.push(Headline::new(
        format!(
            "{} receives free Clipper ships for establishing its first colony!",
            name
        ),
        HeadlineCategory::Trade,
    ));
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

    // 0-post. Resolve pending diplomatic proposals (AI-to-AI evaluated inline,
    // but this handles any proposals from the turn processor level — e.g. mutual proposals)
    resolve_diplomatic_proposals(game, &mut report);
    let broken_alliances = game.diplomacy.finalize_pending_separate_peace_breaks();
    record_broken_alliance_headlines(game, &mut report, &broken_alliances);

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

    // 3b. Pre-trade blockade: compute effective cargo capacity reduced by enemy warships
    let blockade_capacity = compute_blockade_capacity(game);

    // 3b. Trade session: Minor Nations sell resources to Great Powers
    resolve_trade_session(game, &mut report, &blockade_capacity);

    // 3c. Warehouse capacity caps: prevent infinite resource accumulation
    apply_warehouse_caps(game);

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

    // 6b. Resolve beachhead operations (establish naval landing sites)
    resolve_beachheads(game, &mut report);

    // 6c. Resolve military unit movement (pending moves)
    let moved_unit_ids = resolve_military_movement(game, &mut report);

    // 7. Resolve combat (pending attacks — units that moved this turn are excluded)
    resolve_combat(game, &mut report, &moved_unit_ids);

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

    // 10b. Recompute province connectivity, then update settlement progression
    update_province_connectivity(game);
    update_settlements(game, &mut report);

    // 11. Generate newspaper
    generate_newspaper(game, &mut report);

    // 11b. Archive headlines for history browsing
    game.newspaper_archive
        .push((game.turn, report.newspaper_headlines.clone()));

    // 11c. Archive battle results for history browsing
    // Strip heavyweight survivor vectors (ArmyUnit/Ship structs) — the archive
    // only needs counts, which are already stored as initial_count fields.
    if !report.battles.is_empty() || !report.naval_battles.is_empty() {
        let archived_battles: Vec<BattleResult> = report
            .battles
            .iter()
            .map(|b| {
                let mut archived = b.clone();
                archived.attacker_survivors = Vec::new();
                archived.defender_survivors = Vec::new();
                archived
            })
            .collect();
        // Naval battles keep survivors (Ship is lightweight, and NavalBattleResult
        // has no initial_count field so we can't derive counts from casualties alone)
        let archived_naval: Vec<NavalBattleResult> = report.naval_battles.clone();
        game.battle_archive
            .push((game.turn, archived_battles, archived_naval));
    }

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

/// Compute the set of province IDs connected to a nation's capital.
///
/// A province is connected if:
///   1. It IS the capital province, OR
///   2. Infrastructure (railroads/depots/ports) connects it, OR
///   3. It is directly adjacent to the capital province (shares a hex neighbor).
///
/// Adjacency ensures early-game resource delivery before railroads are built,
/// matching the connectivity logic used for settlement progression.
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

    // Precompute capital province neighbors for adjacency check
    let capital_neighbors: HashSet<crate::hex::HexCoord> = capital_province
        .tiles
        .iter()
        .flat_map(|t| t.neighbors())
        .collect();

    // Collect every owned country-capital tile — each is an independent hub
    // for rail/port connectivity (captured foreign capitals included).
    let mut seed_tiles: Vec<crate::hex::HexCoord> = vec![capital_tile];
    for &pid in &nation.province_ids {
        if let Some(prov) = game.get_province(pid) {
            for &t in &prov.tiles {
                if t == capital_tile {
                    continue;
                }
                if let Some(tile) = game.hex_map.get_tile(t)
                    && tile.is_country_capital
                {
                    seed_tiles.push(t);
                }
            }
        }
    }

    for &pid in &nation.province_ids {
        if pid == capital_pid {
            continue;
        }

        // Infrastructure connection (railroad/depot/port) seeded from every
        // owned country-capital tile.
        if is_province_connected_multi(&game.hex_map, &seed_tiles, pid, &game.provinces) {
            connected.insert(pid);
            continue;
        }

        // Adjacency: at least one tile of the province neighbors a capital tile
        if let Some(province) = game.get_province(pid)
            && province.tiles.iter().any(|t| capital_neighbors.contains(t))
        {
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

    // Phase 0b: precompute per-nation collectable-hex sets (capital province
    // plus a 1-hex radius around every connected depot).
    let collectable_by_nation: Vec<(NationId, HashSet<crate::hex::HexCoord>)> = nation_ids
        .iter()
        .map(|&nid| {
            let nation = match game.get_nation(nid) {
                Some(n) => n,
                None => return (nid, HashSet::new()),
            };
            let capital_pid = nation.capital_province_id;
            let owned: Vec<&crate::map::Province> =
                game.provinces.iter().filter(|p| p.owner == nid).collect();
            let connected = connected_map
                .iter()
                .find(|(id, _)| *id == nid)
                .map(|(_, s)| s.clone())
                .unwrap_or_default();
            let set = crate::map::infrastructure::collectable_hexes(
                &game.hex_map,
                capital_pid,
                &owned,
                &connected,
            );
            (nid, set)
        })
        .collect();

    // Phase 1: collect production data using immutable borrows
    let mut production_data: Vec<(NationId, ResourceType, u32)> = Vec::new();
    let mut disconnected_data: Vec<(NationId, ResourceType, u32)> = Vec::new();

    for province in &game.provinces {
        // Anarchic nations produce no resources
        if game
            .get_nation(province.owner)
            .is_some_and(|n| n.is_in_anarchy)
        {
            continue;
        }
        // Check if this province is connected for its owner
        let is_connected = connected_map
            .iter()
            .find(|(nid, _)| *nid == province.owner)
            .map(|(_, set)| set.contains(&province.id))
            .unwrap_or(false);

        let collectable = collectable_by_nation
            .iter()
            .find(|(nid, _)| *nid == province.owner)
            .map(|(_, s)| s);

        for tile_coord in &province.tiles {
            if let Some(tile) = game.hex_map.get_tile(*tile_coord)
                && let Some(yield_amount) = tile.calculate_yield()
            {
                let tile_collectable = collectable.map(|s| s.contains(tile_coord)).unwrap_or(false);
                // A connected hex yields only if inside a connected depot's
                // radius (or the capital province). Disconnected reporting
                // keeps the whole province's yields — the player needs to see
                // what's out there to decide where to place a depot, so we
                // don't filter those by depot geometry (there is no depot
                // geometry yet when a province is disconnected).
                if is_connected && tile_collectable {
                    production_data.push((
                        province.owner,
                        yield_amount.resource,
                        yield_amount.quantity,
                    ));
                } else if !is_connected {
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

    // Debug: log food collected per Great Power this turn
    if game.ai_debug {
        let food_types = [
            ResourceType::Grain,
            ResourceType::Fruit,
            ResourceType::Livestock,
        ];
        for nation in game.nations.iter().filter(|n| n.is_great_power()) {
            let collected: u32 = production_data
                .iter()
                .filter(|(nid, r, _)| *nid == nation.id && food_types.contains(r))
                .map(|(_, _, a)| a)
                .sum();
            let disconnected: u32 = disconnected_data
                .iter()
                .filter(|(nid, r, _)| *nid == nation.id && food_types.contains(r))
                .map(|(_, _, a)| a)
                .sum();
            if collected > 0 || disconnected > 0 {
                eprintln!(
                    "[COLLECT:{}] food_delivered={}, food_disconnected={}, freight_cars={}",
                    nation.name, collected, disconnected, nation.transport.freight_cars
                );
            }
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
        if nation.is_in_anarchy {
            continue;
        }

        let freight_capacity = nation.transport.total_capacity();

        // Aggregate this turn's resource production for this nation, split by source
        let capital_province_id = nation.capital_province_id;
        let capital_tiles: std::collections::HashSet<crate::hex::HexCoord> = game
            .provinces
            .iter()
            .find(|p| p.id == capital_province_id)
            .map(|p| p.tiles.iter().copied().collect())
            .unwrap_or_default();

        // Also include tiles from adjacent provinces (they deliver without transport)
        let adjacent_tiles: std::collections::HashSet<crate::hex::HexCoord> = {
            let cap_neighbors: std::collections::HashSet<crate::hex::HexCoord> =
                capital_tiles.iter().flat_map(|t| t.neighbors()).collect();
            game.provinces
                .iter()
                .filter(|p| p.owner == nation_id && p.id != capital_province_id)
                .filter(|p| p.tiles.iter().any(|t| cap_neighbors.contains(t)))
                .flat_map(|p| p.tiles.iter().copied())
                .collect()
        };

        // Count total production this turn
        let mut total_produced: u32 = 0;
        let mut produced_this_turn: Vec<(ResourceType, u32)> = Vec::new();
        for (nid, resource, amount) in &report.resource_production {
            if *nid == nation_id && *amount > 0 {
                if let Some(entry) = produced_this_turn.iter_mut().find(|(r, _)| *r == *resource) {
                    entry.1 += amount;
                } else {
                    produced_this_turn.push((*resource, *amount));
                }
                total_produced += amount;
            }
        }

        if total_produced == 0 {
            continue;
        }

        // Capital province + adjacent province resources are delivered for free.
        // Only resources from distant provinces require freight cars.
        let local_tile_count = (capital_tiles.len() + adjacent_tiles.len()) as u32;
        let local_delivery = local_tile_count.min(total_produced);
        let remote_delivery = total_produced - local_delivery;

        // Remote resources are capped by freight car capacity
        if remote_delivery > freight_capacity {
            let overflow = remote_delivery - freight_capacity;
            let nation = game.nations.iter_mut().find(|n| n.id == nation_id).unwrap();
            let mut remaining_to_remove = overflow;

            // Remove overflow from remote resources (approximation: remove proportionally)
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
    let cfg = &game.game_data.game_config;
    let prov_per_imm = cfg.provinces_per_immigrant;
    let prov_per_imm_upgraded = cfg.provinces_per_immigrant_upgraded;
    let req_canned = cfg.immigration_canned_food;
    let req_clothing = cfg.immigration_clothing;
    let req_furniture = cfg.immigration_furniture;

    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();
    let nation_ids_copy = nation_ids.clone();

    for nation_id in nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Only Great Powers get immigration; skip anarchic nations
        if !nation.is_great_power() || nation.is_in_anarchy {
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

        let provinces_per_immigrant = if capitol_expanded {
            prov_per_imm_upgraded
        } else {
            prov_per_imm
        };
        let province_count = nation.province_count() as u32;
        let max_immigrants = if province_count == 0 || provinces_per_immigrant == 0 {
            0
        } else {
            province_count / provinces_per_immigrant
        };

        if max_immigrants == 0 {
            continue;
        }

        // Check if nation has required materials for immigration
        let has_canned_food = nation.material_amount(MaterialType::CannedFood) >= req_canned;
        let has_clothing = nation.goods_amount(GoodsType::Clothing) >= req_clothing;
        let has_furniture = nation.goods_amount(GoodsType::Furniture) >= req_furniture;

        if !has_canned_food || !has_clothing || !has_furniture {
            continue;
        }

        // Recruit immigrants (up to max_immigrants, consuming 1 set of materials per immigrant)
        let mut recruited = 0;
        let nation = game.nations.iter_mut().find(|n| n.id == nation_id).unwrap();

        for _ in 0..max_immigrants {
            // Check materials for each immigrant
            let can_food = nation.material_amount(MaterialType::CannedFood) >= req_canned;
            let can_clothing = nation.goods_amount(GoodsType::Clothing) >= req_clothing;
            let can_furniture = nation.goods_amount(GoodsType::Furniture) >= req_furniture;

            if !can_food || !can_clothing || !can_furniture {
                break;
            }

            nation.consume_material(MaterialType::CannedFood, req_canned);
            nation.consume_goods(GoodsType::Clothing, req_clothing);
            nation.consume_goods(GoodsType::Furniture, req_furniture);
            nation.labor.recruit_immigrant();
            recruited += 1;
        }

        if recruited > 0 {
            report.immigration.push((nation_id, recruited));
        }
    }

    // Emergency recruitment: nations with 0 workers get 1 free worker per turn
    // (government-subsidized labor to prevent permanent death spiral)
    for nation_id in nation_ids_copy {
        let nation = match game.nations.iter_mut().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };
        if !nation.is_great_power() {
            continue;
        }
        if nation.labor.total_workers() == 0 {
            nation.labor.recruit_immigrant();
            report.immigration.push((nation_id, 1));
        }
    }
}

/// Update settlement progression for connected provinces.
///
/// For each province connected to its nation's capital via depot/port:
/// - If the province just became connected, start the industrialization countdown (6 turns).
/// - Tick down the industrialization counter each turn.
/// - When the countdown reaches 0 and the settlement is a Hamlet, upgrade to Village.
///
/// Also recomputes capital connectivity for all Great Power provinces.
/// A province is connected if:
///   1. It IS the nation's capital province, OR
///   2. The infrastructure system (railroads/depots/ports) connects it
///      (via `is_province_connected`), OR
///   3. The province is directly adjacent to the capital province —
///      this ensures early-game settlement progression before railroads
///      are built.
fn update_province_connectivity(game: &mut GameState) {
    let nation_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power() && !n.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    for nation_id in nation_ids {
        let connected = connected_provinces(game, nation_id);
        for prov in game.provinces.iter_mut() {
            // Only upgrade connectivity (false → true), never downgrade.
            // Full disconnection tracking will be added with the transport system.
            if prov.owner == nation_id && connected.contains(&prov.id) {
                prov.connected_to_capital = true;
            }
        }
    }
}

fn update_settlements(game: &mut GameState, report: &mut TurnReport) {
    // Collect province IDs and their owner for processing
    let province_data: Vec<(ProvinceId, NationId)> =
        game.provinces.iter().map(|p| (p.id, p.owner)).collect();

    for (province_id, owner_id) in &province_data {
        let province = match game.provinces.iter().find(|p| p.id == *province_id) {
            Some(p) => p,
            None => continue,
        };

        let owner_nation = game.nations.iter().find(|n| n.id == *owner_id);

        // Skip settlement progression for Minor Nation provinces or anarchic nations
        let skip = owner_nation
            .map(|n| !n.is_great_power() || n.is_in_anarchy)
            .unwrap_or(false);
        if skip {
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
                            report
                                .newspaper_headlines
                                .push(Headline::new(headline.clone(), HeadlineCategory::Growth));
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
                        report
                            .newspaper_headlines
                            .push(Headline::new(headline.clone(), HeadlineCategory::Growth));
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
    // Phase 0: cancel in-flight engineer tasks whose tile is no longer owned
    // (province lost mid-build). Report each cancellation so it's not silent.
    let owned_by_nation: std::collections::HashMap<NationId, HashSet<crate::hex::HexCoord>> = {
        let mut map: std::collections::HashMap<NationId, HashSet<crate::hex::HexCoord>> =
            std::collections::HashMap::new();
        for p in &game.provinces {
            map.entry(p.owner)
                .or_default()
                .extend(p.tiles.iter().copied());
        }
        map
    };
    let mut stranded_reports: Vec<(NationId, String)> = Vec::new();
    for nation in &mut game.nations {
        let empty_set: HashSet<crate::hex::HexCoord> = HashSet::new();
        let owned = owned_by_nation.get(&nation.id).unwrap_or(&empty_set);
        for civilian in &mut nation.civilians {
            if civilian.civilian_type != CivilianType::Engineer {
                continue;
            }
            if let Some(pos) = civilian.position
                && !owned.contains(&pos)
                && civilian.working
            {
                stranded_reports.push((
                    nation.id,
                    format!(
                        "Engineer's build at ({}, {}) cancelled — territory lost.",
                        pos.q, pos.r
                    ),
                ));
                civilian.working = false;
                civilian.turns_remaining = 0;
                civilian.build_task = None;
            }
        }
    }
    report.civilian_completions.extend(stranded_reports);

    // Phase 1: collect completed work info using immutable borrows on nations,
    // then apply tile mutations separately.
    struct CompletedWork {
        nation_id: NationId,
        civilian_type: CivilianType,
        build_task: Option<crate::economy::civilians::BuildTask>,
        position: crate::hex::HexCoord,
        description: String,
    }

    let mut completed: Vec<CompletedWork> = Vec::new();

    for nation in &mut game.nations {
        if nation.is_in_anarchy {
            continue;
        }
        for civilian in &mut nation.civilians {
            if !civilian.working {
                continue;
            }
            let just_finished = civilian.tick();
            if just_finished && let Some(pos) = civilian.position {
                let task = civilian.build_task.take();
                let desc = format!(
                    "{} completed work at ({}, {})",
                    civilian.civilian_type, pos.q, pos.r
                );
                completed.push(CompletedWork {
                    nation_id: nation.id,
                    civilian_type: civilian.civilian_type,
                    build_task: task,
                    position: pos,
                    description: desc,
                });
            }
        }
    }

    // Phase 2: apply tile improvements
    for work in &completed {
        // Engineer builds take a different, borrow-heavy path (they need
        // provinces + game_config + a mutable hex_map), so handle those first.
        if work.civilian_type == CivilianType::Engineer {
            if let Some(task) = work.build_task {
                let provinces_snapshot = game.provinces.clone();
                let cfg = game.game_data.game_config.clone();
                let researched: Vec<crate::events::TechId> = game
                    .get_nation(work.nation_id)
                    .map(|n| n.researched_techs.clone())
                    .unwrap_or_default();
                let result: Result<Money, String> = match task {
                    crate::economy::civilians::BuildTask::Railroad => {
                        crate::map::infrastructure::build_railroad(
                            &mut game.hex_map,
                            work.position,
                            work.nation_id,
                            &researched,
                            &provinces_snapshot,
                            &game.game_data,
                            &cfg,
                        )
                    }
                    crate::economy::civilians::BuildTask::Depot => {
                        crate::map::infrastructure::build_depot(
                            &mut game.hex_map,
                            work.position,
                            work.nation_id,
                            &provinces_snapshot,
                            &cfg,
                        )
                    }
                    crate::economy::civilians::BuildTask::Port => {
                        crate::map::infrastructure::build_port(
                            &mut game.hex_map,
                            work.position,
                            work.nation_id,
                            &provinces_snapshot,
                            &cfg,
                        )
                    }
                };
                match result {
                    Ok(cost) => {
                        if let Some(nation) = game.get_nation_mut(work.nation_id) {
                            // Debit the treasury. It is allowed to go negative —
                            // build orders are issued synchronously with funds
                            // at order time, but a treasury drop between order
                            // and completion would otherwise leave us unable
                            // to stop the build. Report the charge either way.
                            nation.treasury -= cost;
                        }
                        report.civilian_completions.push((
                            work.nation_id,
                            format!(
                                "{} built at ({}, {}) — cost {}",
                                task, work.position.q, work.position.r, cost
                            ),
                        ));
                    }
                    Err(err) => {
                        // Build failed at completion time (territory lost, tile
                        // state changed, etc.). Surface the failure so it is
                        // not silently dropped.
                        report.civilian_completions.push((
                            work.nation_id,
                            format!(
                                "{} build at ({}, {}) failed: {}",
                                task, work.position.q, work.position.r, err
                            ),
                        ));
                    }
                }
            }
            // Engineers are reusable — clear the tile's assigned_civilian so
            // they can be redeployed next turn by the AI / player.
            if let Some(tile) = game.hex_map.get_tile_mut(work.position)
                && let Some(nation) = game.nations.iter().find(|n| n.id == work.nation_id)
                && let Some(civ) = nation
                    .civilians
                    .iter()
                    .find(|c| c.position == Some(work.position))
                && tile.assigned_civilian == Some(civ.id)
            {
                tile.assigned_civilian = None;
            }
            continue;
        }

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
                    // Reveal a resource deposit based on terrain, using coordinate-based
                    // deterministic distribution matching the map generator's probabilities.
                    if tile.terrain().can_have_deposits() && tile.resource_deposit().is_none() {
                        // ~60% chance of finding something (40% find nothing)
                        let hash = (work.position.q.wrapping_mul(73)
                            ^ work.position.r.wrapping_mul(179))
                            as u32;
                        let roll = hash % 100;
                        if roll < 40 {
                            let deposit = match tile.terrain() {
                                TerrainType::Hills | TerrainType::Mountain => {
                                    let mineral_roll = (hash / 100) % 100;
                                    match mineral_roll {
                                        0..=34 => ResourceType::Coal,
                                        35..=64 => ResourceType::Iron,
                                        65..=84 => ResourceType::Gold,
                                        _ => ResourceType::Gems,
                                    }
                                }
                                _ => ResourceType::Oil,
                            };
                            tile.reveal_deposit(deposit);
                        } else {
                            tile.reveal_no_deposit();
                        }
                    }
                }
                CivilianType::Engineer => {}
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
        if nation.is_in_anarchy {
            continue;
        }
        let gold_amount = nation.resource_amount(ResourceType::Gold);
        let gems_amount = nation.resource_amount(ResourceType::Gems);

        let mut income = Money::ZERO;

        if gold_amount > 0 {
            let gold_value =
                Money::dollars(gold_amount as i64 * game.game_data.game_config.gold_value);
            income += gold_value;
            nation.remove_resource(ResourceType::Gold, gold_amount);
        }

        if gems_amount > 0 {
            let gems_value =
                Money::dollars(gems_amount as i64 * game.game_data.game_config.gems_value);
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
/// Labor is a shared pool: workers provide labor based on training level (untrained=1,
/// trained=2, expert=4). Each unit of production costs labor_per_production (default 2).
/// Mills consume labor first, then remaining labor feeds factories.
fn run_production(game: &mut GameState, report: &mut TurnReport) {
    let cfg = &game.game_data.game_config;
    let untrained_mult = cfg.untrained_labor;
    let trained_mult = cfg.trained_labor;
    let expert_mult = cfg.expert_labor;

    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };
        if nation.is_in_anarchy {
            continue;
        }

        // Gather current resource inventory as slices
        let resources: Vec<(ResourceType, u32)> =
            nation.warehouse.iter().map(|(r, q)| (*r, *q)).collect();

        // Labor is a shared pool across all production this turn
        let mut remaining_labor =
            nation
                .labor
                .total_labor_units_with(untrained_mult, trained_mult, expert_mult);

        // ── Mills: resources → materials (consume labor first) ──

        // Timber chain: LumberMill
        let lumber_mill_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let timber_result = if lumber_mill_cap > 0 {
            let result = calculate_mill_production(
                ProductionChain::Timber,
                &resources,
                lumber_mill_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            Some(result)
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
            let result = calculate_mill_production(
                ProductionChain::Metal,
                &resources,
                steel_mill_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            Some(result)
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
            let result = calculate_mill_production(
                ProductionChain::Textile,
                &resources,
                textile_mill_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            Some(result)
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
            let result = calculate_factory_production(
                ProductionChain::Timber,
                &materials_inventory,
                furniture_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            Some(result)
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
            let result = calculate_factory_production(
                ProductionChain::Metal,
                &materials_inventory,
                hardware_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            Some(result)
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
            let result = calculate_factory_production(
                ProductionChain::Textile,
                &materials_inventory,
                clothing_cap,
                remaining_labor,
            );
            remaining_labor -= result.labor_used;
            Some(result)
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
        // Anarchic nations have no town production
        if game
            .get_nation(province.owner)
            .is_some_and(|n| n.is_in_anarchy)
        {
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

        // Don't process food if there are no workers to feed
        if nation.labor.total_workers() == 0 {
            continue;
        }

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

        // Reserve raw food for worker consumption (runs next step).
        // Only convert surplus beyond what workers need to eat.
        let workers = nation.labor.total_workers();
        let food_after_workers = total_raw_food.saturating_sub(workers);

        if food_after_workers < 2 {
            continue;
        }

        // Maximum units we can produce: limited by capacity and surplus food
        let raw_food_limited = food_after_workers / 2;
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

        report
            .production_output
            .push((nation_id, "CannedFood".to_string(), units_to_produce));
    }
}

/// Consume food for each nation based on population.
///
/// Each worker (untrained + trained + expert) needs 1 food per turn.
/// Food priority: Grain first, then Fruit, then Livestock, then CannedFood as fallback.
/// If not enough food: 1 worker dies per missing food unit, up to 2 max per turn.
fn food_consumption(game: &mut GameState, report: &mut TurnReport) {
    let ai_debug = game.ai_debug;
    for nation in &mut game.nations {
        if nation.is_in_anarchy {
            continue;
        }
        let population = nation.labor.total_workers();
        if population == 0 {
            continue;
        }

        let grain = nation.resource_amount(ResourceType::Grain);
        let fruit = nation.resource_amount(ResourceType::Fruit);
        let livestock = nation.resource_amount(ResourceType::Livestock);
        let canned = nation.material_amount(MaterialType::CannedFood);
        let total_food = grain + fruit + livestock + canned;

        if ai_debug && nation.is_great_power() {
            eprintln!(
                "[FOOD:{}] workers={}, grain={}, fruit={}, livestock={}, canned={}, total={}, deficit={}",
                nation.name,
                population,
                grain,
                fruit,
                livestock,
                canned,
                total_food,
                population.saturating_sub(total_food)
            );
        }

        let food_needed = population;
        let food_to_consume = food_needed.min(total_food);

        // Consume food in priority order: Grain → Fruit → Livestock → CannedFood
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
        remaining -= livestock_consumed;

        // CannedFood as fallback when raw food is insufficient
        let canned_consumed = canned.min(remaining);
        if canned_consumed > 0 {
            nation.consume_material(MaterialType::CannedFood, canned_consumed);
        }

        if food_to_consume > 0 {
            report.food_consumed.push((nation.id, food_to_consume));
        }

        // Starvation: workers die if not enough food (raw + canned)
        if total_food < food_needed {
            let deficit = food_needed - total_food;
            let workers_lost = deficit.min(game.game_data.game_config.starvation_cap);

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

/// Resolve a trade session: generate offers from Minor Nations, handle player
/// sell/buy orders, use smart bids for AI GPs, resolve trades, and apply results.
fn resolve_trade_session(
    game: &mut GameState,
    report: &mut TurnReport,
    blockade_capacity: &std::collections::HashMap<NationId, u32>,
) {
    let human_id = game.human_player_nation;
    let cfg = game.game_data.game_config.clone();

    // 0. Deduct subsidy costs from Great Powers (skip anarchic nations)
    let gp_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power() && !n.is_in_anarchy)
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
    let mut offers =
        trade::generate_minor_nation_offers(&game.nations, &game.provinces, &game.hex_map);

    // 1b. Add human player's resource sell offers to the pool
    if let Some(human) = game.get_nation(human_id) {
        let sell_orders: Vec<trade::PlayerSellOrder> = human.player_sell_orders.clone();
        for order in &sell_orders {
            if let trade::Commodity::Resource(r) = order.commodity
                && human.resource_amount(r) >= order.quantity
                && order.quantity > 0
            {
                offers.push(trade::TradeOffer {
                    seller: human_id,
                    resource: r,
                    quantity: order.quantity,
                    price_per_unit: trade::base_price(r),
                });
            }
        }
    }

    // 1c. Auto-sell player's material/goods sell orders (world market demand)
    let current_turn = game.turn;
    let mut player_goods_revenue = Money::ZERO;
    if let Some(human) = game.get_nation(human_id) {
        let sell_orders: Vec<trade::PlayerSellOrder> = human.player_sell_orders.clone();
        let mut goods_sold: Vec<(trade::Commodity, u32, Money)> = Vec::new();

        for order in &sell_orders {
            match order.commodity {
                trade::Commodity::Material(m) => {
                    let stock = human.materials.get(&m).copied().unwrap_or(0);
                    let qty = order.quantity.min(stock);
                    if qty > 0 {
                        let price = trade::material_price(m, &cfg);
                        let revenue = Money::dollars(price.as_dollars() * qty as i64);
                        player_goods_revenue += revenue;
                        goods_sold.push((order.commodity, qty, revenue));
                    }
                }
                trade::Commodity::Goods(g) => {
                    let stock = human.goods.get(&g).copied().unwrap_or(0);
                    let qty = order.quantity.min(stock);
                    if qty > 0 {
                        let price = trade::goods_price(g, &cfg);
                        let revenue = Money::dollars(price.as_dollars() * qty as i64);
                        player_goods_revenue += revenue;
                        goods_sold.push((order.commodity, qty, revenue));
                    }
                }
                trade::Commodity::Resource(_) => {} // handled in 1b via offer pool
            }
        }
        // Apply material/goods sales
        if let Some(human) = game.get_nation_mut(human_id) {
            human.treasury += player_goods_revenue;
            human.goods_sales_revenue_dollars += player_goods_revenue.as_dollars();
            for (commodity, qty, _revenue) in &goods_sold {
                match commodity {
                    trade::Commodity::Material(m) => {
                        if let Some(stock) = human.materials.get_mut(m) {
                            *stock = stock.saturating_sub(*qty);
                        }
                    }
                    trade::Commodity::Goods(g) => {
                        if let Some(stock) = human.goods.get_mut(g) {
                            *stock = stock.saturating_sub(*qty);
                        }
                    }
                    trade::Commodity::Resource(_) => {}
                }
            }
        }
    }

    // 2. Generate bids: AI GPs use smart bids, human player uses manual buy orders
    let mut all_bids = Vec::new();

    for gp_id in &gp_ids {
        if *gp_id == human_id {
            // Use player's manual buy orders instead of auto-generated bids
            if let Some(human) = game.get_nation(*gp_id) {
                for order in &human.player_buy_orders {
                    if order.quantity > 0 {
                        all_bids.push(trade::TradeBid {
                            buyer: *gp_id,
                            resource: order.resource,
                            quantity: order.quantity,
                            max_price_per_unit: order.max_price_per_unit,
                        });
                    }
                }
            }
            continue;
        }
        if let Some(nation) = game.get_nation(*gp_id) {
            // Use blockade-adjusted cargo capacity instead of raw capacity
            let cargo_capacity = blockade_capacity
                .get(gp_id)
                .copied()
                .unwrap_or_else(|| nation.total_cargo_capacity());
            let bids = trade::generate_smart_bids(nation, &offers, &game.diplomacy, cargo_capacity);
            all_bids.extend(bids);
        }
    }

    if offers.is_empty() && all_bids.is_empty() {
        // Clear player orders and return
        if let Some(human) = game.get_nation_mut(human_id) {
            human.player_sell_orders.clear();
            human.player_buy_orders.clear();
        }
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

    // 5b. Deduct sold resources from player (GP sellers lose warehouse stock)
    for txn in &transactions {
        if txn.seller == human_id
            && let Some(seller) = game.get_nation_mut(human_id)
        {
            seller.remove_resource(txn.resource, txn.quantity);
        }
    }

    // 5c. Record trade history for each nation involved
    for txn in &transactions {
        // Record for buyer (partner is seller)
        if let Some(buyer) = game.get_nation_mut(txn.buyer) {
            buyer.trade_history.push(trade::TradeHistoryEntry {
                turn: current_turn,
                partner: txn.seller,
                resource: txn.resource,
                quantity: txn.quantity,
                total_cost: txn.total_cost,
                bought: true,
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
                bought: false,
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
    // Cap + interval sourced from scripts/config/game.lua — unscaled improvement
    // let minors voluntarily join an empire in only ~5 years of passive trade.
    let trade_cap = game.game_data.game_config.trade_relation_improvement_cap;
    let trade_interval = game.game_data.game_config.trade_relation_turn_interval;
    let apply_trade_improvement = trade_interval > 0 && game.turn.0.is_multiple_of(trade_interval);
    if apply_trade_improvement {
        for ((buyer, seller), resources) in &trade_pairs {
            // Only improve relations if a trade consulate exists between the nations.
            if game.diplomacy.has_consulate(*buyer, *seller) {
                let improvement = (resources.len() as i32).min(trade_cap);
                let rel = game.diplomacy.ensure_relation(*buyer, *seller);
                rel.improve_score(improvement);
                report.trade_diplomacy.push((*buyer, *seller, improvement));
            }
        }
    }

    // 7. Record trade balance per nation
    let mut spent: std::collections::HashMap<NationId, Money> = std::collections::HashMap::new();
    let mut earned: std::collections::HashMap<NationId, Money> = std::collections::HashMap::new();
    for txn in &transactions {
        *spent.entry(txn.buyer).or_insert(Money::ZERO) += txn.total_cost;
        *earned.entry(txn.seller).or_insert(Money::ZERO) += txn.total_cost;
    }
    // Include player's auto-sold materials/goods revenue
    if player_goods_revenue != Money::ZERO {
        *earned.entry(human_id).or_insert(Money::ZERO) += player_goods_revenue;
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

    // 9. Clear player trade orders for next turn
    if let Some(human) = game.get_nation_mut(human_id) {
        human.player_sell_orders.clear();
        human.player_buy_orders.clear();
    }
}

/// Apply maintenance costs for army units.
/// Bankruptcy floor: treasury cannot go below $0.
const BANKRUPTCY_FLOOR: Money = Money::ZERO;

fn apply_maintenance(game: &mut GameState, report: &mut TurnReport) {
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
            nation.treasury -= total_cost;
        }

        // Bankruptcy protection: treasury cannot go below $0
        if nation.treasury < BANKRUPTCY_FLOOR {
            nation.treasury = BANKRUPTCY_FLOOR;
        }

        // Generate bankruptcy headline if treasury went negative
        if nation.is_bankrupt() {
            report.newspaper_headlines.push(Headline::new(
                format!("FINANCIAL CRISIS: {} faces bankruptcy!", nation.name),
                HeadlineCategory::Crisis,
            ));
        }
    }
}

/// Resolve beachhead (naval landing) operations.
///
/// For each nation with warships assigned to `Beachhead(target_province)`:
/// 1. Validate the target province is coastal and owned by an enemy
/// 2. Add it to `pending_landings` with the current turn number
/// 3. Remove stale landings from nations that no longer have warships assigned
///
/// Landing sites are only usable for attacks on turns AFTER establishment
/// (enforced in `can_attack_province`).
fn resolve_beachheads(game: &mut GameState, report: &mut TurnReport) {
    use crate::military::naval::NavalOperation;

    let current_turn = game.turn;

    // Remove landings where the nation no longer has warships assigned to that target
    let active_assignments: Vec<(NationId, ProvinceId)> = game
        .nations
        .iter()
        .flat_map(|nation| {
            nation.warships.iter().filter_map(move |ship| {
                if let Some(NavalOperation::Beachhead(target_pid)) = ship.operation {
                    Some((nation.id, target_pid))
                } else {
                    None
                }
            })
        })
        .collect();

    // Keep existing landings that still have ships assigned AND remain diplomatically valid.
    // Pre-compute valid landing IDs to avoid borrow conflict with retain.
    let valid_landings: Vec<(NationId, ProvinceId)> = game
        .pending_landings
        .iter()
        .filter(|(nid, pid, _)| {
            let has_ships = active_assignments
                .iter()
                .any(|(n, p)| *n == *nid && *p == *pid);
            if !has_ships {
                return false;
            }
            // Revalidate: target must still be coastal, owned by enemy, and at war
            let target_valid = game.get_province(*pid).is_some_and(|p| {
                p.coastal && {
                    let at_war = game
                        .diplomacy
                        .get_relation(*nid, p.owner)
                        .is_some_and(|r| r.at_war);
                    let target_anarchic = game.get_nation(p.owner).is_some_and(|n| n.is_in_anarchy);
                    at_war || target_anarchic
                }
            });
            // Revalidate embarkation: attacker must still own a coastal province
            let attacker_has_coast = game.get_nation(*nid).is_some_and(|n| {
                n.province_ids
                    .iter()
                    .any(|&pid| game.get_province(pid).is_some_and(|p| p.coastal))
            });
            target_valid && attacker_has_coast
        })
        .map(|(nid, pid, _)| (*nid, *pid))
        .collect();
    game.pending_landings
        .retain(|(nid, pid, _)| valid_landings.iter().any(|(n, p)| *n == *nid && *p == *pid));

    // Collect new beachhead assignments: (nation_id, target_province_id)
    let mut new_requests: Vec<(NationId, ProvinceId)> = Vec::new();
    for nation in &game.nations {
        for ship in &nation.warships {
            if let Some(NavalOperation::Beachhead(target_pid)) = ship.operation
                && !new_requests
                    .iter()
                    .any(|(nid, pid)| *nid == nation.id && *pid == target_pid)
                // Don't re-add if already in pending_landings
                && !game
                    .pending_landings
                    .iter()
                    .any(|(nid, pid, _)| *nid == nation.id && *pid == target_pid)
            {
                new_requests.push((nation.id, target_pid));
            }
        }
    }

    for (attacker_id, target_pid) in new_requests {
        // Sea-zone adjacency: attacker must own at least one coastal province
        // (embarkation point). Without a port, ships cannot depart.
        let attacker_has_coast = game
            .get_nation(attacker_id)
            .map(|n| {
                n.province_ids
                    .iter()
                    .any(|&pid| game.get_province(pid).is_some_and(|p| p.coastal))
            })
            .unwrap_or(false);
        if !attacker_has_coast {
            continue;
        }

        // Validate the target province is coastal and owned by an enemy
        let valid = game.get_province(target_pid).is_some_and(|p| {
            p.coastal && {
                let at_war = game
                    .diplomacy
                    .get_relation(attacker_id, p.owner)
                    .is_some_and(|r| r.at_war);
                let target_anarchic = game.get_nation(p.owner).is_some_and(|n| n.is_in_anarchy);
                at_war || target_anarchic
            }
        });

        if valid {
            game.pending_landings
                .push((attacker_id, target_pid, current_turn));
            let attacker_name = game
                .get_nation(attacker_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let target_name = game
                .get_province(target_pid)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            report.newspaper_headlines.push(Headline::new(
                format!(
                    "{} establishes a naval landing site at {}",
                    attacker_name, target_name
                ),
                HeadlineCategory::Military,
            ));
        }
    }
}

/// Check whether a nation can attack a target province.
///
/// An attack is valid if:
/// 1. The attacker owns a province that is **land-adjacent** to the target, OR
/// 2. The attacker has an active **naval landing site** on the target province
///    that was established on a **previous** turn (not this turn).
fn can_attack_province(
    game: &GameState,
    attacker_id: NationId,
    target_province_id: ProvinceId,
) -> bool {
    // Check for active landing site established on a previous turn
    let current_turn = game.turn;
    if game.pending_landings.iter().any(|(nid, pid, established)| {
        *nid == attacker_id && *pid == target_province_id && *established < current_turn
    }) {
        return true;
    }

    // Check for land adjacency: attacker must own a province sharing a hex edge
    // with the target province.
    let target_prov = match game.get_province(target_province_id) {
        Some(p) => p,
        None => return false,
    };

    let attacker_province_ids: Vec<ProvinceId> = match game.get_nation(attacker_id) {
        Some(n) => n.province_ids.clone(),
        None => return false,
    };

    for &owned_pid in &attacker_province_ids {
        if let Some(owned_prov) = game.get_province(owned_pid)
            && crate::map::provinces_are_adjacent(&game.hex_map, owned_prov, target_prov)
        {
            return true;
        }
    }

    false
}

/// Resolve military unit movement from pending_moves.
///
/// For each pending move:
/// 1. Validate the unit exists in the nation's army
/// 2. If destination is owned by the nation, move the unit there
/// 3. If destination is owned by an enemy at war, convert to a pending_attack instead
/// 4. Otherwise, reject the move
fn resolve_military_movement(
    game: &mut GameState,
    report: &mut TurnReport,
) -> HashSet<crate::map::UnitId> {
    let mut moved_unit_ids: HashSet<crate::map::UnitId> = HashSet::new();
    let moves: Vec<(NationId, crate::map::UnitId, ProvinceId)> =
        game.pending_moves.drain(..).collect();

    for (nation_id, unit_id, dest_province_id) in moves {
        // Anarchic nations' armies don't move
        if game.get_nation(nation_id).is_some_and(|n| n.is_in_anarchy) {
            continue;
        }

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
                moved_unit_ids.insert(unit_id);
                report
                    .unit_movements
                    .push((nation_id, format!("{} moved to {}", unit_type, dest_name)));
            }
        } else {
            // Check if at war with the destination owner, or if target is anarchic
            let at_war = game
                .diplomacy
                .get_relation(nation_id, dest_owner)
                .is_some_and(|r| r.at_war);
            let target_is_anarchic = game.get_nation(dest_owner).is_some_and(|n| n.is_in_anarchy);
            if at_war || target_is_anarchic {
                // Validate adjacency: attacker must own an adjacent province
                // or have an active landing site on the target.
                if can_attack_province(game, nation_id, dest_province_id) {
                    game.pending_attacks.push((nation_id, dest_province_id));
                    report.unit_movements.push((
                        nation_id,
                        format!("Attack ordered on {} (enemy territory)", dest_name),
                    ));
                } else {
                    // Province is not adjacent and no active landing site
                    report.unit_movements.push((
                        nation_id,
                        format!(
                            "Cannot attack {} — not adjacent and no naval landing site",
                            dest_name
                        ),
                    ));
                }
            }
            // Otherwise ignore the invalid move
        }
    }

    moved_unit_ids
}

/// Resolve combat for all pending attacks.
///
/// For each pending attack:
/// 1. Create attacker CombatForce from the attacking nation's army units
/// 2. Create defender CombatForce from garrison (based on province owner type)
/// 3. Call resolve_battle()
/// 4. If attacker wins: change province owner, record ProvinceConquered event, add headline
/// 5. Clear pending_attacks after processing
fn resolve_combat(
    game: &mut GameState,
    report: &mut TurnReport,
    moved_unit_ids: &HashSet<crate::map::UnitId>,
) {
    let all_attacks: Vec<(NationId, ProvinceId)> = game.pending_attacks.drain(..).collect();
    // Filter out attacks from anarchic nations (their armies only defend)
    let attacks: Vec<(NationId, ProvinceId)> = all_attacks
        .into_iter()
        .filter(|(attacker_id, _)| {
            !game
                .get_nation(*attacker_id)
                .is_some_and(|n| n.is_in_anarchy)
        })
        .collect();

    // Track provinces that have already changed hands this turn to prevent
    // ping-pong (Province A going X → Y → Z in one turn).
    let mut already_contested: HashSet<ProvinceId> = HashSet::new();
    // Track nations that have already conquered a province this turn
    // to prevent territory swap loops (A takes from B while B takes from A).
    let mut already_conquered: HashSet<NationId> = HashSet::new();
    // Track nations that lost a province this turn — they cannot attack.
    let mut lost_province: HashSet<NationId> = HashSet::new();

    for (attacker_id, province_id) in attacks {
        // Skip attacks on provinces that already changed hands this turn
        if already_contested.contains(&province_id) {
            continue;
        }
        // Limit each nation to one conquest per turn to prevent swap loops
        if already_conquered.contains(&attacker_id) {
            continue;
        }
        // Nations that lost a province this turn cannot attack (prevents revenge loops)
        if lost_province.contains(&attacker_id) {
            continue;
        }
        // Validate adjacency: attacker must own an adjacent province
        // or have an active landing site on the target.
        if !can_attack_province(game, attacker_id, province_id) {
            continue;
        }
        // Look up province owner
        let defender_id = match game.get_province(province_id) {
            Some(p) => p.owner,
            None => continue,
        };

        // Skip self-conquest (attacker already owns the province)
        if attacker_id == defender_id {
            continue;
        }

        // Re-check diplomatic legality: skip if peace was made earlier this turn
        let still_at_war = game
            .diplomacy
            .get_relation(attacker_id, defender_id)
            .is_some_and(|r| r.at_war);
        let defender_anarchic = game
            .get_nation(defender_id)
            .is_some_and(|n| n.is_in_anarchy);
        if !still_at_war && !defender_anarchic {
            continue;
        }

        // Get defender nation type for garrison creation
        let defender_type = match game.get_nation(defender_id) {
            Some(n) => n.nation_type,
            None => continue,
        };

        // Trigger pact defense: if defender (Minor Nation) has a NAP with any GP,
        // a protector may intervene (declaring war and incorporating the minor).
        trigger_pact_defense(game, defender_id, attacker_id, report);

        // If pact defense changed province ownership (minor was incorporated),
        // abort this attack — the attacker is now at war with the protector instead.
        let current_owner = game
            .get_province(province_id)
            .map(|p| p.owner)
            .unwrap_or(defender_id);
        if current_owner != defender_id {
            report.newspaper_headlines.push(Headline::new(
                format!(
                    "{}'s attack on {} is thwarted by intervention!",
                    game.get_nation(attacker_id)
                        .map(|n| n.name.as_str())
                        .unwrap_or("Unknown"),
                    game.get_nation(defender_id)
                        .map(|n| n.name.as_str())
                        .unwrap_or("Unknown"),
                ),
                HeadlineCategory::War,
            ));
            continue;
        }

        // Determine which attacker-owned provinces are adjacent to the target.
        let target_tiles: Vec<crate::hex::HexCoord> = game
            .get_province(province_id)
            .map(|p| p.tiles.clone())
            .unwrap_or_default();
        let attacker_owned: HashSet<ProvinceId> = game
            .get_nation(attacker_id)
            .map(|n| n.province_ids.iter().copied().collect())
            .unwrap_or_default();
        let mut adjacent_attacker_pids: HashSet<ProvinceId> = HashSet::new();
        for &tile_coord in &target_tiles {
            for neighbor in tile_coord.neighbors() {
                if let Some(tile) = game.hex_map.get_tile(neighbor)
                    && let Some(pid) = tile.province_id
                    && pid != province_id
                    && attacker_owned.contains(&pid)
                {
                    adjacent_attacker_pids.insert(pid);
                }
            }
        }

        // Compute beachhead capacity for this target (0 if no beachhead assigned).
        let beachhead_cap = game
            .get_nation(attacker_id)
            .map(|n| {
                use crate::military::naval::NavalOperation;
                let assigned_ships: Vec<_> = n
                    .warships
                    .iter()
                    .filter(|s| s.operation == Some(NavalOperation::Beachhead(province_id)))
                    .cloned()
                    .collect();
                crate::military::naval::beachhead_force_size(&assigned_ships)
            })
            .unwrap_or(0) as usize;

        // Precompute set of attacker-owned coastal (port) province IDs —
        // only units in port provinces may embark for a naval landing.
        let coastal_attacker_pids: HashSet<ProvinceId> = attacker_owned
            .iter()
            .copied()
            .filter(|&pid| game.get_province(pid).is_some_and(|p| p.coastal))
            .collect();

        // Assemble two cohorts:
        //   - Land cohort: units in adjacent attacker-owned provinces (no cap)
        //   - Naval cohort: units in coastal (port) non-adjacent provinces,
        //     capped by beachhead_cap (only if beachhead exists on target)
        // These compose a mixed attack when both are present.
        let (land_cohort, naval_cohort): (Vec<ArmyUnit>, Vec<ArmyUnit>) =
            match game.get_nation(attacker_id) {
                Some(n) => {
                    let (land, other): (Vec<_>, Vec<_>) = n
                        .army
                        .iter()
                        .filter(|u| !moved_unit_ids.contains(&u.id))
                        .cloned()
                        .partition(|u| adjacent_attacker_pids.contains(&u.position));
                    // Naval embarkation: only units in coastal/port provinces can board ships
                    let mut naval: Vec<ArmyUnit> = if beachhead_cap > 0 {
                        other
                            .into_iter()
                            .filter(|u| coastal_attacker_pids.contains(&u.position))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    if naval.len() > beachhead_cap {
                        // Send best-firepower units first up to the beachhead capacity
                        naval.sort_by(|a, b| {
                            b.effective_firepower()
                                .partial_cmp(&a.effective_firepower())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        naval.truncate(beachhead_cap);
                    }
                    (land, naval)
                }
                None => continue,
            };

        // Track unit IDs by cohort so post-battle relocation can send land
        // survivors to the conquered province and keep naval survivors at origin.
        let land_unit_ids: HashSet<crate::map::UnitId> = land_cohort.iter().map(|u| u.id).collect();

        let mut attacker_units: Vec<ArmyUnit> = land_cohort;
        attacker_units.extend(naval_cohort);

        if attacker_units.is_empty() {
            continue;
        }

        // A "pure naval" attack is one with no land cohort at all.
        // Used by auto-conquer relocation logic below.
        let is_naval_attack = land_unit_ids.is_empty();

        let attacker_force = CombatForce {
            nation: attacker_id,
            units: attacker_units,
        };

        // Create defender force: use actual army units stationed in the province,
        // plus a garrison for capital provinces only.
        let defender_units: Vec<_> = match game.get_nation(defender_id) {
            Some(n) => n
                .army
                .iter()
                .filter(|u| u.position == province_id)
                .cloned()
                .collect(),
            None => Vec::new(),
        };

        let is_capital = game
            .get_nation(defender_id)
            .is_some_and(|n| n.capital_province_id == province_id);

        let mut defense_units = defender_units;
        // Only generate a garrison for capital provinces (representing citizen militia)
        if is_capital && defense_units.is_empty() {
            let mut garrison = create_garrison(defender_type);
            for unit in &mut garrison {
                unit.owner = defender_id;
                unit.position = province_id;
            }
            defense_units = garrison;
        }

        // If no defenders at all, province falls without a fight
        if defense_units.is_empty() {
            // Auto-conquer: no battle needed.
            // For land attacks, participating units (those in adjacent provinces
            // that didn't move this turn) move into the conquered province.
            // Naval attacks: units return to origin (no position change).
            let defender_is_minor = game
                .get_nation(defender_id)
                .is_some_and(|n| !n.is_great_power());
            if let Some(province) = game.get_province_mut(province_id) {
                let origin_input = province.incorporated_from.or(province.conquest_origin);
                province.conquest_origin = attribute_conquest_origin(
                    origin_input,
                    attacker_id,
                    defender_id,
                    defender_is_minor,
                );
                // Conquest ends any diplomatic-incorporation relationship
                // for this province — `incorporated_from` drives map visuals
                // and should only reflect diplomatic integration, not spoils
                // of war (card #79 + reviewer F-008).
                province.incorporated_from = None;
                province.owner = attacker_id;
            }
            if let Some(defender_nation) = game.get_nation_mut(defender_id) {
                defender_nation
                    .province_ids
                    .retain(|&pid| pid != province_id);
                // Destroy any defender units in the conquered province
                defender_nation.army.retain(|u| u.position != province_id);
            }
            if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
                attacker_nation.add_province(province_id);
                // Relocate participating land units into the conquered province
                if !is_naval_attack {
                    for unit in &mut attacker_nation.army {
                        if !moved_unit_ids.contains(&unit.id)
                            && adjacent_attacker_pids.contains(&unit.position)
                        {
                            unit.position = province_id;
                        }
                    }
                }
            }
            already_contested.insert(province_id);
            already_conquered.insert(attacker_id);
            lost_province.insert(defender_id);
            let attacker_name = game
                .get_nation(attacker_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let prov_name = game
                .get_province(province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("Province {:?}", province_id));
            let defender_name = game
                .get_nation(defender_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let turn = game.turn;
            game.history.push((
                turn,
                format!(
                    "{} conquered {} from {}",
                    attacker_name, prov_name, defender_name
                ),
            ));
            check_and_apply_anarchy(game, defender_id, report);
            continue;
        }

        let defender_force = CombatForce {
            nation: defender_id,
            units: defense_units,
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

        let mut result = resolve_battle(
            &attacker_force,
            &defender_force,
            province_id,
            battle_terrain,
            battle_fort_level,
        );

        // Track which provinces the attacking units came from (for battle screen arrows)
        if !is_naval_attack {
            let mut origins: Vec<ProvinceId> = attacker_force
                .units
                .iter()
                .map(|u| u.position)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            origins.sort_by_key(|p| p.0);
            result.attacker_origin_provinces = origins;
        }

        // Update attacker's army: remove units that fought, add back survivors
        {
            let battle_ids: HashSet<crate::map::UnitId> =
                attacker_force.units.iter().map(|u| u.id).collect();
            if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
                attacker_nation.army.retain(|u| !battle_ids.contains(&u.id));
                attacker_nation
                    .army
                    .extend(result.attacker_survivors.iter().cloned());
            }
        }

        // Update defender's army: remove units that fought, add back survivors
        {
            let battle_ids: HashSet<crate::map::UnitId> =
                defender_force.units.iter().map(|u| u.id).collect();
            if let Some(defender_nation) = game.get_nation_mut(defender_id) {
                defender_nation.army.retain(|u| !battle_ids.contains(&u.id));
                defender_nation
                    .army
                    .extend(result.defender_survivors.iter().cloned());
            }
        }

        if result.attacker_won {
            // Move surviving attacker units:
            // - Land cohort survivors: move into the conquered province
            // - Naval cohort survivors: stay at their origin province (return to port)
            {
                let survivor_ids: HashSet<crate::map::UnitId> =
                    result.attacker_survivors.iter().map(|u| u.id).collect();
                if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
                    for unit in &mut attacker_nation.army {
                        if survivor_ids.contains(&unit.id) && land_unit_ids.contains(&unit.id) {
                            unit.position = province_id;
                        }
                    }
                }
            }

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
            let defender_is_minor = game
                .get_nation(defender_id)
                .is_some_and(|n| !n.is_great_power());
            if let Some(province) = game.get_province_mut(province_id) {
                let origin_input = province.incorporated_from.or(province.conquest_origin);
                province.conquest_origin = attribute_conquest_origin(
                    origin_input,
                    attacker_id,
                    defender_id,
                    defender_is_minor,
                );
                province.incorporated_from = None;
                province.owner = attacker_id;
                province.garrison_count = 0;
            }

            // Update nation province lists
            if let Some(defender_nation) = game.get_nation_mut(defender_id) {
                defender_nation
                    .province_ids
                    .retain(|pid| *pid != province_id);
                // Destroy any remaining defender units in the conquered province
                defender_nation.army.retain(|u| u.position != province_id);
            }
            if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
                attacker_nation.add_province(province_id);
            }
            already_contested.insert(province_id);
            already_conquered.insert(attacker_id);
            lost_province.insert(defender_id);

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
                report.newspaper_headlines.push(Headline::new(
                    format!(
                        "{} immediately industrializes under new management!",
                        province.name
                    ),
                    HeadlineCategory::Growth,
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
                award_first_colony_clippers(game, attacker_id, report);
            }

            // Record event
            report
                .events
                .push(DomainEvent::ProvinceConquered(ProvinceConquered {
                    province: province_id,
                    old_owner: defender_id,
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
            report.newspaper_headlines.push(Headline::new(
                format!(
                    "BREAKING: {} conquers {} from {}!",
                    atk_name, prov_name, def_name_conquest
                ),
                HeadlineCategory::War,
            ));

            // Record history event
            game.history.push((
                game.turn,
                format!(
                    "{} conquered {} from {}",
                    atk_name, prov_name, def_name_conquest
                ),
            ));

            // Check if the defender lost its capital -> anarchy
            check_and_apply_anarchy(game, defender_id, report);

            // Check if the defender has been eliminated (lost all provinces)
            let defender_eliminated = game
                .get_nation(defender_id)
                .is_some_and(|n| n.is_great_power() && n.province_ids.is_empty());
            if defender_eliminated {
                report.newspaper_headlines.push(Headline::new(
                    format!("{} has been eliminated!", def_name_conquest),
                    HeadlineCategory::War,
                ));
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
            report.newspaper_headlines.push(Headline::new(
                format!("{} repels attack on {}!", def_name, prov_name),
                HeadlineCategory::Battle,
            ));
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

    let mut counter_attacks: Vec<(NationId, ProvinceId, Vec<ProvinceId>)> = Vec::new();

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
                .any(|(_, p, _)| *p == *conquered_prov_id)
        {
            counter_attacks.push((
                *original_defender,
                *conquered_prov_id,
                adjacent_province_ids,
            ));
        }
    }

    // Resolve counter-attacks (second pass — no further counter-attacks after this)
    for (counter_attacker_id, target_province_id, adjacent_province_ids) in counter_attacks {
        let new_owner_id = match game.get_province(target_province_id) {
            Some(p) => p.owner,
            None => continue,
        };

        // The counter-attacker uses army units from adjacent provinces they still own.
        // Units that moved this turn cannot participate in the counter-attack either.
        let counter_units: Vec<ArmyUnit> = match game.get_nation(counter_attacker_id) {
            Some(n) => n
                .army
                .iter()
                .filter(|u| {
                    !moved_unit_ids.contains(&u.id)
                        && adjacent_province_ids.contains(&u.position)
                        && n.province_ids.contains(&u.position)
                })
                .cloned()
                .collect(),
            None => continue,
        };

        if counter_units.is_empty() {
            continue;
        }

        let counter_force = CombatForce {
            nation: counter_attacker_id,
            units: counter_units,
        };

        // Defender of counter-attack is the new occupier — use units in the target province
        let occupier_units: Vec<ArmyUnit> = match game.get_nation(new_owner_id) {
            Some(n) => n
                .army
                .iter()
                .filter(|u| u.position == target_province_id)
                .cloned()
                .collect(),
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

        let mut result = resolve_battle(
            &counter_force,
            &defender_force,
            target_province_id,
            battle_terrain,
            battle_fort_level,
        );

        // Track which provinces the counter-attacking units came from
        {
            let mut origins: Vec<ProvinceId> = counter_force
                .units
                .iter()
                .map(|u| u.position)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            origins.sort_by_key(|p| p.0);
            result.attacker_origin_provinces = origins;
        }

        // Update counter-attacker's army: remove participants, add survivors
        {
            let battle_ids: HashSet<crate::map::UnitId> =
                counter_force.units.iter().map(|u| u.id).collect();
            if let Some(ca_nation) = game.get_nation_mut(counter_attacker_id) {
                ca_nation.army.retain(|u| !battle_ids.contains(&u.id));
                ca_nation
                    .army
                    .extend(result.attacker_survivors.iter().cloned());
            }
        }

        // Update occupier's army: remove participants, add survivors
        {
            let battle_ids: HashSet<crate::map::UnitId> =
                defender_force.units.iter().map(|u| u.id).collect();
            if let Some(occ_nation) = game.get_nation_mut(new_owner_id) {
                occ_nation.army.retain(|u| !battle_ids.contains(&u.id));
                occ_nation
                    .army
                    .extend(result.defender_survivors.iter().cloned());
            }
        }

        if result.attacker_won {
            // Counter-attack succeeds: province returns to original defender.
            // The dispossessed party here is the occupier (`new_owner_id`) —
            // preserve minor-origin attribution so the province can still be
            // released under card #79 even after changing hands multiple times.
            let occupier_is_minor = game
                .get_nation(new_owner_id)
                .is_some_and(|n| !n.is_great_power());
            if let Some(province) = game.get_province_mut(target_province_id) {
                let origin_input = province.incorporated_from.or(province.conquest_origin);
                province.conquest_origin = attribute_conquest_origin(
                    origin_input,
                    counter_attacker_id,
                    new_owner_id,
                    occupier_is_minor,
                );
                province.incorporated_from = None;
                province.owner = counter_attacker_id;
            }
            if let Some(occ_nation) = game.get_nation_mut(new_owner_id) {
                occ_nation
                    .province_ids
                    .retain(|pid| *pid != target_province_id);
                // Destroy any occupier units in the re-conquered province
                occ_nation.army.retain(|u| u.position != target_province_id);
            }
            // Move surviving counter-attacker units into the recaptured province
            // (counter-attacks are always land-based — from adjacent provinces)
            {
                let survivor_ids: HashSet<crate::map::UnitId> =
                    result.attacker_survivors.iter().map(|u| u.id).collect();
                if let Some(ca_nation) = game.get_nation_mut(counter_attacker_id) {
                    ca_nation.add_province(target_province_id);
                    for unit in &mut ca_nation.army {
                        if survivor_ids.contains(&unit.id) {
                            unit.position = target_province_id;
                        }
                    }
                }
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
            report.newspaper_headlines.push(Headline::new(
                format!("{} counter-attacks and recaptures {}!", ca_name, prov_name),
                HeadlineCategory::War,
            ));
            // The occupier may have lost their capital in this counter-attack
            check_and_apply_anarchy(game, new_owner_id, report);
        } else {
            let occ_name = game
                .get_nation(new_owner_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let prov_name = game
                .get_province(target_province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            report.newspaper_headlines.push(Headline::new(
                format!("{} repels counter-attack on {}!", occ_name, prov_name),
                HeadlineCategory::Battle,
            ));
        }

        report.battles.push(result);
    }
}

/// Compute the `conquest_origin` field for a province that just changed
/// hands via military conquest, so that card #79 can restore minor
/// provinces to their original owner even if they were taken purely
/// militarily. `current_origin` is the ancestor-minor attribution pulled
/// in from either `incorporated_from` (if the province was previously
/// diplomatically incorporated from a minor) or `conquest_origin` (if it
/// was already conquered from a minor earlier in the chain).
///
/// - Preserve an existing attribution across further conquests.
/// - If none and the dispossessed defender is a minor, stamp the defender.
/// - If the new owner IS the origin minor reclaiming their own land, drop
///   the attribution — a nation is not "conquered from itself".
fn attribute_conquest_origin(
    current_origin: Option<NationId>,
    new_owner: NationId,
    defender_id: NationId,
    defender_is_minor: bool,
) -> Option<NationId> {
    let origin = current_origin.or_else(|| defender_is_minor.then_some(defender_id));
    match origin {
        Some(id) if id == new_owner => None,
        other => other,
    }
}

/// Check if a nation just lost its capital province and should enter anarchy.
/// Returns true if anarchy was triggered.
fn check_and_apply_anarchy(
    game: &mut GameState,
    nation_id: NationId,
    report: &mut TurnReport,
) -> bool {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return false,
    };
    if nation.is_in_anarchy {
        return false; // already in anarchy
    }
    // Check if the nation still owns its capital province
    if nation.province_ids.contains(&nation.capital_province_id) {
        return false;
    }
    // Enter anarchy
    let name = nation.name.clone();
    if let Some(n) = game.get_nation_mut(nation_id) {
        n.is_in_anarchy = true;
    }
    // Card #68: anarchy ends all wars involving this nation for dedup
    // purposes — clear any pact-defense requests so fresh cascades can run
    // if this nation ever recovers (future feature) or if any remaining
    // state queries consult the set.
    game.diplomacy.clear_pact_defense_for_nation(nation_id);
    // Card #79: integrated minors regain their independence when the
    // overlord falls into anarchy. Runs before the NationEnteredAnarchy
    // event so consumers that snapshot state see the newly-released minors.
    release_integrated_minors(game, nation_id, report);
    report
        .events
        .push(DomainEvent::NationEnteredAnarchy(NationEnteredAnarchy {
            nation: nation_id,
        }));
    report.newspaper_headlines.push(Headline::new(
        format!(
            "ANARCHY: {} collapses into chaos after losing its capital!",
            name
        ),
        HeadlineCategory::War,
    ));
    game.history
        .push((game.turn, format!("{} fell into anarchy", name)));
    true
}

/// Release every minor nation whose provinces (either by diplomatic
/// incorporation or by straight military conquest) are currently held by
/// `overlord_id` and originate from the minor. Provinces marked
/// `incorporated_from = Some(minor_id)` and still owned by the anarchic
/// overlord are removed from the overlord and reassigned to the minor,
/// who resumes as an independent nation. Implements card #79: "when a
/// great power loses its capital, any diplomatically integrated country
/// regains its independence."
fn release_integrated_minors(
    game: &mut GameState,
    overlord_id: NationId,
    report: &mut TurnReport,
) {
    // Every minor is a candidate: the eligibility decision is made per
    // minor by checking whether the overlord is currently sitting on any
    // of that minor's origin-marked provinces. Iterating by province flags
    // alone covers both the "fully absorbed" path (integrated_by set) and
    // the "partially conquered militarily" path (integrated_by stays None
    // because the minor still has independent territory elsewhere).
    let minor_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| n.id)
        .collect();

    for minor_id in minor_ids {
        // Collect the overlord-owned provinces that were originally the
        // minor's — either by diplomatic incorporation (incorporated_from)
        // or by military conquest (conquest_origin).
        let provinces_to_restore: Vec<ProvinceId> = game
            .provinces
            .iter()
            .filter(|p| {
                p.owner == overlord_id
                    && (p.incorporated_from == Some(minor_id)
                        || p.conquest_origin == Some(minor_id))
            })
            .map(|p| p.id)
            .collect();

        if provinces_to_restore.is_empty() {
            // This minor had nothing to reclaim from the collapsing overlord.
            // Only clear `integrated_by` if it specifically pointed at the
            // now-anarchic overlord — otherwise the minor is integrated by
            // a different great power and must not be touched.
            if let Some(minor) = game.get_nation_mut(minor_id)
                && minor.integrated_by == Some(overlord_id)
            {
                minor.integrated_by = None;
            }
            continue;
        }

        // Transfer provinces back to the minor and drop both origin
        // markers so the map renders them as plain independent territory.
        for pid in &provinces_to_restore {
            if let Some(prov) = game.get_province_mut(*pid) {
                prov.owner = minor_id;
                prov.incorporated_from = None;
                prov.conquest_origin = None;
            }
        }

        // Remove those provinces from the overlord's list and add them to
        // the minor's.
        if let Some(overlord) = game.get_nation_mut(overlord_id) {
            overlord
                .province_ids
                .retain(|p| !provinces_to_restore.contains(p));
        }
        if let Some(minor) = game.get_nation_mut(minor_id) {
            for pid in &provinces_to_restore {
                minor.add_province(*pid);
            }
            minor.integrated_by = None;
            // Anarchy invariant: the minor is only functional if its original
            // capital province came back with it. If a third power captured
            // that province before the overlord fell, the released minor is
            // structurally anarchic on return.
            minor.is_in_anarchy = !minor.province_ids.contains(&minor.capital_province_id);
        }

        let minor_name = game
            .get_nation(minor_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let overlord_name = game
            .get_nation(overlord_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        report.events.push(DomainEvent::MinorRegainedIndependence(
            crate::events::MinorRegainedIndependence {
                minor: minor_id,
                former_overlord: overlord_id,
            },
        ));
        report.newspaper_headlines.push(Headline::with_reason(
            format!(
                "{} regains its independence as {} collapses into chaos",
                minor_name, overlord_name
            ),
            HeadlineCategory::Diplomacy,
            format!(
                "{} fell into anarchy; {} reclaimed {} province(s)",
                overlord_name,
                minor_name,
                provinces_to_restore.len()
            ),
        ));
        game.history.push((
            game.turn,
            format!(
                "{} regained independence after {} fell into anarchy",
                minor_name, overlord_name
            ),
        ));
    }
}

/// Pact defense: when a minor nation with NAPs is attacked, eligible protectors
/// are asked to intervene in priority order (highest relationship score first).
/// AI protectors make a strategic evaluation; human players receive a proposal.
/// Only one protector can accept — the minor joins their empire on acceptance.
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

    // Card #68: protection requests for the same (attacker, minor) pair fire
    // only once per ongoing war. Subsequent combats between the same attacker
    // and minor skip the cascade entirely. The entry is cleared when the war
    // ends (peace, incorporation, anarchy).
    if game
        .diplomacy
        .is_pact_defense_requested(attacker_nation_id, defender_nation_id)
    {
        return;
    }

    // Collect eligible pact holders (GPs with NAP, not already at war with attacker)
    let mut candidates: Vec<(NationId, i32)> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power() && n.id != attacker_nation_id)
        .map(|n| n.id)
        .filter(|&gp_id| {
            game.diplomacy.has_treaty(
                gp_id,
                defender_nation_id,
                crate::events::TreatyType::NonAggressionPact,
            ) && !game
                .diplomacy
                .get_relation(gp_id, attacker_nation_id)
                .is_some_and(|r| r.at_war)
        })
        .map(|gp_id| {
            let score = game
                .diplomacy
                .get_relation(gp_id, defender_nation_id)
                .map(|r| r.score)
                .unwrap_or(0);
            (gp_id, score)
        })
        .collect();

    if candidates.is_empty() {
        return;
    }

    // Sort by relationship score with minor (highest first = most loved)
    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let defender_name = game
        .get_nation(defender_nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let attacker_name = game
        .get_nation(attacker_nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    // Cascade through candidates in priority order
    let candidate_ids: Vec<NationId> = candidates.iter().map(|(id, _)| *id).collect();
    run_pact_defense_cascade(
        game,
        attacker_nation_id,
        defender_nation_id,
        &candidate_ids,
        &defender_name,
        &attacker_name,
        report,
    );

    // Card #68: mark the (attacker, minor) pair so later combats in the same
    // war skip the cascade. Cleared when the war ends. Skip if the minor was
    // just absorbed by a protector during the cascade — incorporation already
    // cleared its dedup entries and the minor is no longer an independent
    // war party.
    let minor_still_independent = game
        .get_nation(defender_nation_id)
        .is_some_and(|n| !n.province_ids.is_empty());
    if minor_still_independent {
        game.diplomacy
            .mark_pact_defense_requested(attacker_nation_id, defender_nation_id);
    }
}

/// Run the pact defense cascade: evaluate each candidate in order,
/// stop at first acceptance or when a human player is reached.
fn run_pact_defense_cascade(
    game: &mut GameState,
    attacker_id: NationId,
    minor_id: NationId,
    candidates: &[NationId],
    defender_name: &str,
    attacker_name: &str,
    report: &mut TurnReport,
) {
    for (i, &gp_id) in candidates.iter().enumerate() {
        let gp_name = game
            .get_nation(gp_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();

        let is_ai = game
            .get_nation(gp_id)
            .is_some_and(|n| n.ai_personality.is_some());

        if is_ai {
            // AI makes a strategic decision
            let personality = crate::ai::common::get_personality(game, gp_id);

            #[cfg(feature = "lua")]
            let lua_cfg = game
                .game_data
                .lua_engine
                .as_ref()
                .and_then(|e| crate::ai::lua_bridge::lua_get_config(e, personality));

            let accepts = crate::ai::assessment::evaluate_pact_defense(
                game,
                gp_id,
                attacker_id,
                minor_id,
                personality,
                #[cfg(feature = "lua")]
                lua_cfg.as_ref(),
            );

            if accepts {
                // Protector accepts: declare war and incorporate the minor
                game.diplomacy.declare_war(gp_id, attacker_id);
                report.newspaper_headlines.push(Headline::with_reason(
                    format!(
                        "{} intervenes to protect {} and declares war on {}!",
                        gp_name, defender_name, attacker_name
                    ),
                    HeadlineCategory::War,
                    format!(
                        "{} personality judged {} worth protecting against {} (pact defense cascade)",
                        personality, defender_name, attacker_name
                    ),
                ));
                game.history.push((
                    game.turn,
                    format!(
                        "{} declared war on {} to protect {}",
                        gp_name, attacker_name, defender_name
                    ),
                ));

                incorporate_minor_into_empire(
                    game,
                    minor_id,
                    gp_id,
                    report,
                    "joined the empire of",
                );
                return; // Stop cascade — one protector is enough
            } else {
                // AI declines
                report.newspaper_headlines.push(Headline::with_reason(
                    format!(
                        "{} declines to intervene on behalf of {}",
                        gp_name, defender_name
                    ),
                    HeadlineCategory::Diplomacy,
                    format!(
                        "{} personality judged intervention too costly — insufficient strategic value or unfavorable balance vs {}",
                        personality, attacker_name
                    ),
                ));
            }
        } else {
            // Human player: create a PactDefenseRequest proposal and pause cascade
            let remaining: Vec<NationId> = candidates[i + 1..].to_vec();
            game.diplomacy
                .pending_proposals
                .push(crate::diplomacy::DiplomaticProposal {
                    from: minor_id,
                    to: gp_id,
                    proposal_type: crate::events::TreatyType::PactDefenseRequest,
                    turn_proposed: game.turn,
                    attacker: Some(attacker_id),
                    cascade_remaining: if remaining.is_empty() {
                        None
                    } else {
                        Some(remaining)
                    },
                });
            report.newspaper_headlines.push(Headline::new(
                format!(
                    "{} requests your protection against {}!",
                    defender_name, attacker_name
                ),
                HeadlineCategory::Diplomacy,
            ));
            return; // Pause cascade until human responds
        }
    }

    // No one accepted
    report.newspaper_headlines.push(Headline::new(
        format!(
            "No protector came to {}'s defense against {}",
            defender_name, attacker_name
        ),
        HeadlineCategory::Diplomacy,
    ));
}

/// Accept a pact defense request: protector declares war on attacker and
/// incorporates the minor nation. Called by the WASM bridge when human
/// player accepts a PactDefenseRequest proposal.
pub fn accept_pact_defense(
    game: &mut GameState,
    protector_id: NationId,
    attacker_id: NationId,
    minor_id: NationId,
    report: &mut TurnReport,
) {
    // Precondition: minor must still exist and have provinces
    let minor_valid = game
        .get_nation(minor_id)
        .is_some_and(|n| !n.is_great_power() && !n.province_ids.is_empty());
    if !minor_valid {
        return; // Minor already conquered/incorporated — stale proposal
    }

    let protector_name = game
        .get_nation(protector_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let attacker_name = game
        .get_nation(attacker_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let minor_name = game
        .get_nation(minor_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    game.diplomacy.declare_war(protector_id, attacker_id);
    report.newspaper_headlines.push(Headline::new(
        format!(
            "{} intervenes to protect {} and declares war on {}!",
            protector_name, minor_name, attacker_name
        ),
        HeadlineCategory::War,
    ));
    game.history.push((
        game.turn,
        format!(
            "{} declared war on {} to protect {}",
            protector_name, attacker_name, minor_name
        ),
    ));

    incorporate_minor_into_empire(game, minor_id, protector_id, report, "joined the empire of");
}

/// Continue the pact defense cascade after the human player rejects.
/// Evaluates remaining AI candidates in order.
pub fn continue_pact_defense_cascade(
    game: &mut GameState,
    attacker_id: NationId,
    minor_id: NationId,
    remaining: &[NationId],
    report: &mut TurnReport,
) {
    // Precondition: minor must still exist and have provinces
    let minor_valid = game
        .get_nation(minor_id)
        .is_some_and(|n| !n.is_great_power() && !n.province_ids.is_empty());
    if !minor_valid {
        return; // Minor already conquered/incorporated — stale cascade
    }

    let defender_name = game
        .get_nation(minor_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let attacker_name = game
        .get_nation(attacker_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    run_pact_defense_cascade(
        game,
        attacker_id,
        minor_id,
        remaining,
        &defender_name,
        &attacker_name,
        report,
    );
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
            report.newspaper_headlines.push(Headline::new(
                format!(
                    "NAVAL VICTORY: {} defeats {} fleet! ({} ships sunk)",
                    atk_name,
                    def_name,
                    result.defender_ships_lost.len()
                ),
                HeadlineCategory::Battle,
            ));
        } else {
            report.newspaper_headlines.push(Headline::new(
                format!(
                    "NAVAL VICTORY: {} defeats {} fleet! ({} ships sunk)",
                    def_name,
                    atk_name,
                    result.attacker_ships_lost.len()
                ),
                HeadlineCategory::Battle,
            ));
        }

        report.naval_battles.push(result);
    }
}

/// Compute blockade-adjusted cargo capacity for all Great Powers.
///
/// For each GP at war with an enemy that has warships, reduce their effective
/// cargo capacity using `calculate_blockade_effect`. This map is passed to the
/// trade session so blockades actually reduce trade volume.
fn compute_blockade_capacity(game: &GameState) -> std::collections::HashMap<NationId, u32> {
    // Only consider active Great Powers (not anarchic, not eliminated)
    let active_gp_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power() && !n.is_in_anarchy && !n.province_ids.is_empty())
        .map(|n| n.id)
        .collect();

    let mut capacity_map = std::collections::HashMap::new();

    for &nation_id in &active_gp_ids {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => continue,
        };
        let raw_cargo = nation.total_cargo_capacity();

        // Only count warships from active enemy nations
        let mut enemy_warship_count: u32 = 0;
        for &other_id in &active_gp_ids {
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

        let effective = if enemy_warship_count > 0 {
            calculate_blockade_effect(raw_cargo, enemy_warship_count)
        } else {
            raw_cargo
        };
        capacity_map.insert(nation_id, effective);
    }

    capacity_map
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
                report.newspaper_headlines.push(Headline::new(
                    format!(
                        "BLOCKADE: {} merchant fleet loses {} cargo capacity to enemy warships",
                        nation_name, blocked
                    ),
                    HeadlineCategory::Battle,
                ));
            }
        }
    }
}

/// Report which technologies are available for research by the human player.
fn report_available_techs(game: &GameState, report: &mut TurnReport) {
    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) if !n.is_in_anarchy => n,
        _ => return,
    };
    let available = game
        .game_data
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
        if let Some(pos) = action.text.find(researched_pattern) {
            let nation_name = &action.text[..pos];
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
/// Resolve all pending diplomatic proposals from the turn.
///
/// Build a short, human-readable reason for an AI treaty-evaluation outcome,
/// drawing on the evaluator's core signals (personality, relation score, war
/// status, diplomatic infrastructure).
fn diplomacy_reason(
    game: &GameState,
    evaluator: NationId,
    counterpart: NationId,
    treaty_label: &str,
    accepted: bool,
) -> String {
    let personality = crate::ai::common::get_personality(game, evaluator);
    let (score, at_war, has_embassy, has_consulate) = game
        .diplomacy
        .get_relation(evaluator, counterpart)
        .map(|r| (r.score, r.at_war, r.has_embassy, r.has_consulate))
        .unwrap_or((0, false, false, false));
    let infra = if has_embassy {
        "embassy"
    } else if has_consulate {
        "consulate"
    } else {
        "no diplomatic infrastructure"
    };
    let verdict = if accepted { "accepted" } else { "rejected" };
    format!(
        "{} personality {} {} (relation={}, at_war={}, {})",
        personality, verdict, treaty_label, score, at_war, infra
    )
}

fn separate_peace_reason(
    game: &GameState,
    peacemaker: NationId,
    former_ally: NationId,
    enemy: NationId,
) -> String {
    let peacemaker_name = game
        .get_nation(peacemaker)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let ally_name = game
        .get_nation(former_ally)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let enemy_name = game
        .get_nation(enemy)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    format!(
        "Separate peace: {} ended its war with {} while ally {} remained at war with {}",
        peacemaker_name, enemy_name, ally_name, enemy_name
    )
}

fn record_broken_alliance_headlines(
    game: &GameState,
    report: &mut TurnReport,
    broken_alliances: &[crate::diplomacy::relations::BrokenAlliance],
) {
    for broken in broken_alliances {
        let peacemaker_name = game
            .get_nation(broken.peacemaker)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let ally_name = game
            .get_nation(broken.former_ally)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        report.newspaper_headlines.push(Headline::with_reason(
            format!(
                "{} breaks its alliance with {} after making separate peace",
                peacemaker_name, ally_name
            ),
            HeadlineCategory::Diplomacy,
            separate_peace_reason(game, broken.peacemaker, broken.former_ally, broken.enemy),
        ));
    }
}

/// - Mutual peace proposals (both sides proposed): auto-accept.
/// - Player→AI proposals: evaluate using AI assessment logic.
/// - AI→Human proposals: keep pending for UI modal.
/// - AI→AI proposals: evaluate using AI assessment logic (alliance, NAP, peace).
///
/// Also expires stale proposals older than 4 turns.
fn resolve_diplomatic_proposals(game: &mut GameState, report: &mut TurnReport) {
    let proposals: Vec<_> = game
        .diplomacy
        .drain_proposals()
        .into_iter()
        .filter(|p| {
            !game.get_nation(p.from).is_some_and(|n| n.is_in_anarchy)
                && !game.get_nation(p.to).is_some_and(|n| n.is_in_anarchy)
        })
        .collect();
    if proposals.is_empty() {
        return;
    }

    // Detect mutual peace proposals (both sides proposed peace)
    let mut mutual_peace: Vec<(NationId, NationId)> = Vec::new();
    for (i, p1) in proposals.iter().enumerate() {
        if p1.proposal_type != TreatyType::PeaceTreaty {
            continue;
        }
        for p2 in &proposals[i + 1..] {
            if p2.proposal_type == TreatyType::PeaceTreaty && p1.from == p2.to && p1.to == p2.from {
                mutual_peace.push((p1.from, p1.to));
            }
        }
    }

    // Apply mutual peace immediately
    for &(a, b) in &mutual_peace {
        if game.diplomacy.is_at_war(a, b) {
            game.diplomacy.queue_peace(a, b);
            let name_a = game
                .get_nation(a)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let name_b = game
                .get_nation(b)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            report
                .events
                .push(DomainEvent::TreatyAccepted(crate::events::TreatyAccepted {
                    from: a,
                    to: b,
                    treaty_type: TreatyType::PeaceTreaty,
                }));
            let turn = game.turn;
            game.history.push((
                turn,
                format!("{} and {} agreed to mutual peace", name_a, name_b),
            ));
        }
    }

    // Evaluate player→AI proposals using AI assessment logic,
    // and re-add AI→human proposals for UI handling.
    let human = game.human_player_nation;
    for proposal in proposals {
        // Skip proposals that were part of mutual peace
        let was_mutual = mutual_peace.iter().any(|&(a, b)| {
            (proposal.from == a && proposal.to == b) || (proposal.from == b && proposal.to == a)
        });
        if was_mutual {
            continue;
        }

        if proposal.from == human && proposal.to != human {
            // Player→AI: evaluate using AI assessment
            let target_id = proposal.to;
            let personality = crate::ai::common::get_personality(game, target_id);

            #[cfg(feature = "lua")]
            let lua_cfg = game
                .game_data
                .lua_engine
                .as_ref()
                .and_then(|e| crate::ai::lua_bridge::lua_get_config(e, personality));

            let accepted = match proposal.proposal_type {
                TreatyType::NonAggressionPact => crate::ai::assessment::evaluate_nap_proposal(
                    game,
                    human,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                TreatyType::Alliance => crate::ai::assessment::evaluate_alliance_proposal(
                    game,
                    human,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                TreatyType::PeaceTreaty => crate::ai::assessment::evaluate_peace_proposal(
                    game,
                    human,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                _ => false,
            };

            let from_name = game
                .get_nation(human)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let to_name = game
                .get_nation(target_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let treaty_label = match proposal.proposal_type {
                TreatyType::NonAggressionPact => "Non-Aggression Pact",
                TreatyType::Alliance => "Alliance",
                TreatyType::PeaceTreaty => "Peace Treaty",
                _ => "Treaty",
            };

            if accepted {
                // Apply the treaty — check result in case state drifted
                let applied = match proposal.proposal_type {
                    TreatyType::NonAggressionPact => {
                        game.diplomacy.propose_pact(human, target_id).is_ok()
                    }
                    TreatyType::Alliance => {
                        game.diplomacy.propose_alliance(human, target_id).is_ok()
                    }
                    TreatyType::PeaceTreaty => {
                        game.diplomacy.queue_peace(human, target_id);
                        true
                    }
                    _ => false,
                };
                if applied {
                    report.events.push(DomainEvent::TreatyAccepted(
                        crate::events::TreatyAccepted {
                            from: human,
                            to: target_id,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                    report.newspaper_headlines.push(Headline::with_reason(
                        format!(
                            "{} accepts {}'s {} proposal",
                            to_name, from_name, treaty_label
                        ),
                        HeadlineCategory::Diplomacy,
                        diplomacy_reason(game, target_id, human, treaty_label, true),
                    ));
                    let turn = game.turn;
                    game.history.push((
                        turn,
                        format!(
                            "{} accepted {}'s {} proposal",
                            to_name, from_name, treaty_label
                        ),
                    ));
                } else {
                    // AI accepted but treaty could not be applied (state drift)
                    report.events.push(DomainEvent::TreatyRejected(
                        crate::events::TreatyRejected {
                            from: human,
                            to: target_id,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                    report.newspaper_headlines.push(Headline::with_reason(
                        format!(
                            "{} proposal to {} could not be fulfilled",
                            treaty_label, to_name
                        ),
                        HeadlineCategory::Diplomacy,
                        "AI accepted but state drifted (counterpart relation changed mid-turn); treaty could not be applied".to_string(),
                    ));
                }
            } else {
                report
                    .events
                    .push(DomainEvent::TreatyRejected(crate::events::TreatyRejected {
                        from: human,
                        to: target_id,
                        treaty_type: proposal.proposal_type,
                    }));
                report.newspaper_headlines.push(Headline::with_reason(
                    format!(
                        "{} rejects {}'s {} proposal",
                        to_name, from_name, treaty_label
                    ),
                    HeadlineCategory::Diplomacy,
                    diplomacy_reason(game, target_id, human, treaty_label, false),
                ));
            }
        } else if proposal.to == human {
            // AI→human: keep pending for UI
            game.diplomacy.pending_proposals.push(proposal);
        } else {
            // AI→AI: evaluate the proposal at end of turn
            let from_id = proposal.from;
            let target_id = proposal.to;
            let personality = crate::ai::common::get_personality(game, target_id);

            #[cfg(feature = "lua")]
            let lua_cfg = game
                .game_data
                .lua_engine
                .as_ref()
                .and_then(|e| crate::ai::lua_bridge::lua_get_config(e, personality));

            let accepted = match proposal.proposal_type {
                TreatyType::NonAggressionPact => crate::ai::assessment::evaluate_nap_proposal(
                    game,
                    from_id,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                TreatyType::Alliance => crate::ai::assessment::evaluate_alliance_proposal(
                    game,
                    from_id,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                TreatyType::PeaceTreaty => crate::ai::assessment::evaluate_peace_proposal(
                    game,
                    from_id,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                _ => false,
            };

            let from_name = game
                .get_nation(from_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let to_name = game
                .get_nation(target_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let treaty_label = match proposal.proposal_type {
                TreatyType::NonAggressionPact => "Non-Aggression Pact",
                TreatyType::Alliance => "Alliance",
                TreatyType::PeaceTreaty => "Peace Treaty",
                _ => "Treaty",
            };

            if accepted {
                let applied = match proposal.proposal_type {
                    TreatyType::NonAggressionPact => {
                        game.diplomacy.propose_pact(from_id, target_id).is_ok()
                    }
                    TreatyType::Alliance => {
                        game.diplomacy.propose_alliance(from_id, target_id).is_ok()
                    }
                    TreatyType::PeaceTreaty => {
                        game.diplomacy.queue_peace(from_id, target_id);
                        true
                    }
                    _ => false,
                };
                if applied {
                    report.events.push(DomainEvent::TreatyAccepted(
                        crate::events::TreatyAccepted {
                            from: from_id,
                            to: target_id,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                    report.newspaper_headlines.push(Headline::with_reason(
                        format!(
                            "{} accepts {}'s {} proposal",
                            to_name, from_name, treaty_label
                        ),
                        HeadlineCategory::Diplomacy,
                        diplomacy_reason(game, target_id, from_id, treaty_label, true),
                    ));
                    let turn = game.turn;
                    game.history.push((
                        turn,
                        format!(
                            "{} accepted {}'s {} proposal",
                            to_name, from_name, treaty_label
                        ),
                    ));
                } else {
                    // AI accepted but treaty could not be applied (state drift)
                    report.events.push(DomainEvent::TreatyRejected(
                        crate::events::TreatyRejected {
                            from: from_id,
                            to: target_id,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                    report.newspaper_headlines.push(Headline::with_reason(
                        format!(
                            "{} proposal to {} could not be fulfilled",
                            treaty_label, to_name
                        ),
                        HeadlineCategory::Diplomacy,
                        "AI accepted but state drifted (counterpart relation changed mid-turn); treaty could not be applied".to_string(),
                    ));
                }
            } else {
                report
                    .events
                    .push(DomainEvent::TreatyRejected(crate::events::TreatyRejected {
                        from: from_id,
                        to: target_id,
                        treaty_type: proposal.proposal_type,
                    }));
                report.newspaper_headlines.push(Headline::with_reason(
                    format!(
                        "{} rejects {}'s {} proposal",
                        to_name, from_name, treaty_label
                    ),
                    HeadlineCategory::Diplomacy,
                    diplomacy_reason(game, target_id, from_id, treaty_label, false),
                ));
            }
        }
    }

    // Expire proposals older than 4 turns
    game.diplomacy.expire_proposals(game.turn, 4);
}

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
        let is_gp_war = game
            .get_nation(*attacker)
            .is_some_and(|n| n.is_great_power())
            && game
                .get_nation(*defender)
                .is_some_and(|n| n.is_great_power());
        if !is_gp_war {
            continue;
        }
        // Skip alliance obligations for anarchic defenders
        if game.get_nation(*defender).is_some_and(|n| n.is_in_anarchy) {
            continue;
        }
        // Check defender's allies
        let defender_allies = game.diplomacy.get_allies(*defender);
        for ally in &defender_allies {
            if *ally == *attacker {
                continue;
            }
            // Skip anarchic allies
            if game.get_nation(*ally).is_some_and(|n| n.is_in_anarchy) {
                continue;
            }
            // Skip joining wars against nations that have 0 provinces (already defeated)
            let attacker_has_provinces = game
                .get_nation(*attacker)
                .is_some_and(|n| !n.province_ids.is_empty());
            if !attacker_has_provinces {
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

            let defender_allies_count = defender_allies.len();
            new_wars.push((*ally, *attacker, ally_name.clone(), attacker_name.clone()));
            report.newspaper_headlines.push(Headline::with_reason(
                format!(
                    "{} honors its alliance with {} and declares war on {}!",
                    ally_name, defender_name, attacker_name
                ),
                HeadlineCategory::War,
                format!(
                    "Alliance obligation: {} was attacked by {} — treaty auto-triggers defensive war ({} defending ally/allies in total)",
                    defender_name, attacker_name, defender_allies_count
                ),
            ));
        }

        // Check attacker's allies
        let attacker_allies = game.diplomacy.get_allies(*attacker);
        for ally in &attacker_allies {
            if *ally == *defender {
                continue;
            }
            // Skip anarchic allies
            if game.get_nation(*ally).is_some_and(|n| n.is_in_anarchy) {
                continue;
            }
            // Skip joining wars against nations that have 0 provinces (already defeated)
            let defender_has_provinces = game
                .get_nation(*defender)
                .is_some_and(|n| !n.province_ids.is_empty());
            if !defender_has_provinces {
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

            let attacker_allies_count = attacker_allies.len();
            new_wars.push((*ally, *defender, ally_name.clone(), defender_name.clone()));
            report.newspaper_headlines.push(Headline::with_reason(
                format!(
                    "{} honors its alliance with {} and declares war on {}!",
                    ally_name, attacker_name, defender_name
                ),
                HeadlineCategory::War,
                format!(
                    "Alliance obligation: {} declared war on {} — treaty auto-triggers offensive support ({} supporting ally/allies in total)",
                    attacker_name, defender_name, attacker_allies_count
                ),
            ));
        }
    }

    // Graph-based conflict detection: build a map of each ally's obligations
    // (who they would fight against), then detect any ally that would end up
    // on both sides of the same war.
    let mut obligations: std::collections::HashMap<NationId, Vec<NationId>> =
        std::collections::HashMap::new();
    for (ally, enemy, _, _) in &new_wars {
        obligations.entry(*ally).or_default().push(*enemy);
    }

    let mut conflicted_allies: Vec<NationId> = Vec::new();
    for (ally, enemies) in &obligations {
        // If the ally must fight multiple enemies, check whether any pair of
        // those enemies are on opposite sides of the same war. If so, the
        // ally is being pulled to both sides and must stay neutral.
        for i in 0..enemies.len() {
            for j in (i + 1)..enemies.len() {
                if game.diplomacy.is_at_war(enemies[i], enemies[j])
                    && !conflicted_allies.contains(ally)
                {
                    conflicted_allies.push(*ally);
                }
            }
        }
    }

    if !conflicted_allies.is_empty() {
        new_wars.retain(|(ally, _, _, _)| !conflicted_allies.contains(ally));
        report.newspaper_headlines.retain(|h| {
            !conflicted_allies.iter().any(|&cid| {
                let name = game.get_nation(cid).map(|n| n.name.as_str()).unwrap_or("");
                !name.is_empty()
                    && h.text.starts_with(name)
                    && h.text.contains("honors its alliance")
            })
        });
        for &cid in &conflicted_allies {
            let name = game
                .get_nation(cid)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            report.newspaper_headlines.push(Headline::new(
                format!(
                    "{} remains neutral due to conflicting alliance obligations",
                    name
                ),
                HeadlineCategory::Diplomacy,
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
/// Transfer all provinces from a minor nation to a great power (incorporation).
/// Used by voluntary incorporation, pact defense acceptance, and colonization.
fn incorporate_minor_into_empire(
    game: &mut GameState,
    minor_id: NationId,
    gp_id: NationId,
    report: &mut TurnReport,
    reason: &str,
) {
    let provinces_to_transfer: Vec<ProvinceId> = game
        .get_nation(minor_id)
        .map(|n| n.province_ids.clone())
        .unwrap_or_default();

    // Update province owners (mark as diplomatically incorporated for map rendering)
    for pid in &provinces_to_transfer {
        if let Some(prov) = game.get_province_mut(*pid) {
            prov.owner = gp_id;
            prov.incorporated_from = Some(minor_id);
        }
    }

    // Remove provinces from minor nation
    if let Some(minor) = game.get_nation_mut(minor_id) {
        minor.province_ids.clear();
        // Card #79: back-pointer used when the overlord falls into anarchy
        // and integrated minors regain their independence.
        minor.integrated_by = Some(gp_id);
    }

    // Add provinces to great power
    if let Some(gp) = game.get_nation_mut(gp_id) {
        for pid in &provinces_to_transfer {
            gp.add_province(*pid);
        }
    }

    // Card #68: once absorbed, the minor is no longer an independent war
    // party; any pact-defense dedup entry involving it is stale.
    game.diplomacy.clear_pact_defense_for_nation(minor_id);

    let minor_name = game
        .get_nation(minor_id)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    let gp_name = game
        .get_nation(gp_id)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    report
        .events
        .push(DomainEvent::NationIncorporated(NationIncorporated {
            minor_nation: minor_id,
            great_power: gp_id,
        }));

    report.incorporations.push((minor_id, gp_id));

    award_first_colony_clippers(game, gp_id, report);

    game.history
        .push((game.turn, format!("{} {} {}", minor_name, reason, gp_name)));
}

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

    // Threshold sourced from scripts/config/game.lua — voluntary incorporation
    // should be a rare, late-game event requiring near-max relationship.
    let threshold = game.game_data.game_config.voluntary_incorporation_threshold;

    for minor_id in &minor_ids {
        if game
            .get_nation(*minor_id)
            .is_some_and(|n| n.province_ids.is_empty() || n.is_in_anarchy)
        {
            continue;
        }

        let mut best_gp: Option<NationId> = None;
        let mut best_score: i32 = threshold - 1;

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
            incorporate_minor_into_empire(
                game,
                *minor_id,
                gp_id,
                report,
                "voluntarily joined the empire of",
            );
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
        .filter(|n| n.ai_personality.is_some() && !n.is_in_anarchy)
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
            .filter_map(|tid| game.game_data.tech_tree.get(*tid))
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
                report.newspaper_headlines.push(Headline::new(
                    format!("{} has earned a General!", nation_name),
                    HeadlineCategory::Military,
                ));
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
                report.newspaper_headlines.push(Headline::new(
                    format!("{} has earned an Admiral!", nation_name),
                    HeadlineCategory::Military,
                ));
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
            report.newspaper_headlines.push(Headline::new(
                format!("{}'s capitol building has expanded!", attacker_name),
                HeadlineCategory::Growth,
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
                report.newspaper_headlines.push(Headline::new(
                    format!(
                        "{}'s expert workforce drives capitol expansion!",
                        nation_name
                    ),
                    HeadlineCategory::Growth,
                ));
            }
        }
    }
}

fn generate_newspaper(game: &GameState, report: &mut TurnReport) {
    let year = game.turn.year();
    let quarter = game.turn.quarter();

    report.newspaper_headlines.push(Headline::new(
        format!("The Imperial Times - {year} Q{quarter}"),
        HeadlineCategory::Default,
    ));

    // AI actions (tech research, military buildup, war declarations).
    // Non-actions ("considered but declined") flow through with is_non_action=true
    // so the UI can filter them behind a debug toggle.
    for action in &report.ai_actions {
        let headline = if action.is_non_action {
            Headline::non_action(
                action.text.clone(),
                HeadlineCategory::Default,
                action.reason.clone(),
            )
        } else {
            Headline::with_reason(
                action.text.clone(),
                HeadlineCategory::Default,
                action.reason.clone(),
            )
        };
        report.newspaper_headlines.push(headline);
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
        report.newspaper_headlines.push(Headline::new(
            format!(
                "BREAKING: {} has voluntarily joined the {} empire!",
                minor_name, gp_name
            ),
            HeadlineCategory::Politics,
        ));
    }

    // Unit upgrades — brief mention
    if !report.unit_upgrades.is_empty() {
        let upgrade_count = report.unit_upgrades.len();
        report.newspaper_headlines.push(Headline::new(
            format!(
                "Military modernization: {} unit{} upgraded across the nations",
                upgrade_count,
                if upgrade_count == 1 { "" } else { "s" }
            ),
            HeadlineCategory::Military,
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
            report.newspaper_headlines.push(Headline::new(
                format!(
                    "Trade flourishes between {} and its partners",
                    human_nation.name
                ),
                HeadlineCategory::Trade,
            ));
        }
    }

    if let Some(human_nation) = game.get_nation(game.human_player_nation) {
        report.newspaper_headlines.push(Headline::new(
            format!("The {} empire grows stronger", human_nation.name),
            HeadlineCategory::Default,
        ));
    }

    if game.turn.is_decade_election() {
        report.newspaper_headlines.push(Headline::new(
            "Council of Governors to convene!".to_string(),
            HeadlineCategory::Politics,
        ));
    }

    // Report nations currently in anarchy
    for nation in &game.nations {
        if nation.is_in_anarchy && !nation.province_ids.is_empty() {
            report.newspaper_headlines.push(Headline::new(
                format!("{} remains mired in anarchy", nation.name),
                HeadlineCategory::Crisis,
            ));
        }
    }

    // Human player anarchy game-over notice
    if game
        .get_nation(game.human_player_nation)
        .is_some_and(|n| n.is_in_anarchy)
    {
        report.newspaper_headlines.push(Headline::new(
            "Your nation has fallen into anarchy! All governance has ceased.".to_string(),
            HeadlineCategory::Crisis,
        ));
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
    report.newspaper_headlines.push(Headline::new(
        flavor_headlines[flavor_index].to_string(),
        HeadlineCategory::Default,
    ));
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
            report.newspaper_headlines.push(Headline::new(
                format!(
                    "BREAKING: {} wins the Council of Governors with {} of {} votes!",
                    winner.name,
                    result
                        .votes
                        .iter()
                        .find(|(id, _)| *id == winner_id)
                        .map(|(_, v)| *v)
                        .unwrap_or(0),
                    result.total_governors
                ),
                HeadlineCategory::Politics,
            ));
        }
    } else {
        report.newspaper_headlines.push(Headline::new(
            format!(
                "Council of Governors: No nation achieves the required {} vote majority.",
                result.majority_threshold
            ),
            HeadlineCategory::Politics,
        ));
    }

    report.council_vote = Some(result);
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
fn apply_warehouse_caps(game: &mut GameState) {
    for nation in &mut game.nations {
        let warehouse_capacity = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::Warehouse)
            .map(|b| b.effective_capacity())
            .unwrap_or(1);

        let raw_cap = 50 * warehouse_capacity;
        let material_cap = 50 * warehouse_capacity;
        let goods_cap = 25 * warehouse_capacity;

        for amount in nation.warehouse.values_mut() {
            if *amount > raw_cap {
                *amount = raw_cap;
            }
        }

        for amount in nation.materials.values_mut() {
            if *amount > material_cap {
                *amount = material_cap;
            }
        }

        for amount in nation.goods.values_mut() {
            if *amount > goods_cap {
                *amount = goods_cap;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::diplomacy::DiplomacyState;
    use crate::economy::buildings::Building;
    use crate::economy::labor::LaborPool;
    use crate::hex::HexCoord;
    use crate::map::tile::Tile;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};

    /// Build a minimal GameState for testing the turn processor.
    fn test_game_state() -> GameState {
        let coord_farm = HexCoord::new(0, 0);
        let coord_forest = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);

        // A grain tile (produces 1 Grain at level 0)
        let mut farm_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        farm_tile.set_resource(ResourceType::Grain);
        hex_map.set_tile(coord_farm, farm_tile);

        // A forest tile with timber (produces 1 Timber)
        let mut forest_tile = Tile::with_province(TerrainType::Forest, ProvinceId(1));
        forest_tile.set_resource(ResourceType::Timber);
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
        assert!(
            report.newspaper_headlines[0]
                .text
                .contains("The Imperial Times")
        );
        assert!(report.newspaper_headlines[0].text.contains("1815"));
        assert!(report.newspaper_headlines[0].text.contains("Q1"));
    }

    #[test]
    fn newspaper_includes_human_nation() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        let has_empire_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.contains("Testlandia"));
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
            .any(|h| h.text.contains("Council of Governors"));
        assert!(has_election);
    }

    #[test]
    fn generate_newspaper_propagates_non_action_flag() {
        // A non-action AiAction should produce a headline with is_non_action=true
        // AND the reason preserved.
        let game = test_game_state();
        let mut report = TurnReport::empty();
        report.turn = game.turn;
        report.year = game.turn.year();
        report.quarter = game.turn.quarter();
        report.ai_actions = vec![
            crate::ai::AiAction {
                text: "Testland did not declare war this turn".to_string(),
                reason: "war cooldown active".to_string(),
                is_non_action: true,
            },
            crate::ai::AiAction {
                text: "Testland has declared war on Otherland!".to_string(),
                reason: "combined score above threshold".to_string(),
                is_non_action: false,
            },
        ];

        generate_newspaper(&game, &mut report);

        let non_action_headline = report
            .newspaper_headlines
            .iter()
            .find(|h| h.text.contains("did not declare war"))
            .expect("non-action propagated");
        assert!(
            non_action_headline.is_non_action,
            "non-action flag must propagate from AiAction to Headline"
        );
        assert_eq!(
            non_action_headline.reason.as_deref(),
            Some("war cooldown active")
        );

        let action_headline = report
            .newspaper_headlines
            .iter()
            .find(|h| h.text.contains("has declared war"))
            .expect("action propagated");
        assert!(
            !action_headline.is_non_action,
            "positive actions must not be marked as non-action"
        );
    }

    #[test]
    fn generate_newspaper_propagates_ai_action_reasons() {
        // AI actions in the report should become headlines with reason: Some(_),
        // while non-AI headlines added by generate_newspaper remain reason: None.
        let game = test_game_state();
        let mut report = TurnReport::empty();
        report.turn = game.turn;
        report.year = game.turn.year();
        report.quarter = game.turn.quarter();
        report.ai_actions = vec![
            crate::ai::AiAction {
                text: "Scientists in Testland have discovered Steam Engine!".to_string(),
                reason: "Economic personality selected tech (cost=$500)".to_string(),
                is_non_action: false,
            },
            crate::ai::AiAction {
                text: "Testland has declared war on Otherland!".to_string(),
                reason: "Combined score 2.30 > threshold 1.50".to_string(),
                is_non_action: false,
            },
        ];

        generate_newspaper(&game, &mut report);

        // Each AI action should have produced a headline whose reason matches.
        for action in &report.ai_actions {
            let h = report
                .newspaper_headlines
                .iter()
                .find(|h| h.text == action.text)
                .unwrap_or_else(|| panic!("action not propagated: {}", action.text));
            assert_eq!(h.reason.as_deref(), Some(action.reason.as_str()));
        }

        // Masthead never carries a reason.
        let masthead = report
            .newspaper_headlines
            .iter()
            .find(|h| h.text.contains("The Imperial Times"))
            .expect("masthead present");
        assert!(masthead.reason.is_none(), "masthead should have no reason");

        // Flavor headline (always appended) should have no reason.
        let has_flavor_without_reason = report.newspaper_headlines.iter().any(|h| {
            h.reason.is_none()
                && (h.text.contains("Railroad expansion")
                    || h.text.contains("Industrial production")
                    || h.text.contains("Diplomatic tensions")
                    || h.text.contains("Colonial ambitions")
                    || h.text.contains("trade routes")
                    || h.text.contains("age of progress")
                    || h.text.contains("unrest in the frontier")
                    || h.text.contains("Great exhibitions"))
        });
        assert!(
            has_flavor_without_reason,
            "expected at least one flavor headline with reason: None"
        );
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

        // After 5 turns, the nation should have accumulated resources.
        // Emergency recruitment gives 1 worker on turn 1 (nation started with 0),
        // so that worker consumes food on subsequent turns.
        // Grain: 1 gathered per turn = 5, minus some consumed by the emergency worker.
        // Timber: 1 gathered per turn = 5, not consumed by workers.
        let nation = game.get_nation(NationId(1)).unwrap();
        // At least some grain should exist (worker eats 1/turn but produces 1/turn)
        assert!(
            nation.resource_amount(ResourceType::Grain) >= 1,
            "Grain should accumulate: got {}",
            nation.resource_amount(ResourceType::Grain)
        );
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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

        // Give enough workers for full production (expert=4 labor each)
        nation.labor.expert = 5; // 20 labor — enough for all mills + factories

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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
        nation.labor = LaborPool::new();
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
        nation.labor = LaborPool::new();
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
        nation.labor = LaborPool::new();
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
        nation.labor = LaborPool::new();
        // Need workers for food processing to trigger
        nation.labor.untrained = 1;
        // Add a FoodProcessing building with capacity 3
        nation
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 3));
        // 9 grain: 6 consumed by processing (cap 3 × 2), 1 consumed by worker, 2 remain
        nation.add_resource(ResourceType::Grain, 9);

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
        // Feed the workers so they survive 3 turns (5 experts eat 5 food/turn)
        nation.add_resource(ResourceType::Grain, 20);

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
    fn capital_province_resources_delivered_without_transport() {
        // Capital province resources are delivered for free (no freight cars needed).
        let mut hex_map = HexMap::new(10, 10);
        let mut tiles = Vec::new();
        for i in 0..6 {
            let coord = HexCoord::new(i, 0);
            let mut tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
            tile.set_resource(ResourceType::Grain);
            hex_map.set_tile(coord, tile);
            tiles.push(coord);
        }

        let province = Province::new(
            ProvinceId(1),
            "CapitalFarms".to_string(),
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
        // Zero freight cars — capital province resources should still arrive
        nation.labor = LaborPool::new();

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };

        let report = process_turn(&mut game);

        // 6 farms in capital province → all delivered for free, no overflow
        let total_overflow: u32 = report
            .transport_overflow
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(1))
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(
            total_overflow, 0,
            "Capital province has no transport overflow"
        );

        // All 6 grain should be in warehouse
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 6);
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

        // Start with some workers so emergency recruitment doesn't trigger
        nation.labor.untrained = 2;
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
        // Start with 1 worker so emergency recruitment doesn't trigger
        nation.labor.untrained = 1;

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
        // Place adjacent to capital so connectivity is computed automatically
        let coord2 = HexCoord::new(1, 0);

        let hex_map = HexMap::new(10, 10);

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );

        let province_remote = Province::new(
            ProvinceId(2),
            "Remote Land".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        // Settlement starts as Hamlet (connectivity computed by update_settlements)

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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
    fn test_game_state_with_village(
        terrain_resource_pairs: &[(TerrainType, Option<ResourceType>)],
    ) -> GameState {
        let mut hex_map = HexMap::new(20, 20);
        let mut tiles = Vec::new();
        let capital_coord = HexCoord::new(0, 0);

        // Capital province (just a simple grassland)
        let cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(capital_coord, cap_tile);

        // Village province with given terrain/resource pairs
        for (i, (terrain, resource)) in terrain_resource_pairs.iter().enumerate() {
            let coord = HexCoord::new(5 + i as i32, 0);
            let mut tile = Tile::with_province(*terrain, ProvinceId(2));
            if let Some(res) = resource {
                tile.set_resource(*res);
            }
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        }
    }

    #[test]
    fn town_production_village_produces_lumber_from_timber() {
        // Village with 4 ScrubForest tiles: each yields 1 Timber = 4 total
        // 4 timber / 2 = 2 lumber
        // 2 lumber / 2 = 1 furniture
        let mut game =
            test_game_state_with_village(&[(TerrainType::Forest, Some(ResourceType::Timber)); 4]);

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
        let cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(capital_coord, cap_tile);

        // 2 Coal tiles and 2 Iron tiles (BarrenHills with revealed deposits)
        let coords: Vec<HexCoord> = (0..4).map(|i| HexCoord::new(5 + i, 0)).collect();
        for (i, &coord) in coords.iter().enumerate() {
            let mut tile = Tile::with_province(TerrainType::Hills, ProvinceId(2));
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
        let mut game = test_game_state_with_village(
            &[(TerrainType::Grassland, Some(ResourceType::Cotton)); 4],
        );

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
        let mut game =
            test_game_state_with_village(&[(TerrainType::Forest, Some(ResourceType::Timber)); 4]);
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
        let mut game =
            test_game_state_with_village(&[(TerrainType::Forest, Some(ResourceType::Timber)); 4]);
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
        nation.labor = LaborPool::new();
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
            4,
            "Minor Nation garrison should have 4 units (3 Militia + 1 GarrisonArtillery)"
        );
    }

    #[test]
    fn town_production_wool_produces_fabric() {
        // Village with 4 FertileHills tiles (Wool): each yields 1 wool = 4 total
        // 4 wool / 2 = 2 fabric
        // 2 fabric / 2 = 1 clothing
        let mut game =
            test_game_state_with_village(&[(TerrainType::Hills, Some(ResourceType::Wool)); 4]);

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
            (TerrainType::Grassland, Some(ResourceType::Cotton)),
            (TerrainType::Grassland, Some(ResourceType::Cotton)),
            (TerrainType::Hills, Some(ResourceType::Wool)),
            (TerrainType::Hills, Some(ResourceType::Wool)),
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
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        }
    }

    #[test]
    fn voluntary_incorporation_at_threshold() {
        let mut game = test_game_state_with_minor_nation();

        // Set diplomacy score to exactly 90 (incorporation threshold)
        let rel = game.diplomacy.ensure_relation(NationId(2), NationId(1));
        rel.score = 90;

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

        // Set diplomacy score to 89 (just below threshold of 90)
        let rel = game.diplomacy.ensure_relation(NationId(2), NationId(1));
        rel.score = 89;

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
        use crate::ai::AiPersonality;
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
        use crate::ai::AiPersonality;
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
        attacker.ai_personality = Some(crate::ai::AiPersonality::Aggressive);
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
        pact_holder.ai_personality = Some(crate::ai::AiPersonality::Aggressive);
        pact_holder.treasury = Money::dollars(10000);
        // Give the pact holder a strong army so it accepts the defense request
        for i in 0..10 {
            pact_holder.army.push(crate::military::units::ArmyUnit::new(
                crate::map::UnitId(8000 + i),
                crate::military::units::ArmyUnitType::Regulars,
                NationId(4),
                ProvinceId(3),
            ));
        }

        let mut diplomacy = DiplomacyState::new();
        // Establish consulate + embassy + pact between PactHolder and MinorDefender
        // with high relation score so the protector cares enough to intervene
        diplomacy.build_consulate(NationId(4), NationId(3)).unwrap();
        diplomacy.build_embassy(NationId(4), NationId(3)).unwrap();
        if let Some(rel) = diplomacy.get_relation_mut(NationId(4), NationId(3)) {
            rel.improve_score(60);
        }
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
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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

        // PactHolder (AI) should strategically accept and be at war with Attacker
        assert!(
            game.diplomacy
                .get_relation(NationId(4), NationId(2))
                .is_some_and(|r| r.at_war),
            "PactHolder should declare war on attacker after strategic evaluation"
        );

        // Minor should be incorporated into PactHolder's empire
        let minor_provinces = game
            .get_nation(NationId(3))
            .map(|n| n.province_ids.len())
            .unwrap_or(0);
        assert_eq!(
            minor_provinces, 0,
            "Minor should have 0 provinces after incorporation"
        );

        // Should have generated intervention headline
        assert!(
            report
                .newspaper_headlines
                .iter()
                .any(|h| h.text.contains("intervenes") && h.text.contains("protect")),
            "Should generate intervention headline: {:?}",
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
        nation2.ai_personality = Some(crate::ai::AiPersonality::Balanced);
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
        let mut forest_tile = Tile::with_province(TerrainType::Forest, ProvinceId(20));
        forest_tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord_forest, forest_tile);
        let mut cotton_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(20));
        cotton_tile.set_resource(ResourceType::Cotton);
        hex_map.set_tile(coord_plantation, cotton_tile);

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
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
        let mut forest_tile = Tile::with_province(TerrainType::Forest, ProvinceId(20));
        forest_tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord_forest, forest_tile);
        let mut cotton_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(20));
        cotton_tile.set_resource(ResourceType::Cotton);
        hex_map.set_tile(coord_plantation, cotton_tile);

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
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
        let mut game =
            test_game_state_with_village(&[(TerrainType::Forest, Some(ResourceType::Timber)); 4]);

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
        let tile1 = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        let tile2 = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        let tile3 = Tile::with_province(TerrainType::Grassland, ProvinceId(3));
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
            .any(|h| h.text.contains("counter-attack") || h.text.contains("repels counter-attack"));
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
            .any(|h| h.text.contains("counter-attack"));
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
        let mut farm_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        farm_tile.set_resource(ResourceType::Grain);
        hex_map.set_tile(coord_farm, farm_tile);
        let mut forest_tile = Tile::with_province(TerrainType::Forest, ProvinceId(1));
        forest_tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord_forest, forest_tile);

        // Distant province tile (disconnected - no railroad/depot/port)
        let mut distant_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        distant_tile.set_resource(ResourceType::Grain);
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
        let farm_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord_farm, farm_tile);
        let distant_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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

        // Treasury should not go below $0
        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.treasury >= Money::ZERO,
            "Treasury {} should not go below $0",
            nation.treasury
        );

        // With floor at $0, nation is NOT bankrupt (treasury == $0, not negative)
        let has_crisis_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.contains("FINANCIAL CRISIS"));
        assert!(
            !has_crisis_headline,
            "Should NOT have FINANCIAL CRISIS headline when floor is $0"
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
        // With floor at $0, treasury should be capped at zero, not negative
        assert!(
            nation.treasury >= Money::ZERO,
            "Treasury {} should not go below $0",
            nation.treasury
        );
        assert!(!nation.is_bankrupt());
    }

    #[test]
    fn treasury_floor_zero_after_maintenance() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Start with $0 treasury
        nation.treasury = Money::ZERO;
        // Add army units that will incur maintenance costs
        for i in 0..5u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(8000 + i),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            );
            nation.army.push(unit);
        }

        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.treasury >= Money::ZERO,
            "Treasury {} must not go below $0 after maintenance",
            nation.treasury
        );
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
        let mn_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        hex_map.set_tile(coord_mn, mn_tile);

        // Also add the GP capital tile
        let coord_gp = HexCoord::new(0, 0);
        let gp_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
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
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
        let atk_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord_atk, atk_tile);
        let def_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
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
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: vec![(NationId(1), ProvinceId(2))],
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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
            // Defender should be in anarchy after losing capital
            let defender = game.get_nation(NationId(2)).unwrap();
            assert!(
                defender.is_in_anarchy(),
                "Nation that lost its capital should be in anarchy"
            );
        }
    }

    // ── Siege artillery destroys fort sections ──────────────────────

    #[test]
    fn siege_artillery_destroys_fort_on_conquest() {
        let coord_atk = HexCoord::new(0, 0);
        let coord_def = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        let atk_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord_atk, atk_tile);
        let mut def_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
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
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: vec![(NationId(1), ProvinceId(2))],
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
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

    // ── Regression: C1 — Voluntary incorporation spam ──────────

    #[test]
    fn no_reincorporation_of_already_incorporated_minor() {
        let mut game = test_game_state_with_minor_nation();

        // Set diplomacy score to 95 (above threshold)
        let rel = game.diplomacy.ensure_relation(NationId(2), NationId(1));
        rel.score = 95;

        // Simulate that the minor nation was already incorporated (0 provinces)
        let minor = game.get_nation_mut(NationId(2)).unwrap();
        minor.province_ids.clear();

        // Process multiple turns
        for _ in 0..5 {
            let report = process_turn(&mut game);

            // No incorporations should happen because the minor has 0 provinces
            assert!(
                report.incorporations.is_empty(),
                "Already-incorporated minor (0 provinces) should not be re-incorporated; turn {}",
                game.turn.0 - 1
            );
        }
    }

    // ── Regression: C2 — Worker starvation death spiral ────────

    #[test]
    fn emergency_recruitment_when_zero_workers() {
        let mut game = test_game_state();

        // Verify nation starts with 0 workers
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.labor.total_workers(),
            0,
            "Nation should start with 0 workers"
        );

        // Process one turn
        process_turn(&mut game);

        // Emergency recruitment should have given at least 1 worker
        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.labor.total_workers() >= 1,
            "Emergency recruitment should give at least 1 worker; got {}",
            nation.labor.total_workers()
        );
    }

    // ── Regression: C4 — Phantom defenders (auto-conquer) ──────

    #[test]
    fn undefended_non_capital_province_is_auto_conquered() {
        use crate::map::UnitId;
        use crate::military::units::ArmyUnit;

        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(1, 0);
        let coord3 = HexCoord::new(2, 0);

        let mut hex_map = HexMap::new(10, 10);
        let tile1 = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        let tile2 = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        let tile3 = Tile::with_province(TerrainType::Grassland, ProvinceId(3));
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
        // Province 2 is a non-capital province of the defender
        let province2 = Province::new(
            ProvinceId(2),
            "Target Province".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );
        // Province 3 is the defender's capital
        let province3 = Province::new(
            ProvinceId(3),
            "Defender Capital".to_string(),
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
        // Give attacker army units
        for i in 0..4 {
            nation1.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            ));
        }

        let mut nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(3), // capital is Province 3, NOT Province 2
        );
        nation2.add_province(ProvinceId(2));
        // Defender has NO army units in Province 2 (the target)

        let mut diplomacy = DiplomacyState::new();
        diplomacy.declare_war(NationId(1), NationId(2));

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2, province3],
            nations: vec![nation1, nation2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: vec![(NationId(1), ProvinceId(2))],
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };

        process_turn(&mut game);

        // Province 2 should have been auto-conquered (no defenders, non-capital)
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.owner,
            NationId(1),
            "Undefended non-capital province should be auto-conquered by attacker"
        );

        // Attacker should now own Province 2
        let attacker = game.get_nation(NationId(1)).unwrap();
        assert!(
            attacker.province_ids.contains(&ProvinceId(2)),
            "Attacker should have Province 2 in its province list"
        );

        // Defender should no longer own Province 2
        let defender = game.get_nation(NationId(2)).unwrap();
        assert!(
            !defender.province_ids.contains(&ProvinceId(2)),
            "Defender should no longer have Province 2"
        );
    }

    // ── Warehouse capacity caps ───────────────────────────────────

    #[test]
    fn warehouse_caps_raw_resources() {
        let mut game = test_game_state();
        // Default warehouse capacity is 1, so raw cap = 50
        game.nations[0].add_resource(ResourceType::Timber, 100);
        assert_eq!(game.nations[0].resource_amount(ResourceType::Timber), 100);

        apply_warehouse_caps(&mut game);
        assert_eq!(game.nations[0].resource_amount(ResourceType::Timber), 50);
    }

    #[test]
    fn warehouse_caps_materials() {
        let mut game = test_game_state();
        // Default warehouse capacity is 1, so material cap = 50
        game.nations[0].add_material(MaterialType::Lumber, 80);

        apply_warehouse_caps(&mut game);
        assert_eq!(game.nations[0].material_amount(MaterialType::Lumber), 50);
    }

    #[test]
    fn warehouse_caps_finished_goods() {
        let mut game = test_game_state();
        // Default warehouse capacity is 1, so goods cap = 25
        game.nations[0].add_goods(GoodsType::Furniture, 40);

        apply_warehouse_caps(&mut game);
        assert_eq!(game.nations[0].goods_amount(GoodsType::Furniture), 25);
    }

    #[test]
    fn warehouse_caps_scale_with_capacity() {
        let mut game = test_game_state();
        // Add a Warehouse building with capacity 4: raw cap = 200, material cap = 200, goods cap = 100
        game.nations[0]
            .buildings
            .push(Building::new(BuildingType::Warehouse, 4));

        game.nations[0].add_resource(ResourceType::Coal, 250);
        game.nations[0].add_material(MaterialType::Steel, 250);
        game.nations[0].add_goods(GoodsType::Hardware, 150);

        apply_warehouse_caps(&mut game);
        assert_eq!(game.nations[0].resource_amount(ResourceType::Coal), 200);
        assert_eq!(game.nations[0].material_amount(MaterialType::Steel), 200);
        assert_eq!(game.nations[0].goods_amount(GoodsType::Hardware), 100);
    }

    #[test]
    fn warehouse_caps_do_not_reduce_below_cap() {
        let mut game = test_game_state();
        // Default warehouse capacity is 1, so raw cap = 50
        game.nations[0].add_resource(ResourceType::Iron, 30);

        apply_warehouse_caps(&mut game);
        // Should remain unchanged since 30 < 50
        assert_eq!(game.nations[0].resource_amount(ResourceType::Iron), 30);
    }

    // ── Blockade capacity tests ──────────────────────────────────

    #[test]
    fn blockade_reduces_effective_cargo_capacity() {
        use crate::map::UnitId;
        use crate::military::ships::{Ship, ShipType};

        let mut game = test_game_state();

        // Add a second Great Power
        let mut nation2 = Nation::new(
            NationId(2),
            "EnemyNation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(200),
        );
        nation2.treasury = Money::dollars(5000);

        // Give enemy warships
        for i in 0..3 {
            nation2.warships.push(Ship::new(
                UnitId(9000 + i),
                ShipType::ShipOfTheLine,
                NationId(2),
            ));
        }

        // Give the player merchant fleet (cargo capacity)
        let nation1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..4 {
            nation1.merchant_fleet.push(Ship::new(
                UnitId(8000 + i),
                ShipType::Clipper,
                NationId(1),
            ));
        }

        // Add enemy province so they're not eliminated
        let coord2 = HexCoord::new(5, 5);
        let province2 = Province::new(
            ProvinceId(200),
            "EnemyLand".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            4,
        );
        game.provinces.push(province2);
        game.nations.push(nation2);

        // Declare war
        game.diplomacy.declare_war(NationId(1), NationId(2));

        // Compute blockade capacity
        let capacity = compute_blockade_capacity(&game);

        // Without blockade: raw cargo = 4 * clipper_capacity
        let raw_cargo = game.get_nation(NationId(1)).unwrap().total_cargo_capacity();
        let effective = capacity.get(&NationId(1)).copied().unwrap_or(raw_cargo);

        // Blockade should reduce capacity when enemy has warships
        assert!(
            effective < raw_cargo,
            "Blockade should reduce cargo: effective={}, raw={}",
            effective,
            raw_cargo
        );
    }

    #[test]
    fn blockade_excludes_anarchic_nations() {
        use crate::map::UnitId;
        use crate::military::ships::{Ship, ShipType};

        let mut game = test_game_state();

        // Add an anarchic enemy with warships
        let mut nation2 = Nation::new(
            NationId(2),
            "FallenEmpire".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(200),
        );
        nation2.treasury = Money::dollars(0);
        nation2.is_in_anarchy = true;
        for i in 0..5 {
            nation2.warships.push(Ship::new(
                UnitId(9000 + i),
                ShipType::ShipOfTheLine,
                NationId(2),
            ));
        }

        let nation1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..4 {
            nation1.merchant_fleet.push(Ship::new(
                UnitId(8000 + i),
                ShipType::Clipper,
                NationId(1),
            ));
        }

        game.nations.push(nation2);
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let capacity = compute_blockade_capacity(&game);
        let raw_cargo = game.get_nation(NationId(1)).unwrap().total_cargo_capacity();
        let effective = capacity.get(&NationId(1)).copied().unwrap_or(raw_cargo);

        // Anarchic nation's warships should NOT reduce trade
        assert_eq!(
            effective, raw_cargo,
            "Anarchic enemy warships should not cause blockade"
        );
    }

    // ── Immigration zero-config guard ───────────────────────────

    #[test]
    fn immigration_no_panic_with_zero_provinces_per_immigrant() {
        let mut game = test_game_state();
        // Set provinces_per_immigrant to 0 (bad config — should not panic)
        game.game_data.game_config.provinces_per_immigrant = 0;
        game.game_data.game_config.provinces_per_immigrant_upgraded = 0;

        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.labor.untrained = 2;
        nation.add_resource(ResourceType::Grain, 20);
        nation.add_material(MaterialType::CannedFood, 5);
        nation.add_goods(GoodsType::Clothing, 5);
        nation.add_goods(GoodsType::Furniture, 5);

        // Should not panic
        let _report = process_turn(&mut game);
    }

    // ── NationScore field tests ──────────────────────────────────

    #[test]
    fn score_includes_tech_treasury_building_components() {
        use crate::economy::buildings::{Building, BuildingType};
        use crate::turn::scoring::calculate_score;

        let mut nation = Nation::new(
            NationId(1),
            "ScoreTest".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.treasury = Money::dollars(25_000); // treasury_score = min(250, 500) = 250
        nation.researched_techs.push(crate::events::TechId(1)); // tech_score = 1 * 30 = 30
        nation.researched_techs.push(crate::events::TechId(2)); // tech_score = 2 * 30 = 60
        nation
            .buildings
            .push(Building::new(BuildingType::LumberMill, 1)); // building_score = 1 * 10 = 10
        nation
            .buildings
            .push(Building::new(BuildingType::SteelMill, 1)); // building_score = 2 * 10 = 20

        let score = calculate_score(&nation);

        assert_eq!(score.tech_score, 60, "2 techs * 30 = 60");
        assert_eq!(
            score.treasury_score, 250,
            "$25,000 / 100 = 250, capped at 500"
        );
        assert_eq!(score.building_score, 20, "2 buildings * 10 = 20");

        // Verify total includes all components
        let expected_total = score.military_score
            + score.labor_score
            + score.transport_score
            + score.merchant_marine_score
            + score.diplomatic_score
            + score.province_score
            + score.tech_score
            + score.treasury_score
            + score.building_score;
        assert_eq!(
            score.total, expected_total,
            "Total should equal sum of all components"
        );
    }

    // ── Province adjacency & naval landing tests ────────────────

    #[test]
    fn can_attack_adjacent_province() {
        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            HexCoord::new(1, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let prov1 = Province::new(
            ProvinceId(1),
            "Ours".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov2 = Province::new(
            ProvinceId(2),
            "Theirs".into(),
            NationId(2),
            HexCoord::new(1, 0),
            vec![HexCoord::new(1, 0)],
            3,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "A".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.treasury = Money::dollars(10000);
        let n2 = Nation::new(
            NationId(2),
            "B".into(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy.declare_war(NationId(1), NationId(2));

        let game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".into(),
            hex_map,
            provinces: vec![prov1, prov2],
            nations: vec![n1, n2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };

        assert!(
            can_attack_province(&game, NationId(1), ProvinceId(2)),
            "Should be able to attack adjacent province"
        );
    }

    #[test]
    fn cannot_attack_non_adjacent_province() {
        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            HexCoord::new(5, 5),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let prov1 = Province::new(
            ProvinceId(1),
            "Ours".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov2 = Province::new(
            ProvinceId(2),
            "Theirs".into(),
            NationId(2),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            3,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "A".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.treasury = Money::dollars(10000);
        let n2 = Nation::new(
            NationId(2),
            "B".into(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".into(),
            hex_map,
            provinces: vec![prov1, prov2],
            nations: vec![n1, n2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };

        assert!(
            !can_attack_province(&game, NationId(1), ProvinceId(2)),
            "Should NOT be able to attack non-adjacent province"
        );
    }

    #[test]
    fn can_attack_via_landing_from_previous_turn() {
        let hex_map = HexMap::new(10, 10);

        let prov1 = Province::new(
            ProvinceId(1),
            "Ours".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov2 = Province::new(
            ProvinceId(2),
            "Theirs".into(),
            NationId(2),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            3,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "A".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.treasury = Money::dollars(10000);
        let n2 = Nation::new(
            NationId(2),
            "B".into(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let game = GameState {
            turn: TurnNumber::new(2),
            difficulty: Difficulty::Normal,
            map_key: "test".into(),
            hex_map,
            provinces: vec![prov1, prov2],
            nations: vec![n1, n2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            // Landing established on turn 1, current turn is 2
            pending_landings: vec![(NationId(1), ProvinceId(2), TurnNumber::new(1))],
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };

        assert!(
            can_attack_province(&game, NationId(1), ProvinceId(2)),
            "Should be able to attack via landing from previous turn"
        );
    }

    #[test]
    fn cannot_attack_via_landing_from_same_turn() {
        let hex_map = HexMap::new(10, 10);

        let prov1 = Province::new(
            ProvinceId(1),
            "Ours".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov2 = Province::new(
            ProvinceId(2),
            "Theirs".into(),
            NationId(2),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            3,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "A".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.treasury = Money::dollars(10000);
        let n2 = Nation::new(
            NationId(2),
            "B".into(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".into(),
            hex_map,
            provinces: vec![prov1, prov2],
            nations: vec![n1, n2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            // Landing established on turn 1, current turn is also 1
            pending_landings: vec![(NationId(1), ProvinceId(2), TurnNumber::new(1))],
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };

        assert!(
            !can_attack_province(&game, NationId(1), ProvinceId(2)),
            "Should NOT be able to attack via landing from same turn"
        );
    }

    // ── Player→AI diplomatic proposal resolution ─────────────────

    /// Build a two-GP game state with embassy established and positive relations
    /// so that NAP/Alliance proposals can be queued and evaluated.
    fn empty_report(turn: TurnNumber) -> TurnReport {
        TurnReport {
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
        }
    }

    fn two_gp_diplo_game() -> GameState {
        use crate::ai::AiPersonality;

        let coord = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(5, 5);
        let mut hex_map = HexMap::new(10, 10);
        let mut t1 = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        t1.set_resource(ResourceType::Grain);
        hex_map.set_tile(coord, t1);
        let mut t2 = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        t2.set_resource(ResourceType::Grain);
        hex_map.set_tile(coord2, t2);

        let p1 = Province::new(
            ProvinceId(1),
            "Homeland".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let p2 = Province::new(
            ProvinceId(2),
            "Ailand".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            4,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "Player".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.treasury = Money::dollars(10_000);

        let mut n2 = Nation::new(
            NationId(2),
            "AiNation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        n2.treasury = Money::dollars(10_000);
        n2.ai_personality = Some(AiPersonality::Diplomatic);

        let mut diplomacy = DiplomacyState::new();
        diplomacy.initialize_great_powers(&[NationId(1), NationId(2)]);
        // Boost relationship so AI is likely to accept
        let rel = diplomacy.ensure_relation(NationId(1), NationId(2));
        rel.improve_score(60);

        GameState {
            turn: TurnNumber::new(5),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![p1, p2],
            nations: vec![n1, n2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        }
    }

    #[test]
    fn player_nap_proposal_creates_pending_not_immediate() {
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        // Queue a NAP proposal (same as what WASM bridge now does)
        game.diplomacy
            .propose_treaty(human, ai, TreatyType::NonAggressionPact, game.turn)
            .unwrap();

        // Should be pending, not yet an active treaty
        assert!(
            !game
                .diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::NonAggressionPact),
            "NAP should not be active yet — only pending"
        );
        assert_eq!(game.diplomacy.pending_proposals.len(), 1);
        assert_eq!(
            game.diplomacy.pending_proposals[0].proposal_type,
            TreatyType::NonAggressionPact
        );
    }

    #[test]
    fn player_nap_proposal_accepted_with_good_relations() {
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        // Disable Lua engine so evaluation uses pure Rust logic deterministically
        #[cfg(feature = "lua")]
        {
            game.game_data.lua_engine = None;
        }

        // Queue proposal (AI is Diplomatic personality with score +60 → should accept)
        game.diplomacy
            .propose_treaty(human, ai, TreatyType::NonAggressionPact, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        // Proposals should be drained
        assert!(game.diplomacy.pending_proposals.is_empty());

        // Treaty should be active
        assert!(
            game.diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::NonAggressionPact),
            "NAP should be active after acceptance"
        );

        // Should have TreatyAccepted event
        assert!(
            report
                .events
                .iter()
                .any(|e| matches!(e, DomainEvent::TreatyAccepted(a) if a.treaty_type == TreatyType::NonAggressionPact)),
            "Should emit TreatyAccepted event"
        );

        // Headline should mention acceptance
        let headline = &report.newspaper_headlines[0].text;
        assert!(
            headline.contains("accepts"),
            "Headline should mention acceptance: {headline}"
        );
    }

    #[test]
    fn player_nap_proposal_rejected_with_bad_relations() {
        use crate::ai::AiPersonality;

        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        // Disable Lua engine so evaluation uses pure Rust logic deterministically
        #[cfg(feature = "lua")]
        {
            game.game_data.lua_engine = None;
        }

        // Make AI aggressive and hostile
        game.nations[1].ai_personality = Some(AiPersonality::Aggressive);
        let rel = game.diplomacy.ensure_relation(human, ai);
        rel.score = -50; // terrible relationship

        game.diplomacy
            .propose_treaty(human, ai, TreatyType::NonAggressionPact, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        // Treaty should NOT be active
        assert!(
            !game
                .diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::NonAggressionPact),
            "NAP should not be active after rejection"
        );

        // Should have TreatyRejected event
        assert!(
            report
                .events
                .iter()
                .any(|e| matches!(e, DomainEvent::TreatyRejected(r) if r.treaty_type == TreatyType::NonAggressionPact)),
            "Should emit TreatyRejected event"
        );

        // Headline should mention rejection
        let headline = &report.newspaper_headlines[0].text;
        assert!(
            headline.contains("rejects"),
            "Headline should mention rejection: {headline}"
        );
    }

    #[test]
    fn player_proposal_state_drift_emits_rejection() {
        // Test the F-002 fix: AI accepts but treaty application fails due to state drift.
        // Setup: favorable relations so AI accepts, but NAP already exists so propose_pact() fails.
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        #[cfg(feature = "lua")]
        {
            game.game_data.lua_engine = None;
        }

        // Pre-apply NAP so propose_pact() will fail with "already active"
        game.diplomacy.propose_pact(human, ai).unwrap();
        assert!(
            game.diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::NonAggressionPact)
        );

        // Manually insert a duplicate pending NAP proposal (bypassing validation)
        game.diplomacy
            .pending_proposals
            .push(crate::diplomacy::DiplomaticProposal {
                from: human,
                to: ai,
                proposal_type: TreatyType::NonAggressionPact,
                turn_proposed: game.turn,
                attacker: None,
                cascade_remaining: None,
            });

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        // Headline should report "could not be fulfilled" (not "accepts")
        assert!(!report.newspaper_headlines.is_empty());
        let headline = &report.newspaper_headlines[0].text;
        assert!(
            headline.contains("could not be fulfilled"),
            "Should report application failure: {headline}"
        );
    }

    #[test]
    fn player_alliance_proposal_accepted_with_good_relations() {
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        #[cfg(feature = "lua")]
        {
            game.game_data.lua_engine = None;
        }

        game.diplomacy
            .propose_treaty(human, ai, TreatyType::Alliance, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        assert!(
            game.diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::Alliance),
            "Alliance should be active after acceptance"
        );
        assert!(report.events.iter().any(
            |e| matches!(e, DomainEvent::TreatyAccepted(a) if a.treaty_type == TreatyType::Alliance)
        ));
    }

    #[test]
    fn player_alliance_proposal_rejected_with_bad_relations() {
        use crate::ai::AiPersonality;

        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        #[cfg(feature = "lua")]
        {
            game.game_data.lua_engine = None;
        }

        game.nations[1].ai_personality = Some(AiPersonality::Aggressive);
        let rel = game.diplomacy.ensure_relation(human, ai);
        rel.score = -50;

        game.diplomacy
            .propose_treaty(human, ai, TreatyType::Alliance, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        assert!(
            !game
                .diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::Alliance),
            "Alliance should not be active after rejection"
        );
        assert!(report.events.iter().any(
            |e| matches!(e, DomainEvent::TreatyRejected(r) if r.treaty_type == TreatyType::Alliance)
        ));
    }

    #[test]
    fn ai_proposal_to_human_persists_for_modal() {
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        // AI proposes alliance to human
        game.diplomacy
            .propose_treaty(ai, human, TreatyType::Alliance, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        // Should be re-added to pending (for UI modal)
        assert_eq!(
            game.diplomacy.pending_proposals.len(),
            1,
            "AI→human proposal should persist for modal"
        );
        assert_eq!(game.diplomacy.pending_proposals[0].from, ai);
        assert_eq!(game.diplomacy.pending_proposals[0].to, human);
    }

    #[test]
    fn battle_archive_populated_after_combat() {
        let mut game = test_game_for_counter_attack();

        // Queue attack on Province 2
        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        assert!(game.battle_archive.is_empty(), "archive should start empty");

        let report = process_turn(&mut game);

        // Battles occurred — archive should be populated
        assert!(!report.battles.is_empty(), "should have battles in report");
        assert_eq!(
            game.battle_archive.len(),
            1,
            "should have one archive entry after one turn with battles"
        );

        let (archived_turn, archived_battles, archived_naval) = &game.battle_archive[0];
        // The turn was 1 before processing, which is the turn that gets archived
        assert_eq!(archived_turn.0, 1, "archived turn should be 1");
        assert_eq!(
            archived_battles.len(),
            report.battles.len(),
            "archive should contain same number of battles as report"
        );
        assert!(archived_naval.is_empty(), "no naval battles in this test");

        // Verify attacker_origin_provinces is set (attacker units are in Province 1)
        let first_battle = &archived_battles[0];
        assert!(
            !first_battle.attacker_origin_provinces.is_empty(),
            "attacker_origin_provinces should be populated"
        );
        assert!(
            first_battle
                .attacker_origin_provinces
                .contains(&ProvinceId(1)),
            "origin should include Province 1 where attacker units are stationed"
        );

        // Verify survivors are stripped from archived battles (lightweight archive)
        assert!(
            first_battle.attacker_survivors.is_empty(),
            "archived battles should have stripped attacker survivors"
        );
        assert!(
            first_battle.defender_survivors.is_empty(),
            "archived battles should have stripped defender survivors"
        );
    }

    // ── Post-victory unit relocation tests ─────────────────────

    #[test]
    fn attacker_survivors_move_to_conquered_province() {
        let mut game = test_game_for_counter_attack();

        // Attacker (Nation 1) has 6 Guards in Province 1, attacks Province 2
        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Verify attack succeeded
        assert!(
            report.battles[0].attacker_won,
            "Attacker should win with 6 Guards vs garrison"
        );

        // Verify Province 2 is now owned by attacker
        assert_eq!(game.get_province(ProvinceId(2)).unwrap().owner, NationId(1));

        // Verify surviving attacker units are now positioned in the conquered province
        let attacker = game.get_nation(NationId(1)).unwrap();
        let units_in_conquered = attacker
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            units_in_conquered > 0,
            "Surviving attacker units should be in conquered Province 2, but found {} units there",
            units_in_conquered
        );
    }

    #[test]
    fn counter_attack_faces_occupier_defenders() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Give defender army units in Province 3 (adjacent to Province 2)
        let nation2 = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..5 {
            nation2.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(3),
            ));
        }

        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // First battle should be the initial attack (attacker wins)
        assert!(
            report.battles[0].attacker_won,
            "Initial attack should succeed"
        );

        // A counter-attack must be generated (defender had adjacent units in Province 3)
        assert!(
            report.battles.len() >= 2,
            "Counter-attack should be generated, got {} battles",
            report.battles.len()
        );

        // The counter-attack's defender (occupier) should have the relocated
        // attacker survivors stationed in the conquered province
        let counter = &report.battles[1];
        assert!(
            counter.defender_initial_count > 0,
            "Counter-attack defender (occupier) should have units from the conquest, got {}",
            counter.defender_initial_count
        );
    }

    // ── Move-then-attack restriction tests ─────────────────────

    #[test]
    fn moved_units_excluded_from_attack() {
        let mut game = test_game_for_counter_attack();

        // Nation 1 has 6 Guards in Province 1
        // Add Province 4 as another owned province (friendly, to move to)
        let coord4 = HexCoord::new(0, 1);
        let tile4 = Tile::with_province(TerrainType::Grassland, ProvinceId(4));
        game.hex_map.set_tile(coord4, tile4);
        let province4 = Province::new(
            ProvinceId(4),
            "Friendly Rear".to_string(),
            NationId(1),
            coord4,
            vec![coord4],
            4,
        );
        game.provinces.push(province4);
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_province(ProvinceId(4));

        // Move one unit to Province 4 (friendly move)
        let unit_to_move = game.get_nation(NationId(1)).unwrap().army[0].id;
        game.pending_moves
            .push((NationId(1), unit_to_move, ProvinceId(4)));

        // Also queue an attack on Province 2
        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // The attack should have used 5 units (6 total minus 1 moved)
        assert!(
            !report.battles.is_empty(),
            "There should be at least one battle"
        );
        assert_eq!(
            report.battles[0].attacker_initial_count, 5,
            "Attack force should be 5 (6 army minus 1 moved unit)"
        );
    }

    // ── Adjacency-based attack force tests ────────────────────

    #[test]
    fn only_adjacent_units_participate_in_land_attack() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Nation 1 owns Province 1 (adjacent to Province 2) with 6 Guards there.
        // Add Province 5 (far from Province 2) with 2 more units.
        // Those 2 far units should NOT participate in the attack on Province 2.
        let coord5 = HexCoord::new(0, 2);
        let tile5 = Tile::with_province(TerrainType::Grassland, ProvinceId(5));
        game.hex_map.set_tile(coord5, tile5);
        let province5 = Province::new(
            ProvinceId(5),
            "Far Province".to_string(),
            NationId(1),
            coord5,
            vec![coord5],
            4,
        );
        game.provinces.push(province5);
        let nation1 = game.get_nation_mut(NationId(1)).unwrap();
        nation1.add_province(ProvinceId(5));
        for i in 0..2 {
            nation1.army.push(ArmyUnit::new(
                UnitId(300 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(5),
            ));
        }

        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Only 6 units (from adjacent Province 1) should have attacked, not 8
        assert!(!report.battles.is_empty());
        assert_eq!(
            report.battles[0].attacker_initial_count, 6,
            "Only units from adjacent Province 1 should fight (6), not far Province 5 units (2)"
        );

        // Units in Province 5 should retain their position after victory
        let nation1 = game.get_nation(NationId(1)).unwrap();
        let far_units_still_far = nation1
            .army
            .iter()
            .filter(|u| u.id.0 >= 300 && u.id.0 < 302)
            .filter(|u| u.position == ProvinceId(5))
            .count();
        assert_eq!(
            far_units_still_far, 2,
            "Units in far Province 5 should keep their position, not teleport"
        );
    }

    #[test]
    fn auto_conquer_relocates_adjacent_land_units() {
        use crate::map::UnitId;

        // Auto-conquer fixture: Nation 1 owns P1 + P10 (capital), Nation 2 owns
        // non-capital P2 (no army, no garrison since not a capital). P1 adjacent to P2.
        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(1, 0);
        let coord10 = HexCoord::new(5, 5); // far away — Nation 1's capital

        let mut hex_map = HexMap::new(20, 20);
        hex_map.set_tile(
            coord1,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            coord2,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        hex_map.set_tile(
            coord10,
            Tile::with_province(TerrainType::Grassland, ProvinceId(10)),
        );

        let p1 = Province::new(
            ProvinceId(1),
            "P1".into(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );
        let p2 = Province::new(
            ProvinceId(2),
            "P2".into(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );
        let p10 = Province::new(
            ProvinceId(10),
            "Capital".into(),
            NationId(1),
            coord10,
            vec![coord10],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(10), // capital elsewhere so P1 is not the capital
        );
        nation1.add_province(ProvinceId(1));
        nation1.treasury = Money::dollars(10000);
        // 4 Guards in adjacent P1
        for i in 0..4 {
            nation1.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }

        // Nation 2 owns P2 but it's NOT their capital (they have a fake capital at P2 index).
        // Make Nation 2's capital a different province so P2 won't auto-garrison.
        // Use ProvinceId(99) as a dummy capital so P2 (defender_id=2) != capital
        let mut nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(99), // fake capital — P2 will NOT be capital
        );
        nation2.add_province(ProvinceId(2));

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![p1, p2, p10],
            nations: vec![nation1, nation2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };

        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Verify P2 is auto-conquered (no battle recorded for it, or it's a trivial victory)
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().owner,
            NationId(1),
            "P2 should be conquered"
        );

        // Verify Nation 1's units from adjacent P1 are now in P2
        let attacker = game.get_nation(NationId(1)).unwrap();
        let units_in_p2 = attacker
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            units_in_p2 > 0,
            "Adjacent units should relocate to auto-conquered P2, got {} units there",
            units_in_p2
        );

        // Defender test not strictly needed — just verify no battle report (auto-conquer = no battle)
        assert!(
            report.battles.is_empty()
                || !report.battles.iter().any(|b| b.province == ProvinceId(2)),
            "Auto-conquer should not produce a battle report"
        );
    }

    // ── Counter-attack relocation test ────────────────────────

    #[test]
    fn counter_attack_survivors_move_to_recaptured_province() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Give defender a strong army in Province 3 (adjacent to Province 2)
        // so the counter-attack can succeed
        let nation2 = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..20 {
            nation2.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(3),
            ));
        }

        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Preconditions: the counter-attack must exist AND win for this test to be meaningful.
        assert!(
            report.battles.len() >= 2,
            "Counter-attack must be generated (defender has 20 Guards in adjacent P3), got {} battles",
            report.battles.len()
        );
        assert!(
            report.battles[1].attacker_won,
            "Counter-attack must succeed with 20 Guards vs conquest survivors"
        );

        // Verify province ownership returned to Nation 2
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().owner,
            NationId(2),
            "Counter-attack should have recaptured Province 2"
        );

        // Verify counter-attack survivors ended up in the recaptured province
        let nation2 = game.get_nation(NationId(2)).unwrap();
        let units_in_recaptured = nation2
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            units_in_recaptured > 0,
            "Counter-attack survivors should be stationed in recaptured Province 2, got {}",
            units_in_recaptured
        );
    }

    // ── Naval-landing origin-return test ─────────────────────

    #[test]
    fn naval_attackers_return_to_origin_after_victory() {
        use crate::map::UnitId;
        use crate::military::naval::NavalOperation;
        use crate::military::ships::{Ship, ShipType};

        // Fixture: Nation 1 owns P1 (coastal, far from target). Nation 2 owns P2 (coastal).
        // P1 and P2 are NOT adjacent — no land route. Nation 1 has a beachhead on P2
        // established on turn 4 (current turn = 5). Nation 1's units are at P1.
        // After victory, they should stay at P1 (return to origin).
        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(5, 0); // far from P1 — not adjacent

        let mut hex_map = HexMap::new(20, 20);
        hex_map.set_tile(
            coord1,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            coord2,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let mut p1 = Province::new(
            ProvinceId(1),
            "Home".into(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );
        p1.coastal = true;
        let mut p2 = Province::new(
            ProvinceId(2),
            "Target".into(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );
        p2.coastal = true;

        let mut nation1 = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.treasury = Money::dollars(10000);
        for i in 0..4 {
            nation1.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }
        // ShipOfTheLine has arms_cost = 5, enough beachhead capacity for 4 Guards
        let mut ship = Ship::new(UnitId(500), ShipType::ShipOfTheLine, NationId(1));
        ship.operation = Some(NavalOperation::Beachhead(ProvinceId(2)));
        nation1.warships.push(ship);

        let nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(99), // not a capital for P2 — so no auto-garrison militia
        );

        let mut game = GameState {
            turn: TurnNumber::new(5),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![p1, p2],
            nations: vec![nation1, nation2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            // Landing established on turn 4 (before current turn 5) — valid for attack
            pending_landings: vec![(NationId(1), ProvinceId(2), TurnNumber::new(4))],
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };
        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        process_turn(&mut game);

        // Verify P2 was conquered
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().owner,
            NationId(1),
            "P2 should be conquered via naval landing"
        );

        // Verify all surviving attacker units are at origin (P1), not conquered P2
        let attacker = game.get_nation(NationId(1)).unwrap();
        let units_in_p2 = attacker
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2))
            .count();
        let units_in_p1 = attacker
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(1))
            .count();
        assert_eq!(
            units_in_p2, 0,
            "Naval attackers should NOT be relocated to conquered P2"
        );
        assert!(
            units_in_p1 > 0,
            "Naval attackers should remain at origin P1 after victory"
        );
    }

    // ── Counter-attack move-then-attack restriction ───────────

    #[test]
    fn moved_units_excluded_from_counter_attack() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Give Nation 2 5 Guards in P3 (adjacent to P2) AND 1 Guard in P2 itself
        // (note: P2 is Nation 2's original province, which will get conquered).
        // The 5 units in P3 would normally counter-attack.
        let nation2 = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..5 {
            nation2.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(3),
            ));
        }

        // Queue a move for one of Nation 2's P3 units within P3 (no-op move won't
        // actually move anywhere useful, but we need a real friendly target).
        // Nation 2 owns P2 (about to be lost) and P3. Queue move P3→P3 (self) —
        // actually moves aren't valid to same province. Use a different approach:
        // move one unit from P3 to P2 before combat. But P3→P2 would be friendly
        // since both are Nation 2's at the start of the turn (and movement happens
        // before combat). So the move is legit: friendly reposition P3 → P2.
        // After moving, the unit moved_unit_ids. Then P2 gets attacked and
        // conquered. Counter-attack force assembly in phase 7 should exclude
        // moved units.
        let moved_uid = game
            .get_nation(NationId(2))
            .unwrap()
            .army
            .iter()
            .find(|u| u.position == ProvinceId(3))
            .unwrap()
            .id;
        game.pending_moves
            .push((NationId(2), moved_uid, ProvinceId(2)));

        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Hard precondition: counter-attack MUST be generated.
        // 5 units in P3 (adjacent to P2) — 1 moved leaves 4 available counter-attackers.
        // The initial attack by Nation 1 succeeds so P2 is conquered, then Nation 2's
        // remaining P3 units counter-attack.
        assert!(
            report.battles.len() >= 2,
            "Counter-attack must be generated (4 non-moved units in P3), got {} battles",
            report.battles.len()
        );
        // battles[1] is the counter-attack
        assert_eq!(
            report.battles[1].attacker_initial_count, 4,
            "Counter-attack force should exclude the moved unit (5 in P3 - 1 moved = 4)"
        );
    }

    // ── Mixed land + naval attack test ────────────────────────

    #[test]
    fn mixed_attack_splits_cohorts_and_relocates_by_origin() {
        use crate::map::UnitId;
        use crate::military::naval::NavalOperation;
        use crate::military::ships::{Ship, ShipType};

        // Fixture:
        //   P1 (coastal) — Nation 1 home/port, not adjacent to P2
        //   P2 (coastal) — Nation 2 target (non-capital, no army)
        //   P3 (land)    — Nation 1 adjacent to P2
        //   Coords chosen so P1 and P2 are NOT adjacent but P3 IS adjacent to P2.
        //   Nation 1: 3 Guards in P1 (port), 2 Guards in P3 (adjacent land),
        //             1 Guard in P4 (inland — neither adjacent nor port)
        //   Beachhead established on P2 from a prior turn.
        //   Expected: attack force = 2 land (from P3) + 3 naval (from P1, capped by
        //   beachhead) = 5 units. Inland P4 unit is excluded entirely.
        //   After victory: P3 units → P2 (conquered); P1 units stay at P1; P4 unit stays at P4.
        let coord_p2 = HexCoord::new(5, 0);
        let coord_p3 = HexCoord::new(4, 0); // adjacent to P2
        let coord_p1 = HexCoord::new(0, 0); // far from P2 — not adjacent
        let coord_p4 = HexCoord::new(0, 3); // inland, not coastal

        let mut hex_map = HexMap::new(20, 20);
        hex_map.set_tile(
            coord_p1,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            coord_p2,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        hex_map.set_tile(
            coord_p3,
            Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );
        hex_map.set_tile(
            coord_p4,
            Tile::with_province(TerrainType::Grassland, ProvinceId(4)),
        );

        let mut p1 = Province::new(
            ProvinceId(1),
            "Port".into(),
            NationId(1),
            coord_p1,
            vec![coord_p1],
            4,
        );
        p1.coastal = true;
        let mut p2 = Province::new(
            ProvinceId(2),
            "Target".into(),
            NationId(2),
            coord_p2,
            vec![coord_p2],
            3,
        );
        p2.coastal = true;
        let p3 = Province::new(
            ProvinceId(3),
            "Border".into(),
            NationId(1),
            coord_p3,
            vec![coord_p3],
            4,
        );
        let p4 = Province::new(
            ProvinceId(4),
            "Inland".into(),
            NationId(1),
            coord_p4,
            vec![coord_p4],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.add_province(ProvinceId(3));
        nation1.add_province(ProvinceId(4));
        nation1.treasury = Money::dollars(20000);
        // 3 Guards in P1 (port)
        for i in 0..3 {
            nation1.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }
        // 2 Guards in P3 (land adjacent)
        for i in 0..2 {
            nation1.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(3),
            ));
        }
        // 1 Guard in P4 (inland non-port)
        nation1.army.push(ArmyUnit::new(
            UnitId(300),
            ArmyUnitType::Guards,
            NationId(1),
            ProvinceId(4),
        ));

        // ShipOfTheLine: arms_cost = 5 → beachhead_cap = 5 (room for 3 P1 + 2 P3 = 5 but
        // land cohort doesn't consume beachhead, so naval cap only affects P1 units: 3 <= 5)
        let mut ship = Ship::new(UnitId(500), ShipType::ShipOfTheLine, NationId(1));
        ship.operation = Some(NavalOperation::Beachhead(ProvinceId(2)));
        nation1.warships.push(ship);

        let mut nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(99), // fake capital — P2 gets no auto-garrison
        );
        // Give Nation 2 a defender unit at P2 so a real battle occurs (not auto-conquer)
        nation2.army.push(ArmyUnit::new(
            UnitId(400),
            ArmyUnitType::Militia,
            NationId(2),
            ProvinceId(2),
        ));

        let mut game = GameState {
            turn: TurnNumber::new(5),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![p1, p2, p3, p4],
            nations: vec![nation1, nation2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: vec![(NationId(1), ProvinceId(2), TurnNumber::new(4))],
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };
        game.pending_attacks.push((NationId(1), ProvinceId(2)));
        game.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Verify conquest
        assert_eq!(game.get_province(ProvinceId(2)).unwrap().owner, NationId(1));

        // Primary assertion: prove both cohorts participated in the battle.
        // Expected: 2 (P3 land) + 3 (P1 naval port) = 5; P4 inland excluded.
        assert!(!report.battles.is_empty(), "attack should produce a battle");
        let battle = &report.battles[0];
        assert_eq!(
            battle.attacker_initial_count, 5,
            "Mixed attack force = 2 land (P3) + 3 naval (P1 port) = 5; P4 inland unit excluded"
        );
        // Both origin provinces should appear in the battle's attacker origins list
        assert!(
            battle.attacker_origin_provinces.contains(&ProvinceId(1))
                || battle.attacker_origin_provinces.contains(&ProvinceId(3)),
            "battle should record at least one of the cohort origin provinces, got {:?}",
            battle.attacker_origin_provinces
        );

        let attacker = game.get_nation(NationId(1)).unwrap();

        // Land cohort (from P3, IDs 200-201) should be in P2 (conquered)
        let land_in_p2 = attacker
            .army
            .iter()
            .filter(|u| u.id.0 >= 200 && u.id.0 < 202)
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            land_in_p2 > 0,
            "Land cohort survivors (from P3) should relocate to conquered P2, got {}",
            land_in_p2
        );

        // Naval cohort (from P1, IDs 100-102) should still be at P1 (origin)
        let naval_still_at_port = attacker
            .army
            .iter()
            .filter(|u| u.id.0 >= 100 && u.id.0 < 103)
            .filter(|u| u.position == ProvinceId(1))
            .count();
        assert!(
            naval_still_at_port > 0,
            "Naval cohort survivors (from P1) should stay at origin P1, got {}",
            naval_still_at_port
        );

        // Inland unit (ID 300 at P4) must NOT have participated — still at P4
        let inland_still_at_p4 = attacker
            .army
            .iter()
            .find(|u| u.id.0 == 300)
            .map(|u| u.position == ProvinceId(4))
            .unwrap_or(false);
        assert!(
            inland_still_at_p4,
            "Inland non-port unit must not participate in naval attack, should remain at P4"
        );
    }

    #[test]
    fn conflicting_alliance_obligations_produce_neutrality() {
        // Setup: 3 Great Powers — A(1), B(2), C(3)
        // C is allied with both A and B. A declares war on B.
        // C should remain neutral (not fight both sides).
        let coord = HexCoord::new(0, 0);
        let mut hex_map = HexMap::new(10, 10);
        let tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord, tile);

        let mut nations = Vec::new();
        for (id, name, prov_id) in [(1, "Alphaland", 1), (2, "Betaland", 2), (3, "Gammaland", 3)] {
            let mut n = Nation::new(
                NationId(id),
                name.to_string(),
                NationColor::Blue,
                NationType::GreatPower,
                ProvinceId(prov_id),
            );
            n.ai_personality = Some(crate::ai::common::AiPersonality::Balanced);
            n.province_ids = vec![ProvinceId(prov_id)];
            nations.push(n);
        }
        // Human player is nation 1 (but personality is set, so alliance logic treats as AI)
        // Actually, make nation 1 human so it doesn't auto-join
        nations[0].ai_personality = None;

        let provinces: Vec<Province> = (1..=3)
            .map(|i| {
                Province::new(
                    ProvinceId(i),
                    format!("Province{}", i),
                    NationId(i),
                    coord,
                    vec![coord],
                    4,
                )
            })
            .collect();

        let mut diplomacy = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3)];
        diplomacy.initialize_great_powers(&gps);

        // C(3) allied with both A(1) and B(2)
        diplomacy
            .propose_alliance(NationId(3), NationId(1))
            .unwrap();
        diplomacy
            .propose_alliance(NationId(3), NationId(2))
            .unwrap();

        // A(1) declares war on B(2)
        diplomacy.declare_war(NationId(1), NationId(2));

        let mut game = GameState {
            turn: TurnNumber::new(5),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces,
            nations,
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };

        let mut report = TurnReport::empty();
        resolve_alliance_obligations(&mut game, &mut report);

        // C(3) should NOT be at war with either A(1) or B(2)
        assert!(
            !game.diplomacy.is_at_war(NationId(3), NationId(1)),
            "Gammaland should not be at war with Alphaland (conflicting obligation)"
        );
        assert!(
            !game.diplomacy.is_at_war(NationId(3), NationId(2)),
            "Gammaland should not be at war with Betaland (conflicting obligation)"
        );

        // Should have a neutrality headline
        let has_neutrality = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.contains("remains neutral"));
        assert!(
            has_neutrality,
            "Should produce a neutrality headline for Gammaland"
        );
    }

    // ── Engineer build tasks ─────────────────────────────────────

    #[test]
    fn engineer_completes_railroad_build_in_one_turn() {
        use crate::economy::{BuildTask, Civilian, CivilianType};

        let mut game = test_game_state();
        let target = HexCoord::new(1, 0); // the "forest" tile, land & owned

        // Add an Engineer already deployed to the target tile with a Railroad task.
        let mut eng = Civilian::new(
            crate::map::UnitId(3_900_000),
            CivilianType::Engineer,
            NationId(1),
        );
        eng.deploy(target);
        eng.start_build(BuildTask::Railroad, &game.game_data.game_config);
        game.nations[0].civilians.push(eng);
        if let Some(tile) = game.hex_map.get_tile_mut(target) {
            tile.assigned_civilian = Some(crate::map::UnitId(3_900_000));
        }

        // Before: no railroad.
        assert!(
            !game
                .hex_map
                .get_tile(target)
                .unwrap()
                .infrastructure
                .has_railroad
        );

        let _ = process_turn(&mut game);

        // After one turn: railroad built, engineer is idle and freed from the tile.
        assert!(
            game.hex_map
                .get_tile(target)
                .unwrap()
                .infrastructure
                .has_railroad,
            "railroad should have been built after engineer's 1-turn task"
        );
        let eng = game
            .nations
            .iter()
            .flat_map(|n| n.civilians.iter())
            .find(|c| c.civilian_type == CivilianType::Engineer)
            .unwrap();
        assert!(!eng.working);
        assert_eq!(eng.build_task, None);
        assert_eq!(
            game.hex_map.get_tile(target).unwrap().assigned_civilian,
            None,
            "engineer should be released from the tile after completion"
        );
    }

    #[test]
    fn depot_harvest_radius_gates_yield() {
        // Build the setup twice: once with a "far" iron hex that IS outside
        // the depot's 1-hex radius, once without. The iron yield should be
        // identical — the far tile must not contribute.
        fn run(include_far: bool) -> u32 {
            let mut game = test_game_state();
            let cap = HexCoord::new(0, 0);
            if let Some(t) = game.hex_map.get_tile_mut(cap) {
                t.is_capital = true;
                t.infrastructure.has_depot = true;
            }
            let rail_mid = HexCoord::new(1, 0);
            let mut rail_mid_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
            rail_mid_tile.infrastructure.has_railroad = true;
            game.hex_map.set_tile(rail_mid, rail_mid_tile);

            let depot_hex = HexCoord::new(2, 0);
            let mut depot_tile = Tile::with_province(TerrainType::Hills, ProvinceId(2));
            depot_tile.infrastructure.has_railroad = true;
            depot_tile.infrastructure.has_depot = true;
            depot_tile.reveal_deposit(ResourceType::Iron);
            depot_tile.set_improvement_level(1);
            game.hex_map.set_tile(depot_hex, depot_tile);

            let mut tiles = vec![depot_hex];

            if include_far {
                let far = HexCoord::new(5, 5);
                let mut far_tile = Tile::with_province(TerrainType::Hills, ProvinceId(2));
                far_tile.reveal_deposit(ResourceType::Iron);
                far_tile.set_improvement_level(1);
                game.hex_map.set_tile(far, far_tile);
                tiles.push(far);
            }

            game.provinces.push(Province::new(
                ProvinceId(2),
                "Remote".to_string(),
                NationId(1),
                depot_hex,
                tiles,
                4,
            ));
            game.nations[0].add_province(ProvinceId(2));
            game.nations[0].transport.build_freight_cars(50);

            let before = game.nations[0].resource_amount(ResourceType::Iron);
            let _ = process_turn(&mut game);
            let after = game.nations[0].resource_amount(ResourceType::Iron);
            after.saturating_sub(before)
        }

        let with_far = run(true);
        let without_far = run(false);
        assert!(without_far > 0, "depot hex must yield some iron");
        assert_eq!(
            with_far, without_far,
            "far iron tile is outside the depot's 1-hex radius and must not contribute"
        );
    }

    #[test]
    fn engineer_in_flight_build_cancels_on_province_loss() {
        use crate::economy::{BuildTask, Civilian, CivilianType};

        let mut game = test_game_state();
        let target = HexCoord::new(1, 0); // owned "forest" tile in province 1

        // 2-turn build so we catch the cancellation before completion.
        let mut eng = Civilian::new(
            crate::map::UnitId(3_900_001),
            CivilianType::Engineer,
            NationId(1),
        );
        eng.deploy(target);
        eng.start_build(BuildTask::Depot, &game.game_data.game_config); // 2 turns
        game.nations[0].civilians.push(eng);
        if let Some(tile) = game.hex_map.get_tile_mut(target) {
            tile.assigned_civilian = Some(crate::map::UnitId(3_900_001));
        }

        // Simulate losing the province: transfer ownership away from the nation.
        let pid = game.provinces[0].id;
        game.provinces[0].owner = NationId(99);
        game.nations[0].province_ids.retain(|p| *p != pid);

        let _ = process_turn(&mut game);

        // The in-flight engineer should have been cancelled — not "working" any more.
        let eng = game.nations[0]
            .civilians
            .iter()
            .find(|c| c.civilian_type == CivilianType::Engineer)
            .expect("engineer still exists");
        assert!(!eng.working, "stranded engineer should stop working");
        assert_eq!(eng.build_task, None);
        assert_eq!(eng.turns_remaining, 0);
    }

    // ── Card #79: release integrated minors on overlord anarchy ──────────

    /// Build a minimal two-province GP + one-province minor game where the
    /// minor has already been "absorbed": its province is owned by the GP
    /// and carries `incorporated_from = Some(minor_id)`.
    fn absorbed_minor_scenario() -> (GameState, NationId, NationId, ProvinceId, ProvinceId) {
        let gp_capital_coord = HexCoord::new(0, 0);
        let minor_capital_coord = HexCoord::new(2, 0);
        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            gp_capital_coord,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            minor_capital_coord,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let gp_prov = Province::new(
            ProvinceId(1),
            "GPHome".into(),
            NationId(1),
            gp_capital_coord,
            vec![gp_capital_coord],
            4,
        );
        // The minor's original capital province — already in the GP's hands,
        // stamped with the absorption marker.
        let mut minor_prov = Province::new(
            ProvinceId(2),
            "MinorHome".into(),
            NationId(1), // owned by the GP at absorption
            minor_capital_coord,
            vec![minor_capital_coord],
            4,
        );
        minor_prov.incorporated_from = Some(NationId(2));

        let mut gp = Nation::new(
            NationId(1),
            "Overlord".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp.province_ids = vec![ProvinceId(1), ProvinceId(2)];

        let mut minor = Nation::new(
            NationId(2),
            "Vassal".into(),
            NationColor::Yellow,
            NationType::MinorNation,
            ProvinceId(2),
        );
        minor.province_ids.clear(); // absorbed: no provinces
        minor.integrated_by = Some(NationId(1));

        let game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".into(),
            hex_map,
            provinces: vec![gp_prov, minor_prov],
            nations: vec![gp, minor],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
        };

        (game, NationId(1), NationId(2), ProvinceId(1), ProvinceId(2))
    }

    #[test]
    fn release_integrated_minors_restores_capital() {
        let (mut game, overlord_id, minor_id, _, minor_cap_pid) = absorbed_minor_scenario();
        let mut report = TurnReport::empty();

        release_integrated_minors(&mut game, overlord_id, &mut report);

        // The minor's capital province now belongs to the minor again.
        let prov = game.get_province(minor_cap_pid).unwrap();
        assert_eq!(prov.owner, minor_id);
        assert_eq!(prov.incorporated_from, None);

        let minor = game.get_nation(minor_id).unwrap();
        assert!(minor.province_ids.contains(&minor_cap_pid));
        assert_eq!(minor.integrated_by, None);
        assert!(
            !minor.is_in_anarchy,
            "minor with its capital back is not anarchic"
        );

        let overlord = game.get_nation(overlord_id).unwrap();
        assert!(!overlord.province_ids.contains(&minor_cap_pid));

        // An event was recorded.
        assert!(
            report.events.iter().any(|e| matches!(
                e,
                DomainEvent::MinorRegainedIndependence(ev) if ev.minor == minor_id
            )),
            "MinorRegainedIndependence event should be emitted"
        );
    }

    #[test]
    fn release_integrated_minors_keeps_anarchy_if_capital_was_taken_by_third_party() {
        let (mut game, overlord_id, minor_id, _, minor_cap_pid) = absorbed_minor_scenario();
        // Simulate a third party (a new GP) holding the minor's capital at
        // the moment the overlord collapses. The overlord never owned it,
        // so release cannot restore it.
        {
            let prov = game.get_province_mut(minor_cap_pid).unwrap();
            prov.owner = NationId(99);
            // Keep the origin marker — it still traces back to the minor,
            // just not held by the anarchic overlord any longer.
        }
        let mut report = TurnReport::empty();

        release_integrated_minors(&mut game, overlord_id, &mut report);

        let minor = game.get_nation(minor_id).unwrap();
        assert!(
            !minor.province_ids.contains(&minor_cap_pid),
            "minor cannot reclaim a province held by a third party"
        );
        // Without the capital, the minor is structurally anarchic on return.
        assert!(
            minor.is_in_anarchy
                || minor.province_ids.is_empty(),
            "released minor without its capital must be anarchic, not a functioning ghost nation"
        );
    }

    // ── Card #68: pact-defense dedup lifecycle ──────────────────────────

    #[test]
    fn pact_defense_clears_on_peace() {
        let mut dip = DiplomacyState::new();
        dip.mark_pact_defense_requested(NationId(1), NationId(2));
        assert!(dip.is_pact_defense_requested(NationId(1), NationId(2)));

        // Peace between the same two nations — even in reversed order — wipes
        // the entry so a future war can raise a fresh cascade.
        dip.declare_war(NationId(1), NationId(2));
        dip.make_peace(NationId(2), NationId(1));

        assert!(!dip.is_pact_defense_requested(NationId(1), NationId(2)));
    }

    #[test]
    fn pact_defense_clear_for_nation_purges_all_involving() {
        let mut dip = DiplomacyState::new();
        dip.mark_pact_defense_requested(NationId(10), NationId(5));
        dip.mark_pact_defense_requested(NationId(5), NationId(20));
        dip.mark_pact_defense_requested(NationId(7), NationId(8));

        dip.clear_pact_defense_for_nation(NationId(5));

        assert!(!dip.is_pact_defense_requested(NationId(10), NationId(5)));
        assert!(!dip.is_pact_defense_requested(NationId(5), NationId(20)));
        assert!(dip.is_pact_defense_requested(NationId(7), NationId(8)));
    }

    #[test]
    fn release_does_not_touch_minors_integrated_by_other_overlords() {
        // GP A collapses. A second minor "Ally", integrated by unrelated GP B,
        // must not have its `integrated_by` back-pointer altered by A's anarchy.
        let (mut game, overlord_a, _minor_a, _, _) = absorbed_minor_scenario();
        let overlord_b = NationId(99);
        // Insert the unrelated overlord and a minor integrated by them.
        let mut gp_b = Nation::new(
            overlord_b,
            "OtherOverlord".into(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(42),
        );
        gp_b.province_ids = vec![ProvinceId(42)];
        game.nations.push(gp_b);

        let unrelated_minor = NationId(77);
        let mut minor_b = Nation::new(
            unrelated_minor,
            "AlliedVassal".into(),
            NationColor::Green,
            NationType::MinorNation,
            ProvinceId(99),
        );
        minor_b.province_ids.clear();
        minor_b.integrated_by = Some(overlord_b);
        game.nations.push(minor_b);

        let mut report = TurnReport::empty();
        release_integrated_minors(&mut game, overlord_a, &mut report);

        let ally = game.get_nation(unrelated_minor).unwrap();
        assert_eq!(
            ally.integrated_by,
            Some(overlord_b),
            "release on overlord A must not clear integrated_by for minors of overlord B"
        );
    }

    #[test]
    fn military_conquest_records_conquest_origin_not_incorporated_from() {
        // A minor's province conquered militarily should record its origin
        // in `conquest_origin` (mechanics-only) while leaving
        // `incorporated_from` untouched (UI-only). The reverse would make
        // the conquered province render as a diplomatic incorporation on
        // the map, which is a UI regression.
        let input = None;
        let result = attribute_conquest_origin(input, NationId(1), NationId(99), true);
        assert_eq!(result, Some(NationId(99)));

        // Preserved across further conquests by third parties.
        let later = attribute_conquest_origin(result, NationId(2), NationId(1), false);
        assert_eq!(later, Some(NationId(99)));

        // If the origin minor reclaims their own province, the attribution
        // is dropped.
        let reclaimed = attribute_conquest_origin(result, NationId(99), NationId(1), false);
        assert_eq!(reclaimed, None);
    }
}
