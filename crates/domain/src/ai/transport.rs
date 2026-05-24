//! AI transport allocation — assigns freight cars to actual remote demand.
//!
//! Without explicit allocations the transport system delivers nothing remote
//! (see `TransportSystem::calculate_deliveries`). Human players use the freight
//! panel; AI nations call [`ai_allocate_transport`] each turn to pick allocations
//! that cover their workers' food, prefer downstream town outputs over upstream
//! raw inputs, and still soak all remaining remote supply when capacity allows.

use crate::economy::FreightTarget;
use crate::economy::buildings::BuildingType;
use crate::game_state::GameState;
use crate::types::*;
use std::collections::{BTreeMap, HashMap, hash_map::Entry};

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

/// Lower number = filled first.
const PRIORITY_WORKER_FOOD: u8 = 1;
const PRIORITY_TOWN_GOODS: u8 = 2;
const PRIORITY_TOWN_MATERIALS: u8 = 3;
const PRIORITY_ACTIVE_CHAIN_RAWS: u8 = 4;
const PRIORITY_CASH_GOLD_GEMS: u8 = 5;
const PRIORITY_OPPORTUNISTIC: u8 = 6;

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

    // Trello #484: capital-tile ("local") yields now also need freight, so
    // fold them into the same demand pool as remote items. Passing an empty
    // local supply to `compute_remote_demand` makes it stop treating local
    // food/fiber/meat as "already covered for free".
    let (local_items, remote_items) =
        crate::economy::current_collectable_resources(game, nation_id);
    let mut all_items: Vec<(ResourceType, u32)> = Vec::new();
    for (resource, qty) in local_items.into_iter().chain(remote_items.into_iter()) {
        if qty == 0 {
            continue;
        }
        if let Some(entry) = all_items.iter_mut().find(|(r, _)| *r == resource) {
            entry.1 = entry.1.saturating_add(qty);
        } else {
            all_items.push((resource, qty));
        }
    }
    // Mirror `resolve_transport` (processor.rs:1442): AI difficulty multipliers
    // scale up yields. Apply the same multiplier here so demand caps and
    // delivery-time availability agree.
    let multiplier = ai_difficulty_multiplier(game, nation_id);
    if (multiplier - 1.0).abs() > f64::EPSILON {
        for (_, qty) in all_items.iter_mut() {
            *qty = (*qty as f64 * multiplier).round() as u32;
        }
    }
    if all_items.is_empty() {
        if let Some(n) = game.get_nation_mut(nation_id) {
            n.economy.transport.allocations.clear();
        }
        return;
    }

    let per_province =
        crate::economy::project_town_outputs(game, nation_id, &game.game_data.game_config);
    let town_totals = crate::economy::aggregate_town_outputs(&per_province);
    let nation = game.get_nation(nation_id).expect("nation present");
    let demand = compute_remote_demand(
        game,
        nation,
        &game.game_data,
        &[],
        &all_items,
        &town_totals,
    );

    let allocations = distribute_freight(total_capacity, &demand);

    let nation = game.get_nation_mut(nation_id).expect("nation present");
    nation.economy.transport.allocations.clear();
    for (target, units) in allocations {
        if units > 0 {
            nation.economy.transport.set_allocation(target, units);
        }
    }
}

/// Split a single-pool demand of `total` units across two interchangeable
/// resources by their remote availability share.
fn apportion_pool_to_pair(
    total: u32,
    a: ResourceType,
    b: ResourceType,
    remote_supply: &BTreeMap<ResourceType, u32>,
    demand: &mut BTreeMap<ResourceType, u32>,
) {
    if total == 0 {
        return;
    }
    let avail_a = remote_supply.get(&a).copied().unwrap_or(0);
    let avail_b = remote_supply.get(&b).copied().unwrap_or(0);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FreightDemand {
    target: FreightTarget,
    critical: u32,
    slack: u32,
    priority: u8,
}

/// Compute per-resource freight demand split into critical (must-haul) vs
fn compute_remote_demand(
    game: &GameState,
    nation: &crate::nation::Nation,
    game_data: &crate::data::GameData,
    local_items: &[(ResourceType, u32)],
    remote_items: &[(ResourceType, u32)],
    remote_outputs: &[(FreightTarget, u32)],
) -> Vec<FreightDemand> {
    let local_supply: BTreeMap<ResourceType, u32> = local_items.iter().copied().collect();
    let remote_supply: BTreeMap<ResourceType, u32> = remote_items.iter().copied().collect();
    let mut active_raw_critical: BTreeMap<ResourceType, u32> = BTreeMap::new();
    let local_supply = |r: ResourceType| -> u32 { local_supply.get(&r).copied().unwrap_or(0) };

    let mut demand = Vec::new();

    // ── Current worker food support ──
    let workers = nation.economy.labor.total_workers();
    if workers > 0 && game_data.game_config.food_per_worker > 0 {
        let (grain_need, fruit_need, meat_need) =
            crate::economy::labor::worker_food_demand(workers);
        push_resource_demand(
            &mut demand,
            ResourceType::Grain,
            grain_need.saturating_sub(local_supply(ResourceType::Grain)),
            remote_supply
                .get(&ResourceType::Grain)
                .copied()
                .unwrap_or(0),
            PRIORITY_WORKER_FOOD,
        );
        push_resource_demand(
            &mut demand,
            ResourceType::Fruit,
            fruit_need.saturating_sub(local_supply(ResourceType::Fruit)),
            remote_supply
                .get(&ResourceType::Fruit)
                .copied()
                .unwrap_or(0),
            PRIORITY_WORKER_FOOD,
        );

        let local_meat =
            local_supply(ResourceType::Livestock).saturating_add(local_supply(ResourceType::Fish));
        let mut worker_meat = BTreeMap::new();
        apportion_pool_to_pair(
            meat_need.saturating_sub(local_meat),
            ResourceType::Fish,
            ResourceType::Livestock,
            &remote_supply,
            &mut worker_meat,
        );
        for resource in [ResourceType::Fish, ResourceType::Livestock] {
            push_resource_demand(
                &mut demand,
                resource,
                worker_meat.get(&resource).copied().unwrap_or(0),
                remote_supply.get(&resource).copied().unwrap_or(0),
                PRIORITY_WORKER_FOOD,
            );
        }
    }

    // ── Downstream town outputs: goods before materials before raws ──
    for target in [
        FreightTarget::Goods(GoodsType::Furniture),
        FreightTarget::Goods(GoodsType::Hardware),
        FreightTarget::Goods(GoodsType::Clothing),
        FreightTarget::Material(MaterialType::Lumber),
        FreightTarget::Material(MaterialType::Steel),
        FreightTarget::Material(MaterialType::Fabric),
    ] {
        let available = remote_outputs
            .iter()
            .find(|(stockpile, _)| *stockpile == target)
            .map(|(_, qty)| *qty)
            .unwrap_or(0);
        if available == 0 {
            continue;
        }
        demand.push(FreightDemand {
            target,
            critical: available,
            slack: 0,
            priority: match target {
                FreightTarget::Goods(_) => PRIORITY_TOWN_GOODS,
                FreightTarget::Material(_) => PRIORITY_TOWN_MATERIALS,
                FreightTarget::Resource(_) => PRIORITY_OPPORTUNISTIC,
            },
        });
    }

    // ── Active chain raw-input demand ──
    let mut lumber_mill_cap: u32 = 0;
    let mut steel_mill_cap: u32 = 0;
    let mut textile_mill_cap: u32 = 0;
    let mut cannery_cap: u32 = 0;
    for building in &nation.economy.buildings {
        let cap = building.effective_capacity();
        if cap == 0 {
            continue;
        }
        match building.building_type {
            BuildingType::LumberMill => lumber_mill_cap = lumber_mill_cap.saturating_add(cap),
            BuildingType::SteelMill => steel_mill_cap = steel_mill_cap.saturating_add(cap),
            BuildingType::TextileMill | BuildingType::AdvancedTextileMill => {
                textile_mill_cap = textile_mill_cap.saturating_add(cap);
            }
            BuildingType::FoodProcessing => cannery_cap = cannery_cap.saturating_add(cap),
            _ => {}
        }
    }
    let targets = &nation.economy.chain_targets;
    let lumber_run = lumber_mill_cap.min(targets.timber_mill);
    let steel_run = steel_mill_cap.min(targets.metal_mill);
    let textile_run = textile_mill_cap.min(targets.textile_mill);
    let cannery_run = cannery_cap.min(targets.canned_food_factory);
    if lumber_run > 0 {
        *active_raw_critical.entry(ResourceType::Timber).or_insert(0) +=
            lumber_run.saturating_mul(2);
    }
    if steel_run > 0 {
        *active_raw_critical.entry(ResourceType::Coal).or_insert(0) += steel_run;
        *active_raw_critical.entry(ResourceType::Iron).or_insert(0) += steel_run;
    }
    let textile_fiber_demand = textile_run.saturating_mul(2);
    if cannery_run > 0 {
        *active_raw_critical.entry(ResourceType::Grain).or_insert(0) += cannery_run;
        *active_raw_critical.entry(ResourceType::Fruit).or_insert(0) += cannery_run;
    }
    let local_fiber =
        local_supply(ResourceType::Cotton).saturating_add(local_supply(ResourceType::Wool));
    apportion_pool_to_pair(
        textile_fiber_demand.saturating_sub(local_fiber),
        ResourceType::Cotton,
        ResourceType::Wool,
        &remote_supply,
        &mut active_raw_critical,
    );

    let local_meat =
        local_supply(ResourceType::Livestock).saturating_add(local_supply(ResourceType::Fish));
    apportion_pool_to_pair(
        cannery_run.saturating_sub(local_meat),
        ResourceType::Fish,
        ResourceType::Livestock,
        &remote_supply,
        &mut active_raw_critical,
    );

    let cash_strapped = is_cash_strapped(game, nation.id);
    for (resource, avail) in &remote_supply {
        if *avail == 0 {
            continue;
        }
        let critical = active_raw_critical
            .get(resource)
            .copied()
            .unwrap_or(0)
            .min(*avail);
        let priority =
            if matches!(resource, ResourceType::Gold | ResourceType::Gems) && cash_strapped {
                PRIORITY_CASH_GOLD_GEMS
            } else if critical > 0 {
                PRIORITY_ACTIVE_CHAIN_RAWS
            } else {
                PRIORITY_OPPORTUNISTIC
            };
        let extra_critical = if priority == PRIORITY_CASH_GOLD_GEMS {
            avail.saturating_sub(critical)
        } else {
            0
        };
        demand.push(FreightDemand {
            target: FreightTarget::Resource(*resource),
            critical: critical.saturating_add(extra_critical),
            slack: avail.saturating_sub(critical.saturating_add(extra_critical)),
            priority,
        });
    }

    merge_demands(demand, &remote_supply, remote_outputs)
}

fn distribute_freight(freight_cars: u32, demand: &[FreightDemand]) -> Vec<(FreightTarget, u32)> {
    let mut allocations: Vec<(FreightTarget, u32)> =
        demand.iter().map(|fd| (fd.target, 0)).collect();
    if freight_cars == 0 || demand.is_empty() {
        return allocations;
    }

    let mut remaining = freight_cars;

    for priority in PRIORITY_WORKER_FOOD..=PRIORITY_OPPORTUNISTIC {
        if remaining == 0 {
            break;
        }
        for (i, fd) in demand.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            if fd.priority != priority {
                continue;
            }
            let grant = fd.critical.min(remaining);
            if grant > 0 {
                allocations[i].1 += grant;
                remaining -= grant;
            }
        }
    }

    for priority in PRIORITY_WORKER_FOOD..=PRIORITY_OPPORTUNISTIC {
        if remaining == 0 {
            break;
        }
        for (i, fd) in demand.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            if fd.priority != priority {
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

fn push_resource_demand(
    demand: &mut Vec<FreightDemand>,
    resource: ResourceType,
    critical: u32,
    available: u32,
    priority: u8,
) {
    if available == 0 {
        return;
    }
    demand.push(FreightDemand {
        target: FreightTarget::Resource(resource),
        critical: critical.min(available),
        slack: available.saturating_sub(critical.min(available)),
        priority,
    });
}

fn merge_demands(
    demand: Vec<FreightDemand>,
    remote_supply: &BTreeMap<ResourceType, u32>,
    remote_outputs: &[(FreightTarget, u32)],
) -> Vec<FreightDemand> {
    let mut merged: HashMap<FreightTarget, FreightDemand> = HashMap::new();
    for fd in demand {
        match merged.entry(fd.target) {
            Entry::Vacant(v) => {
                v.insert(fd);
            }
            Entry::Occupied(mut o) => {
                let existing = o.get_mut();
                existing.critical = existing.critical.saturating_add(fd.critical);
                existing.slack = existing.slack.saturating_add(fd.slack);
                existing.priority = existing.priority.min(fd.priority);
            }
        }
    }

    let output_caps: HashMap<FreightTarget, u32> = remote_outputs.iter().copied().collect();
    let mut merged_vec: Vec<FreightDemand> = merged
        .into_values()
        .map(|mut fd| {
            let cap = match fd.target {
                FreightTarget::Resource(resource) => {
                    remote_supply.get(&resource).copied().unwrap_or(0)
                }
                FreightTarget::Material(_) | FreightTarget::Goods(_) => {
                    output_caps.get(&fd.target).copied().unwrap_or(0)
                }
            };
            fd.critical = fd.critical.min(cap);
            fd.slack = fd.slack.min(cap.saturating_sub(fd.critical));
            fd
        })
        .collect();
    merged_vec.sort_by_key(|fd| (fd.priority, target_tiebreak(fd.target)));
    merged_vec
}

fn is_cash_strapped(game: &GameState, nation_id: NationId) -> bool {
    let Some(nation) = game.get_nation(nation_id) else {
        return false;
    };
    let personality = super::common::get_personality(game, nation_id);
    let defaults = super::common::PersonalityConfig::for_personality(personality);
    let reserve = super::common::lua_or(
        super::lua_bridge::get_personality_config(game, personality)
            .and_then(|cfg| cfg.treasury_reserve.map(Money::dollars)),
        defaults.spending_reserve,
    );
    nation.economy.available_treasury() <= reserve
}

fn target_tiebreak(target: FreightTarget) -> u8 {
    match target {
        FreightTarget::Goods(GoodsType::Furniture) => 0,
        FreightTarget::Goods(GoodsType::Hardware) => 1,
        FreightTarget::Goods(GoodsType::Clothing) => 2,
        FreightTarget::Material(MaterialType::Lumber) => 3,
        FreightTarget::Material(MaterialType::Steel) => 4,
        FreightTarget::Material(MaterialType::Fabric) => 5,
        FreightTarget::Resource(ResourceType::Grain) => 6,
        FreightTarget::Resource(ResourceType::Fruit) => 7,
        FreightTarget::Resource(ResourceType::Livestock) => 8,
        FreightTarget::Resource(ResourceType::Fish) => 9,
        FreightTarget::Resource(ResourceType::Coal) => 10,
        FreightTarget::Resource(ResourceType::Iron) => 11,
        FreightTarget::Resource(ResourceType::Cotton) => 12,
        FreightTarget::Resource(ResourceType::Wool) => 13,
        FreightTarget::Resource(ResourceType::Timber) => 14,
        FreightTarget::Resource(ResourceType::Gold) => 15,
        FreightTarget::Resource(ResourceType::Gems) => 16,
        FreightTarget::Resource(ResourceType::Horses) => 17,
        FreightTarget::Resource(ResourceType::Oil) => 18,
        FreightTarget::Material(_) => 19,
        FreightTarget::Goods(_) => 20,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::test_game_with_ai;
    use crate::economy::buildings::Building;

    fn test_game_and_nation() -> GameState {
        test_game_with_ai()
    }

    fn find_demand(demand: &[FreightDemand], target: FreightTarget) -> Option<FreightDemand> {
        demand.iter().copied().find(|fd| fd.target == target)
    }

    fn allocation_for(allocations: &[(FreightTarget, u32)], target: FreightTarget) -> u32 {
        allocations
            .iter()
            .find(|(t, _)| *t == target)
            .map(|(_, units)| *units)
            .unwrap_or(0)
    }

    #[test]
    fn worker_food_priority_beats_active_chain_raws() {
        let demand = vec![
            FreightDemand {
                target: FreightTarget::Resource(ResourceType::Timber),
                critical: 4,
                slack: 0,
                priority: PRIORITY_ACTIVE_CHAIN_RAWS,
            },
            FreightDemand {
                target: FreightTarget::Resource(ResourceType::Grain),
                critical: 2,
                slack: 0,
                priority: PRIORITY_WORKER_FOOD,
            },
        ];

        let out = distribute_freight(3, &demand);
        assert_eq!(
            allocation_for(&out, FreightTarget::Resource(ResourceType::Grain)),
            2
        );
        assert_eq!(
            allocation_for(&out, FreightTarget::Resource(ResourceType::Timber)),
            1
        );
    }

    #[test]
    fn downstream_outputs_beat_upstream_raws() {
        let demand = vec![
            FreightDemand {
                target: FreightTarget::Resource(ResourceType::Timber),
                critical: 3,
                slack: 0,
                priority: PRIORITY_ACTIVE_CHAIN_RAWS,
            },
            FreightDemand {
                target: FreightTarget::Material(MaterialType::Lumber),
                critical: 1,
                slack: 0,
                priority: PRIORITY_TOWN_MATERIALS,
            },
            FreightDemand {
                target: FreightTarget::Goods(GoodsType::Furniture),
                critical: 1,
                slack: 0,
                priority: PRIORITY_TOWN_GOODS,
            },
        ];

        let out = distribute_freight(2, &demand);
        assert_eq!(
            allocation_for(&out, FreightTarget::Goods(GoodsType::Furniture)),
            1
        );
        assert_eq!(
            allocation_for(&out, FreightTarget::Material(MaterialType::Lumber)),
            1
        );
        assert_eq!(
            allocation_for(&out, FreightTarget::Resource(ResourceType::Timber)),
            0
        );
    }

    #[test]
    fn cash_strapped_gold_becomes_critical() {
        let mut game = test_game_and_nation();
        let nation_id = NationId(2);
        game.get_nation_mut(nation_id).unwrap().economy.treasury = Money::ZERO;

        let nation = game.get_nation(nation_id).unwrap();
        let demand = compute_remote_demand(
            &game,
            nation,
            &game.game_data,
            &[],
            &[(ResourceType::Gold, 5)],
            &[],
        );

        let gold = find_demand(&demand, FreightTarget::Resource(ResourceType::Gold)).unwrap();
        assert_eq!(gold.critical, 5);
        assert_eq!(gold.priority, PRIORITY_CASH_GOLD_GEMS);
    }

    #[test]
    fn cash_healthy_gold_stays_opportunistic() {
        let mut game = test_game_and_nation();
        let nation_id = NationId(2);
        game.get_nation_mut(nation_id).unwrap().economy.treasury = Money::dollars(50_000);

        let nation = game.get_nation(nation_id).unwrap();
        let demand = compute_remote_demand(
            &game,
            nation,
            &game.game_data,
            &[],
            &[(ResourceType::Gold, 5)],
            &[],
        );

        let gold = find_demand(&demand, FreightTarget::Resource(ResourceType::Gold)).unwrap();
        assert_eq!(gold.critical, 0);
        assert_eq!(gold.slack, 5);
        assert_eq!(gold.priority, PRIORITY_OPPORTUNISTIC);
    }

    #[test]
    fn local_supply_offsets_worker_food_needs() {
        let mut game = test_game_and_nation();
        let nation_id = NationId(2);
        game.get_nation_mut(nation_id)
            .unwrap()
            .economy
            .labor
            .untrained = 8;

        let nation = game.get_nation(nation_id).unwrap();
        let demand = compute_remote_demand(
            &game,
            nation,
            &game.game_data,
            &[(ResourceType::Grain, 3), (ResourceType::Fruit, 2)],
            &[
                (ResourceType::Grain, 10),
                (ResourceType::Fruit, 10),
                (ResourceType::Fish, 10),
            ],
            &[],
        );

        let grain = find_demand(&demand, FreightTarget::Resource(ResourceType::Grain)).unwrap();
        let fruit = find_demand(&demand, FreightTarget::Resource(ResourceType::Fruit)).unwrap();
        assert_eq!(grain.critical, 1);
        assert_eq!(fruit.critical, 0);
    }

    #[test]
    fn chain_targets_cap_active_raw_demand() {
        let mut game = test_game_and_nation();
        let nation_id = NationId(2);
        let ai = game.get_nation_mut(nation_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 10));
        ai.economy.chain_targets.metal_mill = 3;

        let nation = game.get_nation(nation_id).unwrap();
        let demand = compute_remote_demand(
            &game,
            nation,
            &game.game_data,
            &[],
            &[(ResourceType::Coal, 50), (ResourceType::Iron, 50)],
            &[],
        );

        let coal = find_demand(&demand, FreightTarget::Resource(ResourceType::Coal)).unwrap();
        let iron = find_demand(&demand, FreightTarget::Resource(ResourceType::Iron)).unwrap();
        assert_eq!(coal.critical, 3);
        assert_eq!(iron.critical, 3);
    }

    #[test]
    fn transport_uses_all_capacity_when_supply_remains() {
        let demand = vec![
            FreightDemand {
                target: FreightTarget::Resource(ResourceType::Horses),
                critical: 0,
                slack: 2,
                priority: PRIORITY_OPPORTUNISTIC,
            },
            FreightDemand {
                target: FreightTarget::Resource(ResourceType::Oil),
                critical: 0,
                slack: 3,
                priority: PRIORITY_OPPORTUNISTIC,
            },
        ];

        let out = distribute_freight(4, &demand);
        let total: u32 = out.iter().map(|(_, units)| *units).sum();
        assert_eq!(total, 4);
    }

    #[test]
    fn town_output_priorities_sort_goods_before_materials_before_raws() {
        let mut game = test_game_and_nation();
        let nation_id = NationId(2);
        let ai = game.get_nation_mut(nation_id).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 4));
        ai.economy.chain_targets.timber_mill = 4;

        let nation = game.get_nation(nation_id).unwrap();
        let demand = compute_remote_demand(
            &game,
            nation,
            &game.game_data,
            &[],
            &[(ResourceType::Timber, 10)],
            &[
                (FreightTarget::Goods(GoodsType::Furniture), 1),
                (FreightTarget::Material(MaterialType::Lumber), 2),
            ],
        );

        let furniture_idx = demand
            .iter()
            .position(|fd| fd.target == FreightTarget::Goods(GoodsType::Furniture))
            .unwrap();
        let lumber_idx = demand
            .iter()
            .position(|fd| fd.target == FreightTarget::Material(MaterialType::Lumber))
            .unwrap();
        let timber_idx = demand
            .iter()
            .position(|fd| fd.target == FreightTarget::Resource(ResourceType::Timber))
            .unwrap();

        assert!(furniture_idx < lumber_idx);
        assert!(lumber_idx < timber_idx);
    }

    #[test]
    fn duplicate_resource_demands_merge_before_allocation() {
        let merged = merge_demands(
            vec![
                FreightDemand {
                    target: FreightTarget::Resource(ResourceType::Grain),
                    critical: 4,
                    slack: 4,
                    priority: PRIORITY_WORKER_FOOD,
                },
                FreightDemand {
                    target: FreightTarget::Resource(ResourceType::Grain),
                    critical: 1,
                    slack: 7,
                    priority: PRIORITY_ACTIVE_CHAIN_RAWS,
                },
            ],
            &BTreeMap::from([(ResourceType::Grain, 8)]),
            &[],
        );

        assert_eq!(merged.len(), 1);
        let grain = merged[0];
        assert_eq!(grain.target, FreightTarget::Resource(ResourceType::Grain));
        assert_eq!(grain.priority, PRIORITY_WORKER_FOOD);
        assert_eq!(grain.critical, 5);
        assert_eq!(grain.slack, 3);
    }
}
