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

use super::common::{AiPersonality, PersonalityConfig, get_personality};

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
            nation.economy.buildings.push(Building::new(mill_type, 2));
        }
    }

    // Build factories: first one of each type is free (bootstrap), same as mills.
    // Paper Factory is bootstrapped alongside the LumberMill (it consumes
    // Lumber the same way the FurnitureFactory does) — without this the AI
    // never builds paper, blocking worker training & tech research on harder
    // difficulties where free starting factories aren't given.
    let mill_factory_pairs = [
        (BuildingType::LumberMill, BuildingType::FurnitureFactory),
        (BuildingType::LumberMill, BuildingType::PaperFactory),
        (BuildingType::SteelMill, BuildingType::HardwareFactory),
        (BuildingType::TextileMill, BuildingType::ClothingFactory),
    ];
    for (mill, factory) in mill_factory_pairs {
        if nation.has_building(mill) && !nation.has_building(factory) {
            nation.economy.buildings.push(Building::new(factory, 1));
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

    for building in &nation.economy.buildings {
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

    let money_urgency = if nation.economy.treasury < Money::dollars(3000) {
        4.0
    } else if nation.economy.treasury < Money::dollars(8000) {
        2.0
    } else {
        1.0
    };
    *demand.entry(ResourceType::Gold).or_default() += 5.0 * money_urgency;
    *demand.entry(ResourceType::Gems).or_default() += 10.0 * money_urgency;
    *demand.entry(ResourceType::Oil).or_default() += 2.0 * money_urgency;

    let total_food = nation.resource_amount(ResourceType::Grain)
        + nation.resource_amount(ResourceType::Fruit)
        + nation.resource_amount(ResourceType::Livestock)
        + nation.resource_amount(ResourceType::Fish);
    let workers = nation.economy.labor.total_workers();
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
    for entry in &nation.archives.trade_history {
        if !entry.bought {
            continue;
        }
        if entry.turn.0 < cutoff {
            continue;
        }
        *history.entry(entry.resource).or_default() += entry.quantity as f64;
    }

    for other in &game.world.nations {
        if other.id == nation.id || other.is_great_power() || other.province_ids.is_empty() {
            continue;
        }
        let has_consulate = game
            .world
            .diplomacy
            .get_relation(nation.id, other.id)
            .is_some_and(|r| r.has_consulate);
        if !has_consulate {
            continue;
        }
        for pid in &other.province_ids {
            if let Some(p) = game.get_province(*pid) {
                for &coord in &p.tiles {
                    if let Some(tile) = game.world.hex_map.get_tile(coord)
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
        .world
        .provinces
        .iter()
        .filter(|p| p.owner == nation_id)
        .collect();
    let already_covered = collectable_hexes(&game.world.hex_map, &owned_provinces, &connected);

    let owned_hexes: HashSet<HexCoord> = owned_provinces
        .iter()
        .flat_map(|p| p.tiles.iter().copied())
        .collect();

    // Seeds: every owned country-capital tile, plus every owned tile with a
    // built port. Ports are sea-accessible rail-network entry points — once
    // a port exists in an isolated province, rail can be extended from it
    // to further provinces behind a tech-blocked terrain barrier. Include
    // the home capital defensively even if its `is_country_capital` flag is
    // somehow unset (map-gen edge case).
    let capital_tile = match game.get_province(nation.capital_province_id) {
        Some(p) => p.capital_tile,
        None => return PlanOutcome::Fresh(None),
    };
    let mut capital_seeds: Vec<HexCoord> = vec![capital_tile];
    for &h in &owned_hexes {
        if h == capital_tile {
            continue;
        }
        if let Some(tile) = game.world.hex_map.get_tile(h)
            && (tile.is_country_capital || tile.infrastructure.has_port)
        {
            capital_seeds.push(h);
        }
    }

    // ── Check existing commitment first ───────────────────────
    if let Some(t) = nation
        .diplomacy
        .ai_priority_state
        .committed_infra_target
        .as_ref()
    {
        let cand_tile = game.world.hex_map.get_tile(t.candidate);
        let fulfilled = cand_tile.is_some_and(|tile| tile.infrastructure.has_depot);
        let candidate_ownership_ok = owned_hexes.contains(&t.candidate);
        // Accept the origin as valid if (a) the tile still has
        // `is_country_capital`, OR (b) it's still the nation's own
        // `capital_province.capital_tile`. Case (b) mirrors the defensive
        // home-capital fallback in seed construction above: if the flag is
        // somehow unset on the home-capital tile, the planner still treats
        // it as a valid origin, and commitment validation must agree —
        // otherwise the commitment would clear every turn (F-001).
        let origin_tile = game.world.hex_map.get_tile(t.origin_capital);
        let origin_is_home_capital = t.origin_capital == capital_tile;
        let origin_ok = owned_hexes.contains(&t.origin_capital)
            && (origin_is_home_capital
                || origin_tile
                    .is_some_and(|tile| tile.is_country_capital || tile.infrastructure.has_port));

        if !fulfilled && candidate_ownership_ok && origin_ok {
            // Single-source Dijkstra from the committed origin capital to
            // verify the commitment is still reachable under current tech
            // and ownership, and to refresh the path with whatever rail has
            // been laid since.
            let mut origin_seed = HashSet::new();
            origin_seed.insert(t.origin_capital);
            let (dist, prev, _source) = dijkstra_from_seeds(
                &game.world.hex_map,
                &origin_seed,
                &owned_hexes,
                cfg,
                &nation.researched_techs,
                &game.game_data,
            );
            if t.candidate == t.origin_capital || dist.contains_key(&t.candidate) {
                let path = reconstruct_path(&game.world.hex_map, &prev, t.candidate);
                let path_cost = sum_path_cost(&game.world.hex_map, &path, cfg);
                let coverage_value = coverage_around(
                    &game.world.hex_map,
                    t.candidate,
                    &owned_hexes,
                    &already_covered,
                    &compute_resource_demand(nation, game, cfg),
                    &game.game_data.tech_tree,
                    &nation.researched_techs,
                    cfg.infra_improvability_weight,
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
        &game.world.hex_map,
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
        let tile = match game.world.hex_map.get_tile(candidate) {
            Some(t) => t,
            None => continue,
        };
        if !tile.terrain().is_land() || tile.infrastructure.has_depot {
            continue;
        }

        let coverage_value = coverage_around(
            &game.world.hex_map,
            candidate,
            &owned_hexes,
            &already_covered,
            &demand,
            &game.game_data.tech_tree,
            &nation.researched_techs,
            cfg.infra_improvability_weight,
        );
        if coverage_value == 0 {
            continue;
        }

        // Unreachable from any country capital → skip.
        if !seed_set.contains(&candidate) && !dist.contains_key(&candidate) {
            continue;
        }

        let path = reconstruct_path(&game.world.hex_map, &prev, candidate);
        let path_cost = sum_path_cost(&game.world.hex_map, &path, cfg);

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
                        && (plan.candidate.q, plan.candidate.r) < (b.candidate.q, b.candidate.r))
            }
        };
        if better {
            best = Some(plan);
        }
    }

    PlanOutcome::Fresh(best)
}

/// Find the best coastal tile in an owned province that is completely
/// unreachable by rail from any country capital given current technology.
///
/// Returns `Some(coord)` when such a province exists and has a coastal tile
/// suitable for a port build — i.e. the province is tech-blocked (no path
/// through the Dijkstra graph) and not yet served by a built port.
/// Returns `None` when every owned province is either rail-reachable, already
/// has a port, or has no qualifying coastal tile.
pub(super) fn find_stranded_port_target(game: &GameState, nation_id: NationId) -> Option<HexCoord> {
    use crate::map::infrastructure::collectable_hexes;
    use crate::turn::connected_provinces;

    let nation = game.get_nation(nation_id)?;
    let cfg = &game.game_data.game_config;

    let owned_provinces: Vec<&crate::map::Province> = game
        .world
        .provinces
        .iter()
        .filter(|p| p.owner == nation_id)
        .collect();

    let owned_hexes: HashSet<HexCoord> = owned_provinces
        .iter()
        .flat_map(|p| p.tiles.iter().copied())
        .collect();

    // Capital seeds — mirrors plan_next_depot: country capitals + built ports.
    let capital_tile = game.get_province(nation.capital_province_id)?.capital_tile;
    let mut capital_seeds: HashSet<HexCoord> = std::iter::once(capital_tile).collect();
    for &h in &owned_hexes {
        if h == capital_tile {
            continue;
        }
        if game
            .world
            .hex_map
            .get_tile(h)
            .is_some_and(|t| t.is_country_capital || t.infrastructure.has_port)
        {
            capital_seeds.insert(h);
        }
    }

    // Tech-gated Dijkstra — same as plan_next_depot.
    let (dist, _, _) = dijkstra_from_seeds(
        &game.world.hex_map,
        &capital_seeds,
        &owned_hexes,
        cfg,
        &nation.researched_techs,
        &game.game_data,
    );

    let connected = connected_provinces(game, nation_id);
    let already_covered = collectable_hexes(&game.world.hex_map, &owned_provinces, &connected);
    let demand = compute_resource_demand(nation, game, cfg);

    let mut best_coord: Option<HexCoord> = None;
    let mut best_coverage: u32 = 0;

    for province in &owned_provinces {
        if province.id == nation.capital_province_id {
            continue;
        }

        // Province is rail-reachable — the normal depot path handles it.
        let any_rail_reachable = province
            .tiles
            .iter()
            .any(|c| capital_seeds.contains(c) || dist.contains_key(c));
        if any_rail_reachable {
            continue;
        }

        // Already served by a port — nothing to do.
        let already_has_port = province.tiles.iter().any(|c| {
            game.world
                .hex_map
                .get_tile(*c)
                .is_some_and(|t| t.infrastructure.has_port)
        });
        if already_has_port {
            continue;
        }

        // Only bother if there's resource value worth connecting.
        let coverage = coverage_around(
            &game.world.hex_map,
            province.capital_tile,
            &owned_hexes,
            &already_covered,
            &demand,
            &game.game_data.tech_tree,
            &nation.researched_techs,
            cfg.infra_improvability_weight,
        );
        if coverage == 0 {
            continue;
        }

        // Find a qualifying coastal tile (prefer province capital first).
        let coastal = std::iter::once(province.capital_tile)
            .chain(
                province
                    .tiles
                    .iter()
                    .copied()
                    .filter(|&c| c != province.capital_tile),
            )
            .find(|c| {
                let Some(tile) = game.world.hex_map.get_tile(*c) else {
                    return false;
                };
                if !tile.terrain().is_land() || tile.infrastructure.has_port {
                    return false;
                }
                if tile.assigned_civilian.is_some() {
                    return false;
                }
                // Must be adjacent to real ocean (not a lake).
                c.neighbors().iter().any(|n| {
                    let Some(nt) = game.world.hex_map.get_tile(*n) else {
                        return false;
                    };
                    if nt.terrain().is_land() {
                        return false;
                    }
                    !game
                        .world
                        .sea_zones
                        .iter()
                        .any(|z| z.is_lake && z.hexes.contains(n))
                })
            });

        if let Some(coord) = coastal
            && coverage > best_coverage
        {
            best_coverage = coverage;
            best_coord = Some(coord);
        }
    }

    best_coord
}

/// Demand-weighted coverage of the 1-hex radius around `center`, excluding
/// tiles already covered by another connected collector.
///
/// Card #217: in addition to a tile's *current* demand-weighted yield, each
/// tile also contributes a smaller "improvability" term, equal to
/// `(tech_capped_max - current_improvement) * demand_weight *
/// cfg.infra_improvability_weight`. This makes the planner prefer candidates
/// covering tiles that will yield once worked, not just tiles yielding today.
#[allow(clippy::too_many_arguments)]
fn coverage_around(
    hex_map: &HexMap,
    center: HexCoord,
    owned_hexes: &HashSet<HexCoord>,
    already_covered: &HashSet<HexCoord>,
    demand: &HashMap<ResourceType, f64>,
    tech_tree: &crate::tech::TechTree,
    researched_techs: &[crate::events::TechId],
    improvability_weight: f64,
) -> u32 {
    let mut v: f64 = 0.0;
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
        v += score_tile_for_demand(hex_map, *r_hex, demand) as f64;
        if improvability_weight > 0.0
            && let Some(tile) = hex_map.get_tile(*r_hex)
            // Per the manual, hidden minerals are unknown until prospected.
            // Mirror the visibility check `score_civilian` already applies
            // so the depot planner doesn't bias toward secret deposits.
            && tile.has_visible_resource()
        {
            let resource = tile.resource_deposit();
            let max = tech_tree.effective_max_improvement_level(
                tile.terrain(),
                resource,
                researched_techs,
            );
            let current = tile.improvement_level();
            if max > current {
                let demand_w = resource
                    .and_then(|r| demand.get(&r).copied())
                    .unwrap_or(1.0);
                v += (max - current) as f64 * demand_w * improvability_weight;
            }
        }
    }
    v as u32
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
        Some(n) => n.economy.treasury,
        None => return,
    };

    // Need at least enough for a depot
    if treasury < Money::dollars(2000) {
        return;
    }

    // ── Read infrastructure budget from Lua config ──────────────
    let personality = get_personality(game, nation_id);

    let lua_cfg = super::lua_bridge::get_personality_config(game, personality);
    let pc = PersonalityConfig::for_personality(personality);
    let base_infrastructure_budget: Money = 'val: {
        if let Some(budget) = lua_cfg.as_ref().map(|c| c.infrastructure_budget) {
            break 'val Money::dollars(budget);
        }
        Money::dollars(pc.infra_base_budget_dollars as i64)
    };

    // Scale budget with treasury: spend more aggressively when cash-rich
    let scale_threshold: i64 = 'val: {
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
        game.world
            .hex_map
            .get_tile(*coord)
            .is_some_and(|t| t.infrastructure.has_depot)
    });

    if !capital_has_depot {
        let provinces_snapshot = game.world.provinces.clone();
        let cfg = game.game_data.game_config.clone();
        if let Ok(cost) = build_depot(
            &mut game.world.hex_map,
            capital_tile,
            nation_id,
            &provinces_snapshot,
            &cfg,
        ) {
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.economy.treasury -= cost;
            }
            game.transient.pending_ai_cash_spending.push((
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
            let score = game.get_province(pid).map(|p| {
                score_province(&game.world.hex_map, p, nation_ref, game, &cfg_for_scoring)
            })?;
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
        if is_province_connected(
            &game.world.hex_map,
            capital_tile,
            *pid,
            &game.world.provinces,
        ) {
            continue;
        }

        // Ensure the target province has a depot
        let target_depot_tile = match game.get_province(*pid) {
            Some(p) => p.capital_tile,
            None => continue,
        };
        let has_depot = game
            .world
            .hex_map
            .get_tile(target_depot_tile)
            .is_some_and(|t| t.infrastructure.has_depot);

        let provinces_snapshot = game.world.provinces.clone();
        let cfg = game.game_data.game_config.clone();
        if !has_depot
            && budget - spent >= Money::dollars(2000)
            && let Ok(cost) = build_depot(
                &mut game.world.hex_map,
                target_depot_tile,
                nation_id,
                &provinces_snapshot,
                &cfg,
            )
        {
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.economy.treasury -= cost;
            }
            game.transient.pending_ai_cash_spending.push((
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
        let network = get_railroad_network(&game.world.hex_map, capital_tile);
        let researched: Vec<crate::events::TechId> = game
            .get_nation(nation_id)
            .map(|n| n.researched_techs.clone())
            .unwrap_or_default();
        if let Some(path) = find_cheapest_path(
            &game.world.hex_map,
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
                    &mut game.world.hex_map,
                    coord,
                    nation_id,
                    &researched,
                    &provinces_snapshot,
                    &game.game_data,
                    &cfg,
                ) {
                    if let Some(nation) = game.get_nation_mut(nation_id) {
                        nation.economy.treasury -= cost;
                    }
                    game.transient.pending_ai_cash_spending.push((
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
            .map(|n| n.economy.treasury)
            .unwrap_or(Money::ZERO);
        if spent < budget && current_treasury > infrastructure_budget * 3 {
            continue;
        }
        break;
    }
}

/// The AI keeps a reserve of each good (Lua-configurable) and sells excess when treasury is low.
///
/// Builds a `NationEconomySnapshot` for all read operations; mutations go
/// through `game` directly (Trello #163).
#[allow(unused_variables)] // personality used only with cfg(feature = "lua")
pub fn ai_manage_resources(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<super::AiAction>,
) {
    let personality = get_personality(game, nation_id);

    // ── Read Lua config (feature-gated) ──────────────────────
    let lua_cfg = super::lua_bridge::get_personality_config(game, personality);

    let goods_sell_threshold: i64 = 'val: {
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.goods_sell_treasury_threshold)
        {
            break 'val v;
        }
        3000
    };
    let goods_reserve: u32 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.goods_reserve) {
            break 'val v;
        }
        2
    };
    // Fat-stockpile dump: even with a healthy treasury, liquidate goods that
    // pile up far beyond any plausible turn-over-turn consumption. Without
    // this, GPs hoard hundreds of furniture/clothing/hardware once trade
    // brings the treasury above `goods_sell_threshold` and the minor-bid
    // path (1 unit per minor per turn) can't drain the warehouse.
    let goods_fat_stockpile_threshold: u32 = 'val: {
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.goods_fat_stockpile_threshold)
        {
            break 'val v;
        }
        30
    };

    // Build snapshot for all planning reads (Trello #163).
    let snapshot = super::snapshot::NationEconomySnapshot::build(game, nation_id);

    // Two trigger conditions for the goods auto-sale:
    //   1. Treasury below `goods_sell_threshold` — emergency cash run
    //      (legacy behavior).
    //   2. Any single goods-type stockpile above `goods_fat_stockpile_threshold`
    //      — drain the warehouse so production keeps flowing instead of
    //      capacity going to waste.
    let treasury_low = snapshot.treasury < Money::dollars(goods_sell_threshold);
    let any_fat = [
        GoodsType::Furniture,
        GoodsType::Hardware,
        GoodsType::Clothing,
    ]
    .iter()
    .any(|g| snapshot.goods(*g) >= goods_fat_stockpile_threshold);
    if !treasury_low && !any_fat {
        return;
    }

    let nation_name = game
        .get_nation(nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    // Define goods to sell and their prices
    let goods_prices: [(GoodsType, i64); 3] = [
        (GoodsType::Furniture, 200),
        (GoodsType::Hardware, 250),
        (GoodsType::Clothing, 200),
    ];

    let mut total_revenue = Money::ZERO;

    for (goods_type, price_per_unit) in &goods_prices {
        // Read from snapshot (stable view of the turn).
        let amount = snapshot.goods(*goods_type);
        // Two floors: the static `goods_reserve` is the absolute minimum we
        // ever drop to, used in the emergency-cash branch. The fat-stockpile
        // branch only drains down to `goods_fat_stockpile_threshold` so the
        // AI keeps a healthy buffer for war/disasters and doesn't yo-yo
        // between dump and refill.
        let floor = if treasury_low {
            goods_reserve
        } else {
            goods_fat_stockpile_threshold
        };
        if amount <= floor {
            continue;
        }
        let excess = amount - floor;
        let revenue = Money::dollars(*price_per_unit) * excess as i64;

        // Mutations go through game.
        let current_turn = game.turn;
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return;
        };
        nation.consume_goods(*goods_type, excess);
        nation.economy.treasury += revenue;
        total_revenue += revenue;
        // Record in trade history: AI GP auto-sold goods to world market (NationId(0) sentinel)
        nation
            .archives
            .trade_history
            .push(trade::TradeHistoryEntry {
                turn: current_turn,
                partner: NationId(0),
                resource: ResourceType::Timber, // sentinel; commodity_label carries the real name
                commodity_label: format!("{goods_type:?}"),
                quantity: excess,
                total_cost: revenue,
                bought: false,
            });
        game.transient.pending_ai_goods_outflows.push((
            nation_id,
            *goods_type,
            crate::economy::ledger::ResourceOut::AutoSoldToMarket,
            excess,
        ));
    }

    if total_revenue > Money::ZERO {
        game.transient
            .pending_ai_cash_income
            .push((nation_id, total_revenue));
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
            .map(|n| n.economy.treasury.as_dollars())
            .unwrap_or(0);
        eprintln!(
            "[AI:{}:economy] treasury=${}, personality={}",
            nation_name, treasury, personality
        );
    }

    // Build infrastructure handles mills and factories
    ai_build_infrastructure(game, nation_id);

    // ── Read Lua config (feature-gated) ──────────────────────
    let lua_cfg = super::lua_bridge::get_personality_config(game, personality);

    let pc_eco = PersonalityConfig::for_personality(personality);
    let expansion_threshold_multiplier: u32 = 'val: {
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.expansion_threshold_multiplier)
        {
            break 'val v;
        }
        pc_eco.expansion_threshold_multiplier
    };

    let use_tier_expansion: bool = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.use_tier_expansion) {
            break 'val v;
        }
        true
    };

    let high_treasury_threshold: i64 = 'val: {
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.high_treasury_expansion_threshold)
        {
            break 'val v;
        }
        15_000
    };

    // Build snapshot after infrastructure bootstrap so it reflects new buildings.
    // Used for all resource/building reads below; mutations go through `game` (Trello #163).
    let snap = super::snapshot::NationEconomySnapshot::build(game, nation_id);

    // Expand mills when input resources exceed capacity * threshold
    let mill_types = [
        BuildingType::LumberMill,
        BuildingType::SteelMill,
        BuildingType::TextileMill,
    ];
    let expansions_needed: Vec<BuildingType> = mill_types
        .iter()
        .filter_map(|&bt| {
            let cap = snap.building_capacity(bt);
            if cap == 0 || snap.is_expanding(bt) {
                return None;
            }
            let input_resources = match bt {
                BuildingType::LumberMill => snap.resource(ResourceType::Timber),
                BuildingType::SteelMill => {
                    snap.resource(ResourceType::Coal)
                        .min(snap.resource(ResourceType::Iron))
                        * 2
                }
                BuildingType::TextileMill => {
                    snap.resource(ResourceType::Cotton) + snap.resource(ResourceType::Wool)
                }
                _ => return None,
            };
            if input_resources > cap * expansion_threshold_multiplier {
                Some(bt)
            } else {
                None
            }
        })
        .collect();

    for bt in expansions_needed {
        expand_building(game, nation_id, bt, use_tier_expansion);
    }

    // Expand factories when their input material exceeds capacity * threshold.
    let factory_types = [
        BuildingType::FurnitureFactory,
        BuildingType::HardwareFactory,
        BuildingType::ClothingFactory,
    ];
    let factory_expansions: Vec<BuildingType> = factory_types
        .iter()
        .filter_map(|&bt| {
            let cap = snap.building_capacity(bt);
            if cap == 0 || snap.is_expanding(bt) {
                return None;
            }
            let input_materials = match bt {
                BuildingType::FurnitureFactory => snap.material(MaterialType::Lumber),
                BuildingType::HardwareFactory => snap.material(MaterialType::Steel),
                BuildingType::ClothingFactory => snap.material(MaterialType::Fabric),
                _ => return None,
            };
            if input_materials > cap * 2 * expansion_threshold_multiplier {
                Some(bt)
            } else {
                None
            }
        })
        .collect();

    for bt in factory_expansions {
        expand_building(game, nation_id, bt, use_tier_expansion);
    }

    // Expand FoodProcessing when food surplus exceeds capacity * threshold.
    let food_threshold: u32 = 'val: {
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.food_processing_expansion_threshold)
        {
            break 'val v;
        }
        2
    };

    let food_cap = snap.building_capacity(BuildingType::FoodProcessing);
    let food_surplus = snap.total_food().saturating_sub(snap.total_workers);

    if food_surplus > food_cap * food_threshold
        && food_cap > 0
        && !snap.is_expanding(BuildingType::FoodProcessing)
    {
        expand_building(
            game,
            nation_id,
            BuildingType::FoodProcessing,
            use_tier_expansion,
        );
    }

    // When treasury is very high, speculative-expand mills and factories.
    if snap.treasury > Money::dollars(high_treasury_threshold) {
        let all_prod_types = [
            BuildingType::LumberMill,
            BuildingType::SteelMill,
            BuildingType::TextileMill,
            BuildingType::FurnitureFactory,
            BuildingType::HardwareFactory,
            BuildingType::ClothingFactory,
        ];
        let expandable: Vec<BuildingType> = all_prod_types
            .iter()
            .filter_map(|&bt| {
                let cap = snap.building_capacity(bt);
                if cap == 0 || snap.is_expanding(bt) {
                    return None;
                }
                let input = match bt {
                    BuildingType::LumberMill => snap.resource(ResourceType::Timber),
                    BuildingType::SteelMill => {
                        snap.resource(ResourceType::Coal)
                            .min(snap.resource(ResourceType::Iron))
                            * 2
                    }
                    BuildingType::TextileMill => {
                        snap.resource(ResourceType::Cotton) + snap.resource(ResourceType::Wool)
                    }
                    BuildingType::FurnitureFactory => snap.material(MaterialType::Lumber),
                    BuildingType::HardwareFactory => snap.material(MaterialType::Steel),
                    BuildingType::ClothingFactory => snap.material(MaterialType::Fabric),
                    _ => return None,
                };
                if cap <= input.max(1) * 2 {
                    Some(bt)
                } else {
                    None
                }
            })
            .collect();

        for bt in expandable {
            expand_building(game, nation_id, bt, use_tier_expansion);
        }
    }
}

/// Expand a building, paying the correct material cost.
/// When `use_tier` is true, uses tier progression (2→4→8→12...) with proportional cost.
/// When false, expands by +1 capacity for 1 lumber + 1 steel.
fn expand_building(game: &mut GameState, nation_id: NationId, bt: BuildingType, use_tier: bool) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Guard against double-expansion when the caller's snapshot is stale (F-001/#163).
    // The snapshot is built once before the multi-phase expansion loop; by the time the
    // high-treasury pass runs, earlier phases may have already started an expansion.
    if nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == bt)
        .is_some_and(|b| b.pending_capacity > 0)
    {
        return;
    }

    let increase = if use_tier {
        nation
            .economy
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
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Lumber,
            crate::economy::ledger::ResourceOut::ConstructionConsumed,
            lumber_cost,
        ));
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Steel,
            crate::economy::ledger::ResourceOut::ConstructionConsumed,
            steel_cost,
        ));
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return;
        };
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

/// Lumber + steel the AI should hold back for building expansion.
///
/// Returns `(lumber_reserve, steel_reserve)`. Both equal `worst_step ×
/// effective_expansions` where `worst_step` is the largest single-tier
/// capacity jump across all buildings the nation already owns and
/// `effective_expansions = max(expansions_per_turn, building_count ×
/// buildings_factor)`. The buildings-factor lets the reserve grow as the
/// economy grows, so a nation with many buildings under construction can
/// hold back more material to keep expanding all of them in parallel.
///
/// Without this reserve every turn's lumber/steel is consumed by the
/// hardware factory + arms production + freight cars + ship construction +
/// minor-nation auto-bids before `expand_building` has a chance to pay for
/// the next mill/factory tier, leaving economies stuck at starter capacity.
pub(crate) fn reserve_for_expansion(
    game: &GameState,
    nation_id: NationId,
    expansions_per_turn: u32,
    buildings_factor: f64,
) -> (u32, u32) {
    let Some(nation) = game.get_nation(nation_id) else {
        return (0, 0);
    };
    // Largest next-tier delta across the nation's existing buildings.
    // expansion_cost(delta) = (delta, delta), so worst-step lumber == steel.
    let worst_step = nation
        .economy
        .buildings
        .iter()
        .map(|b| b.next_capacity().saturating_sub(b.capacity))
        .max()
        .unwrap_or(0);
    let worst_step = worst_step.max(1); // always reserve at least a +1 expansion

    // Building-count contribution. We count only mills/factories/armory/
    // food-processing/paper because those are the buildings the AI actually
    // expands; capitol/shipyard/etc. are fixed.
    let expandable_count = nation
        .economy
        .buildings
        .iter()
        .filter(|b| {
            matches!(
                b.building_type,
                BuildingType::LumberMill
                    | BuildingType::SteelMill
                    | BuildingType::TextileMill
                    | BuildingType::FurnitureFactory
                    | BuildingType::HardwareFactory
                    | BuildingType::ClothingFactory
                    | BuildingType::Armory
                    | BuildingType::PaperFactory
                    | BuildingType::FoodProcessing
            )
        })
        .count() as u32;
    let from_buildings = ((expandable_count as f64) * buildings_factor)
        .ceil()
        .max(0.0) as u32;
    let effective_expansions = expansions_per_turn.max(from_buildings);

    let target = worst_step.saturating_mul(effective_expansions);
    (target, target)
}

/// Lua-tunable read for the expansion-reserve multiplier (turns per expansion).
pub(crate) fn expansions_per_turn_target(game: &GameState, personality: AiPersonality) -> u32 {
    {
        if let Some(cfg) = super::lua_bridge::get_personality_config(game, personality)
            && let Some(v) = cfg.expansions_per_turn_target
        {
            return v;
        }
    }
    let _ = (game, personality);
    2
}

/// Lua-tunable: minimum treasury (dollars) the AI keeps before placing
/// buy-side trade bids. Card [3/6] cash-guard. Default $5,000.
pub(crate) fn trade_buy_treasury_floor(game: &GameState, personality: AiPersonality) -> i64 {
    {
        if let Some(cfg) = super::lua_bridge::get_personality_config(game, personality)
            && let Some(v) = cfg.trade_buy_treasury_floor
        {
            return v;
        }
    }
    let _ = (game, personality);
    1_500
}

/// Lua-tunable: minimum arms held back from any auto-sale path on top of the
/// queued-recruit demand. Card #465. Default 10 (pre-builds about 5 trained
/// infantry units at 2 arms apiece).
pub(crate) fn arms_sell_reserve(game: &GameState, personality: AiPersonality) -> u32 {
    {
        if let Some(cfg) = super::lua_bridge::get_personality_config(game, personality)
            && let Some(v) = cfg.arms_sell_reserve
        {
            return v;
        }
    }
    let _ = (game, personality);
    10
}

/// Lua-tunable: how many turns of input the AI tries to buffer per resource
/// when placing buy-side trade bids. Card [3/6]. Default 2 turns: every
/// nation aims to keep two turns' worth of every chain input in the
/// warehouse, and bids on the world market to close any shortfall.
pub(crate) fn trade_buy_buffer_turns(game: &GameState, personality: AiPersonality) -> u32 {
    {
        if let Some(cfg) = super::lua_bridge::get_personality_config(game, personality)
            && let Some(v) = cfg.trade_buy_buffer_turns
        {
            return v;
        }
    }
    let _ = (game, personality);
    2
}

/// Lua-tunable: per-building multiplier that grows the expansion reserve as
/// the economy grows. A nation with N expandable buildings reserves enough
/// material for `ceil(N × factor)` simultaneous expansions (capped from
/// below by `expansions_per_turn_target`). Default 0.0 keeps existing
/// behavior; set to 0.5 to reserve enough for half the buildings to expand
/// in parallel each turn.
pub(crate) fn expansion_reserve_buildings_factor(
    game: &GameState,
    personality: AiPersonality,
) -> f64 {
    {
        if let Some(cfg) = super::lua_bridge::get_personality_config(game, personality)
            && let Some(v) = cfg.expansion_reserve_buildings_factor
        {
            return v;
        }
    }
    let _ = (game, personality);
    0.0
}

/// Set per-chain output targets so labor & feed allocation favor chains
/// the AI can actually run, instead of leaving every slider at `u32::MAX`.
///
/// For each step, target = min(input_supply, capacity, demand_estimate).
/// Shared inputs (lumber → furniture vs paper, steel → hardware vs armory)
/// are split via Lua weights so personalities differ. War nations bias
/// steel toward the armory; peacetime nations bias it toward hardware.
///
/// Cards [2/6] AI: Production target setter — fill all 9 ChainOutputTargets sliders
#[allow(unused_variables)] // personality used only with cfg(feature = "lua")
pub(crate) fn ai_set_production_targets(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);

    let lua_cfg = super::lua_bridge::get_personality_config(game, personality);

    // ── Chain split weights (Lua-tunable per personality) ────────────
    // Lumber gets split between furniture and paper.
    let lumber_furniture_weight: f64 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.lumber_furniture_weight) {
            break 'val v;
        }
        0.7
    };
    // Steel gets split between hardware (peacetime) and armory (wartime).
    let steel_armory_weight_peace: f64 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.steel_armory_weight_peace) {
            break 'val v;
        }
        0.2
    };
    let steel_armory_weight_war: f64 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.steel_armory_weight_war) {
            break 'val v;
        }
        0.7
    };
    // Buffer multiplier for canned-food target relative to projected immigration.
    let canned_food_buffer: f64 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.canned_food_buffer) {
            break 'val v;
        }
        1.5
    };
    // Floor target so a chain that ran out of inputs this turn still asks for
    // labor next turn — without this, a transient shortage would zero the
    // target and leave the chain dormant even after inputs return.
    let min_chain_target: u32 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.min_chain_target) {
            break 'val v;
        }
        1
    };
    // Workforce → paper-output scaling. One paper unit per N workers, capped
    // by `paper_target_max`. Default: 1 paper per 4 workers, max 40.
    let paper_workers_per_unit: u32 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.paper_workers_per_unit) {
            break 'val v;
        }
        4
    };
    let paper_target_max: u32 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.paper_target_max) {
            break 'val v;
        }
        40
    };

    let snap = super::snapshot::NationEconomySnapshot::build(game, nation_id);

    // Wartime flag: any active war.
    let at_war = game.world.diplomacy.is_at_war_with_anyone(nation_id);

    // Projected immigration demand (this turn's queue + next turn's likely queue).
    let pending_immigration = game
        .get_nation(nation_id)
        .map(|n| n.economy.pending_immigration)
        .unwrap_or(0);

    // ── Mill targets ──────────────────────────────────────────────────
    // Target = min(capacity, input-limited output). Mills produce 1 unit per
    // 2 raw inputs (per-resource for timber/cotton/wool, per-pair for
    // coal+iron). Setting target above the input-limited output would inflate
    // the chain's labor-allocator weight (= `min(target, cap) × 2`) and steal
    // labor from chains that can actually run this turn. The Lua-tunable
    // floor `min_chain_target` keeps a chain's slider non-zero so a one-turn
    // freight gap doesn't permanently silence it.
    let timber_mill_cap = snap.building_capacity(BuildingType::LumberMill);
    let metal_mill_cap = snap.building_capacity(BuildingType::SteelMill);
    let textile_mill_cap = snap.building_capacity(BuildingType::TextileMill);

    let timber_supply = snap.resource(ResourceType::Timber);
    let coal_iron_supply = snap
        .resource(ResourceType::Coal)
        .min(snap.resource(ResourceType::Iron));
    let cotton_wool_supply = snap
        .resource(ResourceType::Cotton)
        .saturating_add(snap.resource(ResourceType::Wool));

    let timber_mill_runnable = timber_supply / 2;
    let metal_mill_runnable = coal_iron_supply;
    let textile_mill_runnable = cotton_wool_supply / 2;

    let timber_mill_target = mill_target(timber_mill_runnable, timber_mill_cap, min_chain_target);
    let metal_mill_target = mill_target(metal_mill_runnable, metal_mill_cap, min_chain_target);
    let textile_mill_target =
        mill_target(textile_mill_runnable, textile_mill_cap, min_chain_target);

    // ── Factory targets ──────────────────────────────────────────────
    // Factories consume 2 units of material per 1 good (materials_per_good=2).
    // Lumber gets split between furniture and paper; steel gets split between
    // hardware and armory. The factory-side projection uses *capped* mill
    // output (`mill_target` already bounded by capacity AND runnable input)
    // so we never over-promise downstream targets.
    //
    // Hold back lumber + steel for building expansion before the factory
    // production phase consumes everything. Without this reserve the
    // hardware factory swallows every steel unit and `expand_building`
    // never finds materials to pay for the next tier-jump.
    let (lumber_reserve, steel_reserve) = reserve_for_expansion(
        game,
        nation_id,
        expansions_per_turn_target(game, personality),
        expansion_reserve_buildings_factor(game, personality),
    );
    // Hold back materials queued for the next merchant hull so the chain
    // projection (which drives factory output and feeds expansion) doesn't
    // commit lumber/steel/fabric the AI is saving for a merchant ship.
    let (m_fabric_reserve, m_lumber_reserve, m_steel_reserve, _m_coal) =
        super::naval::merchant_navy_material_reserve(game, nation_id);
    // Hold back lumber + steel for the next-turn freight-car build when the
    // remote freight network is already saturating capacity. Without this,
    // the hardware factory + armory chain projection swallows every steel
    // unit and `ai_build_transport_proactive` (which runs later in the AI
    // loop) finds nothing to spend on cars.
    let (freight_lumber_reserve, freight_steel_reserve) =
        freight_expansion_material_reserve(game, nation_id);
    let lumber_supply = snap
        .material(MaterialType::Lumber)
        .saturating_add(timber_mill_target)
        .saturating_sub(lumber_reserve)
        .saturating_sub(m_lumber_reserve)
        .saturating_sub(freight_lumber_reserve);
    let steel_supply = snap
        .material(MaterialType::Steel)
        .saturating_add(metal_mill_target)
        .saturating_sub(steel_reserve)
        .saturating_sub(m_steel_reserve)
        .saturating_sub(freight_steel_reserve);
    let fabric_supply = snap
        .material(MaterialType::Fabric)
        .saturating_add(textile_mill_target)
        .saturating_sub(m_fabric_reserve);

    let furniture_cap = snap.building_capacity(BuildingType::FurnitureFactory);
    let hardware_cap = snap.building_capacity(BuildingType::HardwareFactory);
    let clothing_cap = snap.building_capacity(BuildingType::ClothingFactory);
    let armory_cap = snap.building_capacity(BuildingType::Armory);
    let paper_cap = snap.building_capacity(BuildingType::PaperFactory);
    let canned_food_cap = snap.building_capacity(BuildingType::FoodProcessing);

    // Lumber split: furniture vs paper. Each unit of factory output needs
    // 2 lumber. We split the available lumber (warehouse + projected mill
    // output) by a Lua-tunable share so personalities can favor industry
    // (more furniture) vs research (more paper).
    let lumber_for_furniture = ((lumber_supply as f64) * lumber_furniture_weight) as u32;
    let lumber_for_paper = lumber_supply.saturating_sub(lumber_for_furniture);
    let furniture_target = factory_target(
        lumber_for_furniture / 2,
        lumber_for_furniture >= 2,
        furniture_cap,
        min_chain_target,
    );
    // Paper backs worker training & tech research, both of which scale with
    // workforce size. Compute a worker-derived floor so a nation that's
    // adding workers gets paper output to keep up — clamped to a hard cap
    // (default 40) so we don't lock the entire lumber supply into paper.
    // The floor still respects supply/capacity via `factory_target`, so when
    // there's no lumber the chain falls back to `min_chain_target`.
    let paper_worker_floor = if paper_workers_per_unit == 0 {
        paper_target_max.min(paper_cap)
    } else {
        (snap.total_workers / paper_workers_per_unit)
            .min(paper_target_max)
            .min(paper_cap)
    };
    let raw_paper_target = (lumber_for_paper / 2).max(paper_worker_floor);
    let paper_target = factory_target(
        raw_paper_target,
        lumber_for_paper >= 2 || paper_worker_floor > 0,
        paper_cap,
        min_chain_target,
    );

    // Steel split: armory vs hardware. War shifts the split toward armory.
    // Hardware needs 2 steel/unit, armory needs 1 steel/arm.
    let armory_share = if at_war {
        steel_armory_weight_war
    } else {
        steel_armory_weight_peace
    };
    let steel_for_armory = ((steel_supply as f64) * armory_share) as u32;
    let steel_for_hardware = steel_supply.saturating_sub(steel_for_armory);
    let hardware_target = factory_target(
        steel_for_hardware / 2,
        steel_for_hardware >= 2,
        hardware_cap,
        min_chain_target,
    );
    let armory_target = factory_target(
        steel_for_armory,
        steel_for_armory >= 1,
        armory_cap,
        min_chain_target,
    );

    // Fabric → clothing.
    let clothing_target = factory_target(
        fabric_supply / 2,
        fabric_supply >= 2,
        clothing_cap,
        min_chain_target,
    );

    // Canned food: target = min(projected demand, cannery input bottleneck).
    // The cannery consumes 1 grain + 1 fruit + 1 fish/livestock per canned
    // food unit, so the runnable output equals the smallest of the three.
    let grain = snap.resource(ResourceType::Grain);
    let fruit = snap.resource(ResourceType::Fruit);
    let fish_or_livestock = snap
        .resource(ResourceType::Fish)
        .saturating_add(snap.resource(ResourceType::Livestock));
    let cannery_input_cap = grain.min(fruit).min(fish_or_livestock);
    let immigration_demand = ((pending_immigration as f64) * canned_food_buffer).ceil() as u32;
    // Always allow at least 1 unit of demand so workers can be fed even when
    // the immigration queue is empty.
    let canned_food_demand = immigration_demand.max(1);
    let canned_food_runnable = canned_food_demand.min(cannery_input_cap);
    let canned_food_target = factory_target(
        canned_food_runnable,
        cannery_input_cap >= 1,
        canned_food_cap,
        min_chain_target,
    );

    // Write targets back to the nation.
    let Some(nation) = game.get_nation_mut(nation_id) else {
        return;
    };
    let t = &mut nation.economy.chain_targets;
    t.timber_mill = timber_mill_target;
    t.metal_mill = metal_mill_target;
    t.textile_mill = textile_mill_target;
    t.lumber_factory = furniture_target;
    t.steel_factory = hardware_target;
    t.garment_factory = clothing_target;
    t.armory = armory_target;
    t.paper_factory = paper_target;
    t.canned_food_factory = canned_food_target;

    if game.ai_debug {
        let nation_name = game
            .get_nation(nation_id)
            .map(|n| n.name.as_str())
            .unwrap_or("?");
        eprintln!(
            "[AI:{}:targets] timber={} metal={} textile={} furn={} hard={} cloth={} armory={} paper={} food={} (war={})",
            nation_name,
            timber_mill_target,
            metal_mill_target,
            textile_mill_target,
            furniture_target,
            hardware_target,
            clothing_target,
            armory_target,
            paper_target,
            canned_food_target,
            at_war,
        );
    }
}

/// Mill target: clamp to the minimum of capacity and the input-limited
/// runnable output. When `runnable == 0` we fall back to `min_target` so a
/// transient supply gap doesn't permanently silence a chain (re-promoting it
/// next turn means waiting another full turn for any output).
fn mill_target(runnable: u32, capacity: u32, min_target: u32) -> u32 {
    if capacity == 0 {
        return 0;
    }
    if runnable == 0 {
        return min_target.min(capacity);
    }
    runnable.min(capacity)
}

/// Factory target: when supply is sufficient, target = min(desired, capacity)
/// — the desired figure is already runnable (callers pass the split-share
/// converted to output units), so we never inflate it. When supply is
/// insufficient we drop to the anti-oscillation floor.
fn factory_target(desired: u32, has_supply: bool, capacity: u32, min_target: u32) -> u32 {
    if capacity == 0 {
        return 0;
    }
    if !has_supply {
        return min_target.min(capacity);
    }
    desired.min(capacity)
}

/// Sell excess tradeable resources on the market for cash.
///
/// Trello card #463: GPs are the world's manufacturers — the bulk of trade
/// revenue must come from finished goods (Furniture/Clothing/Hardware), not
/// from dumping raw resources. This function therefore only sells a resource
/// when the warehouse holds significantly more than the AI's own production
/// chains will consume. The floor per resource is:
///
///   floor = max(trade_resource_reserve, projected_per_turn × buffer_turns) + safety
///
/// `projected_per_turn` is computed from `chain_targets` (same projection used
/// by the buy-side need-based bids in card [3/6]). Resources with active
/// downstream chains stay in the warehouse; resources the AI has no use for
/// (e.g. Gold/Gems, surplus Horses with no cavalry recruiting) still sell down
/// to the static reserve.
#[allow(unused_variables)] // personality used only with cfg(feature = "lua")
pub(crate) fn ai_trade(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);

    // ── Read Lua config (feature-gated) ──────────────────────
    let lua_cfg = super::lua_bridge::get_personality_config(game, personality);

    let trade_treasury_cap: i64 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.trade_treasury_cap) {
            break 'val v;
        }
        20_000
    };
    let trade_resource_reserve: u32 = 'val: {
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.trade_resource_reserve) {
            break 'val v;
        }
        10
    };
    // Same buffer used for buy-side bids — keep N turns of input on hand
    // before considering anything "excess".
    let buffer_turns = trade_buy_buffer_turns(game, personality);

    let current_turn = game.turn;
    let mut total_revenue = Money::ZERO;
    // Hold back coal queued for the next merchant hull — Paddlewheeler
    // and Freighter both list coal_cost > 0, so unconditional auto-sell
    // would starve their construction even with the reserve elsewhere.
    let (_, _, _, m_coal_reserve) = super::naval::merchant_navy_material_reserve(game, nation_id);
    {
        let nation = match game.get_nation_mut(nation_id) {
            Some(n) => n,
            None => return,
        };

        // Don't sell resources when already sitting on a large treasury —
        // keep the materials for building ships, units, and infrastructure instead.
        if nation.economy.treasury > Money::dollars(trade_treasury_cap) {
            return;
        }

        // Per-turn consumption from chain targets (LumberMill→2 Timber,
        // SteelMill→Coal+Iron, Textile→Cotton/Wool, Cannery→Grain/Fruit/Fish/Livestock).
        let needs = trade::projected_resource_needs(nation);

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
            // Per-resource floor: the larger of the static reserve, the
            // need-based buffer, and (for coal) the merchant-navy reserve.
            // Resources with no active chain (Horses, Oil, …) fall back to
            // the static reserve; chain inputs hold back the projected
            // buffer × turns.
            let need_floor = needs
                .get(&resource)
                .copied()
                .unwrap_or(0)
                .saturating_mul(buffer_turns);
            let merchant_floor = if resource == ResourceType::Coal {
                m_coal_reserve
            } else {
                0
            };
            let floor = trade_resource_reserve.max(need_floor).max(merchant_floor);
            if amount > floor {
                let excess = amount - floor;
                let price = trade::base_price(resource);
                if price != Money::ZERO {
                    let revenue = price * excess as i64;
                    nation.remove_resource(resource, excess);
                    nation.economy.treasury += revenue;
                    total_revenue += revenue;
                    // Record in trade history: AI GP sold excess resource to world market
                    nation
                        .archives
                        .trade_history
                        .push(trade::TradeHistoryEntry {
                            turn: current_turn,
                            partner: NationId(0), // world-market sentinel
                            resource,
                            commodity_label: format!("{resource:?}"),
                            quantity: excess,
                            total_cost: revenue,
                            bought: false,
                        });
                }
            }
        }
    }
    if total_revenue > Money::ZERO {
        game.transient
            .pending_ai_cash_income
            .push((nation_id, total_revenue));
    }
}

/// Build freight cars if the nation has none and has the required materials.
///
/// Cost per freight car: 1 lumber + 1 steel (labor requirement simplified away).
/// Builds 2 freight cars if possible.
fn ai_build_transport(game: &mut GameState, nation_id: NationId) {
    // Hold back the merchant-navy reserve so the next merchant hull isn't
    // starved by freight-car construction. Compute before the mut borrow.
    let (_, m_lumber_reserve, m_steel_reserve, _) =
        super::naval::merchant_navy_material_reserve(game, nation_id);

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Build freight cars if we have fewer than needed (scale with province count)
    let target_cars = (nation.province_count() as u32).max(2);
    if nation.military.transport.freight_cars >= target_cars {
        return;
    }

    // Build up to 2 freight cars per turn (cost: 1 lumber + 1 steel each).
    let cars_to_build = (target_cars - nation.military.transport.freight_cars).min(2);
    let lumber_available = nation
        .material_amount(MaterialType::Lumber)
        .saturating_sub(m_lumber_reserve);
    let steel_available = nation
        .material_amount(MaterialType::Steel)
        .saturating_sub(m_steel_reserve);
    let affordable = cars_to_build.min(lumber_available).min(steel_available);

    if affordable > 0 {
        nation.consume_material(MaterialType::Lumber, affordable);
        nation.consume_material(MaterialType::Steel, affordable);
        nation.military.transport.build_freight_cars(affordable);
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Lumber,
            crate::economy::ledger::ResourceOut::ConstructionConsumed,
            affordable,
        ));
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Steel,
            crate::economy::ledger::ResourceOut::ConstructionConsumed,
            affordable,
        ));
    }
}

/// Per-turn cap on how many freight cars `ai_build_transport_proactive`
/// constructs once the freight network is saturated. Mirrors the build cap
/// used inside the function so the reserve and the build agree.
const FREIGHT_BUILD_CAP_PER_TURN: u32 = 2;

/// Returns `true` when the AI's combined rail + sea capacity can't cover the
/// raw-resource yields its provinces could collect this turn — i.e. when
/// expanding freight would actually unlock more deliveries.
///
/// Mirrors the gate inside `ai_build_transport_proactive`: either the
/// remote-collectable network already exceeds capacity, or last turn's
/// freight pool ran nearly saturated.
pub(crate) fn wants_more_freight_cars(game: &GameState, nation_id: NationId) -> bool {
    let Some(nation) = game.get_nation(nation_id) else {
        return false;
    };
    let capacity = nation.total_transport_capacity(&game.game_data);
    let freight_unused = nation.economy.logistics.freight_unused;
    let (_local, remote_items) = crate::economy::current_collectable_resources(game, nation_id);
    let remote_total: u32 = remote_items.iter().map(|(_, qty)| *qty).sum();
    // Same gate as `ai_build_transport_proactive` (negated): we *want* to
    // build when remote yield exceeds capacity OR the freight pool was nearly
    // saturated last turn.
    !(remote_total <= capacity && (capacity == 0 || freight_unused > 2))
}

/// Lumber + steel the AI holds back from other consumers (factory chain
/// projection, warship arms-conversion, scored-spending arms production) so
/// the next-turn freight-car build has the materials it needs. Returns
/// `(lumber, steel)`.
///
/// Returns `(0, 0)` when `wants_more_freight_cars` is false — the reserve
/// only kicks in when remote freight is actually saturated. Otherwise
/// returns enough for `FREIGHT_BUILD_CAP_PER_TURN` cars (1 lumber + 1 steel
/// each).
///
/// Mirrors `reserve_for_expansion` and `merchant_navy_material_reserve`: a
/// small, predictable hold-back that keeps materials around long enough for
/// the next-turn freight build to succeed instead of being drained by
/// hardware-factory + arms + warship consumers earlier in the AI loop.
pub(crate) fn freight_expansion_material_reserve(
    game: &GameState,
    nation_id: NationId,
) -> (u32, u32) {
    if !wants_more_freight_cars(game, nation_id) {
        return (0, 0);
    }
    // Each freight car costs 1 lumber + 1 steel.
    let (_, lumber_per, steel_per) = crate::economy::TransportSystem::build_freight_car_cost();
    (
        FREIGHT_BUILD_CAP_PER_TURN.saturating_mul(lumber_per),
        FREIGHT_BUILD_CAP_PER_TURN.saturating_mul(steel_per),
    )
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

    // Use combined rail + sea capacity so the gate matches what
    // `resolve_transport` actually delivers (Trello bug #461). Without this,
    // an AI nation with a strong merchant marine but little rail would still
    // get pushed to overbuild rail cars.
    let capacity = nation.total_transport_capacity(&game.game_data);
    let freight_unused = nation.economy.logistics.freight_unused;
    let (_local_items, remote_items) =
        crate::economy::current_collectable_resources(game, nation_id);
    let remote_total: u32 = remote_items.iter().map(|(_, qty)| *qty).sum();

    // Build when the remote network is already beyond combined capacity, or
    // when last turn's freight pool was nearly saturated.
    if remote_total <= capacity && (capacity == 0 || freight_unused > 2) {
        return;
    }

    // Build additional freight cars (1 lumber + 1 steel each, up to 2 per turn).
    // Hold back the merchant-navy reserve so the next merchant hull isn't
    // starved by freight-car construction.
    let (_, m_lumber_reserve, m_steel_reserve, _) =
        super::naval::merchant_navy_material_reserve(game, nation_id);
    let cars_to_build = FREIGHT_BUILD_CAP_PER_TURN;
    let lumber_available = nation
        .material_amount(MaterialType::Lumber)
        .saturating_sub(m_lumber_reserve);
    let steel_available = nation
        .material_amount(MaterialType::Steel)
        .saturating_sub(m_steel_reserve);
    let affordable = cars_to_build.min(lumber_available).min(steel_available);

    if affordable > 0 {
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return;
        };
        nation.consume_material(MaterialType::Lumber, affordable);
        nation.consume_material(MaterialType::Steel, affordable);
        nation.military.transport.build_freight_cars(affordable);
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Lumber,
            crate::economy::ledger::ResourceOut::ConstructionConsumed,
            affordable,
        ));
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Steel,
            crate::economy::ledger::ResourceOut::ConstructionConsumed,
            affordable,
        ));
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
        *ai.economy
            .materials
            .entry(MaterialType::Lumber)
            .or_insert(0) = 3;
        *ai.economy.materials.entry(MaterialType::Steel).or_insert(0) = 3;

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
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        // Give materials for factory construction
        *ai.economy
            .materials
            .entry(MaterialType::Lumber)
            .or_insert(0) = 2;
        *ai.economy.materials.entry(MaterialType::Steel).or_insert(0) = 2;

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
        ai.economy.treasury = Money::dollars(1000);
        // Give AI 100 timber — well above any need-aware floor.
        ai.add_resource(ResourceType::Timber, 100);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Card #463: floor = max(static_reserve=10, per_turn × buffer_turns).
        // Bootstrap creates a LumberMill capacity 2 ⇒ per_turn=4, buffer=3 ⇒
        // need_floor = 12 ⇒ effective floor = 12.
        // Timber after AI turn must be at most the effective floor.
        let timber = ai.resource_amount(ResourceType::Timber);
        assert!(
            timber <= 12,
            "AI should sell timber down to the need-aware floor of 12, got {timber}"
        );
        assert!(
            ai.economy.treasury > Money::dollars(1000),
            "Treasury should increase from selling timber"
        );
    }

    #[test]
    fn ai_does_not_sell_resources_below_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(1000);
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
        ai.economy.treasury = Money::dollars(0);
        // Card #463: floors are need-aware. With bootstrap LumberMill cap 2
        // and SteelMill cap 2 + buffer_turns=3 (Balanced):
        //   timber floor = max(10, 4×3) = 12
        //   coal floor   = max(10, 2×3) = 10
        // Stock 100 each so the surplus is unambiguous.
        ai.add_resource(ResourceType::Timber, 100);
        ai.add_resource(ResourceType::Coal, 100);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(ai.resource_amount(ResourceType::Timber) <= 12);
        assert!(ai.resource_amount(ResourceType::Coal) <= 10);
        assert!(
            ai.economy.treasury > Money::dollars(0),
            "Treasury should increase from selling surplus"
        );
    }

    #[test]
    fn ai_keeps_chain_inputs_above_need_floor_card_463() {
        // Card #463: AI must NOT sell raw resources its own production lines
        // are about to consume. Floor = projected_per_turn × buffer_turns.
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(1000);
        // Force a SteelMill at high capacity so coal+iron are heavily needed.
        ai.economy
            .buildings
            .push(crate::economy::buildings::Building::new(
                crate::economy::buildings::BuildingType::SteelMill,
                5,
            ));
        ai.economy.chain_targets.metal_mill = 5; // explicit target ⇒ 5 coal+iron/turn
        // Give exactly the buffer × demand (5 × 3 = 15) — no surplus to sell.
        ai.add_resource(ResourceType::Coal, 15);
        ai.add_resource(ResourceType::Iron, 15);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // ai_trade must have left the warehouse intact (the in-turn mill run
        // may have consumed some, but the auto-sell path itself can't drop
        // below the 15-unit floor, so any drop is from production).
        assert!(
            ai.resource_amount(ResourceType::Coal) >= 10,
            "AI should not auto-sell coal needed by its own steel mill, got {}",
            ai.resource_amount(ResourceType::Coal)
        );
    }

    #[test]
    fn ai_sells_tradeable_grain_when_in_surplus() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(0);
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
            ai.economy.treasury > Money::ZERO,
            "AI should have earned money from selling grain"
        );
    }

    #[test]
    fn ai_builds_freight_cars_when_it_has_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give all buildings so infrastructure doesn't consume materials
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::FurnitureFactory, 1));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::HardwareFactory, 1));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::ClothingFactory, 1));
        // Give enough materials for both potential mill expansion and freight cars
        ai.add_material(MaterialType::Lumber, 20);
        ai.add_material(MaterialType::Steel, 20);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.military.transport.freight_cars >= 2,
            "AI should build at least 2 freight cars, got {}",
            ai.military.transport.freight_cars
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
            ai.military.transport.freight_cars, 0,
            "AI should not build freight cars without materials"
        );
    }

    #[test]
    fn ai_scales_freight_cars_with_provinces() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.military.transport.build_freight_cars(1); // start with 1 car
        // Give plenty of materials (some may be consumed by economy/infra building)
        ai.add_material(MaterialType::Lumber, 20);
        ai.add_material(MaterialType::Steel, 20);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // With 1 province, target = max(1*2, 5) = 5, so AI builds more
        // (up to 2 per turn, from 1 → 3)
        assert!(
            ai.military.transport.freight_cars > 1,
            "AI should build more freight cars to meet target (has {})",
            ai.military.transport.freight_cars
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
        if game.world.hex_map.get_tile(cap_tile).is_none() {
            // Still verify the function doesn't panic on missing tiles
            ai_build_map_infrastructure(&mut game, ai_id);
            return;
        }

        assert!(
            !game
                .world
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
            game.world
                .hex_map
                .get_tile(cap_tile)
                .unwrap()
                .infrastructure
                .has_depot,
            "AI should build depot on capital tile"
        );

        // Treasury should have decreased by $2,000
        let ai = game.get_nation(ai_id).unwrap();
        assert_eq!(ai.economy.treasury, Money::dollars(8000));
    }

    #[test]
    fn ai_sells_excess_goods_when_treasury_low() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Set treasury below $3,000 threshold
        game.get_nation_mut(ai_id).unwrap().economy.treasury = Money::dollars(1000);

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
            ai.economy.treasury,
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
        game.get_nation_mut(ai_id).unwrap().economy.treasury = Money::dollars(5000);

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
    fn ai_dumps_fat_stockpile_even_when_treasury_high() {
        // Trello fat-stockpile dump: even with treasury well above the
        // emergency threshold, goods piled up far beyond
        // `goods_fat_stockpile_threshold` get auto-sold to the world market.
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy.treasury = Money::dollars(50_000); // way above sell threshold
        ai.add_goods(GoodsType::Furniture, 100); // far above default 30 threshold
        ai.add_goods(GoodsType::Hardware, 5); // below threshold — must stay

        let mut actions = Vec::new();
        ai_manage_resources(&mut game, ai_id, &mut actions);

        let ai = game.get_nation(ai_id).unwrap();
        // Fat-stockpile branch drains down to the threshold (30 by default),
        // not all the way to the static reserve.
        assert!(
            ai.goods_amount(GoodsType::Furniture) <= 30,
            "fat-stockpile dump must drain Furniture to threshold, got {}",
            ai.goods_amount(GoodsType::Furniture)
        );
        assert_eq!(
            ai.goods_amount(GoodsType::Hardware),
            5,
            "Hardware below threshold must not be touched"
        );
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
            ai.military.transport.freight_cars >= 2,
            "AI should build freight cars proactively, got {}",
            ai.military.transport.freight_cars
        );
    }

    // ── Trade-aware demand (card #132) ─────────────────────────

    fn prime_timber_deficit(game: &mut GameState, nation_id: NationId) {
        // Give the nation a LumberMill so compute_resource_demand registers
        // a real Timber deficit (effective_capacity * 2, minus any Timber
        // already in warehouse).
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        // Clear any starting timber so the deficit is clean.
        nation.economy.warehouse.insert(ResourceType::Timber, 0);
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
            .archives
            .trade_history
            .push(crate::economy::trade::TradeHistoryEntry {
                turn: TurnNumber::new(1),
                partner: NationId(1),
                resource: ResourceType::Timber,
                commodity_label: "Timber".to_string(),
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
            .archives
            .trade_history
            .push(crate::economy::trade::TradeHistoryEntry {
                turn: TurnNumber::new(1),
                partner: NationId(1),
                resource: ResourceType::Timber,
                commodity_label: "Timber".to_string(),
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
            .archives
            .trade_history
            .push(crate::economy::trade::TradeHistoryEntry {
                turn: TurnNumber::new(1), // 49 turns stale
                partner: NationId(1),
                resource: ResourceType::Timber,
                commodity_label: "Timber".to_string(),
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
            .economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        nation.economy.treasury = Money::dollars(20_000);

        crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "t".to_string(),
        hex_map: hex_map,
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
        next_unit_id: 6_000_000,}
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
            .diplomacy
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
        if let Some(t) = game.world.hex_map.get_tile_mut(a) {
            t.infrastructure.has_depot = true;
        }
        game.get_nation_mut(NationId(1))
            .unwrap()
            .diplomacy
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
            .diplomacy
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
            .economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        nation.economy.treasury = Money::dollars(20_000);

        let game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "t".to_string(),
        hex_map: hex_map,
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
        next_unit_id: 6_000_000,};

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
            if let Some(t) = game.world.hex_map.get_tile_mut(h) {
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
        game.world.hex_map.set_tile(swamp_coord, swamp_tile);

        // Install commitment to the candidate.
        game.get_nation_mut(NationId(1))
            .unwrap()
            .diplomacy
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
            .diplomacy
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

    #[test]
    fn plan_improvability_weight_increases_coverage() {
        // Card #217: depot planner must factor in tech-capped improvability,
        // not just current yield. With Seed Drill researched, a Grain L0 tile
        // has +1 max-level headroom → coverage_value rises with the
        // `infra_improvability_weight` knob.
        let capital = HexCoord::new(0, 0);
        let candidate = HexCoord::new(2, 0);
        let intermediate = HexCoord::new(1, 0);
        // Place an extra Grain L0 tile inside the candidate's 1-hex radius
        // (3,0) so the tile contributes to coverage_around.
        let extra = HexCoord::new(3, 0);

        let mut game = planner_game(
            capital,
            &[
                (intermediate, ResourceType::Grain),
                (candidate, ResourceType::Grain),
                (extra, ResourceType::Grain),
            ],
        );
        // Seed Drill so Farm is improvable to L1.
        game.get_nation_mut(NationId(1))
            .unwrap()
            .researched_techs
            .push(crate::events::TechId(2));

        // Snapshot coverage with weight = 0 (legacy behavior).
        game.game_data.game_config.infra_improvability_weight = 0.0;
        let cov_off = plan_next_depot(&game, NationId(1))
            .as_plan()
            .expect("plan should exist")
            .coverage_value;

        // Same scenario with improvability weight > 0 must yield strictly more.
        game.game_data.game_config.infra_improvability_weight = 1.0;
        let cov_on = plan_next_depot(&game, NationId(1))
            .as_plan()
            .expect("plan should exist")
            .coverage_value;

        assert!(
            cov_on > cov_off,
            "improvability weight must lift coverage_value (off={}, on={})",
            cov_off,
            cov_on,
        );
    }

    #[test]
    fn plan_picks_more_improvable_candidate_at_equal_path_cost() {
        // Card #217: with two candidates equidistant from the capital and
        // identical path cost, the planner should prefer the one whose
        // 1-hex radius covers tiles with more improvability headroom once
        // `infra_improvability_weight` is on. With the weight at 0 (legacy
        // behavior), the higher *current* yield wins.
        //
        // Setup (Grassland everywhere, every tile path-cost == $100):
        //   - capital at (0,0): country-capital Grain L0 with depot.
        //   - cand_more_imp at (3,0): empty grassland, neighbour (4,0)
        //     carries Grain at improvement_level 0  → delta=2 with Farm L2 tech.
        //   - cand_more_curr at (-3,0): empty grassland, neighbour (-4,0)
        //     carries Grain at improvement_level 1 → current yield=2, delta=1.
        //
        // Yield (surface resource = 1 + level):
        //   - (4,0) at L0: current yield 1, improvability +2W.
        //   - (-4,0) at L1: current yield 2, improvability +1W.
        //
        // With W=0: more_curr beats more_imp (2 vs 1) → planner picks (-3,0).
        // With W=2: more_imp 1+4=5 beats more_curr 2+2=4 → planner picks (3,0).
        let capital = HexCoord::new(0, 0);
        let cand_more_imp = HexCoord::new(3, 0);
        let cand_more_curr = HexCoord::new(-3, 0);
        let imp_neighbour = HexCoord::new(4, 0);
        let curr_neighbour = HexCoord::new(-4, 0);

        // Path filler so Dijkstra can reach both candidates.
        let mut game = planner_game(
            capital,
            &[
                (HexCoord::new(1, 0), ResourceType::Grain),
                (HexCoord::new(2, 0), ResourceType::Grain),
                (cand_more_imp, ResourceType::Grain),
                (imp_neighbour, ResourceType::Grain),
                (HexCoord::new(-1, 0), ResourceType::Grain),
                (HexCoord::new(-2, 0), ResourceType::Grain),
                (cand_more_curr, ResourceType::Grain),
                (curr_neighbour, ResourceType::Grain),
            ],
        );

        // Lift `curr_neighbour` to improvement_level 1.
        if let Some(t) = game.world.hex_map.get_tile_mut(curr_neighbour) {
            t.set_improvement_level(1);
        }

        // Research Seed Drill (Farm L1) + Steel and Iron Plows (Farm L2) so
        // both neighbours are still improvable: imp_neighbour 0→2,
        // curr_neighbour 1→2.
        let nat = game.get_nation_mut(NationId(1)).unwrap();
        nat.researched_techs.push(crate::events::TechId(2));
        nat.researched_techs.push(crate::events::TechId(10));
        // Clear the LumberMill that planner_game seeds — we want Grain to
        // dominate the demand profile so each Grain tile contributes a
        // demand_weight ~1.0 with no Timber bias.
        nat.economy.buildings.clear();

        // Legacy behaviour (weight = 0): the higher-yield neighbour side wins.
        game.game_data.game_config.infra_improvability_weight = 0.0;
        let chosen_legacy = plan_next_depot(&game, NationId(1))
            .as_plan()
            .expect("planner must pick a target")
            .candidate;
        assert_eq!(
            chosen_legacy, cand_more_curr,
            "with improvability_weight=0, current yield wins → expected cand_more_curr"
        );

        // Improvability-weighted: improvable side wins despite lower current yield.
        game.game_data.game_config.infra_improvability_weight = 2.0;
        let chosen_imp = plan_next_depot(&game, NationId(1))
            .as_plan()
            .expect("planner must pick a target")
            .candidate;
        assert_eq!(
            chosen_imp, cand_more_imp,
            "with improvability_weight=2.0, improvable side must win at equal path cost"
        );
    }

    #[test]
    fn expand_building_idempotent_within_turn() {
        // F-010 regression: expand_building must not charge materials twice when
        // called for the same building in the same turn (guards stale-snapshot risk).
        let mut game = test_game_with_ai();
        let nation_id = NationId(2);

        // Give nation a LumberMill and ample materials for multiple expansions.
        game.get_nation_mut(nation_id)
            .unwrap()
            .economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 4));
        *game
            .get_nation_mut(nation_id)
            .unwrap()
            .economy
            .materials
            .entry(MaterialType::Lumber)
            .or_insert(0) = 20;
        *game
            .get_nation_mut(nation_id)
            .unwrap()
            .economy
            .materials
            .entry(MaterialType::Steel)
            .or_insert(0) = 20;

        // Call expand_building twice — second call must be a no-op.
        expand_building(&mut game, nation_id, BuildingType::LumberMill, true);
        let lumber_after_first = game
            .get_nation(nation_id)
            .unwrap()
            .material_amount(MaterialType::Lumber);
        let steel_after_first = game
            .get_nation(nation_id)
            .unwrap()
            .material_amount(MaterialType::Steel);

        expand_building(&mut game, nation_id, BuildingType::LumberMill, true);
        let lumber_after_second = game
            .get_nation(nation_id)
            .unwrap()
            .material_amount(MaterialType::Lumber);
        let steel_after_second = game
            .get_nation(nation_id)
            .unwrap()
            .material_amount(MaterialType::Steel);

        // Materials must not have been charged a second time.
        assert_eq!(
            lumber_after_first, lumber_after_second,
            "second expand_building call should not charge lumber again"
        );
        assert_eq!(
            steel_after_first, steel_after_second,
            "second expand_building call should not charge steel again"
        );

        // Exactly one pending expansion in the building.
        let nation = game.get_nation(nation_id).unwrap();
        let mill = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .unwrap();
        assert!(
            mill.pending_capacity > 0,
            "building should have one pending expansion"
        );
    }

    // ── Card [2/6]: production target setter tests ────────────────────

    #[test]
    fn ai_set_production_targets_uses_full_mill_capacity_with_supply() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        // Bootstrap a steel mill at cap=4 with abundant coal+iron
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 4));
        ai.add_resource(ResourceType::Coal, 10);
        ai.add_resource(ResourceType::Iron, 10);

        ai_set_production_targets(&mut game, ai_id);

        let ai = game.get_nation(ai_id).unwrap();
        assert_eq!(
            ai.economy.chain_targets.metal_mill, 4,
            "metal_mill target should equal cap when coal+iron supply is plentiful"
        );
    }

    #[test]
    fn ai_set_production_targets_drops_chain_when_no_input() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        // Steel mill exists but no coal/iron in warehouse.
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 4));
        // ensure clean slate
        ai.economy.warehouse.remove(&ResourceType::Coal);
        ai.economy.warehouse.remove(&ResourceType::Iron);

        ai_set_production_targets(&mut game, ai_id);

        let ai = game.get_nation(ai_id).unwrap();
        assert!(
            ai.economy.chain_targets.metal_mill < 4,
            "metal_mill target should drop below cap when coal/iron are missing — got {}",
            ai.economy.chain_targets.metal_mill
        );
    }

    #[test]
    fn ai_set_production_targets_zero_for_missing_buildings() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        // Make sure the AI has no buildings of the relevant types.
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy.buildings.retain(|b| {
            !matches!(
                b.building_type,
                BuildingType::SteelMill
                    | BuildingType::LumberMill
                    | BuildingType::TextileMill
                    | BuildingType::PaperFactory
                    | BuildingType::Armory
                    | BuildingType::FurnitureFactory
                    | BuildingType::HardwareFactory
                    | BuildingType::ClothingFactory
                    | BuildingType::FoodProcessing
            )
        });

        ai_set_production_targets(&mut game, ai_id);

        let ai = game.get_nation(ai_id).unwrap();
        let t = &ai.economy.chain_targets;
        assert_eq!(t.metal_mill, 0);
        assert_eq!(t.timber_mill, 0);
        assert_eq!(t.textile_mill, 0);
        assert_eq!(t.armory, 0);
        assert_eq!(t.paper_factory, 0);
        assert_eq!(t.canned_food_factory, 0);
    }

    #[test]
    fn ai_set_production_targets_war_shifts_steel_to_armory() {
        // Two AI games on the same map; one at war, one not. The wartime
        // AI should set a higher armory target relative to hardware.
        use crate::diplomacy::DiplomacyState;

        let setup = |at_war: bool| {
            let mut game = test_game_with_ai();
            let ai_id = NationId(2);
            let ai = game.get_nation_mut(ai_id).unwrap();
            // Provide both factories at equal capacity and abundant steel.
            ai.economy
                .buildings
                .push(Building::new(BuildingType::HardwareFactory, 50));
            ai.economy
                .buildings
                .push(Building::new(BuildingType::Armory, 50));
            ai.add_material(MaterialType::Steel, 20);
            if at_war {
                game.world.diplomacy = DiplomacyState::new();
                game.world
                    .diplomacy
                    .initialize_great_powers(&[NationId(1), NationId(2)]);
                game.world.diplomacy.declare_war(NationId(2), NationId(1));
            }
            ai_set_production_targets(&mut game, ai_id);
            let ai = game.get_nation(ai_id).unwrap();
            (
                ai.economy.chain_targets.steel_factory,
                ai.economy.chain_targets.armory,
            )
        };

        let (peace_hardware, peace_armory) = setup(false);
        let (war_hardware, war_armory) = setup(true);

        assert!(
            war_armory > peace_armory,
            "war should raise armory target (war={}, peace={})",
            war_armory,
            peace_armory
        );
        assert!(
            peace_hardware >= war_hardware,
            "peace should keep hardware target at least as high as war (peace={}, war={})",
            peace_hardware,
            war_hardware
        );
    }

    #[test]
    fn ai_set_production_targets_metal_mill_clamped_by_partial_input() {
        // Coal/iron at 1 each — the steel mill can only run 1 unit even though
        // capacity is 10. Target must clamp to 1, not stay at cap.
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 10));
        ai.add_resource(ResourceType::Coal, 1);
        ai.add_resource(ResourceType::Iron, 1);

        ai_set_production_targets(&mut game, ai_id);

        let ai = game.get_nation(ai_id).unwrap();
        assert_eq!(
            ai.economy.chain_targets.metal_mill, 1,
            "metal_mill target must clamp to coal/iron bottleneck (1), not cap (10)"
        );
    }

    #[test]
    fn ai_set_production_targets_cannery_clamped_by_input_bottleneck() {
        // Cannery cap=10, demand pretends to be high (we'd need a queue but
        // we'll let it default to 1 which gets buffered up). Inputs: only
        // 2 grain + 5 fruit + 5 fish. Target should clamp to 2.
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 10));
        ai.economy.pending_immigration = 50; // big projected demand
        ai.add_resource(ResourceType::Grain, 2);
        ai.add_resource(ResourceType::Fruit, 5);
        ai.add_resource(ResourceType::Fish, 5);

        ai_set_production_targets(&mut game, ai_id);

        let ai = game.get_nation(ai_id).unwrap();
        assert_eq!(
            ai.economy.chain_targets.canned_food_factory, 2,
            "canned_food target must clamp to scarcest ingredient (grain=2)"
        );
    }

    #[test]
    fn mill_target_does_not_inflate_when_runnable_is_zero() {
        // Direct exercise of the helper to lock in the contract: even with a
        // non-default min_target, an unrunnable mill should never target above
        // its anti-oscillation floor.
        // (LuaAiConfig sanitization caps min_chain_target at 2, but defense
        // in depth: the helper itself must not multiply the floor by capacity.)
        assert_eq!(mill_target(0, 10, 2), 2);
        assert_eq!(mill_target(0, 10, 0), 0);
        assert_eq!(mill_target(3, 10, 2), 3);
        assert_eq!(mill_target(20, 10, 2), 10);
    }

    #[test]
    fn factory_target_does_not_inflate_floor_above_desired() {
        // When supply is present, factory target is min(desired, capacity).
        // The floor only applies when supply is missing.
        assert_eq!(factory_target(5, true, 10, 2), 5);
        assert_eq!(factory_target(0, true, 10, 2), 0);
        assert_eq!(factory_target(20, true, 10, 2), 10);
        assert_eq!(factory_target(5, false, 10, 2), 2); // floor wins
        assert_eq!(factory_target(5, false, 10, 0), 0);
    }

    #[test]
    fn reserve_for_expansion_scales_with_worst_step() {
        // Buildings at cap 8 each. Next tier is 12, delta=4. Worst-step
        // expansion costs (4 lumber, 4 steel). With expansions_per_turn=2
        // we reserve (8, 8).
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 8));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 8));

        let (lumber, steel) = reserve_for_expansion(&game, ai_id, 2, 0.0);
        assert_eq!(lumber, 8, "delta=4 × 2 expansions = 8 lumber");
        assert_eq!(steel, 8, "delta=4 × 2 expansions = 8 steel");
    }

    #[test]
    fn reserve_for_expansion_scales_with_buildings_factor() {
        // 4 expandable buildings at cap=8 → next-tier delta=4. With
        // expansions_per_turn=2 and buildings_factor=0.5, effective =
        // max(2, ceil(4 × 0.5)) = max(2, 2) = 2 → reserve (8, 8). Bump
        // buildings_factor to 1.0 → effective = max(2, 4) = 4 → (16, 16).
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 8));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 8));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::TextileMill, 8));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::HardwareFactory, 8));

        let (lumber_lo, _) = reserve_for_expansion(&game, ai_id, 2, 0.5);
        let (lumber_hi, _) = reserve_for_expansion(&game, ai_id, 2, 1.0);
        let (lumber_huge, _) = reserve_for_expansion(&game, ai_id, 2, 2.0);
        assert_eq!(
            lumber_lo, 8,
            "factor 0.5 × 4 = 2 ≤ floor(2) → reserve still 8"
        );
        assert_eq!(lumber_hi, 16, "factor 1.0 × 4 = 4 → reserve 16");
        assert_eq!(lumber_huge, 32, "factor 2.0 × 4 = 8 → reserve 32");
    }

    #[test]
    fn reserve_for_expansion_scales_with_zero_buildings() {
        // No buildings at all → worst-step defaults to 1 (always reserve at
        // least one expansion's worth so a fresh AI can bootstrap).
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy.buildings.clear();

        let (lumber, steel) = reserve_for_expansion(&game, ai_id, 2, 0.0);
        assert_eq!(lumber, 2);
        assert_eq!(steel, 2);
    }

    #[test]
    fn reserve_held_back_from_factory_targets() {
        // Factory has cap=4 and we have 20 lumber + 20 steel (huge surplus).
        // Without reserve, hardware target would be ~7 (steel 20 × 0.8 / 2 = 8,
        // capped at cap=4). With reserve subtracting (8,8), steel_supply
        // becomes 12, hardware share = 9.6, hardware target = 4 (still cap).
        // The point is the reserve subtraction doesn't break high-capacity
        // factories, but does shrink targets when supply is tight.
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 8));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::HardwareFactory, 4));
        // Tight supply: 6 steel + 0 mill output projection. Reserve = 8
        // (delta=4 × 2). After reserve, steel_supply = 0 → hardware target
        // drops to min_chain_target floor.
        ai.add_material(MaterialType::Steel, 6);

        ai_set_production_targets(&mut game, ai_id);

        let ai = game.get_nation(ai_id).unwrap();
        assert!(
            ai.economy.chain_targets.steel_factory <= 1,
            "with steel<reserve, hardware target should drop to floor, got {}",
            ai.economy.chain_targets.steel_factory
        );
    }

    #[test]
    fn ai_set_production_targets_paper_gets_share_of_lumber() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::PaperFactory, 2));
        ai.economy
            .buildings
            .push(Building::new(BuildingType::FurnitureFactory, 2));
        ai.add_material(MaterialType::Lumber, 20);

        ai_set_production_targets(&mut game, ai_id);

        let ai = game.get_nation(ai_id).unwrap();
        assert!(
            ai.economy.chain_targets.paper_factory > 0,
            "paper factory should get a share of available lumber, got {}",
            ai.economy.chain_targets.paper_factory
        );
        assert!(
            ai.economy.chain_targets.lumber_factory > 0,
            "furniture factory should also get a share, got {}",
            ai.economy.chain_targets.lumber_factory
        );
    }
}
