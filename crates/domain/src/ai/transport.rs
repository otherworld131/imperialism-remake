//! AI transport allocation — assigns freight cars to actual remote demand.
//!
//! Without explicit allocations the transport system delivers nothing remote
//! (see `TransportSystem::calculate_deliveries`). Human players use the freight
//! panel; AI nations call [`ai_allocate_transport`] each turn to pick allocations
//! that cover their workers' food and the raw inputs of every active mill or
//! cannery.

use crate::economy::buildings::BuildingType;
use crate::game_state::GameState;
use crate::types::*;
use std::collections::BTreeMap;

/// Match `resolve_transport`'s difficulty bonus so AI allocation caps line up
/// with the quantities actually delivered each turn.
fn ai_difficulty_multiplier(game: &GameState, nation_id: NationId) -> f64 {
    if nation_id == game.human_player_nation {
        return 1.0;
    }
    match game.difficulty {
        Difficulty::Hard => 1.1,
        Difficulty::NighOnImpossible => 1.25,
        _ => 1.0,
    }
}

/// Resources that can drive non-trivial freight demand for an AI nation.
/// Order is significant only for floor-allocation tie-breaking.
const DEMAND_RESOURCES: [ResourceType; 9] = [
    ResourceType::Coal,
    ResourceType::Iron,
    ResourceType::Grain,
    ResourceType::Fruit,
    ResourceType::Livestock,
    ResourceType::Fish,
    ResourceType::Timber,
    ResourceType::Cotton,
    ResourceType::Wool,
];

/// Set freight allocations for an AI nation based on its workforce and active
/// production buildings.
///
/// Steps:
/// 1. Enumerate remote (freight-gated) raw-resource availability via
///    [`crate::economy::current_collectable_resources`].
/// 2. Compute per-resource demand from worker food and mill / cannery inputs.
/// 3. Distribute the nation's freight cars to demanded resources, first by
///    granting 1 car to each non-zero demand (floor) and then by filling the
///    remainder proportionally to demand.
///
/// Resources without demand or remote availability receive zero allocation.
pub fn ai_allocate_transport(game: &mut GameState, nation_id: NationId) {
    let Some(nation) = game.get_nation(nation_id) else {
        return;
    };
    let rail_capacity = nation.military.transport.freight_cars;
    let sea_capacity = nation.total_cargo_capacity(&game.game_data);
    // Combined rail + sea pool — matches what `resolve_transport` actually
    // delivers each turn (Trello bug #461). Even with zero rail, sea cargo can
    // carry remote yields (e.g. an island nation with a Trader).
    let total_capacity = rail_capacity.saturating_add(sea_capacity);
    if total_capacity == 0 {
        return;
    }

    let (_local, mut remote_items) =
        crate::economy::current_collectable_resources(game, nation_id);
    // Mirror `resolve_transport` (processor.rs:1252): AI difficulty multipliers
    // scale up remote yields. Apply the same multiplier here so demand caps and
    // delivery-time availability agree.
    let multiplier = ai_difficulty_multiplier(game, nation_id);
    if (multiplier - 1.0).abs() > f64::EPSILON {
        for (_, qty) in remote_items.iter_mut() {
            *qty = (*qty as f64 * multiplier).round() as u32;
        }
    }
    if remote_items.is_empty() {
        if let Some(n) = game.get_nation_mut(nation_id) {
            n.military.transport.allocations.clear();
        }
        return;
    }

    let nation = game.get_nation(nation_id).expect("nation present");
    let demand = compute_remote_demand(nation, &game.game_data, &remote_items);

    let allocations = distribute_freight(total_capacity, &demand);

    let nation = game.get_nation_mut(nation_id).expect("nation present");
    nation.military.transport.allocations.clear();
    for (resource, units) in allocations {
        if units > 0 {
            nation.military.transport.set_allocation(resource, units);
        }
    }
}

/// Split a single-pool demand of `total` units across two interchangeable
/// resources by their remote availability share. If neither has any remote
/// supply this turn, demand is silently dropped (we cannot transport what we
/// cannot collect).
fn apportion_pool_to_pair(
    total: u32,
    a: ResourceType,
    b: ResourceType,
    remote_items: &[(ResourceType, u32)],
    demand: &mut BTreeMap<ResourceType, u32>,
) {
    if total == 0 {
        return;
    }
    let avail_a = remote_items
        .iter()
        .find(|(r, _)| *r == a)
        .map(|(_, q)| *q)
        .unwrap_or(0);
    let avail_b = remote_items
        .iter()
        .find(|(r, _)| *r == b)
        .map(|(_, q)| *q)
        .unwrap_or(0);
    let total_avail = avail_a.saturating_add(avail_b);
    if total_avail == 0 {
        return;
    }
    let share_a = ((avail_a as u64) * (total as u64) / total_avail as u64) as u32;
    let share_b = total.saturating_sub(share_a);
    if share_a > 0 {
        *demand.entry(a).or_insert(0) += share_a;
    }
    if share_b > 0 {
        *demand.entry(b).or_insert(0) += share_b;
    }
}

/// For each resource in `DEMAND_RESOURCES` compute `min(remote_available, demand)`.
/// Returns only entries with positive value.
fn compute_remote_demand(
    nation: &crate::nation::Nation,
    game_data: &crate::data::GameData,
    remote_items: &[(ResourceType, u32)],
) -> Vec<(ResourceType, u32)> {
    let mut demand: BTreeMap<ResourceType, u32> = BTreeMap::new();

    // ── Worker food demand ──
    // Workers eat from grain → fruit → livestock → fish in priority order.
    // We project the total food need across whichever of the four foods are
    // remotely collectable, weighted by current warehouse share so the freight
    // mix tracks what the map actually yields.
    let workers = nation.economy.labor.total_workers();
    let food_per_worker = game_data.game_config.food_per_worker;
    let food_need = workers.saturating_mul(food_per_worker);
    if food_need > 0 {
        let food_types = [
            ResourceType::Grain,
            ResourceType::Fruit,
            ResourceType::Livestock,
            ResourceType::Fish,
        ];
        let remote_food: Vec<(ResourceType, u32)> = food_types
            .iter()
            .filter_map(|r| {
                remote_items
                    .iter()
                    .find(|(rr, _)| rr == r)
                    .map(|(rr, q)| (*rr, *q))
            })
            .filter(|(_, q)| *q > 0)
            .collect();
        if !remote_food.is_empty() {
            // Distribute food need proportionally to remote availability.
            let total_avail: u32 = remote_food.iter().map(|(_, q)| *q).sum();
            if total_avail > 0 {
                let mut assigned = 0u32;
                let mut last: Option<ResourceType> = None;
                for (r, avail) in &remote_food {
                    let share = ((*avail as u64) * (food_need as u64) / total_avail as u64) as u32;
                    *demand.entry(*r).or_insert(0) += share;
                    assigned = assigned.saturating_add(share);
                    last = Some(*r);
                }
                if let Some(r) = last
                    && assigned < food_need
                {
                    *demand.entry(r).or_insert(0) += food_need - assigned;
                }
            }
        }
    }

    // ── Mill / cannery raw-input demand ──
    let mut textile_fiber_demand: u32 = 0;
    let mut cannery_meat_demand: u32 = 0;
    for building in &nation.economy.buildings {
        let cap = building.effective_capacity();
        if cap == 0 {
            continue;
        }
        match building.building_type {
            BuildingType::LumberMill => {
                *demand.entry(ResourceType::Timber).or_insert(0) += cap.saturating_mul(2);
            }
            BuildingType::SteelMill => {
                *demand.entry(ResourceType::Coal).or_insert(0) += cap;
                *demand.entry(ResourceType::Iron).or_insert(0) += cap;
            }
            BuildingType::TextileMill | BuildingType::AdvancedTextileMill => {
                // Cotton + wool feed a single fiber pool (`2 × cap` total).
                // Apportion across cotton/wool by remote availability below.
                textile_fiber_demand =
                    textile_fiber_demand.saturating_add(cap.saturating_mul(2));
            }
            BuildingType::FoodProcessing => {
                // Cannery: 1 grain + 1 fruit + 1 (fish OR livestock) per unit.
                *demand.entry(ResourceType::Grain).or_insert(0) += cap;
                *demand.entry(ResourceType::Fruit).or_insert(0) += cap;
                // Meat slot is a single pool of size `cap`, split below across
                // fish/livestock by remote availability.
                cannery_meat_demand = cannery_meat_demand.saturating_add(cap);
            }
            _ => {}
        }
    }

    // Apportion textile fiber (single 2×cap pool) across cotton/wool by remote
    // availability. If only one is remote, all demand goes to it.
    apportion_pool_to_pair(
        textile_fiber_demand,
        ResourceType::Cotton,
        ResourceType::Wool,
        remote_items,
        &mut demand,
    );

    // Apportion cannery meat (single cap pool) across fish/livestock by remote
    // availability.
    apportion_pool_to_pair(
        cannery_meat_demand,
        ResourceType::Fish,
        ResourceType::Livestock,
        remote_items,
        &mut demand,
    );

    // Cap each demand by what the map actually yields remotely. There is no
    // point reserving freight for a resource we cannot collect this turn.
    let mut out: Vec<(ResourceType, u32)> = Vec::new();
    for r in DEMAND_RESOURCES {
        let want = demand.get(&r).copied().unwrap_or(0);
        if want == 0 {
            continue;
        }
        let avail = remote_items
            .iter()
            .find(|(rr, _)| *rr == r)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        let effective = want.min(avail);
        if effective > 0 {
            out.push((r, effective));
        }
    }
    out
}

/// Distribute `freight_cars` across `demand` entries. Each entry first gets a
/// 1-car floor (capacity permitting), then any remaining cars are filled by
/// proportional share rounded down, with leftover cars handed to the largest
/// remaining demand to avoid wasting capacity.
fn distribute_freight(
    freight_cars: u32,
    demand: &[(ResourceType, u32)],
) -> Vec<(ResourceType, u32)> {
    if freight_cars == 0 || demand.is_empty() {
        return Vec::new();
    }

    // Cap each entry's share at min(freight_cars, demand) — never assign more
    // cars to a resource than what we actually expect to collect.
    let mut allocations: Vec<(ResourceType, u32)> = demand.iter().map(|(r, _)| (*r, 0)).collect();
    let mut remaining = freight_cars;

    // Floor pass: 1 car per resource with positive demand, in DEMAND_RESOURCES
    // priority order.
    for (i, (_, want)) in demand.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        if *want > 0 {
            allocations[i].1 = 1;
            remaining -= 1;
        }
    }

    // Water-filling pass: in each round, distribute the remaining cars by
    // proportional share among resources that still have unmet demand. Cap each
    // grant by the resource's remaining `room`, accumulate leftovers, and
    // repeat until either no rooms remain or no progress is made.
    while remaining > 0 {
        // Live total of unmet demand among resources still under their cap.
        let mut active_demand: u64 = 0;
        for (i, (_, want)) in demand.iter().enumerate() {
            if allocations[i].1 < *want {
                active_demand += *want as u64;
            }
        }
        if active_demand == 0 {
            break;
        }

        let mut progress = false;
        let initial_remaining = remaining;
        for (i, (_, want)) in demand.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            let already = allocations[i].1;
            let room = want.saturating_sub(already);
            if room == 0 {
                continue;
            }
            let share =
                ((*want as u64 * initial_remaining as u64) / active_demand).max(1) as u32;
            let extra = share.min(room).min(remaining);
            if extra > 0 {
                allocations[i].1 = already + extra;
                remaining -= extra;
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }

    allocations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::buildings::Building;
    use crate::nation::{Nation, NationColor};

    fn make_test_nation() -> Nation {
        Nation::new(
            NationId(1),
            "Test".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        )
    }

    #[test]
    fn demand_steel_mill_drives_coal_and_iron() {
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 4));
        let remote = vec![
            (ResourceType::Coal, 10),
            (ResourceType::Iron, 10),
            (ResourceType::Timber, 10),
        ];
        let demand = compute_remote_demand(&nation, &data, &remote);
        let coal = demand
            .iter()
            .find(|(r, _)| *r == ResourceType::Coal)
            .map(|(_, q)| *q);
        let iron = demand
            .iter()
            .find(|(r, _)| *r == ResourceType::Iron)
            .map(|(_, q)| *q);
        assert!(coal.is_some_and(|q| q > 0), "steel mill should demand coal");
        assert!(iron.is_some_and(|q| q > 0), "steel mill should demand iron");
    }

    #[test]
    fn demand_cannery_meat_is_single_pool() {
        // Cannery cap=4 needs 4 meat, split between fish/livestock by remote
        // availability. With fish=10 remote and livestock=0 remote, all 4 meat
        // demand should land on fish (and total fish+livestock demand = 4, not 8).
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 4));
        let remote = vec![
            (ResourceType::Grain, 10),
            (ResourceType::Fruit, 10),
            (ResourceType::Fish, 10),
            // No livestock available remotely.
        ];
        let demand = compute_remote_demand(&nation, &data, &remote);
        let fish = demand
            .iter()
            .find(|(r, _)| *r == ResourceType::Fish)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        let livestock = demand
            .iter()
            .find(|(r, _)| *r == ResourceType::Livestock)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        assert_eq!(fish + livestock, 4, "meat is a single 4-unit pool, not 8");
        assert_eq!(livestock, 0, "no livestock remote → all meat to fish");
    }

    #[test]
    fn demand_textile_fiber_is_single_pool() {
        // Textile mill cap=3 → 6 fiber units split across cotton/wool.
        // With cotton=4 remote and wool=4 remote, expect ~3 each (not 6+3=9).
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::TextileMill, 3));
        let remote = vec![
            (ResourceType::Cotton, 4),
            (ResourceType::Wool, 4),
        ];
        let demand = compute_remote_demand(&nation, &data, &remote);
        let cotton = demand
            .iter()
            .find(|(r, _)| *r == ResourceType::Cotton)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        let wool = demand
            .iter()
            .find(|(r, _)| *r == ResourceType::Wool)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        assert_eq!(cotton + wool, 6, "fiber is a single 6-unit pool, not 9");
    }

    #[test]
    fn demand_cannery_drives_food_inputs() {
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 3));
        let remote = vec![
            (ResourceType::Grain, 10),
            (ResourceType::Fruit, 10),
            (ResourceType::Fish, 10),
        ];
        let demand = compute_remote_demand(&nation, &data, &remote);
        for r in [ResourceType::Grain, ResourceType::Fruit, ResourceType::Fish] {
            assert!(
                demand.iter().any(|(rr, q)| *rr == r && *q > 0),
                "cannery should demand {:?}",
                r
            );
        }
    }

    #[test]
    fn demand_capped_by_remote_availability() {
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        // Capacity-4 lumber mill wants 8 timber, but only 3 are collectable.
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 4));
        let remote = vec![(ResourceType::Timber, 3)];
        let demand = compute_remote_demand(&nation, &data, &remote);
        let timber = demand
            .iter()
            .find(|(r, _)| *r == ResourceType::Timber)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        assert_eq!(timber, 3, "demand must be capped by remote availability");
    }

    #[test]
    fn demand_empty_when_no_buildings_no_workers() {
        let nation = make_test_nation();
        let data = crate::data::GameData::default();
        let remote = vec![(ResourceType::Coal, 5)];
        let demand = compute_remote_demand(&nation, &data, &remote);
        assert!(
            demand.is_empty(),
            "no production buildings + no workers → no demand"
        );
    }

    #[test]
    fn floor_distributes_when_freight_below_demand_count() {
        // 3 demand resources but only 2 freight slots — floor pass should
        // allocate 1 each to two of them and 0 to the third.
        let out = distribute_freight(
            2,
            &[
                (ResourceType::Coal, 10),
                (ResourceType::Iron, 10),
                (ResourceType::Grain, 10),
            ],
        );
        let total: u32 = out.iter().map(|(_, u)| *u).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn empty_demand_no_allocations() {
        let out = distribute_freight(10, &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn zero_freight_no_allocations() {
        let out = distribute_freight(
            0,
            &[(ResourceType::Coal, 5), (ResourceType::Iron, 3)],
        );
        assert!(out.iter().all(|(_, u)| *u == 0));
    }

    #[test]
    fn floor_one_per_resource() {
        // 5 freight cars, 2 resources with equal demand → at least 1 each.
        let out = distribute_freight(
            5,
            &[(ResourceType::Coal, 10), (ResourceType::Iron, 10)],
        );
        let coal = out.iter().find(|(r, _)| *r == ResourceType::Coal).unwrap().1;
        let iron = out.iter().find(|(r, _)| *r == ResourceType::Iron).unwrap().1;
        assert!(coal >= 1);
        assert!(iron >= 1);
        assert_eq!(coal + iron, 5);
    }

    #[test]
    fn allocation_capped_by_demand() {
        // Demand only 2 of coal but 10 freight cars available. Coal stops at 2.
        let out = distribute_freight(
            10,
            &[(ResourceType::Coal, 2), (ResourceType::Iron, 100)],
        );
        let coal = out.iter().find(|(r, _)| *r == ResourceType::Coal).unwrap().1;
        let iron = out.iter().find(|(r, _)| *r == ResourceType::Iron).unwrap().1;
        assert_eq!(coal, 2);
        assert_eq!(iron, 8);
    }

    #[test]
    fn allocation_total_does_not_exceed_freight() {
        let out = distribute_freight(
            7,
            &[
                (ResourceType::Coal, 5),
                (ResourceType::Iron, 5),
                (ResourceType::Grain, 5),
            ],
        );
        let total: u32 = out.iter().map(|(_, u)| *u).sum();
        assert!(total <= 7);
    }
}
