#![allow(unused_labels)]
use crate::economy::buildings::{Building, BuildingType};
use crate::economy::trade;
use crate::game_state::GameState;
use crate::hex::HexCoord;
use crate::map::hex_map::HexMap;
use crate::map::{Province, railroad_cost};
#[cfg(test)]
use crate::map::{build_depot, build_railroad, is_province_connected};
use crate::nation::Nation;
use crate::types::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use super::common::{AiPersonality, get_personality};

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

/// BFS from the capital tile to find all tiles reachable via the connected
/// railroad/depot network. Mirrors the traversal logic in `is_province_connected`.
#[allow(dead_code)]
pub(super) fn get_railroad_network(hex_map: &HexMap, capital_tile: HexCoord) -> HashSet<HexCoord> {
    get_rail_network_for_nation(hex_map, &[capital_tile])
}

/// Multi-source railroad BFS. Seeds from every tile in `capital_tiles` (which
/// should be the nation's own capital plus every owned country-capital tile —
/// captured foreign capitals act as independent rail-network anchors).
pub(super) fn get_rail_network_for_nation(
    hex_map: &HexMap,
    capital_tiles: &[HexCoord],
) -> HashSet<HexCoord> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let capital_set: HashSet<HexCoord> = capital_tiles.iter().copied().collect();
    for &c in capital_tiles {
        queue.push_back(c);
        visited.insert(c);
    }

    while let Some(current) = queue.pop_front() {
        if let Some(tile) = hex_map.get_tile(current)
            && (tile.infrastructure.has_railroad
                || tile.infrastructure.has_depot
                || capital_set.contains(&current))
        {
            for neighbor in current.neighbors() {
                if !visited.contains(&neighbor)
                    && let Some(n_tile) = hex_map.get_tile(neighbor)
                    && (n_tile.infrastructure.has_railroad || n_tile.infrastructure.has_depot)
                {
                    visited.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }
    }
    visited
}

/// Score a province by how much its resources are needed right now.
///
/// Considers mill input deficits, treasury needs, and current warehouse surplus.
/// A province producing Coal when the SteelMill is starving for input scores
/// far higher than one producing Timber the nation already has 20 of.
/// Per-resource demand weights for this nation based on mill deficits,
/// money urgency, and food security. Used by both `score_province` and
/// the depot-placement planner to value resource tiles consistently.
pub(super) fn compute_resource_demand(nation: &Nation) -> HashMap<ResourceType, f64> {
    let mut demand: HashMap<ResourceType, f64> = HashMap::new();

    for building in &nation.buildings {
        match building.building_type {
            BuildingType::LumberMill => {
                let need = building.effective_capacity() * 2;
                let have = nation.resource_amount(ResourceType::Timber);
                let deficit = need.saturating_sub(have);
                *demand.entry(ResourceType::Timber).or_default() += deficit as f64;
            }
            BuildingType::SteelMill => {
                let cap = building.effective_capacity();
                let coal_deficit = cap.saturating_sub(nation.resource_amount(ResourceType::Coal));
                let iron_deficit = cap.saturating_sub(nation.resource_amount(ResourceType::Iron));
                *demand.entry(ResourceType::Coal).or_default() += coal_deficit as f64;
                *demand.entry(ResourceType::Iron).or_default() += iron_deficit as f64;
            }
            BuildingType::TextileMill => {
                let need = building.effective_capacity() * 2;
                let have = nation.resource_amount(ResourceType::Cotton)
                    + nation.resource_amount(ResourceType::Wool);
                let deficit = need.saturating_sub(have);
                *demand.entry(ResourceType::Cotton).or_default() += deficit as f64 / 2.0;
                *demand.entry(ResourceType::Wool).or_default() += deficit as f64 / 2.0;
            }
            _ => {}
        }
    }

    let money_urgency = if nation.treasury < Money::dollars(3000) {
        4.0
    } else if nation.treasury < Money::dollars(8000) {
        2.0
    } else {
        1.0
    };
    *demand.entry(ResourceType::Gold).or_default() += 5.0 * money_urgency;
    *demand.entry(ResourceType::Gems).or_default() += 10.0 * money_urgency;
    *demand.entry(ResourceType::Oil).or_default() += 2.0 * money_urgency;

    let total_food = nation.resource_amount(ResourceType::Grain)
        + nation.resource_amount(ResourceType::Fruit)
        + nation.resource_amount(ResourceType::Livestock);
    let workers = nation.labor.total_workers();
    let food_urgency = if total_food <= workers {
        10.0
    } else if total_food <= workers * 2 {
        5.0
    } else {
        1.0
    };
    for r in [
        ResourceType::Grain,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Horses,
    ] {
        demand.entry(r).or_insert(food_urgency);
    }

    demand
}

/// Demand-weighted yield for a single tile (≥1 per producing tile).
pub(super) fn score_tile_for_demand(
    hex_map: &HexMap,
    coord: HexCoord,
    demand: &HashMap<ResourceType, f64>,
) -> u32 {
    if let Some(tile) = hex_map.get_tile(coord)
        && let Some(yield_info) = tile.calculate_yield()
    {
        let weight = demand.get(&yield_info.resource).copied().unwrap_or(1.0);
        return (yield_info.quantity as f64 * weight).max(1.0) as u32;
    }
    0
}

#[allow(dead_code)]
pub(super) fn score_province(hex_map: &HexMap, province: &Province, nation: &Nation) -> u32 {
    let demand = compute_resource_demand(nation);
    let mut score = 0u32;
    for &coord in &province.tiles {
        score += score_tile_for_demand(hex_map, coord, &demand);
    }
    score
}

/// A single planned depot placement.
pub(super) struct DepotPlan {
    /// Where the depot will go.
    pub candidate: HexCoord,
    /// Unbuilt hexes from the existing network to `candidate`, in build order.
    /// Empty when the candidate is already reached by rail.
    pub path: Vec<HexCoord>,
    /// Total $ to lay every hex in `path`. Kept for debugging / test inspection.
    #[allow(dead_code)]
    pub path_cost: Money,
    /// Demand-weighted yield unlocked by the new depot's 1-hex radius
    /// (tiles already covered by another connected depot are excluded).
    /// Kept for debugging / test inspection.
    #[allow(dead_code)]
    pub coverage_value: u32,
    /// `coverage_value * horizon - path_cost - depot_cost`, used as the priority.
    pub net_score: f64,
}

/// Plan the single best depot placement for `nation_id` under the current
/// tech/ownership/rail network state, or `None` if nothing is worth building.
///
/// Strategy:
/// 1. Enumerate every owned land hex without a depot as a candidate.
/// 2. Score each candidate by the demand-weighted yield of tiles within its
///    1-hex radius (minus hexes already covered by another connected depot).
/// 3. Drop candidates unreachable by tech-enabled rail on owned hexes.
/// 4. Compute `net_score = coverage * horizon - path_cost - depot_cost` and
///    return the argmax (if positive).
pub(super) fn plan_next_depot(game: &GameState, nation_id: NationId) -> Option<DepotPlan> {
    use crate::map::infrastructure::{collectable_hexes, is_province_connected, railroad_cost};
    use crate::turn::connected_provinces;

    let nation = game.get_nation(nation_id)?;
    let cfg = &game.game_data.game_config;
    let capital_tile = game.get_province(nation.capital_province_id)?.capital_tile;

    let connected = connected_provinces(game, nation_id);
    let owned_provinces: Vec<&Province> = game
        .provinces
        .iter()
        .filter(|p| p.owner == nation_id)
        .collect();
    let already_covered = collectable_hexes(
        &game.hex_map,
        nation.capital_province_id,
        &owned_provinces,
        &connected,
    );

    let owned_hexes: HashSet<HexCoord> = owned_provinces
        .iter()
        .flat_map(|p| p.tiles.iter().copied())
        .collect();

    // Rail network seeds: the nation's own capital PLUS every owned
    // country-capital tile. Captured foreign capitals are independent hubs.
    let mut seeds: Vec<HexCoord> = vec![capital_tile];
    for &h in &owned_hexes {
        if h == capital_tile {
            continue;
        }
        if let Some(tile) = game.hex_map.get_tile(h)
            && tile.is_country_capital
        {
            seeds.push(h);
        }
    }
    let network = get_rail_network_for_nation(&game.hex_map, &seeds);
    let demand = compute_resource_demand(nation);
    let horizon = cfg.infrastructure_horizon_turns as f64;
    let depot_cost = Money::dollars(cfg.depot_cost);

    // Single multi-source Dijkstra from the network → distance + prev map for
    // every reachable owned hex. Avoids running one Dijkstra per candidate.
    let (dist, prev) = dijkstra_from_network(
        &game.hex_map,
        &network,
        &owned_hexes,
        cfg,
        &nation.researched_techs,
        &game.game_data,
    );

    let mut best: Option<DepotPlan> = None;

    for &candidate in &owned_hexes {
        let tile = match game.hex_map.get_tile(candidate) {
            Some(t) => t,
            None => continue,
        };
        if !tile.terrain().is_land() || tile.infrastructure.has_depot {
            continue;
        }

        let mut coverage_value: u32 = 0;
        let radius: Vec<HexCoord> = std::iter::once(candidate)
            .chain(candidate.neighbors().iter().copied())
            .collect();
        for r_hex in &radius {
            if !owned_hexes.contains(r_hex) {
                continue;
            }
            if already_covered.contains(r_hex) {
                continue;
            }
            coverage_value += score_tile_for_demand(&game.hex_map, *r_hex, &demand);
        }
        if coverage_value == 0 {
            continue;
        }

        // Skip unreachable candidates (no path recorded by the multi-source run).
        if !network.contains(&candidate) && !dist.contains_key(&candidate) {
            continue;
        }

        // Reconstruct the path from network to candidate (tiles NOT in network).
        let mut path: Vec<HexCoord> = Vec::new();
        let mut c = candidate;
        while let Some(&p) = prev.get(&c) {
            if !network.contains(&c) {
                path.push(c);
            }
            c = p;
        }
        path.reverse();

        let path_cost_cents: i64 = path
            .iter()
            .filter_map(|c| game.hex_map.get_tile(*c))
            .filter(|t| !t.infrastructure.has_railroad && !t.infrastructure.has_depot)
            .filter_map(|t| railroad_cost(t.terrain(), cfg).map(|m| m.cents()))
            .sum();
        let path_cost = Money::from_cents(path_cost_cents);

        // Net score is used for ranking candidates (higher = better). We do
        // *not* gate on `> 0`: a nation should always be developing something
        // if an opportunity exists — depots are long-lived and coverage
        // compounds beyond any finite horizon, so even a "break-even" depot is
        // worth building over hoarding treasury.
        let net_score = coverage_value as f64 * horizon
            - path_cost.as_dollars() as f64
            - depot_cost.as_dollars() as f64;

        let plan = DepotPlan {
            candidate,
            path,
            path_cost,
            coverage_value,
            net_score,
        };
        match &best {
            None => best = Some(plan),
            Some(b) if plan.net_score > b.net_score => best = Some(plan),
            _ => {}
        }
    }

    let _ = is_province_connected; // silence unused-import warning in some builds
    best
}

/// Single multi-source Dijkstra from every tile in `network` out to every
/// reachable owned land hex (respecting tech constraints). Returns the cost
/// map and a predecessor map so callers can reconstruct paths to any hex
/// without re-running Dijkstra per target.
fn dijkstra_from_network(
    hex_map: &HexMap,
    network: &HashSet<HexCoord>,
    owned_hexes: &HashSet<HexCoord>,
    cfg: &crate::data::GameConfig,
    researched_techs: &[crate::events::TechId],
    game_data: &crate::data::GameData,
) -> (HashMap<HexCoord, i64>, HashMap<HexCoord, HexCoord>) {
    let mut dist: HashMap<HexCoord, i64> = HashMap::new();
    let mut prev: HashMap<HexCoord, HexCoord> = HashMap::new();
    let mut heap: BinaryHeap<Reverse<(i64, HexCoord)>> = BinaryHeap::new();

    for &coord in network {
        dist.insert(coord, 0);
        heap.push(Reverse((0, coord)));
    }

    while let Some(Reverse((cost, current))) = heap.pop() {
        if cost > *dist.get(&current).unwrap_or(&i64::MAX) {
            continue;
        }
        for neighbor in current.neighbors() {
            let tile = match hex_map.get_tile(neighbor) {
                Some(t) => t,
                None => continue,
            };
            if !tile.terrain().is_land() {
                continue;
            }
            if !owned_hexes.contains(&neighbor) && !network.contains(&neighbor) {
                continue;
            }
            let has_existing = tile.infrastructure.has_railroad || tile.infrastructure.has_depot;
            if !has_existing
                && !crate::map::infrastructure::rail_terrain_enabled(
                    tile.terrain(),
                    researched_techs,
                    game_data,
                    cfg,
                )
            {
                continue;
            }
            let edge_cost = if has_existing {
                0i64
            } else {
                match railroad_cost(tile.terrain(), cfg) {
                    Some(m) => m.cents(),
                    None => continue,
                }
            };
            let new_cost = cost + edge_cost;
            if new_cost < *dist.get(&neighbor).unwrap_or(&i64::MAX) {
                dist.insert(neighbor, new_cost);
                prev.insert(neighbor, current);
                heap.push(Reverse((new_cost, neighbor)));
            }
        }
    }
    (dist, prev)
}

/// Dijkstra from the existing railroad network to a target tile.
/// Returns the list of tiles (not yet in the network) that need railroads built,
/// ordered from closest-to-network to target.
///
/// `owned_hexes` constrains the path to tiles the AI's nation actually owns —
/// railroads cannot be built on foreign territory, so routing through it would
/// produce an unbuildable path. The network tiles themselves are allowed as
/// seeds even if not owned (they already exist).
#[allow(dead_code)]
pub(super) fn find_cheapest_path(
    hex_map: &HexMap,
    network: &HashSet<HexCoord>,
    target: HexCoord,
    cfg: &crate::data::GameConfig,
    owned_hexes: &HashSet<HexCoord>,
    researched_techs: &[crate::events::TechId],
    game_data: &crate::data::GameData,
) -> Option<Vec<HexCoord>> {
    let mut dist: HashMap<HexCoord, i64> = HashMap::new();
    let mut prev: HashMap<HexCoord, HexCoord> = HashMap::new();
    let mut heap: BinaryHeap<Reverse<(i64, HexCoord)>> = BinaryHeap::new();

    // Seed all network tiles at cost 0
    for &coord in network {
        dist.insert(coord, 0);
        heap.push(Reverse((0, coord)));
    }

    while let Some(Reverse((cost, current))) = heap.pop() {
        if current == target {
            // Reconstruct path: only tiles NOT already in the network
            let mut path = Vec::new();
            let mut c = target;
            while let Some(&p) = prev.get(&c) {
                if !network.contains(&c) {
                    path.push(c);
                }
                c = p;
            }
            path.reverse();
            return Some(path);
        }

        if cost > *dist.get(&current).unwrap_or(&i64::MAX) {
            continue;
        }

        for neighbor in current.neighbors() {
            if let Some(tile) = hex_map.get_tile(neighbor) {
                if !tile.terrain().is_land() {
                    continue;
                }
                // Only traverse owned hexes (or existing network tiles, which
                // may be seeded even if the ownership map has drifted).
                if !owned_hexes.contains(&neighbor) && !network.contains(&neighbor) {
                    continue;
                }
                // Skip terrain we don't have tech to lay rail on (unless it
                // already has rail/depot — existing infrastructure is always
                // traversable even if the current nation couldn't rebuild it).
                let has_existing_rail =
                    tile.infrastructure.has_railroad || tile.infrastructure.has_depot;
                if !has_existing_rail
                    && !crate::map::infrastructure::rail_terrain_enabled(
                        tile.terrain(),
                        researched_techs,
                        game_data,
                        cfg,
                    )
                {
                    continue;
                }
                let edge_cost = if has_existing_rail {
                    0i64
                } else {
                    match railroad_cost(tile.terrain(), cfg) {
                        Some(money) => money.cents(),
                        None => continue,
                    }
                };
                let new_cost = cost + edge_cost;
                if new_cost < *dist.get(&neighbor).unwrap_or(&i64::MAX) {
                    dist.insert(neighbor, new_cost);
                    prev.insert(neighbor, current);
                    heap.push(Reverse((new_cost, neighbor)));
                }
            }
        }
    }
    None
}

/// AI builds map infrastructure: depots and railroads to connect provinces.
///
/// Strategy: prioritise provinces by resource value, then use Dijkstra to find
/// the cheapest railroad path from the existing network. Spends up to
/// `infrastructure_budget` per turn (read from Lua personality config).
#[cfg(test)]
pub(crate) fn ai_build_map_infrastructure(game: &mut GameState, nation_id: NationId) {
    let treasury = match game.get_nation(nation_id) {
        Some(n) => n.treasury,
        None => return,
    };

    // Need at least enough for a depot
    if treasury < Money::dollars(2000) {
        return;
    }

    // ── Read infrastructure budget from Lua config ──────────────
    let personality = get_personality(game, nation_id);

    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
    #[cfg(not(feature = "lua"))]
    let _lua_cfg: Option<()> = None;

    let base_infrastructure_budget: Money = 'val: {
        #[cfg(feature = "lua")]
        if let Some(budget) = lua_cfg.as_ref().map(|c| c.infrastructure_budget) {
            break 'val Money::dollars(budget);
        }
        match personality {
            AiPersonality::Economic => Money::dollars(3000),
            AiPersonality::Diplomatic => Money::dollars(2500),
            AiPersonality::Aggressive => Money::dollars(1500),
            AiPersonality::Balanced => Money::dollars(2000),
        }
    };

    // Scale budget with treasury: spend more aggressively when cash-rich
    let scale_threshold: i64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.infra_budget_scale_threshold)
        {
            break 'val v;
        }
        20_000
    };
    let infrastructure_budget = if treasury > Money::dollars(scale_threshold * 3) {
        base_infrastructure_budget * 3
    } else if treasury > Money::dollars(scale_threshold) {
        base_infrastructure_budget * 2
    } else {
        base_infrastructure_budget
    };

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
    let capital_tile = match game.get_province(capital_province_id) {
        Some(p) => p.capital_tile,
        None => return,
    };

    let capital_tiles: Vec<HexCoord> = game
        .get_province(capital_province_id)
        .map(|p| p.tiles.clone())
        .unwrap_or_default();

    let capital_has_depot = capital_tiles.iter().any(|coord| {
        game.hex_map
            .get_tile(*coord)
            .is_some_and(|t| t.infrastructure.has_depot)
    });

    if !capital_has_depot {
        let provinces_snapshot = game.provinces.clone();
        let cfg = game.game_data.game_config.clone();
        if let Ok(cost) = build_depot(
            &mut game.hex_map,
            capital_tile,
            nation_id,
            &provinces_snapshot,
            &cfg,
        ) && let Some(nation) = game.get_nation_mut(nation_id)
        {
            nation.treasury -= cost;
        }
        return; // One major action per turn
    }

    // Step 2: Score and sort non-capital provinces by current economic need
    let nation_ref = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    let mut province_scores: Vec<(ProvinceId, u32)> = province_ids
        .iter()
        .filter(|&&pid| pid != capital_province_id)
        .filter_map(|&pid| {
            let score = game
                .get_province(pid)
                .map(|p| score_province(&game.hex_map, p, nation_ref))?;
            if score > 0 { Some((pid, score)) } else { None }
        })
        .collect();
    province_scores.sort_by(|a, b| b.1.cmp(&a.1));

    // Step 3: Find the first disconnected province with resources and connect it
    let mut spent = Money::ZERO;
    let budget = infrastructure_budget.min(
        treasury
            .checked_sub(Money::dollars(500))
            .unwrap_or(Money::ZERO),
    );

    for (pid, _score) in &province_scores {
        if is_province_connected(&game.hex_map, capital_tile, *pid, &game.provinces) {
            continue;
        }

        // Ensure the target province has a depot
        let target_depot_tile = match game.get_province(*pid) {
            Some(p) => p.capital_tile,
            None => continue,
        };
        let has_depot = game
            .hex_map
            .get_tile(target_depot_tile)
            .is_some_and(|t| t.infrastructure.has_depot);

        let provinces_snapshot = game.provinces.clone();
        let cfg = game.game_data.game_config.clone();
        if !has_depot
            && budget - spent >= Money::dollars(2000)
            && let Ok(cost) = build_depot(
                &mut game.hex_map,
                target_depot_tile,
                nation_id,
                &provinces_snapshot,
                &cfg,
            )
        {
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.treasury -= cost;
            }
            spent += cost;
        }

        // Find cheapest path from network to target depot (owned hexes only).
        let owned_hexes: HashSet<HexCoord> = provinces_snapshot
            .iter()
            .filter(|p| p.owner == nation_id)
            .flat_map(|p| p.tiles.iter().copied())
            .collect();
        let network = get_railroad_network(&game.hex_map, capital_tile);
        let researched: Vec<crate::events::TechId> = game
            .get_nation(nation_id)
            .map(|n| n.researched_techs.clone())
            .unwrap_or_default();
        if let Some(path) = find_cheapest_path(
            &game.hex_map,
            &network,
            target_depot_tile,
            &cfg,
            &owned_hexes,
            &researched,
            &game.game_data,
        ) {
            for &coord in &path {
                if spent >= budget {
                    break;
                }
                if let Ok(cost) = build_railroad(
                    &mut game.hex_map,
                    coord,
                    nation_id,
                    &researched,
                    &provinces_snapshot,
                    &game.game_data,
                    &cfg,
                ) {
                    if let Some(nation) = game.get_nation_mut(nation_id) {
                        nation.treasury -= cost;
                    }
                    spent += cost;
                }
            }
        }

        // When cash-rich, allow connecting additional provinces per turn
        let current_treasury = game
            .get_nation(nation_id)
            .map(|n| n.treasury)
            .unwrap_or(Money::ZERO);
        if spent < budget && current_treasury > infrastructure_budget * 3 {
            continue;
        }
        break;
    }
}

/// The AI keeps a reserve of each good (Lua-configurable) and sells excess when treasury is low.
#[allow(unused_variables)] // personality used only with cfg(feature = "lua")
pub fn ai_manage_resources(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<super::AiAction>,
) {
    let personality = get_personality(game, nation_id);

    // ── Read Lua config (feature-gated) ──────────────────────
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));

    let goods_sell_threshold: i64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.goods_sell_treasury_threshold)
        {
            break 'val v;
        }
        3000
    };
    let goods_reserve: u32 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.goods_reserve) {
            break 'val v;
        }
        2
    };

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Only sell goods when treasury is low
    if nation.treasury >= Money::dollars(goods_sell_threshold) {
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
        if amount <= goods_reserve {
            continue;
        }
        let excess = amount - goods_reserve;
        let revenue = Money::dollars(*price_per_unit) * excess as i64;

        let Some(nation) = game.get_nation_mut(nation_id) else {
            return;
        };
        nation.consume_goods(*goods_type, excess);
        nation.treasury += revenue;
        total_revenue += revenue;
    }

    if total_revenue > Money::ZERO {
        actions.push(super::AiAction {
            text: format!(
                "{} sold excess goods for ${}",
                nation_name,
                total_revenue.as_dollars()
            ),
            reason: format!(
                "Treasury below ${} sell threshold; liquidated surplus goods for ${}",
                goods_sell_threshold,
                total_revenue.as_dollars()
            ),
            is_non_action: false,
        });
    }
}

/// Consolidate AI economic decisions.
///
/// - If AI has no mills and has lumber+steel materials: build a LumberMill
/// - If AI has mills producing materials, build corresponding factories
/// - Expand mills using tier progression (2→4→8→12→16→20...) when resources exceed threshold
/// - All constants are Lua-configurable per personality
pub(crate) fn ai_manage_economy(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);

    if game.ai_debug {
        let nation_name = game
            .get_nation(nation_id)
            .map(|n| n.name.as_str())
            .unwrap_or("?");
        let treasury = game
            .get_nation(nation_id)
            .map(|n| n.treasury.as_dollars())
            .unwrap_or(0);
        eprintln!(
            "[AI:{}:economy] treasury=${}, personality={}",
            nation_name, treasury, personality
        );
    }

    // Build infrastructure handles mills and factories
    ai_build_infrastructure(game, nation_id);

    // ── Read Lua config (feature-gated) ──────────────────────
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));

    // Economic personality expands more aggressively (Lua overrides Rust defaults)
    let expansion_threshold_multiplier: u32 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.expansion_threshold_multiplier)
        {
            break 'val v;
        }
        match personality {
            AiPersonality::Economic => 1,
            _ => 2,
        }
    };

    let use_tier_expansion: bool = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.use_tier_expansion) {
            break 'val v;
        }
        true
    };

    let high_treasury_threshold: i64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.high_treasury_expansion_threshold)
        {
            break 'val v;
        }
        15_000
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
                    // Use min(coal, iron) * 2 to match actual 1:1 production ratio
                    nation
                        .resource_amount(ResourceType::Coal)
                        .min(nation.resource_amount(ResourceType::Iron))
                        * 2
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
        expand_building(game, nation_id, bt, use_tier_expansion);
    }

    // Expand factories when their input material exceeds capacity * threshold.
    // Factory input = the corresponding material in the warehouse.
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let factory_expansions: Vec<BuildingType> = nation
        .buildings
        .iter()
        .filter_map(|b| {
            let input_materials = match b.building_type {
                BuildingType::FurnitureFactory => nation.material_amount(MaterialType::Lumber),
                BuildingType::HardwareFactory => nation.material_amount(MaterialType::Steel),
                BuildingType::ClothingFactory => nation.material_amount(MaterialType::Fabric),
                _ => return None,
            };
            // Factories consume 2 materials per unit, so check against capacity * 2 * threshold
            if input_materials > b.effective_capacity() * 2 * expansion_threshold_multiplier
                && b.pending_capacity == 0
            {
                Some(b.building_type)
            } else {
                None
            }
        })
        .collect();

    for bt in factory_expansions {
        expand_building(game, nation_id, bt, use_tier_expansion);
    }

    // Expand FoodProcessing when food surplus exceeds capacity * threshold.
    // This builds the CannedFood pipeline for immigration and starvation buffer.
    let food_threshold: u32 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.food_processing_expansion_threshold)
        {
            break 'val v;
        }
        2
    };

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let total_raw_food = nation.resource_amount(ResourceType::Grain)
        + nation.resource_amount(ResourceType::Fruit)
        + nation.resource_amount(ResourceType::Livestock);
    let food_cap = nation
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::FoodProcessing)
        .map(|b| b.effective_capacity())
        .unwrap_or(0);
    let workers = nation.labor.total_workers();
    let food_surplus = total_raw_food.saturating_sub(workers);

    if food_surplus > food_cap * food_threshold
        && food_cap > 0
        && nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::FoodProcessing)
            .map(|b| b.pending_capacity == 0)
            .unwrap_or(false)
    {
        expand_building(
            game,
            nation_id,
            BuildingType::FoodProcessing,
            use_tier_expansion,
        );
    }

    // When treasury is very high, expand existing mills and factories even without
    // surplus resources — invest in future capacity growth.
    // Only expand if capacity isn't already far ahead of actual input supply.
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    if nation.treasury > Money::dollars(high_treasury_threshold) {
        let expandable: Vec<BuildingType> = nation
            .buildings
            .iter()
            .filter(|b| {
                if b.pending_capacity != 0 {
                    return false;
                }
                // Cap speculative expansion: don't expand if capacity already > 2x input
                let input = match b.building_type {
                    BuildingType::LumberMill => nation.resource_amount(ResourceType::Timber),
                    BuildingType::SteelMill => {
                        nation
                            .resource_amount(ResourceType::Coal)
                            .min(nation.resource_amount(ResourceType::Iron))
                            * 2
                    }
                    BuildingType::TextileMill => {
                        nation.resource_amount(ResourceType::Cotton)
                            + nation.resource_amount(ResourceType::Wool)
                    }
                    BuildingType::FurnitureFactory => nation.material_amount(MaterialType::Lumber),
                    BuildingType::HardwareFactory => nation.material_amount(MaterialType::Steel),
                    BuildingType::ClothingFactory => nation.material_amount(MaterialType::Fabric),
                    _ => return false,
                };
                // Only speculative-expand if capacity <= 2x current input (room to grow into)
                b.effective_capacity() <= input.max(1) * 2
            })
            .map(|b| b.building_type)
            .collect();

        for bt in expandable {
            expand_building(game, nation_id, bt, use_tier_expansion);
        }
    }
}

/// Expand a building, paying the correct material cost.
/// When `use_tier` is true, uses tier progression (2→4→8→12...) with proportional cost.
/// When false, expands by +1 capacity for 1 lumber + 1 steel (legacy behavior).
fn expand_building(game: &mut GameState, nation_id: NationId, bt: BuildingType, use_tier: bool) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let increase = if use_tier {
        nation
            .buildings
            .iter()
            .find(|b| b.building_type == bt)
            .map(|b| b.next_capacity() - b.capacity)
            .unwrap_or(1)
    } else {
        1
    };

    let (lumber_cost, steel_cost) = Building::expansion_cost(increase);
    let has_lumber = nation.material_amount(MaterialType::Lumber) >= lumber_cost;
    let has_steel = nation.material_amount(MaterialType::Steel) >= steel_cost;

    if has_lumber && has_steel {
        let ai_debug = game.ai_debug;
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return;
        };
        nation.consume_material(MaterialType::Lumber, lumber_cost);
        nation.consume_material(MaterialType::Steel, steel_cost);
        if let Some(building) = nation.get_building_mut(bt) {
            if use_tier {
                building.start_expansion_to_next_tier();
            } else {
                building.start_expansion(1);
            }

            if ai_debug {
                eprintln!(
                    "[AI:{}:economy] expanding {:?} by +{} (cost: {} lumber, {} steel)",
                    nation.name, bt, increase, lumber_cost, steel_cost
                );
            }
        }
    }
}

/// Sell excess tradeable resources on the market for cash.
///
/// Reserve amount and treasury cap are Lua-configurable per personality.
#[allow(unused_variables)] // personality used only with cfg(feature = "lua")
pub(crate) fn ai_trade(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);

    // ── Read Lua config (feature-gated) ──────────────────────
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));

    let trade_treasury_cap: i64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.trade_treasury_cap) {
            break 'val v;
        }
        20_000
    };
    let trade_resource_reserve: u32 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.trade_resource_reserve) {
            break 'val v;
        }
        10
    };

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Don't sell resources when already sitting on a large treasury —
    // keep the materials for building ships, units, and infrastructure instead.
    if nation.treasury > Money::dollars(trade_treasury_cap) {
        return;
    }

    // Check all tradeable resource types for surplus
    let tradeable_resources = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Grain,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Horses,
        ResourceType::Oil,
    ];

    for resource in tradeable_resources {
        let amount = nation.resource_amount(resource);
        if amount > trade_resource_reserve {
            let excess = amount - trade_resource_reserve;
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
pub(crate) fn ai_build_transport_proactive(game: &mut GameState, nation_id: NationId) {
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
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return;
        };
        nation.consume_material(MaterialType::Lumber, affordable);
        nation.consume_material(MaterialType::Steel, affordable);
        nation.transport.build_freight_cars(affordable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::test_game_with_ai;
    use crate::ai::run_ai_turns;
    use crate::economy::buildings::{Building, BuildingType};

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
    fn ai_sells_tradeable_grain_when_in_surplus() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(0);
        ai.add_resource(ResourceType::Grain, 20);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Grain is tradeable: AI sells excess above reserve (10),
        // minus 1 consumed for worker recruitment.
        // 20 - 1 (recruitment) - 9 (sold: 19 - 10 reserve) = 10
        assert!(
            ai.resource_amount(ResourceType::Grain) <= 10,
            "AI should sell excess grain, has {}",
            ai.resource_amount(ResourceType::Grain)
        );
        assert!(
            ai.treasury > Money::ZERO,
            "AI should have earned money from selling grain"
        );
    }

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
        // Give enough materials for both potential mill expansion and freight cars
        ai.add_material(MaterialType::Lumber, 20);
        ai.add_material(MaterialType::Steel, 20);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.transport.freight_cars >= 2,
            "AI should build at least 2 freight cars, got {}",
            ai.transport.freight_cars
        );
        // Materials consumed by freight cars + any expansion
        assert!(
            ai.material_amount(MaterialType::Lumber) < 20,
            "AI should consume some lumber"
        );
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
            actions.iter().any(|a| a.text.contains("sold excess goods")),
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
}
