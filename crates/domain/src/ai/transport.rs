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

/// Strict freight priority tiers (lower number = filled first).
///
/// Tier 1: Food (Grain, Fruit, Livestock, Fish) — workers must eat or chains
///         halt; this tier is filled up to the nation's per-turn food need.
/// Tier 2: Iron + Coal (and Steel, once materials become freight-routable) —
///         the steel-mill chain feeds the entire industrial base.
/// Tier 3: Timber (and Lumber, once materials become freight-routable) —
///         feeds furniture/paper for goods + tech.
/// Tier 4: Everything else (Cotton, Wool, Horses, Oil, Gold, Gems).
///
/// Within a tier the allocator fills each resource up to its computed demand
/// (capped by remote availability). Lower tiers receive zero capacity until
/// higher tiers are fully satisfied.
fn freight_priority_tier(r: ResourceType) -> u8 {
    match r {
        ResourceType::Grain
        | ResourceType::Fruit
        | ResourceType::Livestock
        | ResourceType::Fish => 1,
        ResourceType::Iron | ResourceType::Coal => 2,
        ResourceType::Timber => 3,
        // Cotton, Wool, Horses, Oil, Gold, Gems → tier 4.
        _ => 4,
    }
}

/// Resources that can drive non-trivial freight demand for an AI nation,
/// listed in priority order (matches `freight_priority_tier`). Materials
/// (Steel/Lumber) are not freight-routable yet; when that changes, slot Steel
/// into tier 2 and Lumber into tier 3 here AND in `freight_priority_tier`.
const DEMAND_RESOURCES: [ResourceType; 9] = [
    // Tier 1: food.
    ResourceType::Grain,
    ResourceType::Fruit,
    ResourceType::Livestock,
    ResourceType::Fish,
    // Tier 2: industrial cores (steel-mill inputs).
    ResourceType::Iron,
    ResourceType::Coal,
    // Tier 3: timber chain.
    ResourceType::Timber,
    // Tier 4: everything else.
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
    // Combined rail + sea pool — matches what `resolve_transport` actually
    // delivers each turn (Trello bug #461). Trade doesn't deplete sea cargo,
    // so the full merchant-marine cargo is available for remote yields too.
    let total_capacity = nation.total_transport_capacity(&game.game_data);
    if total_capacity == 0 {
        return;
    }

    let (_local, mut remote_items) = crate::economy::current_collectable_resources(game, nation_id);
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

/// Per-resource freight demand split into a critical part (mandatory: worker
/// food need + active mill/cannery inputs) and a slack part (anything else
/// up to remote availability — e.g. stockpile coal beyond the steel-mill's
/// current need).
///
/// `distribute_freight` fills critical demand strictly tier-by-tier first,
/// then revisits each resource in tier order to soak slack capacity. This
/// guarantees that food and steel-mill chains are funded before, say, cotton
/// — even when remote yield is huge and would otherwise dilute proportional
/// allocation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FreightDemand {
    critical: u32,
    slack: u32,
}

/// Compute per-resource freight demand split into critical (must-haul) vs
/// slack (nice-to-haul-up-to-availability). Critical demand reflects what
/// the nation actually consumes per turn; slack is the surplus on the map
/// that we haul if there's leftover capacity after every higher tier's
/// critical demand is met.
fn compute_remote_demand(
    nation: &crate::nation::Nation,
    game_data: &crate::data::GameData,
    remote_items: &[(ResourceType, u32)],
) -> Vec<(ResourceType, FreightDemand)> {
    let mut critical: BTreeMap<ResourceType, u32> = BTreeMap::new();

    // ── Worker food demand (tier 1 critical) ──
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
                    *critical.entry(*r).or_insert(0) += share;
                    assigned = assigned.saturating_add(share);
                    last = Some(*r);
                }
                if let Some(r) = last
                    && assigned < food_need
                {
                    *critical.entry(r).or_insert(0) += food_need - assigned;
                }
            }
        }
    }

    // ── Mill / cannery raw-input demand (tier 2/3/4 critical) ──
    let mut textile_fiber_demand: u32 = 0;
    let mut cannery_meat_demand: u32 = 0;
    for building in &nation.economy.buildings {
        let cap = building.effective_capacity();
        if cap == 0 {
            continue;
        }
        match building.building_type {
            BuildingType::LumberMill => {
                *critical.entry(ResourceType::Timber).or_insert(0) += cap.saturating_mul(2);
            }
            BuildingType::SteelMill => {
                *critical.entry(ResourceType::Coal).or_insert(0) += cap;
                *critical.entry(ResourceType::Iron).or_insert(0) += cap;
            }
            BuildingType::TextileMill | BuildingType::AdvancedTextileMill => {
                // Cotton + wool feed a single fiber pool (`2 × cap` total).
                // Apportion across cotton/wool by remote availability below.
                textile_fiber_demand = textile_fiber_demand.saturating_add(cap.saturating_mul(2));
            }
            BuildingType::FoodProcessing => {
                // Cannery: 1 grain + 1 fruit + 1 (fish OR livestock) per unit.
                *critical.entry(ResourceType::Grain).or_insert(0) += cap;
                *critical.entry(ResourceType::Fruit).or_insert(0) += cap;
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
        &mut critical,
    );

    // Apportion cannery meat (single cap pool) across fish/livestock by remote
    // availability.
    apportion_pool_to_pair(
        cannery_meat_demand,
        ResourceType::Fish,
        ResourceType::Livestock,
        remote_items,
        &mut critical,
    );

    // Build the (resource, FreightDemand) list in priority order. For each
    // resource:
    //   critical = min(computed need, remote availability)
    //   slack    = remote availability - critical
    // Slack-only entries (no active chain need but remote yield exists) still
    // get a tier-4 slack allocation so surplus map yield is hauled into the
    // warehouse when capacity allows.
    let mut out: Vec<(ResourceType, FreightDemand)> = Vec::new();
    let mut seen: std::collections::BTreeSet<ResourceType> = std::collections::BTreeSet::new();
    for r in DEMAND_RESOURCES {
        let avail = remote_items
            .iter()
            .find(|(rr, _)| *rr == r)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        if avail == 0 {
            continue;
        }
        let need = critical.get(&r).copied().unwrap_or(0);
        let crit = need.min(avail);
        let slack = avail.saturating_sub(crit);
        out.push((
            r,
            FreightDemand {
                critical: crit,
                slack,
            },
        ));
        seen.insert(r);
    }
    // Resources outside `DEMAND_RESOURCES` (Horses, Oil, Gold, Gems …) — still
    // worth hauling as tier-4 slack if the map yields them and capacity allows.
    for (r, avail) in remote_items {
        if seen.contains(r) || *avail == 0 {
            continue;
        }
        out.push((
            *r,
            FreightDemand {
                critical: 0,
                slack: *avail,
            },
        ));
    }
    out
}

/// Distribute `freight_cars` strictly tier-by-tier:
///   1. Walk tiers 1..=4 in order. For each tier, give every resource its
///      full `critical` demand before moving to the next tier. Inside a tier,
///      iterate in the input list's order (which reflects intra-tier
///      preference, e.g. Iron before Coal).
///   2. After all critical demand is satisfied (or capacity runs out), revisit
///      tiers in the same order and allocate `slack` (extra remote yield up to
///      availability), again tier-by-tier so surplus capacity prefers
///      higher-tier stockpiles.
///
/// This guarantees that food and steel-mill chains are funded before, say,
/// cotton, regardless of how large the lower-tier remote yield is.
fn distribute_freight(
    freight_cars: u32,
    demand: &[(ResourceType, FreightDemand)],
) -> Vec<(ResourceType, u32)> {
    let mut allocations: Vec<(ResourceType, u32)> = demand.iter().map(|(r, _)| (*r, 0)).collect();
    if freight_cars == 0 || demand.is_empty() {
        return allocations;
    }

    let mut remaining = freight_cars;

    // Phase 1: critical demand, tier-by-tier.
    for tier in 1u8..=4 {
        if remaining == 0 {
            break;
        }
        for (i, (resource, fd)) in demand.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            if freight_priority_tier(*resource) != tier {
                continue;
            }
            let grant = fd.critical.min(remaining);
            if grant > 0 {
                allocations[i].1 += grant;
                remaining -= grant;
            }
        }
    }

    // Phase 2: slack (soak surplus map yield), tier-by-tier in the same order.
    for tier in 1u8..=4 {
        if remaining == 0 {
            break;
        }
        for (i, (resource, fd)) in demand.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            if freight_priority_tier(*resource) != tier {
                continue;
            }
            let grant = fd.slack.min(remaining);
            if grant > 0 {
                allocations[i].1 += grant;
                remaining -= grant;
            }
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

    /// Total freight demand for a resource (critical + slack).
    fn total_demand(out: &[(ResourceType, FreightDemand)], r: ResourceType) -> u32 {
        out.iter()
            .find(|(rr, _)| *rr == r)
            .map(|(_, fd)| fd.critical + fd.slack)
            .unwrap_or(0)
    }

    /// Helper: build a `(ResourceType, FreightDemand)` list with all demand as
    /// `critical` (back-compat with old tests that exercise `distribute_freight`
    /// against single-value totals).
    fn crit(items: &[(ResourceType, u32)]) -> Vec<(ResourceType, FreightDemand)> {
        items
            .iter()
            .map(|(r, q)| {
                (
                    *r,
                    FreightDemand {
                        critical: *q,
                        slack: 0,
                    },
                )
            })
            .collect()
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
        let coal = demand.iter().find(|(r, _)| *r == ResourceType::Coal);
        let iron = demand.iter().find(|(r, _)| *r == ResourceType::Iron);
        // SteelMill cap=4 → 4 critical coal + 4 critical iron (cap=4 each input).
        assert!(
            coal.is_some_and(|(_, fd)| fd.critical >= 4),
            "steel mill should drive critical coal demand"
        );
        assert!(
            iron.is_some_and(|(_, fd)| fd.critical >= 4),
            "steel mill should drive critical iron demand"
        );
    }

    #[test]
    fn demand_cannery_meat_pulls_remote_when_freight_slack() {
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
        let fish = total_demand(&demand, ResourceType::Fish);
        let livestock = total_demand(&demand, ResourceType::Livestock);
        // critical = min(4 meat need, 10 fish remote) = 4. slack = 10-4 = 6.
        // Total visible demand including slack soak = 10.
        assert_eq!(
            fish, 10,
            "fish total demand should equal remote availability"
        );
        assert_eq!(livestock, 0, "no livestock remote → no livestock demand");
    }

    #[test]
    fn demand_textile_fiber_pulls_remote_when_freight_slack() {
        // Textile mill cap=3 → 6 fiber units split across cotton/wool.
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::TextileMill, 3));
        let remote = vec![(ResourceType::Cotton, 4), (ResourceType::Wool, 4)];
        let demand = compute_remote_demand(&nation, &data, &remote);
        let cotton = total_demand(&demand, ResourceType::Cotton);
        let wool = total_demand(&demand, ResourceType::Wool);
        // Total demand (critical + slack) = remote availability when slack is
        // soaked.
        assert_eq!(
            cotton + wool,
            8,
            "total demand should equal sum of remote availability"
        );
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
                total_demand(&demand, r) > 0,
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
        let timber = total_demand(&demand, ResourceType::Timber);
        assert_eq!(timber, 3, "demand must be capped by remote availability");
        let entry = demand
            .iter()
            .find(|(r, _)| *r == ResourceType::Timber)
            .unwrap()
            .1;
        // critical caps at avail (3), slack is 0.
        assert_eq!(entry.critical, 3);
        assert_eq!(entry.slack, 0);
    }

    #[test]
    fn demand_falls_through_to_remote_when_no_per_need() {
        // Even with no buildings and no workers, the AI should still ask for
        // any remote yield it can collect — surplus is hauled as slack.
        let nation = make_test_nation();
        let data = crate::data::GameData::default();
        let remote = vec![(ResourceType::Coal, 5)];
        let demand = compute_remote_demand(&nation, &data, &remote);
        let entry = demand
            .iter()
            .find(|(r, _)| *r == ResourceType::Coal)
            .unwrap()
            .1;
        // No SteelMill → critical=0, slack=5 (everything is opportunistic haul).
        assert_eq!(entry.critical, 0);
        assert_eq!(entry.slack, 5);
    }

    #[test]
    fn floor_distributes_when_freight_below_demand_count() {
        // 3 demand resources but only 2 freight slots. Strict tier order:
        // Grain (tier 1) wins all 2 cars before Coal/Iron (tier 2) get any.
        let demand = crit(&[
            (ResourceType::Coal, 10),
            (ResourceType::Iron, 10),
            (ResourceType::Grain, 10),
        ]);
        let out = distribute_freight(2, &demand);
        let total: u32 = out.iter().map(|(_, u)| *u).sum();
        assert_eq!(total, 2);
        let grain = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Grain)
            .unwrap()
            .1;
        // Tier 1 (Grain) takes priority over tier 2 (Coal/Iron) under scarcity.
        assert_eq!(grain, 2, "tier-1 grain should consume both cars");
    }

    #[test]
    fn empty_demand_no_allocations() {
        let out = distribute_freight(10, &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn zero_freight_no_allocations() {
        let demand = crit(&[(ResourceType::Coal, 5), (ResourceType::Iron, 3)]);
        let out = distribute_freight(0, &demand);
        assert!(out.iter().all(|(_, u)| *u == 0));
    }

    #[test]
    fn allocation_capped_by_demand() {
        // Demand only 2 of coal but 10 freight cars available. Coal stops at 2,
        // remaining 8 cars go to Iron.
        let demand = crit(&[(ResourceType::Coal, 2), (ResourceType::Iron, 100)]);
        let out = distribute_freight(10, &demand);
        let coal = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Coal)
            .unwrap()
            .1;
        let iron = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Iron)
            .unwrap()
            .1;
        assert_eq!(coal, 2);
        assert_eq!(iron, 8);
    }

    #[test]
    fn allocation_total_does_not_exceed_freight() {
        let demand = crit(&[
            (ResourceType::Coal, 5),
            (ResourceType::Iron, 5),
            (ResourceType::Grain, 5),
        ]);
        let out = distribute_freight(7, &demand);
        let total: u32 = out.iter().map(|(_, u)| *u).sum();
        assert!(total <= 7);
    }

    #[test]
    fn strict_tiers_food_before_industrial() {
        // Grain (tier 1) and Iron (tier 2) both demand 5; only 5 cars available.
        // Grain must take all 5; Iron gets 0.
        let demand = crit(&[(ResourceType::Iron, 5), (ResourceType::Grain, 5)]);
        let out = distribute_freight(5, &demand);
        let grain = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Grain)
            .unwrap()
            .1;
        let iron = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Iron)
            .unwrap()
            .1;
        assert_eq!(grain, 5, "tier-1 must be filled before tier-2");
        assert_eq!(iron, 0, "tier-2 gets nothing while tier-1 is unmet");
    }

    #[test]
    fn strict_tiers_industrial_before_timber_before_other() {
        // Iron (tier 2), Timber (tier 3), Cotton (tier 4) each demand 3.
        // 6 cars: Iron gets 3, Timber gets 3, Cotton 0.
        let demand = crit(&[
            (ResourceType::Cotton, 3),
            (ResourceType::Iron, 3),
            (ResourceType::Timber, 3),
        ]);
        let out = distribute_freight(6, &demand);
        let iron = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Iron)
            .unwrap()
            .1;
        let timber = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Timber)
            .unwrap()
            .1;
        let cotton = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Cotton)
            .unwrap()
            .1;
        assert_eq!(iron, 3);
        assert_eq!(timber, 3);
        assert_eq!(cotton, 0, "tier 4 gets nothing while tiers 2-3 unmet");
    }

    #[test]
    fn slack_phase_runs_after_critical_satisfied() {
        // Coal critical=2 slack=8, Cotton critical=2 slack=8. With 20 cars,
        // critical phase (4 total) finishes both, then slack phase fills tier-2
        // coal slack first (8 cars), then tier-4 cotton slack (8 cars). Total=20.
        let demand = vec![
            (
                ResourceType::Cotton,
                FreightDemand {
                    critical: 2,
                    slack: 8,
                },
            ),
            (
                ResourceType::Coal,
                FreightDemand {
                    critical: 2,
                    slack: 8,
                },
            ),
        ];
        let out = distribute_freight(20, &demand);
        let coal = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Coal)
            .unwrap()
            .1;
        let cotton = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Cotton)
            .unwrap()
            .1;
        // With 20 cars and total demand 20, both are fully filled.
        assert_eq!(coal, 10);
        assert_eq!(cotton, 10);
    }

    #[test]
    fn slack_phase_prefers_higher_tier_under_scarcity() {
        // Critical zero, only slack. Coal (tier 2) slack=5, Cotton (tier 4)
        // slack=5. With 5 cars, all go to coal.
        let demand = vec![
            (
                ResourceType::Cotton,
                FreightDemand {
                    critical: 0,
                    slack: 5,
                },
            ),
            (
                ResourceType::Coal,
                FreightDemand {
                    critical: 0,
                    slack: 5,
                },
            ),
        ];
        let out = distribute_freight(5, &demand);
        let coal = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Coal)
            .unwrap()
            .1;
        let cotton = out
            .iter()
            .find(|(r, _)| *r == ResourceType::Cotton)
            .unwrap()
            .1;
        assert_eq!(coal, 5, "tier-2 slack outranks tier-4 slack");
        assert_eq!(cotton, 0);
    }

    #[test]
    fn freight_priority_tier_assignments() {
        // Verify the user-requested tier ordering.
        // Tier 1: foods.
        for r in [
            ResourceType::Grain,
            ResourceType::Fruit,
            ResourceType::Livestock,
            ResourceType::Fish,
        ] {
            assert_eq!(freight_priority_tier(r), 1, "{:?} must be tier 1", r);
        }
        // Tier 2: Iron, Coal.
        assert_eq!(freight_priority_tier(ResourceType::Iron), 2);
        assert_eq!(freight_priority_tier(ResourceType::Coal), 2);
        // Tier 3: Timber.
        assert_eq!(freight_priority_tier(ResourceType::Timber), 3);
        // Tier 4: everything else.
        for r in [
            ResourceType::Cotton,
            ResourceType::Wool,
            ResourceType::Horses,
            ResourceType::Oil,
            ResourceType::Gold,
            ResourceType::Gems,
        ] {
            assert_eq!(freight_priority_tier(r), 4, "{:?} must be tier 4", r);
        }
    }
}
