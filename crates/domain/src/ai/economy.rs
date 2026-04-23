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
///
/// Card #132: the raw deficit is discounted by "importable via trade" — the
/// AI should prefer connecting provinces producing resources it can't cover
/// through trade over resources already flowing in from partners.
pub(super) fn compute_resource_demand(
    nation: &Nation,
    game: &GameState,
    cfg: &crate::data::GameConfig,
) -> HashMap<ResourceType, f64> {
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

    // Card #132: subtract importable-via-trade from raw demand. If a need
    // is already being covered by trade history or by a consulated partner
    // producing the resource, the AI should not chase it by building new
    // infrastructure.
    //
    // Discount is explicitly capped by the current `demand[r]` value
    // (the raw deficit) so a large historical import stream can't make
    // the AI ignore a real need over multiple turns — the worst trade
    // can do is take the resource's demand to zero, never negative.
    if cfg.trade_discount_weight > 0.0 {
        let (history, potential) = importable_via_trade(nation, game, cfg);
        let lookback = cfg.trade_lookback_turns.max(1) as f64;
        for (resource, slot) in demand.iter_mut() {
            let history_rate = history.get(resource).copied().unwrap_or(0.0) / lookback;
            let potential_rate = potential.get(resource).copied().unwrap_or(0.0);
            let raw_discount = history_rate * cfg.trade_history_weight
                + potential_rate * cfg.trade_consulate_potential_weight;
            let discount = raw_discount * cfg.trade_discount_weight;
            let capped = discount.min(*slot); // never more than the raw deficit
            *slot = (*slot - capped).max(0.0);
        }
    }

    demand
}

/// Estimate how much of each resource a nation can plausibly obtain through
/// trade this turn. Returns a `(history, consulate_potential)` pair so the
/// caller can weight each signal separately (cards #131 / #132):
///
/// - **history**: total quantities bought within the last
///   `cfg.trade_lookback_turns` turns. All entries in the window are weighted
///   equally; the caller divides by the window length to get a per-turn rate.
/// - **consulate_potential**: tradeable tile yield owned by non-GP minors
///   we hold a consulate with. Reflects what's available to us but we may
///   not yet be purchasing; weighted lower than history so potential alone
///   can't silence a real deficit.
pub(super) fn importable_via_trade(
    nation: &Nation,
    game: &GameState,
    cfg: &crate::data::GameConfig,
) -> (HashMap<ResourceType, f64>, HashMap<ResourceType, f64>) {
    let mut history: HashMap<ResourceType, f64> = HashMap::new();
    let mut potential: HashMap<ResourceType, f64> = HashMap::new();

    let lookback = cfg.trade_lookback_turns;
    let current = game.turn.0;
    let cutoff = current.saturating_sub(lookback);
    for entry in &nation.trade_history {
        if !entry.bought {
            continue;
        }
        if entry.turn.0 < cutoff {
            continue;
        }
        *history.entry(entry.resource).or_default() += entry.quantity as f64;
    }

    for other in &game.nations {
        if other.id == nation.id || other.is_great_power() || other.province_ids.is_empty() {
            continue;
        }
        let has_consulate = game
            .diplomacy
            .get_relation(nation.id, other.id)
            .is_some_and(|r| r.has_consulate);
        if !has_consulate {
            continue;
        }
        for pid in &other.province_ids {
            if let Some(p) = game.get_province(*pid) {
                for &coord in &p.tiles {
                    if let Some(tile) = game.hex_map.get_tile(coord)
                        && let Some(y) = tile.calculate_yield()
                        && y.resource.is_tradeable()
                    {
                        *potential.entry(y.resource).or_default() += y.quantity as f64;
                    }
                }
            }
        }
    }

    (history, potential)
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
pub(super) fn score_province(
    hex_map: &HexMap,
    province: &Province,
    nation: &Nation,
    game: &GameState,
    cfg: &crate::data::GameConfig,
) -> u32 {
    let demand = compute_resource_demand(nation, game, cfg);
    let mut score = 0u32;
    for &coord in &province.tiles {
        score += score_tile_for_demand(hex_map, coord, &demand);
    }
    score
}

/// A single planned depot placement.
#[derive(Debug, Clone)]
pub(super) struct DepotPlan {
    /// Where the depot will go.
    pub candidate: HexCoord,
    /// Unbuilt hexes from `origin_capital` to `candidate`, in build order.
    /// Empty when the candidate is already reached by rail.
    pub path: Vec<HexCoord>,
    /// Country-capital tile the path roots from. Card #132: planning is
    /// always anchored at a country capital (original or conquered), never
    /// at an arbitrary rail-network tip.
    pub origin_capital: HexCoord,
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

/// Outcome of a planning call. Card #132: the AI must commit to a depot
/// target across turns, so the planner either tells the spending loop to
/// honor the existing commitment or tells it to replace it.
#[derive(Debug, Clone)]
pub(super) enum PlanOutcome {
    /// The nation's `committed_infra_target` is still valid. Follow the
    /// returned plan; do not mutate the commitment.
    KeepCommitment(DepotPlan),
    /// No valid commitment (either never existed, was fulfilled, or its
    /// target became unreachable). The `Option` holds the newly-picked
    /// plan, or `None` if nothing is worth building this turn. The caller
    /// must write the plan's `(candidate, origin_capital)` back to
    /// `committed_infra_target` — or clear it to `None`.
    Fresh(Option<DepotPlan>),
}

impl PlanOutcome {
    /// Convenience: the current plan to act on, if any.
    pub(super) fn as_plan(&self) -> Option<&DepotPlan> {
        match self {
            PlanOutcome::KeepCommitment(p) => Some(p),
            PlanOutcome::Fresh(opt) => opt.as_ref(),
        }
    }
}

/// Plan the next depot target for `nation_id` (card #132).
///
/// Card-mandated behaviour:
/// 1. Seeds for Dijkstra are **country-capital tiles only** (the nation's own
///    capital plus every owned `is_country_capital` tile). Province centroids
///    and mid-network rail tips do NOT seed new plans.
/// 2. Existing rail/depot hexes are zero-cost edges — the shortest path from
///    a capital through an existing spur and out one more hex is still cheap.
/// 3. The candidate with the highest trade-adjusted coverage ÷ distance score
///    wins. Demand already discounts resources available via trade (see
///    `compute_resource_demand` / `importable_via_trade`).
/// 4. The pick is **hard-committed**: if the nation's `committed_infra_target`
///    still points at an owned, reachable, non-depot candidate, that target
///    is returned unchanged — no re-shopping turn over turn. Commitment is
///    released only when the depot is built, the candidate/origin is lost,
///    or no tech-enabled path remains.
///
/// Returns a `PlanOutcome` the spending loop uses to persist or clear the
/// nation's `committed_infra_target` field.
pub(super) fn plan_next_depot(game: &GameState, nation_id: NationId) -> PlanOutcome {
    use crate::map::infrastructure::collectable_hexes;
    use crate::turn::connected_provinces;

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return PlanOutcome::Fresh(None),
    };
    let cfg = &game.game_data.game_config;

    let connected = connected_provinces(game, nation_id);
    let owned_provinces: Vec<&Province> = game
        .provinces
        .iter()
        .filter(|p| p.owner == nation_id)
        .collect();
    let already_covered = collectable_hexes(&game.hex_map, &owned_provinces, &connected);

    let owned_hexes: HashSet<HexCoord> = owned_provinces
        .iter()
        .flat_map(|p| p.tiles.iter().copied())
        .collect();

    // Seeds: every owned country-capital tile. These are the only valid
    // anchors for a new railway under card #132. Include the home capital
    // defensively even if its tile's `is_country_capital` flag is somehow
    // unset (map-gen edge case).
    let capital_tile = match game.get_province(nation.capital_province_id) {
        Some(p) => p.capital_tile,
        None => return PlanOutcome::Fresh(None),
    };
    let mut capital_seeds: Vec<HexCoord> = vec![capital_tile];
    for &h in &owned_hexes {
        if h == capital_tile {
            continue;
        }
        if let Some(tile) = game.hex_map.get_tile(h)
            && tile.is_country_capital
        {
            capital_seeds.push(h);
        }
    }

    // ── Check existing commitment first ───────────────────────
    if let Some(t) = nation.ai_priority_state.committed_infra_target.as_ref() {
        let cand_tile = game.hex_map.get_tile(t.candidate);
        let fulfilled = cand_tile.is_some_and(|tile| tile.infrastructure.has_depot);
        let candidate_ownership_ok = owned_hexes.contains(&t.candidate);
        // Accept the origin as valid if (a) the tile still has
        // `is_country_capital`, OR (b) it's still the nation's own
        // `capital_province.capital_tile`. Case (b) mirrors the defensive
        // home-capital fallback in seed construction above: if the flag is
        // somehow unset on the home-capital tile, the planner still treats
        // it as a valid origin, and commitment validation must agree —
        // otherwise the commitment would clear every turn (F-001).
        let origin_tile = game.hex_map.get_tile(t.origin_capital);
        let origin_is_home_capital = t.origin_capital == capital_tile;
        let origin_ok = owned_hexes.contains(&t.origin_capital)
            && (origin_is_home_capital
                || origin_tile.is_some_and(|tile| tile.is_country_capital));

        if !fulfilled && candidate_ownership_ok && origin_ok {
            // Single-source Dijkstra from the committed origin capital to
            // verify the commitment is still reachable under current tech
            // and ownership, and to refresh the path with whatever rail has
            // been laid since.
            let mut origin_seed = HashSet::new();
            origin_seed.insert(t.origin_capital);
            let (dist, prev, _source) = dijkstra_from_seeds(
                &game.hex_map,
                &origin_seed,
                &owned_hexes,
                cfg,
                &nation.researched_techs,
                &game.game_data,
            );
            if t.candidate == t.origin_capital || dist.contains_key(&t.candidate) {
                let path = reconstruct_path(&game.hex_map, &prev, t.candidate);
                let path_cost = sum_path_cost(&game.hex_map, &path, cfg);
                let coverage_value = coverage_around(
                    &game.hex_map,
                    t.candidate,
                    &owned_hexes,
                    &already_covered,
                    &compute_resource_demand(nation, game, cfg),
                );
                let net_score = net_score(coverage_value, path_cost, cfg);
                return PlanOutcome::KeepCommitment(DepotPlan {
                    candidate: t.candidate,
                    path,
                    origin_capital: t.origin_capital,
                    path_cost,
                    coverage_value,
                    net_score,
                });
            }
            // else: fall through to re-plan (commitment is now unreachable)
        }
        // fulfilled / lost candidate / lost origin / unreachable → clear and re-plan
    }

    // ── Re-plan from country-capital seeds ────────────────────
    let demand = compute_resource_demand(nation, game, cfg);

    let seed_set: HashSet<HexCoord> = capital_seeds.iter().copied().collect();
    let (dist, prev, source_of) = dijkstra_from_seeds(
        &game.hex_map,
        &seed_set,
        &owned_hexes,
        cfg,
        &nation.researched_techs,
        &game.game_data,
    );

    let mut best: Option<DepotPlan> = None;

    // Sort to ensure deterministic selection when scores are tied.
    let mut sorted_candidates: Vec<HexCoord> = owned_hexes.iter().copied().collect();
    sorted_candidates.sort_unstable_by_key(|c| (c.q, c.r));

    for candidate in sorted_candidates {
        let tile = match game.hex_map.get_tile(candidate) {
            Some(t) => t,
            None => continue,
        };
        if !tile.terrain().is_land() || tile.infrastructure.has_depot {
            continue;
        }

        let coverage_value = coverage_around(
            &game.hex_map,
            candidate,
            &owned_hexes,
            &already_covered,
            &demand,
        );
        if coverage_value == 0 {
            continue;
        }

        // Unreachable from any country capital → skip.
        if !seed_set.contains(&candidate) && !dist.contains_key(&candidate) {
            continue;
        }

        let path = reconstruct_path(&game.hex_map, &prev, candidate);
        let path_cost = sum_path_cost(&game.hex_map, &path, cfg);

        // Origin capital = the seed this candidate was reached from. If the
        // candidate is itself a seed (a country-capital tile), it's its own
        // origin.
        let origin_capital = if seed_set.contains(&candidate) {
            candidate
        } else {
            match source_of.get(&candidate).copied() {
                Some(o) => o,
                None => continue, // no source recorded — shouldn't happen, skip defensively
            }
        };

        let ns = net_score(coverage_value, path_cost, cfg);

        let plan = DepotPlan {
            candidate,
            path,
            origin_capital,
            path_cost,
            coverage_value,
            net_score: ns,
        };
        let better = match &best {
            None => true,
            Some(b) => {
                plan.net_score > b.net_score
                    || (plan.net_score == b.net_score && plan.path_cost < b.path_cost)
                    || (plan.net_score == b.net_score
                        && plan.path_cost == b.path_cost
                        && (plan.candidate.q, plan.candidate.r)
                            < (b.candidate.q, b.candidate.r))
            }
        };
        if better {
            best = Some(plan);
        }
    }

    PlanOutcome::Fresh(best)
}

/// Demand-weighted coverage of the 1-hex radius around `center`, excluding
/// tiles already covered by another connected collector.
fn coverage_around(
    hex_map: &HexMap,
    center: HexCoord,
    owned_hexes: &HashSet<HexCoord>,
    already_covered: &HashSet<HexCoord>,
    demand: &HashMap<ResourceType, f64>,
) -> u32 {
    let mut v: u32 = 0;
    let radius: Vec<HexCoord> = std::iter::once(center)
        .chain(center.neighbors().iter().copied())
        .collect();
    for r_hex in &radius {
        if !owned_hexes.contains(r_hex) {
            continue;
        }
        if already_covered.contains(r_hex) {
            continue;
        }
        v += score_tile_for_demand(hex_map, *r_hex, demand);
    }
    v
}

/// Reconstruct the build-order path to `candidate`, dropping any tiles that
/// already have railroad or a depot (zero-cost edges reused from the network).
fn reconstruct_path(
    hex_map: &HexMap,
    prev: &HashMap<HexCoord, HexCoord>,
    candidate: HexCoord,
) -> Vec<HexCoord> {
    let mut path: Vec<HexCoord> = Vec::new();
    let mut c = candidate;
    while let Some(&p) = prev.get(&c) {
        let already_built = hex_map
            .get_tile(c)
            .is_some_and(|t| t.infrastructure.has_railroad || t.infrastructure.has_depot);
        if !already_built {
            path.push(c);
        }
        c = p;
    }
    path.reverse();
    path
}

/// Sum the $ cost to lay railroad on every hex in `path` that doesn't
/// already have rail/depot. Existing rail is traversed at $0.
fn sum_path_cost(hex_map: &HexMap, path: &[HexCoord], cfg: &crate::data::GameConfig) -> Money {
    let cents: i64 = path
        .iter()
        .filter_map(|c| hex_map.get_tile(*c))
        .filter(|t| !t.infrastructure.has_railroad && !t.infrastructure.has_depot)
        .filter_map(|t| {
            crate::map::infrastructure::railroad_cost(t.terrain(), cfg).map(|m| m.cents())
        })
        .sum();
    Money::from_cents(cents)
}

/// Weighted net score for ranking candidates. Card #132 exposes the
/// coverage and path-cost weights in Lua via `cfg.infra_coverage_weight`
/// and `cfg.infra_path_cost_weight` so designers can tilt toward short
/// routes vs. high-coverage remote candidates without touching Rust.
fn net_score(coverage_value: u32, path_cost: Money, cfg: &crate::data::GameConfig) -> f64 {
    let horizon = cfg.infrastructure_horizon_turns as f64;
    let depot_cost = cfg.depot_cost as f64;
    coverage_value as f64 * horizon * cfg.infra_coverage_weight
        - path_cost.as_dollars() as f64 * cfg.infra_path_cost_weight
        - depot_cost
}

/// Multi-source Dijkstra from every tile in `seeds` out to every reachable
/// owned land hex (tech-gated; existing rail/depot hexes are zero-cost edges).
/// Returns:
/// - `dist`: cheapest build cost to each reachable hex, in cents.
/// - `prev`: predecessor map for path reconstruction.
/// - `source_of`: which seed each hex was reached from. Used by the depot
///   planner (card #132) to tag every candidate with the nearest country
///   capital so the `DepotPlan.origin_capital` is always the correct anchor.
fn dijkstra_from_seeds(
    hex_map: &HexMap,
    seeds: &HashSet<HexCoord>,
    owned_hexes: &HashSet<HexCoord>,
    cfg: &crate::data::GameConfig,
    researched_techs: &[crate::events::TechId],
    game_data: &crate::data::GameData,
) -> (
    HashMap<HexCoord, i64>,
    HashMap<HexCoord, HexCoord>,
    HashMap<HexCoord, HexCoord>,
) {
    let mut dist: HashMap<HexCoord, i64> = HashMap::new();
    let mut prev: HashMap<HexCoord, HexCoord> = HashMap::new();
    let mut source_of: HashMap<HexCoord, HexCoord> = HashMap::new();
    let mut heap: BinaryHeap<Reverse<(i64, HexCoord)>> = BinaryHeap::new();

    for &coord in seeds {
        dist.insert(coord, 0);
        source_of.insert(coord, coord);
        heap.push(Reverse((0, coord)));
    }

    while let Some(Reverse((cost, current))) = heap.pop() {
        if cost > *dist.get(&current).unwrap_or(&i64::MAX) {
            continue;
        }
        let current_source = match source_of.get(&current).copied() {
            Some(s) => s,
            None => continue,
        };
        for neighbor in current.neighbors() {
            let tile = match hex_map.get_tile(neighbor) {
                Some(t) => t,
                None => continue,
            };
            if !tile.terrain().is_land() {
                continue;
            }
            if !owned_hexes.contains(&neighbor) && !seeds.contains(&neighbor) {
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
                source_of.insert(neighbor, current_source);
                heap.push(Reverse((new_cost, neighbor)));
            }
        }
    }
    (dist, prev, source_of)
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
        ) {
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.treasury -= cost;
            }
            game.pending_ai_cash_spending.push((
                nation_id,
                crate::economy::ledger::CashSink::AiInfrastructure,
                cost,
                None,
            ));
        }
        return; // One major action per turn
    }

    // Step 2: Score and sort non-capital provinces by current economic need
    let nation_ref = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    let cfg_for_scoring = game.game_data.game_config.clone();
    let mut province_scores: Vec<(ProvinceId, u32)> = province_ids
        .iter()
        .filter(|&&pid| pid != capital_province_id)
        .filter_map(|&pid| {
            let score = game
                .get_province(pid)
                .map(|p| score_province(&game.hex_map, p, nation_ref, game, &cfg_for_scoring))?;
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
            game.pending_ai_cash_spending.push((
                nation_id,
                crate::economy::ledger::CashSink::AiInfrastructure,
                cost,
                None,
            ));
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
                    game.pending_ai_cash_spending.push((
                        nation_id,
                        crate::economy::ledger::CashSink::AiInfrastructure,
                        cost,
                        None,
                    ));
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
        game.pending_ai_cash_income.push((nation_id, total_revenue));
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
            nation_id,
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

    let mut total_revenue = Money::ZERO;
    {
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
                    total_revenue += revenue;
                }
            }
        }
    }
    if total_revenue > Money::ZERO {
        game.pending_ai_cash_income.push((nation_id, total_revenue));
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

    // ── Trade-aware demand (card #132) ─────────────────────────

    fn prime_timber_deficit(game: &mut GameState, nation_id: NationId) {
        // Give the nation a LumberMill so compute_resource_demand registers
        // a real Timber deficit (effective_capacity * 2, minus any Timber
        // already in warehouse).
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        // Clear any starting timber so the deficit is clean.
        nation.warehouse.insert(ResourceType::Timber, 0);
    }

    #[test]
    fn demand_discounted_by_recent_imports() {
        // Card #132: when the nation has been importing timber recently, its
        // timber demand score drops (we're already covering the need via
        // trade). Same setup minus the history produces a higher score.
        let mut game_a = test_game_with_ai();
        let mut game_b = test_game_with_ai();
        prime_timber_deficit(&mut game_a, NationId(2));
        prime_timber_deficit(&mut game_b, NationId(2));

        // game_b has recent buy history for timber.
        let nation_b = game_b.get_nation_mut(NationId(2)).unwrap();
        nation_b
            .trade_history
            .push(crate::economy::trade::TradeHistoryEntry {
                turn: TurnNumber::new(1),
                partner: NationId(1),
                resource: ResourceType::Timber,
                quantity: 30,
                total_cost: Money::dollars(600),
                bought: true,
            });

        let cfg = game_a.game_data.game_config.clone();
        let demand_a =
            compute_resource_demand(game_a.get_nation(NationId(2)).unwrap(), &game_a, &cfg);
        let demand_b =
            compute_resource_demand(game_b.get_nation(NationId(2)).unwrap(), &game_b, &cfg);

        let a_timber = demand_a.get(&ResourceType::Timber).copied().unwrap_or(0.0);
        let b_timber = demand_b.get(&ResourceType::Timber).copied().unwrap_or(0.0);
        assert!(a_timber > 0.0, "baseline must have positive timber demand");
        assert!(
            b_timber < a_timber,
            "recent imports should reduce timber demand (a={a_timber}, b={b_timber})"
        );
    }

    #[test]
    fn demand_not_discounted_when_weight_zero() {
        // Safety valve: if trade_discount_weight is 0, trade history and
        // consulates must not influence demand at all.
        let mut game = test_game_with_ai();
        prime_timber_deficit(&mut game, NationId(2));
        game.get_nation_mut(NationId(2))
            .unwrap()
            .trade_history
            .push(crate::economy::trade::TradeHistoryEntry {
                turn: TurnNumber::new(1),
                partner: NationId(1),
                resource: ResourceType::Timber,
                quantity: 100,
                total_cost: Money::dollars(2000),
                bought: true,
            });
        let mut cfg = game.game_data.game_config.clone();
        cfg.trade_discount_weight = 0.0;

        let demand = compute_resource_demand(game.get_nation(NationId(2)).unwrap(), &game, &cfg);
        // With weight=0, demand should equal the raw LumberMill deficit
        // (capacity 2 × 2 − warehouse 0 = 4).
        let timber = demand.get(&ResourceType::Timber).copied().unwrap_or(0.0);
        assert_eq!(timber, 4.0);
    }

    #[test]
    fn demand_lookback_window_drops_stale_imports() {
        // Imports older than cfg.trade_lookback_turns are ignored.
        let mut game = test_game_with_ai();
        prime_timber_deficit(&mut game, NationId(2));
        // Game is at turn 1 by default; an entry on turn 0 is still inside
        // the default 8-turn window. Advance the game turn past the window.
        game.turn = TurnNumber::new(50);
        game.get_nation_mut(NationId(2))
            .unwrap()
            .trade_history
            .push(crate::economy::trade::TradeHistoryEntry {
                turn: TurnNumber::new(1), // 49 turns stale
                partner: NationId(1),
                resource: ResourceType::Timber,
                quantity: 100,
                total_cost: Money::dollars(2000),
                bought: true,
            });

        let cfg = game.game_data.game_config.clone();
        let demand = compute_resource_demand(game.get_nation(NationId(2)).unwrap(), &game, &cfg);
        let timber = demand.get(&ResourceType::Timber).copied().unwrap_or(0.0);
        // Stale history ignored → still the raw deficit.
        assert_eq!(timber, 4.0);
    }

    // ── Commitment-aware planner (card #132) ───────────────────

    use crate::data::GameData;
    use crate::map::tile::Tile;

    /// Build a minimal nation-1 game with a country-capital tile at `capital`
    /// (grain deposit, depot), plus extra tiles for the planner to consider.
    /// Province 1 owns `capital` and all `extras`.
    fn planner_game(capital: HexCoord, extras: &[(HexCoord, ResourceType)]) -> GameState {
        let mut hex_map = HexMap::new(20, 20);
        // Capital tile: country-capital + depot, grain resource so it yields.
        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap_tile.set_resource(ResourceType::Grain);
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        hex_map.set_tile(capital, cap_tile);

        let mut tiles: Vec<HexCoord> = vec![capital];
        for (coord, resource) in extras {
            let mut t = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
            t.set_resource(*resource);
            hex_map.set_tile(*coord, t);
            tiles.push(*coord);
        }

        let province = Province::new(
            ProvinceId(1),
            "P".to_string(),
            NationId(1),
            capital,
            tiles,
            4,
        );
        let mut nation = Nation::new(
            NationId(1),
            "N1".to_string(),
            crate::nation::NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        // LumberMill so the planner sees timber demand.
        nation
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        nation.treasury = Money::dollars(20_000);

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "t".to_string(),
            hex_map,
            provinces: vec![province],
            nations: vec![nation],
            human_player_nation: NationId(99), // unused in planner tests
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: crate::diplomacy::DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            political_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
            last_cash_flow: std::collections::HashMap::new(),
            last_resource_flow: std::collections::HashMap::new(),
            pending_ai_cash_spending: Vec::new(),
            pending_ai_cash_income: Vec::new(),
        }
    }

    #[test]
    fn plan_honors_commitment_over_better_candidate() {
        // Two candidates reachable from the capital; B has higher coverage.
        // Seed a commitment to A; planner must KeepCommitment(A) anyway.
        let capital = HexCoord::new(0, 0);
        let a = HexCoord::new(2, 0);
        let b = HexCoord::new(0, 2);
        let mut game = planner_game(
            capital,
            &[
                (a, ResourceType::Timber),
                (b, ResourceType::Timber),
                // Extra timber tiles around `b` to make its coverage higher.
                (HexCoord::new(1, 2), ResourceType::Timber),
                (HexCoord::new(-1, 2), ResourceType::Timber),
                // Path filler between capital and a, b.
                (HexCoord::new(1, 0), ResourceType::Grain),
                (HexCoord::new(0, 1), ResourceType::Grain),
            ],
        );

        // Install a commitment to `a`.
        game.get_nation_mut(NationId(1))
            .unwrap()
            .ai_priority_state
            .committed_infra_target = Some(crate::nation::CommittedInfraTarget {
            candidate: a,
            origin_capital: capital,
            turn_committed: 1,
        });

        let outcome = plan_next_depot(&game, NationId(1));
        match outcome {
            PlanOutcome::KeepCommitment(plan) => {
                assert_eq!(plan.candidate, a, "planner must honor committed target");
                assert_eq!(plan.origin_capital, capital);
            }
            other => panic!("expected KeepCommitment, got {other:?}"),
        }
    }

    #[test]
    fn plan_clears_fulfilled_commitment() {
        // Commitment's candidate already has a depot → ClearAndReplan (Fresh).
        let capital = HexCoord::new(0, 0);
        let a = HexCoord::new(2, 0);
        let mut game = planner_game(
            capital,
            &[
                (a, ResourceType::Timber),
                (HexCoord::new(1, 0), ResourceType::Grain),
            ],
        );
        // Place a depot at `a` so the commitment is "fulfilled".
        if let Some(t) = game.hex_map.get_tile_mut(a) {
            t.infrastructure.has_depot = true;
        }
        game.get_nation_mut(NationId(1))
            .unwrap()
            .ai_priority_state
            .committed_infra_target = Some(crate::nation::CommittedInfraTarget {
            candidate: a,
            origin_capital: capital,
            turn_committed: 1,
        });

        let outcome = plan_next_depot(&game, NationId(1));
        assert!(
            matches!(outcome, PlanOutcome::Fresh(_)),
            "fulfilled commitment must clear and re-plan"
        );
    }

    #[test]
    fn plan_clears_unreachable_commitment() {
        // Commitment to a candidate we no longer own → Fresh.
        let capital = HexCoord::new(0, 0);
        let a = HexCoord::new(2, 0);
        let mut game = planner_game(
            capital,
            &[
                (a, ResourceType::Timber),
                (HexCoord::new(1, 0), ResourceType::Grain),
            ],
        );
        // Install a commitment to a coordinate NOT in any owned province.
        let off_map = HexCoord::new(15, 15);
        game.get_nation_mut(NationId(1))
            .unwrap()
            .ai_priority_state
            .committed_infra_target = Some(crate::nation::CommittedInfraTarget {
            candidate: off_map,
            origin_capital: capital,
            turn_committed: 1,
        });

        let outcome = plan_next_depot(&game, NationId(1));
        assert!(
            matches!(outcome, PlanOutcome::Fresh(_)),
            "unowned commitment candidate must clear and re-plan"
        );
    }

    #[test]
    fn plan_roots_from_nearest_country_capital() {
        // Two country capitals. A target is close to the second one; the
        // returned plan's origin_capital must be the nearer of the two.
        let cap1 = HexCoord::new(0, 0);
        let cap2 = HexCoord::new(10, 0);
        // A midway corridor of owned tiles between the two capitals so
        // both Dijkstras can reach a candidate; the Dijkstra frontier
        // meets somewhere in the middle.
        let corridor: Vec<HexCoord> = (1..=9).map(|q| HexCoord::new(q, 0)).collect();
        // A timber tile outside cap2's direct radius so candidates near
        // cap2 have positive coverage after `already_covered` is deducted.
        // cap2's radius only covers (10,0)±1.
        let timber_yield = HexCoord::new(7, 0);

        let mut hex_map = HexMap::new(20, 20);
        let mut cap1_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap1_tile.set_resource(ResourceType::Grain);
        cap1_tile.is_country_capital = true;
        cap1_tile.infrastructure.has_depot = true;
        hex_map.set_tile(cap1, cap1_tile);

        let mut cap2_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap2_tile.set_resource(ResourceType::Grain);
        cap2_tile.is_country_capital = true;
        cap2_tile.infrastructure.has_depot = true;
        hex_map.set_tile(cap2, cap2_tile);

        // Fill the corridor (every tile between the capitals is owned).
        for c in &corridor {
            let resource = if *c == timber_yield {
                ResourceType::Timber
            } else {
                ResourceType::Grain
            };
            let mut t = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
            t.set_resource(resource);
            hex_map.set_tile(*c, t);
        }

        let mut all_tiles = vec![cap1, cap2];
        all_tiles.extend(corridor.iter().copied());
        let province = Province::new(
            ProvinceId(1),
            "Big".to_string(),
            NationId(1),
            cap1,
            all_tiles,
            4,
        );
        let mut nation = Nation::new(
            NationId(1),
            "N1".to_string(),
            crate::nation::NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        nation.treasury = Money::dollars(20_000);

        let game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "t".to_string(),
            hex_map,
            provinces: vec![province],
            nations: vec![nation],
            human_player_nation: NationId(99),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: crate::diplomacy::DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            political_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
            last_cash_flow: std::collections::HashMap::new(),
            last_resource_flow: std::collections::HashMap::new(),
            pending_ai_cash_spending: Vec::new(),
            pending_ai_cash_income: Vec::new(),
        };

        let outcome = plan_next_depot(&game, NationId(1));
        let plan = outcome
            .as_plan()
            .expect("planner should pick a target")
            .clone();
        // Whatever candidate wins, its origin_capital must be whichever of
        // cap1/cap2 yields the shorter path. With a linear corridor of
        // grassland hexes, the nearer capital by Dijkstra is the nearer
        // capital by q-axis distance.
        let dist_to_cap1 = (plan.candidate.q - cap1.q).unsigned_abs();
        let dist_to_cap2 = (plan.candidate.q - cap2.q).unsigned_abs();
        let expected_origin = if dist_to_cap2 <= dist_to_cap1 {
            cap2
        } else {
            cap1
        };
        assert_eq!(
            plan.origin_capital, expected_origin,
            "plan must root from the nearer country capital (candidate={:?}, \
             dist to cap1={}, dist to cap2={})",
            plan.candidate, dist_to_cap1, dist_to_cap2
        );
    }

    #[test]
    fn plan_reuses_existing_rail_at_zero_cost() {
        // Lay rail all the way to the target; path_cost must be zero
        // (candidate reachable entirely through existing rail).
        let capital = HexCoord::new(0, 0);
        let rail_hex = HexCoord::new(1, 0);
        let target = HexCoord::new(2, 0);
        // Put a timber-bearing tile next to target so `target` as a depot
        // candidate has coverage (the target tile itself is within the
        // capital's already-covered radius only out to distance 1).
        let extra_timber = HexCoord::new(3, 0);
        let mut game = planner_game(
            capital,
            &[
                (rail_hex, ResourceType::Grain), // filler
                (target, ResourceType::Timber),
                (extra_timber, ResourceType::Timber),
            ],
        );
        // Mark the intermediate hex AND the target as existing rail.
        for h in [rail_hex, target] {
            if let Some(t) = game.hex_map.get_tile_mut(h) {
                t.infrastructure.has_railroad = true;
            }
        }

        let outcome = plan_next_depot(&game, NationId(1));
        let plan = outcome
            .as_plan()
            .expect("planner should pick a target")
            .clone();
        assert_eq!(
            plan.path_cost,
            Money::ZERO,
            "existing rail reused at zero cost → path cost 0, got {:?}",
            plan.path_cost
        );
    }

    #[test]
    fn plan_clears_unreachable_commitment_owned_but_no_path() {
        // Candidate IS owned but the only path from the capital runs through
        // Swamp terrain. Nation has no "Iron Railroad Bridge" tech, so swamp is
        // impassable. Dijkstra cannot reach the candidate → commitment must clear.
        let capital = HexCoord::new(0, 0);
        let swamp_coord = HexCoord::new(1, 0);
        let candidate = HexCoord::new(2, 0);

        // planner_game adds both hexes to province 1 as grassland.
        let mut game = planner_game(
            capital,
            &[
                (swamp_coord, ResourceType::Grain),
                (candidate, ResourceType::Timber),
            ],
        );

        // Replace the intermediate hex with swamp terrain (tech-gated).
        let mut swamp_tile = Tile::with_province(TerrainType::Swamp, ProvinceId(1));
        swamp_tile.set_resource(ResourceType::Grain);
        game.hex_map.set_tile(swamp_coord, swamp_tile);

        // Install commitment to the candidate.
        game.get_nation_mut(NationId(1))
            .unwrap()
            .ai_priority_state
            .committed_infra_target = Some(crate::nation::CommittedInfraTarget {
            candidate,
            origin_capital: capital,
            turn_committed: 1,
        });

        // Nation has no techs → swamp is impassable → candidate is unreachable.
        let outcome = plan_next_depot(&game, NationId(1));
        assert!(
            matches!(outcome, PlanOutcome::Fresh(_)),
            "tech-blocked path must clear commitment and re-plan, got {outcome:?}"
        );
    }

    #[test]
    fn plan_commitment_stable_across_turns() {
        // Simulate two consecutive AI "turns" (two plan_next_depot calls).
        // After the first call sets a commitment, the second call must return
        // KeepCommitment with the same target — no silent retargeting.
        let capital = HexCoord::new(0, 0);
        let intermediate = HexCoord::new(1, 0);
        let candidate = HexCoord::new(2, 0);
        let extra_timber = HexCoord::new(3, 0);

        let mut game = planner_game(
            capital,
            &[
                (intermediate, ResourceType::Grain),
                (candidate, ResourceType::Timber),
                (extra_timber, ResourceType::Timber),
            ],
        );

        // Turn 1: no commitment yet — planner picks a fresh target.
        let outcome1 = plan_next_depot(&game, NationId(1));
        let plan1 = outcome1
            .as_plan()
            .expect("planner should find a target on turn 1")
            .clone();

        // Simulate apply_plan_outcome: write the commitment into game state.
        game.get_nation_mut(NationId(1))
            .unwrap()
            .ai_priority_state
            .committed_infra_target = Some(crate::nation::CommittedInfraTarget {
            candidate: plan1.candidate,
            origin_capital: plan1.origin_capital,
            turn_committed: 1,
        });

        // Turn 2: commitment is set — planner must honor it (KeepCommitment)
        // and return the *same* candidate.
        let outcome2 = plan_next_depot(&game, NationId(1));
        match outcome2 {
            PlanOutcome::KeepCommitment(plan2) => {
                assert_eq!(
                    plan2.candidate, plan1.candidate,
                    "commitment must not retarget across turns"
                );
            }
            other => panic!("expected KeepCommitment on turn 2, got {other:?}"),
        }
    }
}
