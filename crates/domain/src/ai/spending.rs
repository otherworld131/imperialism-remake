#![allow(unused_labels)]
//! Need-based spending system for AI treasury allocation.
//!
//! Replaces hard-capped independent spending functions with a centralized
//! scoring loop. Each iteration scores every spending category, picks the
//! highest-value action, and executes it. Repeats until treasury hits the
//! reserve floor or nothing scores above the minimum threshold.
//!
//! All weights are Lua-configurable per personality.

use crate::economy::civilians::{Civilian, CivilianType, next_civilian_id};
use crate::game_state::GameState;
use crate::map::{build_depot, build_railroad, is_province_connected};
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;

use super::common::{AiPersonality, get_personality, next_unit_id};
use super::economy::{get_railroad_network, find_cheapest_path, score_province};

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpendingCategory {
    Military,
    Infrastructure,
    Consulate,
    Embassy,
    Civilian,
}

struct SpendingWeights {
    military_weight: f64,
    economy_weight: f64,
    diplomacy_weight: f64,
    reserve: Money,
    min_threshold: f64,
}

impl SpendingWeights {
    fn from_personality(personality: AiPersonality) -> Self {
        match personality {
            AiPersonality::Aggressive => Self {
                military_weight: 1.8,
                economy_weight: 0.6,
                diplomacy_weight: 0.3,
                reserve: Money::dollars(500),
                min_threshold: 3.0,
            },
            AiPersonality::Balanced => Self {
                military_weight: 1.0,
                economy_weight: 1.0,
                diplomacy_weight: 0.8,
                reserve: Money::dollars(1000),
                min_threshold: 5.0,
            },
            AiPersonality::Economic => Self {
                military_weight: 0.7,
                economy_weight: 1.5,
                diplomacy_weight: 0.8,
                reserve: Money::dollars(1500),
                min_threshold: 5.0,
            },
            AiPersonality::Diplomatic => Self {
                military_weight: 0.5,
                economy_weight: 1.2,
                diplomacy_weight: 1.8,
                reserve: Money::dollars(1000),
                min_threshold: 5.0,
            },
        }
    }
}

struct ScoredAction {
    category: SpendingCategory,
    score: f64,
    cost: Money,
}

// ── Main entry point ─────────────────────────────────────────────

/// Run the need-based spending loop for one AI nation.
///
/// Scores military, infrastructure, consulate, embassy, and civilian hiring
/// each iteration and executes the highest-value action. Stops when treasury
/// hits the reserve floor or nothing scores above threshold.
pub(crate) fn ai_scored_spending(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<String>,
) {
    let personality = get_personality(game, nation_id);
    let weights = load_weights(game, personality);

    // Safety limit: max 20 spending actions per turn to prevent infinite loops
    for _iteration in 0..20 {
        let treasury = match game.get_nation(nation_id) {
            Some(n) => n.treasury,
            None => return,
        };

        let available = match treasury.checked_sub(weights.reserve) {
            Some(a) if a > Money::ZERO => a,
            _ => break,
        };

        // Score all categories
        let mut options: Vec<ScoredAction> = Vec::new();

        if let Some(opt) = score_military(game, nation_id, &weights)
            && opt.cost <= available
        {
            options.push(opt);
        }
        if let Some(opt) = score_infrastructure(game, nation_id, &weights)
            && opt.cost <= available
        {
            options.push(opt);
        }
        if let Some(opt) = score_consulate(game, nation_id, &weights)
            && opt.cost <= available
        {
            options.push(opt);
        }
        if let Some(opt) = score_embassy(game, nation_id, &weights)
            && opt.cost <= available
        {
            options.push(opt);
        }
        if let Some(opt) = score_civilian(game, nation_id, &weights)
            && opt.cost <= available
        {
            options.push(opt);
        }

        // Pick highest-scoring action above threshold
        options.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        match options.first() {
            Some(best) if best.score > weights.min_threshold => {
                let cat = best.category;
                if game.ai_debug {
                    let name = game
                        .get_nation(nation_id)
                        .map(|n| n.name.as_str())
                        .unwrap_or("?");
                    eprintln!(
                        "[AI:{}:spending] {:?} score={:.1} cost=${}",
                        name,
                        cat,
                        best.score,
                        best.cost.as_dollars()
                    );
                }
                execute(game, nation_id, cat, actions);
            }
            _ => break,
        }
    }
}

// ── Scoring functions ────────────────────────────────────────────

fn score_military(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;
    let army_count = nation.army.len() as f64;
    let province_count = nation.province_count() as f64;

    // Threat: based on relative military strength vs other Great Powers
    let mut threat = 0.0f64;
    let mut strongest_rival_army = 0usize;
    for other in &game.nations {
        if other.id == nation_id || !other.is_great_power() {
            continue;
        }
        strongest_rival_army = strongest_rival_army.max(other.army.len());
        let at_war = game
            .diplomacy
            .get_relation(nation_id, other.id)
            .is_some_and(|r| r.at_war);
        if at_war {
            threat += 20.0 + other.army.len() as f64 * 2.0;
        } else {
            // Rivals with larger armies are a latent threat
            let their_army = other.army.len() as f64;
            if their_army > army_count {
                threat += (their_army - army_count) * 1.5;
            }
        }
    }

    // Desired army scales with territory and relative power
    let desired = (province_count * 1.5).max(strongest_rival_army as f64 * 0.6);
    let deficit = (desired - army_count).max(0.0) * 8.0;
    let territory = province_count * 2.0;
    let saturation = army_count * 3.0;

    // Penalty for building army when economy is weak — soldiers without workers
    // are unsustainable. Scale down military value when worker count is low.
    // Exception: nations with very few units still need basic defense.
    let workers = nation.labor.total_workers() as f64;
    let economy_penalty = if army_count >= 8.0 && workers <= 1.0 {
        0.3 // large army with no economy — stop building
    } else if army_count >= 5.0 && workers <= 2.0 {
        0.6 // moderate army, weak economy — slow down
    } else {
        1.0
    };

    let raw = (threat + deficit + territory - saturation).max(0.0) * economy_penalty;
    let score = raw * weights.military_weight;

    // Cost of cheapest unit
    let cost = Money::dollars(500); // Regulars

    if score > 0.0 {
        Some(ScoredAction {
            category: SpendingCategory::Military,
            score,
            cost,
        })
    } else {
        None
    }
}

fn score_infrastructure(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;
    let capital_pid = nation.capital_province_id;
    let capital_tile = game.get_province(capital_pid)?.capital_tile;

    // Count disconnected provinces with resources
    let mut disconnected_count = 0u32;
    let mut resource_value = 0u32;
    for &pid in &nation.province_ids {
        if pid == capital_pid {
            continue;
        }
        if !is_province_connected(&game.hex_map, capital_tile, pid, &game.provinces) {
            // Also check adjacency (matching connected_provinces() logic)
            let capital_province = game.get_province(capital_pid);
            let adjacent = capital_province
                .map(|cp| {
                    let cap_neighbors: std::collections::HashSet<_> =
                        cp.tiles.iter().flat_map(|t| t.neighbors()).collect();
                    game.get_province(pid)
                        .map(|p| p.tiles.iter().any(|t| cap_neighbors.contains(t)))
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if !adjacent {
                disconnected_count += 1;
                if let Some(prov) = game.get_province(pid) {
                    resource_value += score_province(&game.hex_map, prov, nation);
                }
            }
        }
    }

    if disconnected_count == 0 {
        return None;
    }

    // Food urgency
    let total_food = nation.resource_amount(ResourceType::Grain)
        + nation.resource_amount(ResourceType::Fruit)
        + nation.resource_amount(ResourceType::Livestock);
    let workers = nation.labor.total_workers();
    let food_urgency = if total_food <= workers {
        20.0
    } else if total_food <= workers * 2 {
        10.0
    } else {
        2.0
    };

    let raw = disconnected_count as f64 * 8.0 + resource_value as f64 / 10.0 + food_urgency;
    let score = raw * weights.economy_weight;

    // Cost: minimum $500 for a railroad segment, $2000 for a depot
    let cost = Money::dollars(500);

    Some(ScoredAction {
        category: SpendingCategory::Infrastructure,
        score,
        cost,
    })
}

fn score_consulate(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let _nation = game.get_nation(nation_id)?;

    // Count minor nations without consulates that have tradeable resources
    let mut available = 0u32;
    let mut trade_potential = 0u32;
    let mut existing_consulates = 0u32;

    for n in &game.nations {
        if n.is_great_power() || n.province_ids.is_empty() {
            continue;
        }
        if let Some(rel) = game.diplomacy.get_relation(nation_id, n.id)
            && rel.has_consulate
        {
            existing_consulates += 1;
            continue;
        }
        // Count tradeable resource tiles
        let potential: u32 = game
            .provinces
            .iter()
            .filter(|p| p.owner == n.id)
            .flat_map(|p| &p.tiles)
            .filter_map(|coord| {
                game.hex_map
                    .get_tile(*coord)
                    .and_then(|t| t.calculate_yield())
            })
            .filter(|y| y.resource.is_tradeable())
            .map(|y| y.quantity)
            .sum();
        if potential > 0 {
            available += 1;
            trade_potential += potential;
        }
    }

    if available == 0 {
        return None;
    }

    let raw = available as f64 * 5.0 + trade_potential as f64 / 10.0
        - existing_consulates as f64 * 2.0;
    let score = raw.max(0.0) * weights.diplomacy_weight;

    Some(ScoredAction {
        category: SpendingCategory::Consulate,
        score,
        cost: Money::dollars(game.game_data.game_config.consulate_cost),
    })
}

fn score_embassy(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let _nation = game.get_nation(nation_id)?;

    // Count minor nations with consulate but no embassy
    let mut upgradeable = 0u32;
    for n in &game.nations {
        if n.is_great_power() || n.province_ids.is_empty() {
            continue;
        }
        if let Some(rel) = game.diplomacy.get_relation(nation_id, n.id)
            && rel.has_consulate
            && !rel.has_embassy
        {
            upgradeable += 1;
        }
    }

    if upgradeable == 0 {
        return None;
    }

    // Election proximity bonus
    let year = game.turn.year();
    let turns_to_election = (10 - (year % 10)) * 4; // rough quarters to next decade
    let election_bonus = if turns_to_election <= 16 { 20.0 } else { 3.0 };

    let raw = upgradeable as f64 * 4.0 + election_bonus;
    let score = raw * weights.diplomacy_weight;

    Some(ScoredAction {
        category: SpendingCategory::Embassy,
        score,
        cost: Money::dollars(game.game_data.game_config.embassy_cost),
    })
}

fn score_civilian(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;

    // Civilian construction requires an expert worker
    if game.game_data.game_config.civilian_costs_expert && nation.labor.expert == 0 {
        return None;
    }

    // Count improvable tiles across owned provinces
    let mut improvable_tiles = 0u32;
    for &pid in &nation.province_ids {
        if let Some(province) = game.get_province(pid) {
            for &coord in &province.tiles {
                if let Some(tile) = game.hex_map.get_tile(coord) {
                    let terrain = tile.terrain();
                    if terrain.is_improvable() && tile.improvement_level() < terrain.max_improvement_level()
                    {
                        improvable_tiles += 1;
                    }
                }
            }
        }
    }

    let civilian_count = nation.civilians.len();
    let idle_civilians = nation
        .civilians
        .iter()
        .filter(|c| !c.working && c.turns_remaining == 0)
        .count();

    let coverage = if civilian_count == 0 {
        15.0
    } else if civilian_count < 2 {
        10.0
    } else if civilian_count < 4 {
        5.0
    } else {
        1.0
    };
    let idle_penalty = idle_civilians as f64 * 8.0;

    let raw = (improvable_tiles as f64 * 2.0 + coverage - idle_penalty).max(0.0);
    let score = raw * weights.economy_weight;

    // Cost of cheapest civilian (Farmer: $100)
    let cost = Money::dollars(100);

    if score > 0.0 {
        Some(ScoredAction {
            category: SpendingCategory::Civilian,
            score,
            cost,
        })
    } else {
        None
    }
}

// ── Execution functions ──────────────────────────────────────────

fn execute(
    game: &mut GameState,
    nation_id: NationId,
    category: SpendingCategory,
    actions: &mut Vec<String>,
) {
    match category {
        SpendingCategory::Military => execute_military(game, nation_id, actions),
        SpendingCategory::Infrastructure => execute_infrastructure(game, nation_id),
        SpendingCategory::Consulate => execute_consulate(game, nation_id),
        SpendingCategory::Embassy => execute_embassy(game, nation_id),
        SpendingCategory::Civilian => execute_civilian(game, nation_id),
    }
}

fn execute_military(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let turn_number = game.turn.0;
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let capital = nation.capital_province_id;
    let nation_name = nation.name.clone();
    let variety_seed = (turn_number as usize).wrapping_mul(nation_id.0 as usize + 7);
    let army_count = nation.army.len();

    // Pick unit type based on army composition and variety
    let unit_type = if army_count < 3 {
        // Early game: mostly regulars
        let options = [ArmyUnitType::Regulars, ArmyUnitType::Regulars, ArmyUnitType::Grenadiers];
        options[variety_seed % options.len()]
    } else if army_count < 8 {
        // Mid game: mix
        let options = [ArmyUnitType::Grenadiers, ArmyUnitType::LightArtillery, ArmyUnitType::Grenadiers];
        options[variety_seed % options.len()]
    } else {
        // Late game: artillery focus
        let options = [ArmyUnitType::LightArtillery, ArmyUnitType::Grenadiers, ArmyUnitType::LightArtillery];
        options[variety_seed % options.len()]
    };

    let cost = match unit_type {
        ArmyUnitType::LightArtillery => Money::dollars(2000),
        ArmyUnitType::Grenadiers => Money::dollars(1000),
        _ => Money::dollars(500),
    };

    if let Some(remaining) = nation.treasury.checked_sub(cost) {
        nation.treasury = remaining;
        let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
        nation.army.push(unit);
        actions.push(format!(
            "{} has been expanding its military forces",
            nation_name
        ));
    }
}

fn execute_infrastructure(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    let capital_pid = nation.capital_province_id;
    let capital_tile = match game.get_province(capital_pid) {
        Some(p) => p.capital_tile,
        None => return,
    };

    // Build depot on capital if missing
    let cap_has_depot = game
        .hex_map
        .get_tile(capital_tile)
        .is_some_and(|t| t.infrastructure.has_depot);
    if !cap_has_depot {
        if let Ok(cost) = build_depot(&mut game.hex_map, capital_tile)
            && let Some(nation) = game.get_nation_mut(nation_id)
        {
            nation.treasury -= cost;
        }
        return;
    }

    // Find the highest-scoring disconnected province
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    let mut best_pid: Option<ProvinceId> = None;
    let mut best_score = 0u32;
    for &pid in &nation.province_ids {
        if pid == capital_pid {
            continue;
        }
        if is_province_connected(&game.hex_map, capital_tile, pid, &game.provinces) {
            continue;
        }
        // Check adjacency
        let capital_province = game.get_province(capital_pid);
        let adjacent = capital_province
            .map(|cp| {
                let cap_neighbors: std::collections::HashSet<_> =
                    cp.tiles.iter().flat_map(|t| t.neighbors()).collect();
                game.get_province(pid)
                    .map(|p| p.tiles.iter().any(|t| cap_neighbors.contains(t)))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if adjacent {
            continue;
        }
        if let Some(prov) = game.get_province(pid) {
            let s = score_province(&game.hex_map, prov, nation);
            if s > best_score {
                best_score = s;
                best_pid = Some(pid);
            }
        }
    }

    let target_pid = match best_pid {
        Some(pid) => pid,
        None => return,
    };

    // Build depot on target if needed
    let target_tile = match game.get_province(target_pid) {
        Some(p) => p.capital_tile,
        None => return,
    };
    let has_depot = game
        .hex_map
        .get_tile(target_tile)
        .is_some_and(|t| t.infrastructure.has_depot);
    if !has_depot {
        if let Ok(cost) = build_depot(&mut game.hex_map, target_tile)
            && let Some(nation) = game.get_nation_mut(nation_id)
        {
            nation.treasury -= cost;
        }
        return;
    }

    // Build railroad path
    let network = get_railroad_network(&game.hex_map, capital_tile);
    if let Some(path) = find_cheapest_path(&game.hex_map, &network, target_tile) {
        for &coord in &path {
            let treasury = game
                .get_nation(nation_id)
                .map(|n| n.treasury)
                .unwrap_or(Money::ZERO);
            if treasury < Money::dollars(500) {
                break;
            }
            if let Ok(cost) = build_railroad(&mut game.hex_map, coord)
                && let Some(nation) = game.get_nation_mut(nation_id)
            {
                nation.treasury -= cost;
            }
        }
    }
}

fn execute_consulate(game: &mut GameState, nation_id: NationId) {
    let cost = Money::dollars(game.game_data.game_config.consulate_cost);

    // Find best minor nation to build consulate with (most trade potential)
    let mut best_mn: Option<NationId> = None;
    let mut best_potential = 0u32;

    for n in &game.nations {
        if n.is_great_power() || n.province_ids.is_empty() {
            continue;
        }
        if game
            .diplomacy
            .get_relation(nation_id, n.id)
            .is_some_and(|r| r.has_consulate)
        {
            continue;
        }
        let potential: u32 = game
            .provinces
            .iter()
            .filter(|p| p.owner == n.id)
            .flat_map(|p| &p.tiles)
            .filter_map(|coord| {
                game.hex_map
                    .get_tile(*coord)
                    .and_then(|t| t.calculate_yield())
            })
            .filter(|y| y.resource.is_tradeable())
            .map(|y| y.quantity)
            .sum();
        if potential > best_potential {
            best_potential = potential;
            best_mn = Some(n.id);
        }
    }

    if let Some(mn_id) = best_mn
        && game.diplomacy.build_consulate(nation_id, mn_id).is_ok()
        && let Some(nation) = game.get_nation_mut(nation_id)
    {
        nation.treasury -= cost;
    }
}

fn execute_embassy(game: &mut GameState, nation_id: NationId) {
    let cost = Money::dollars(game.game_data.game_config.embassy_cost);

    // Find best minor nation to upgrade (has consulate, no embassy)
    let mut best_mn: Option<NationId> = None;
    let mut best_relation = i32::MIN;

    for n in &game.nations {
        if n.is_great_power() || n.province_ids.is_empty() {
            continue;
        }
        if let Some(rel) = game.diplomacy.get_relation(nation_id, n.id)
            && rel.has_consulate
            && !rel.has_embassy
            && rel.score > best_relation
        {
            best_relation = rel.score;
            best_mn = Some(n.id);
        }
    }

    if let Some(mn_id) = best_mn
        && game.diplomacy.build_embassy(nation_id, mn_id).is_ok()
        && let Some(nation) = game.get_nation_mut(nation_id)
    {
        nation.treasury -= cost;
    }
}

fn execute_civilian(game: &mut GameState, nation_id: NationId) {
    let civilian_costs_expert = game.game_data.game_config.civilian_costs_expert;

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Civilian construction requires an expert worker (original Imperialism rule)
    if civilian_costs_expert && nation.labor.expert == 0 {
        return;
    }

    let civilian_count = nation.civilians.len();

    // Pick civilian type based on needs
    let civ_type = if civilian_count < 2 {
        CivilianType::Farmer
    } else {
        let has_forester = nation
            .civilians
            .iter()
            .any(|c| c.civilian_type == CivilianType::Forester);
        if has_forester {
            CivilianType::Miner
        } else {
            CivilianType::Forester
        }
    };

    let cost = civ_type.creation_cost();
    if let Some(remaining) = nation.treasury.checked_sub(cost) {
        nation.treasury = remaining;
        if civilian_costs_expert {
            nation.labor.expert -= 1;
        }
        let civilian = Civilian::new(next_civilian_id(), civ_type, nation_id);
        nation.civilians.push(civilian);
    }
}

// ── Config loading ───────────────────────────────────────────────

fn load_weights(game: &GameState, personality: AiPersonality) -> SpendingWeights {
    let mut w = SpendingWeights::from_personality(personality);

    #[cfg(feature = "lua")]
    if let Some(engine) = &game.game_data.lua_engine
        && let Some(cfg) = super::lua_bridge::lua_get_config(engine, personality)
    {
        if let Some(v) = cfg.spending_military_weight {
            w.military_weight = v;
        }
        if let Some(v) = cfg.spending_economy_weight {
            w.economy_weight = v;
        }
        if let Some(v) = cfg.spending_diplomacy_weight {
            w.diplomacy_weight = v;
        }
        if let Some(v) = cfg.treasury_reserve {
            w.reserve = Money::dollars(v);
        }
        if let Some(v) = cfg.min_score_threshold {
            w.min_threshold = v;
        }
    }

    // Suppress unused variable warning when lua feature is off
    let _ = game;
    w
}
