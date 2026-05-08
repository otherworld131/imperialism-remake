#![allow(unused_labels)]
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::game_state::GameState;
use crate::map::sea_zones::SeaZoneId;
use crate::military::naval::{
    NavalOperation, find_nation_home_sea_zone, move_warship_group_one_zone,
};
use crate::military::ships::{Ship, ShipType};
use crate::types::*;

use super::common::{AiPersonality, PersonalityConfig, get_personality};

/// Warship types ordered from highest to lowest firepower, used by
/// `best_buildable_warship` to pick the most capable ship the AI can afford.
const WARSHIP_PRIORITY: &[ShipType] = &[
    ShipType::Dreadnought,
    ShipType::Battlecruiser,
    ShipType::AdvancedIronclad,
    ShipType::Ironclad,
    ShipType::ArmouredCruiser,
    ShipType::ShipOfTheLine,
    ShipType::Raider,
    ShipType::Frigate,
];

/// Merchant types ordered from highest to lowest cargo, used by
/// `best_buildable_merchant` to pick the largest hull whose tech is met
/// and whose materials are on hand.
const MERCHANT_PRIORITY: &[ShipType] = &[
    ShipType::Freighter,
    ShipType::Paddlewheeler,
    ShipType::Indiaman,
    ShipType::Clipper,
    ShipType::Trader,
];

/// Returns true if any merchant ship is buildable purely on tech grounds
/// (ignoring materials). Used by `merchant_navy_material_reserve` so
/// nations without yet-researched-tech still reserve for the most basic
/// hull they can build.
fn nation_has_tech_for(game: &GameState, nation_id: NationId, ship_type: ShipType) -> bool {
    let Some(nation) = game.get_nation(nation_id) else {
        return false;
    };
    let costs = game.game_data.ship_stats(ship_type);
    let Some(ref tech_name) = costs.prerequisite_tech else {
        return true;
    };
    game.game_data
        .tech_tree
        .all_techs()
        .iter()
        .any(|t| t.name == *tech_name && nation.researched_techs.contains(&t.id))
}

/// Highest-cargo merchant ship the nation has tech for AND materials for,
/// ignoring the merchant-navy reservation (so this can be called from
/// `ai_build_merchant_ships` itself). Returns `None` if no merchant hull is
/// buildable right now.
fn best_buildable_merchant(game: &GameState, nation_id: NationId) -> Option<ShipType> {
    let nation = game.get_nation(nation_id)?;
    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);
    let steel_have = nation.material_amount(MaterialType::Steel);
    let coal_have = nation.resource_amount(ResourceType::Coal);

    for &ship_type in MERCHANT_PRIORITY {
        if !nation_has_tech_for(game, nation_id, ship_type) {
            continue;
        }
        let costs = game.game_data.ship_stats(ship_type);
        if fabric_have < costs.fabric_cost
            || lumber_have < costs.lumber_cost
            || steel_have < costs.steel_cost
            || coal_have < costs.coal_cost
        {
            continue;
        }
        return Some(ship_type);
    }
    None
}

/// Highest-cargo merchant ship the nation has tech for, ignoring materials.
/// Used by the reservation logic — even if we don't have the materials yet,
/// we want to reserve for the *best* hull we'll eventually build.
fn best_tech_merchant(game: &GameState, nation_id: NationId) -> ShipType {
    for &ship_type in MERCHANT_PRIORITY {
        if nation_has_tech_for(game, nation_id, ship_type) {
            return ship_type;
        }
    }
    ShipType::Trader // fallback: every nation has Trader from turn 1
}

/// Sum of projected per-turn raw-resource consumption from chain targets.
fn projected_raw_consumption_per_turn(game: &GameState, nation_id: NationId) -> u32 {
    let Some(nation) = game.get_nation(nation_id) else {
        return 0;
    };
    crate::economy::trade::projected_resource_needs(nation)
        .values()
        .copied()
        .sum()
}

/// Per-turn import gap: the slice of raw-resource consumption that cannot be
/// sourced from the nation's own provinces (local + remote yield) and must
/// be bought on the world market.
///
/// Imports flow exclusively over merchant cargo in the current trade model
/// (`generate_need_based_bids` caps total bid quantity at `total_cargo_capacity`),
/// so this is the *correct* number to compare against cargo when deciding
/// whether to grow the merchant navy. Comparing gross consumption (the old
/// behavior) wildly overstates cargo need for self-sufficient nations.
fn projected_import_gap_per_turn(game: &GameState, nation_id: NationId) -> u32 {
    let Some(nation) = game.get_nation(nation_id) else {
        return 0;
    };
    let needs = crate::economy::trade::projected_resource_needs(nation);
    if needs.is_empty() {
        return 0;
    }
    let (local, remote) = crate::economy::current_collectable_resources(game, nation_id);
    let supply_for = |r: ResourceType| -> u32 {
        let l = local
            .iter()
            .find(|(rr, _)| *rr == r)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        let m = remote
            .iter()
            .find(|(rr, _)| *rr == r)
            .map(|(_, q)| *q)
            .unwrap_or(0);
        l.saturating_add(m)
    };
    let mut gap = 0u32;
    for (r, need) in needs {
        gap = gap.saturating_add(need.saturating_sub(supply_for(r)));
    }
    gap
}

/// Returns `true` when the AI's projected per-turn imports — the slice of
/// chain consumption it can't source locally — exceed current merchant cargo,
/// and another hull would relieve the bottleneck.
///
/// This is the trigger for the merchant-navy expansion reserve and for
/// continuing to build merchant ships past the static personality cap. We
/// gate on **import gap**, not gross consumption: a self-sufficient steel
/// economy doesn't need more cargo no matter how big its mills are, while a
/// nation whose iron is only available abroad needs cargo immediately even
/// if its overall consumption is small.
pub(crate) fn wants_more_merchant_cargo(game: &GameState, nation_id: NationId) -> bool {
    let Some(nation) = game.get_nation(nation_id) else {
        return false;
    };
    // No chain targets → no import pressure at all. Avoids spurious material
    // reservations in test scenarios with no buildings.
    if projected_raw_consumption_per_turn(game, nation_id) == 0 {
        return false;
    }
    let import_gap = projected_import_gap_per_turn(game, nation_id);
    if import_gap == 0 {
        // Fully self-sufficient — no need to grow cargo regardless of size.
        return false;
    }
    import_gap > nation.total_cargo_capacity(&game.game_data)
}

/// Materials the AI holds back from other consumers (warship build, freight
/// cars, factory chains, auto-trade with minors) so the next merchant-navy
/// expansion has the materials it needs. Returns `(fabric, lumber, steel,
/// coal)` for one ship of the best tech-available merchant hull.
///
/// Returns `(0, 0, 0, 0)` when `wants_more_merchant_cargo` is false — the
/// reserve only kicks in when the AI is actually demand-bound on cargo.
///
/// Mirrors `reserve_for_expansion`: a small, predictable hold-back that
/// keeps the materials around long enough for the next-turn build to
/// succeed instead of being drained by warship/freight/factory consumers.
pub(crate) fn merchant_navy_material_reserve(
    game: &GameState,
    nation_id: NationId,
) -> (u32, u32, u32, u32) {
    if !wants_more_merchant_cargo(game, nation_id) {
        return (0, 0, 0, 0);
    }
    let ship_type = best_tech_merchant(game, nation_id);
    let costs = game.game_data.ship_stats(ship_type);
    (
        costs.fabric_cost,
        costs.lumber_cost,
        costs.steel_cost,
        costs.coal_cost,
    )
}

/// Returns the highest-firepower warship whose tech is met AND whose
/// materials (fabric, lumber, arms + steel→arms conversion, coal) are
/// available. Falls back to `None` if nothing is buildable.
fn best_buildable_warship(game: &GameState, nation_id: NationId) -> Option<ShipType> {
    let nation = game.get_nation(nation_id)?;
    // Hold back materials the AI is queueing for the next merchant hull so
    // warship construction can't drain them.
    let (m_fabric, m_lumber, m_steel, m_coal) = merchant_navy_material_reserve(game, nation_id);
    let fabric_have = nation
        .material_amount(MaterialType::Fabric)
        .saturating_sub(m_fabric);
    let lumber_have = nation
        .material_amount(MaterialType::Lumber)
        .saturating_sub(m_lumber);
    let arms_have = nation.material_amount(MaterialType::Arms);
    let steel_have = nation
        .material_amount(MaterialType::Steel)
        .saturating_sub(m_steel);
    let coal_have = nation
        .resource_amount(ResourceType::Coal)
        .saturating_sub(m_coal);
    let researched = &nation.researched_techs;

    for &ship_type in WARSHIP_PRIORITY {
        let costs = game.game_data.ship_stats(ship_type);

        // Check tech requirement.
        if let Some(ref tech_name) = costs.prerequisite_tech {
            let has_tech = game
                .game_data
                .tech_tree
                .all_techs()
                .iter()
                .any(|t| t.name == *tech_name && researched.contains(&t.id));
            if !has_tech {
                continue;
            }
        }

        // Check coal (raw resource, not material).
        if coal_have < costs.coal_cost {
            continue;
        }

        // Check fabric and lumber — no substitution available.
        if fabric_have < costs.fabric_cost || lumber_have < costs.lumber_cost {
            continue;
        }

        // Steel: direct construction cost (not for arms conversion).
        if steel_have < costs.steel_cost {
            continue;
        }

        // Arms: steel remaining after steel_cost can substitute.
        let steel_for_arms = steel_have - costs.steel_cost;
        let effective_arms = arms_have + steel_for_arms;
        if effective_arms < costs.arms_cost {
            continue;
        }

        return Some(ship_type);
    }
    None
}

/// Try to build the best available warship for `nation_id`. Returns `true` if
/// a ship was added. Picks the highest-firepower type whose tech is met and
/// whose materials are on hand (converts steel → arms if needed, as before).
///
/// Trello card #112: the hard warship caps were removed. Card #427: now
/// selects the best era-appropriate ship instead of always building Frigates.
pub(crate) fn build_one_warship(game: &mut GameState, nation_id: NationId) -> bool {
    let ship_type = match best_buildable_warship(game, nation_id) {
        Some(t) => t,
        None => return false,
    };
    let costs = game.game_data.ship_stats(ship_type).clone();
    let fabric_need = costs.fabric_cost;
    let lumber_need = costs.lumber_cost;
    let arms_need = costs.arms_cost;
    let steel_need = costs.steel_cost;
    let coal_need = costs.coal_cost;

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return false,
    };

    let arms_have = nation.material_amount(MaterialType::Arms);
    let steel_have = nation.material_amount(MaterialType::Steel);

    // Produce arms from steel if we're short.
    // Reserve steel_need for the hull first; only surplus steel can substitute for arms.
    // Also hold back the AI's expansion-reserve steel so the conversion never
    // starves a planned mill/factory upgrade, and hold back the merchant-
    // navy reserve so warship construction doesn't drain materials the AI
    // is queueing for the next merchant hull.
    let personality = get_personality(game, nation_id);
    let (_, steel_reserve) = super::economy::reserve_for_expansion(
        game,
        nation_id,
        super::economy::expansions_per_turn_target(game, personality),
        super::economy::expansion_reserve_buildings_factor(game, personality),
    );
    let (_, _, merchant_steel, _) = merchant_navy_material_reserve(game, nation_id);
    let (_, freight_steel) = super::economy::freight_expansion_material_reserve(game, nation_id);
    let steel_for_arms = steel_have
        .saturating_sub(steel_need)
        .saturating_sub(steel_reserve)
        .saturating_sub(merchant_steel)
        .saturating_sub(freight_steel);
    if arms_have < arms_need && steel_for_arms > 0 {
        let arms_to_produce = (arms_need - arms_have).min(steel_for_arms);
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return false;
        };
        nation.consume_material(MaterialType::Steel, arms_to_produce);
        nation.add_material(MaterialType::Arms, arms_to_produce);
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Steel,
            crate::economy::ledger::ResourceOut::FactoryConsumed,
            arms_to_produce,
        ));
        game.transient.pending_ai_material_inflows.push((
            nation_id,
            MaterialType::Arms,
            crate::economy::ledger::ResourceIn::FactoryOutput,
            arms_to_produce,
        ));
    }

    // Re-check materials after possible arms production. Apply the
    // merchant-navy reserve again so a planned merchant hull is still
    // funded even after the steel→arms conversion above.
    let (m_fabric, m_lumber, m_steel, m_coal) = merchant_navy_material_reserve(game, nation_id);
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return false,
    };
    let fabric_have = nation
        .material_amount(MaterialType::Fabric)
        .saturating_sub(m_fabric);
    let lumber_have = nation
        .material_amount(MaterialType::Lumber)
        .saturating_sub(m_lumber);
    let arms_have = nation.material_amount(MaterialType::Arms);
    let steel_have = nation
        .material_amount(MaterialType::Steel)
        .saturating_sub(m_steel);
    let coal_have = nation
        .resource_amount(ResourceType::Coal)
        .saturating_sub(m_coal);

    if fabric_have >= fabric_need
        && lumber_have >= lumber_need
        && arms_have >= arms_need
        && steel_have >= steel_need
        && coal_have >= coal_need
    {
        let uid = game.alloc_unit_id();
        let ship = Ship::with_data(uid, ship_type, nation_id, &game.game_data);
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return false;
        };
        nation.consume_material(MaterialType::Fabric, fabric_need);
        nation.consume_material(MaterialType::Lumber, lumber_need);
        nation.consume_material(MaterialType::Arms, arms_need);
        if steel_need > 0 {
            nation.consume_material(MaterialType::Steel, steel_need);
        }
        if coal_need > 0 {
            nation.remove_resource(ResourceType::Coal, coal_need);
        }
        nation.military.warships.push(ship);
        nation.military.warships_built += 1;
        let out = crate::economy::ledger::ResourceOut::ConstructionConsumed;
        if fabric_need > 0 {
            game.transient.pending_ai_material_outflows.push((
                nation_id,
                MaterialType::Fabric,
                out,
                fabric_need,
            ));
        }
        if lumber_need > 0 {
            game.transient.pending_ai_material_outflows.push((
                nation_id,
                MaterialType::Lumber,
                out,
                lumber_need,
            ));
        }
        if arms_need > 0 {
            game.transient.pending_ai_material_outflows.push((
                nation_id,
                MaterialType::Arms,
                out,
                arms_need,
            ));
        }
        if steel_need > 0 {
            game.transient.pending_ai_material_outflows.push((
                nation_id,
                MaterialType::Steel,
                out,
                steel_need,
            ));
        }
        return true;
    }
    false
}

/// True if `nation_id` has the materials to build at least one warship
/// (any type). Used by the scored-spending system to gate the Warship
/// category.
pub(crate) fn can_build_warship(game: &GameState, nation_id: NationId) -> bool {
    best_buildable_warship(game, nation_id).is_some()
}

pub(crate) fn ai_build_merchant_ships(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let treasury = nation.economy.treasury;

    // ── Read Lua config (feature-gated) ──────────────────────
    let lua_cfg = super::lua_bridge::get_personality_config(game, personality);
    let pc = PersonalityConfig::for_personality(personality);
    // Ship cap depends on personality; wealthy nations always aim for 5.
    // The cap is overridden when projected per-turn import demand exceeds
    // current cargo capacity (`wants_more_merchant_cargo`) — the AI keeps
    // building until cargo catches up with the economy.
    let demand_bound = wants_more_merchant_cargo(game, nation_id);
    let static_cap: usize = if treasury > Money::dollars(5_000) {
        5
    } else {
        'val: {
            if let Some(v) = lua_cfg.as_ref().and_then(|c| c.max_merchant_ships) {
                break 'val v;
            }
            pc.max_merchant_ships
        }
    };
    // When demand-bound, size the target fleet against the actual import gap
    // (chain consumption minus own-province yield). The static cap is honored
    // as a floor.
    let max_ships: usize = if demand_bound {
        let import_gap = projected_import_gap_per_turn(game, nation_id);
        let hull_cargo = game
            .game_data
            .ship_stats(best_tech_merchant(game, nation_id))
            .cargo
            .max(1);
        let target = import_gap.div_ceil(hull_cargo) as usize;
        static_cap.max(target)
    } else {
        static_cap
    };

    // For non-Economic with low treasury, only build if cargo capacity is 0
    // OR projected import demand exceeds current cargo capacity. The latter
    // lets balanced/aggressive personalities scale up their merchant marine
    // when their economy starts demanding more imports than ships can carry.
    if personality != AiPersonality::Economic
        && treasury <= Money::dollars(5_000)
        && nation.total_cargo_capacity(&game.game_data) > 0
        && !demand_bound
    {
        return;
    }

    if nation.merchant_ship_count() >= max_ships {
        return;
    }

    let Some(ship_type) = best_buildable_merchant(game, nation_id) else {
        return;
    };
    let costs = game.game_data.ship_stats(ship_type).clone();

    let uid = game.alloc_unit_id();
    let ship = Ship::with_data(uid, ship_type, nation_id, &game.game_data);
    let Some(nation) = game.get_nation_mut(nation_id) else {
        return;
    };
    if costs.fabric_cost > 0 {
        nation.consume_material(MaterialType::Fabric, costs.fabric_cost);
    }
    if costs.lumber_cost > 0 {
        nation.consume_material(MaterialType::Lumber, costs.lumber_cost);
    }
    if costs.steel_cost > 0 {
        nation.consume_material(MaterialType::Steel, costs.steel_cost);
    }
    if costs.coal_cost > 0 {
        nation.remove_resource(ResourceType::Coal, costs.coal_cost);
    }
    nation.military.merchant_fleet.push(ship);
    let out = crate::economy::ledger::ResourceOut::ConstructionConsumed;
    if costs.fabric_cost > 0 {
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Fabric,
            out,
            costs.fabric_cost,
        ));
    }
    if costs.lumber_cost > 0 {
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Lumber,
            out,
            costs.lumber_cost,
        ));
    }
    if costs.steel_cost > 0 {
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Steel,
            out,
            costs.steel_cost,
        ));
    }
}

/// Clear `NavalOperation::Beachhead` assignments whose target is no longer
/// enemy-owned (or is now reachable overland, making the landing redundant).
///
/// Called every turn at the top of `ai_naval_strategy` so the decision can
/// rerun from a clean slate — including on turns when the AI is outmatched
/// at sea and returns early.
fn clear_stale_beachheads(game: &mut GameState, nation_id: NationId, enemies: &[NationId]) {
    let our_province_ids: Vec<ProvinceId> = game
        .get_nation(nation_id)
        .map(|n| n.province_ids.clone())
        .unwrap_or_default();
    let beachhead_targets: Vec<ProvinceId> = game
        .get_nation(nation_id)
        .map(|n| {
            n.military
                .warships
                .iter()
                .filter_map(|s| match s.operation {
                    Some(crate::military::naval::NavalOperation::Beachhead(pid)) => Some(pid),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if beachhead_targets.is_empty() {
        return;
    }
    let mut stale_targets: Vec<ProvinceId> = Vec::new();
    for target_pid in beachhead_targets {
        let Some(target_prov) = game.get_province(target_pid) else {
            stale_targets.push(target_pid);
            continue;
        };
        let still_hostile = enemies.contains(&target_prov.owner);
        let reachable_overland = our_province_ids.iter().any(|&our_pid| {
            game.get_province(our_pid).is_some_and(|our_p| {
                crate::map::provinces_are_adjacent(&game.world.hex_map, our_p, target_prov)
            })
        });
        if !still_hostile || reachable_overland {
            stale_targets.push(target_pid);
        }
    }
    if stale_targets.is_empty() {
        return;
    }
    if let Some(nation) = game.get_nation_mut(nation_id) {
        for ship in &mut nation.military.warships {
            if let Some(crate::military::naval::NavalOperation::Beachhead(pid)) = ship.operation
                && stale_targets.contains(&pid)
            {
                ship.operation = None;
            }
        }
    }
}

fn shortest_zone_path_to_any(
    game: &GameState,
    start: SeaZoneId,
    goals: &HashSet<SeaZoneId>,
) -> Option<Vec<SeaZoneId>> {
    if goals.contains(&start) {
        return Some(vec![start]);
    }

    let mut queue = VecDeque::from([start]);
    let mut previous: HashMap<SeaZoneId, Option<SeaZoneId>> = HashMap::from([(start, None)]);

    while let Some(current) = queue.pop_front() {
        let Some(zone) = game.world.sea_zones.iter().find(|z| z.id == current) else {
            continue;
        };
        for &next in &zone.adjacent_zone_ids {
            if previous.contains_key(&next) {
                continue;
            }
            previous.insert(next, Some(current));
            if goals.contains(&next) {
                let mut path = vec![next];
                let mut cursor = next;
                while let Some(Some(prev)) = previous.get(&cursor) {
                    path.push(*prev);
                    cursor = *prev;
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(next);
        }
    }

    None
}

fn advance_beachhead_fleets(game: &mut GameState, nation_id: NationId) {
    let target_pids: BTreeSet<ProvinceId> = game
        .get_nation(nation_id)
        .map(|nation| {
            nation
                .military
                .warships
                .iter()
                .filter_map(|ship| match ship.operation {
                    Some(NavalOperation::Beachhead(pid)) => Some(pid),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if target_pids.is_empty() {
        return;
    }

    let home_zone = find_nation_home_sea_zone(game, nation_id);

    for target_pid in target_pids {
        if let Some(home_zone) = home_zone
            && let Some(nation) = game.get_nation_mut(nation_id)
        {
            for ship in &mut nation.military.warships {
                if ship.operation == Some(NavalOperation::Beachhead(target_pid))
                    && ship.sea_zone.is_none()
                {
                    ship.sea_zone = Some(home_zone);
                }
            }
        }

        let Some(target_province) = game.get_province(target_pid) else {
            continue;
        };
        let target_zones: HashSet<SeaZoneId> =
            crate::map::sea_zones::ocean_zones_adjacent_to_province(
                &game.world.sea_zones,
                target_province,
                &game.world.hex_map,
            )
            .into_iter()
            .collect();
        if target_zones.is_empty() {
            continue;
        }

        loop {
            let source_zones: BTreeSet<SeaZoneId> = game
                .get_nation(nation_id)
                .map(|nation| {
                    nation
                        .military
                        .warships
                        .iter()
                        .filter(|ship| {
                            ship.operation == Some(NavalOperation::Beachhead(target_pid))
                        })
                        .filter_map(|ship| ship.sea_zone)
                        .filter(|zone| !target_zones.contains(zone))
                        .collect()
                })
                .unwrap_or_default();
            if source_zones.is_empty() {
                break;
            }

            let mut moved_any = false;
            for from_z in source_zones {
                let Some(path) = shortest_zone_path_to_any(game, from_z, &target_zones) else {
                    continue;
                };
                if path.len() < 2 {
                    continue;
                }
                let zone_is_dedicated_to_target = game
                    .get_nation(nation_id)
                    .map(|nation| {
                        nation
                            .military
                            .warships
                            .iter()
                            .filter(|ship| ship.sea_zone == Some(from_z))
                            .all(|ship| {
                                ship.operation == Some(NavalOperation::Beachhead(target_pid))
                            })
                    })
                    .unwrap_or(false);
                if !zone_is_dedicated_to_target {
                    continue;
                }
                let to_z = path[1];
                if move_warship_group_one_zone(game, nation_id, from_z, to_z) {
                    moved_any = true;
                }
            }

            if !moved_any {
                break;
            }
        }
    }
}

/// AI naval strategy: build warships when outmatched, plan blockades, evaluate
/// beachhead viability for coastal attacks.
///
/// - If at war and enemy has more naval firepower: try to build additional warships
/// - If at war and AI has naval superiority: report blockade capability
/// - Estimate enemy strength (provinces × 4 for garrison + known army size)
/// - Prefer coastal attack targets when AI has naval superiority
pub fn ai_naval_strategy(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<super::AiAction>,
) {
    let _personality = get_personality(game, nation_id);

    // ── Read Lua config (feature-gated) ──────────────────────
    let lua_cfg = super::lua_bridge::get_personality_config(game, _personality);
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let our_naval_fp = nation.total_naval_firepower(&game.game_data);
    let nation_name = nation.name.clone();

    if game.ai_debug {
        eprintln!(
            "[AI:{}:naval] warships={}, naval_fp={}",
            nation_name,
            nation.warship_count(),
            our_naval_fp
        );
    }

    // Find enemies we are at war with
    let enemies: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.id != nation_id)
        .filter(|n| {
            game.world
                .diplomacy
                .get_relation(nation_id, n.id)
                .map(|r| r.at_war)
                .unwrap_or(false)
        })
        .map(|n| n.id)
        .collect();

    // ── Clear stale Beachhead operations from previous turns ────────
    // Must run BEFORE both the peacetime `enemies.is_empty()` return AND
    // the "outmatched at sea" shipbuilding branch below; otherwise stale
    // ops persist indefinitely when the AI has no active war or is
    // rebuilding its fleet. When `enemies` is empty, every Beachhead is
    // stale by definition (target province has no hostile owner), so
    // `clear_stale_beachheads` will wipe them all.
    clear_stale_beachheads(game, nation_id, &enemies);

    if enemies.is_empty() {
        return;
    }

    // Calculate max enemy naval firepower
    let max_enemy_naval_fp: u32 = enemies
        .iter()
        .filter_map(|&eid| game.get_nation(eid))
        .map(|n| n.total_naval_firepower(&game.game_data))
        .max()
        .unwrap_or(0);

    // If enemy has more naval firepower: try to build another warship right
    // now on top of whatever the scored-spending rotation already did.
    if max_enemy_naval_fp > our_naval_fp && build_one_warship(game, nation_id) {
        actions.push(super::AiAction {
            text: format!(
                "{} is building warships to counter enemy naval superiority",
                nation_name
            ),
            reason: format!(
                "Enemy naval firepower {} vs our {}; building frigates to close the gap",
                max_enemy_naval_fp, our_naval_fp
            ),
            is_non_action: false,
            nation_id,
        });
        return; // Focus on shipbuilding when outmatched
    }

    // If AI has naval superiority, consider beachhead operations
    if our_naval_fp > 0 && our_naval_fp > max_enemy_naval_fp {
        // Blockade is applied automatically by the game engine.
        // Launch amphibious landings only when overland attack is not a
        // practical option — that is, when every land-adjacent enemy
        // province is defended more heavily than our field army can
        // overcome (card #7).

        // Load min army size for naval invasion from Lua config
        let min_army_for_invasion: usize = lua_cfg
            .as_ref()
            .and_then(|c| c.min_army_naval_invasion)
            .unwrap_or(4);
        // Lua-tunable "too hard" ratio: an adjacent enemy province counts as
        // a viable overland target if its defenders are <= army * ratio.
        let adj_strength_ratio: f64 = lua_cfg
            .as_ref()
            .and_then(|c| c.naval_min_adjacent_strength_ratio)
            .unwrap_or(1.5);
        // Only movable field-army units can embark for a naval invasion —
        // garrison militia are locked to their home province.
        let our_army_size = game
            .get_nation(nation_id)
            .map(|n| n.field_army_count())
            .unwrap_or(0);
        let our_province_ids: Vec<ProvinceId> = game
            .get_nation(nation_id)
            .map(|n| n.province_ids.clone())
            .unwrap_or_default();

        // Sea-zone adjacency: must own at least one coastal province to embark
        let we_have_coast = our_province_ids
            .iter()
            .any(|&pid| game.get_province(pid).is_some_and(|p| p.coastal));

        for &enemy_id in &enemies {
            if our_army_size < min_army_for_invasion || !we_have_coast {
                continue;
            }

            // Count enemy **field army** units stationed per province
            // (for the strength check). Militia / GarrisonArtillery are
            // excluded because `enemy_prov.garrison_count` is added
            // separately below — counting them here would double-count
            // defenders and over-trigger beachheads.
            let enemy_army_per_prov: Vec<(ProvinceId, usize)> = {
                let mut counts: Vec<(ProvinceId, usize)> = Vec::new();
                if let Some(en) = game.get_nation(enemy_id) {
                    for u in en.field_army_iter() {
                        if let Some(entry) = counts.iter_mut().find(|(p, _)| *p == u.position) {
                            entry.1 += 1;
                        } else {
                            counts.push((u.position, 1));
                        }
                    }
                }
                counts
            };

            // Gather land-adjacent enemy provinces and check how many of
            // them are "soft" (total defenders within our reach).
            let mut any_land_adjacent = false;
            let mut any_soft_land_target = false;
            let strength_cap = (our_army_size as f64 * adj_strength_ratio).ceil() as usize;
            for enemy_prov in game.world.provinces.iter().filter(|p| p.owner == enemy_id) {
                let is_land_adj = our_province_ids.iter().any(|&our_pid| {
                    game.get_province(our_pid).is_some_and(|our_prov| {
                        crate::map::provinces_are_adjacent(
                            &game.world.hex_map,
                            our_prov,
                            enemy_prov,
                        )
                    })
                });
                if !is_land_adj {
                    continue;
                }
                any_land_adjacent = true;

                let garrison = enemy_prov.garrison_count as usize;
                let stationed = enemy_army_per_prov
                    .iter()
                    .find(|(pid, _)| *pid == enemy_prov.id)
                    .map(|(_, c)| *c)
                    .unwrap_or(0);
                // Minor-nation capital has an extra GarrisonArtillery unit
                // that `field_army_iter()` skips; add it back explicitly.
                let artillery = game
                    .get_nation(enemy_id)
                    .map(|n| n.has_garrison_artillery_at(enemy_prov.id))
                    .unwrap_or(false) as usize;
                let defenders = garrison + stationed + artillery;
                if defenders <= strength_cap {
                    any_soft_land_target = true;
                    break;
                }
            }

            // Use naval invasion only when overland has either no reach
            // (every enemy prov is across water) OR every reachable prov
            // is defended more heavily than our field army can overcome.
            let need_naval = !any_land_adjacent || !any_soft_land_target;
            if !need_naval {
                continue;
            }

            // Find ocean-coastal enemy province to target (lake shores are excluded)
            let coastal_target = game
                .world
                .provinces
                .iter()
                .find(|p| p.owner == enemy_id && p.ocean_coastal);

            if let Some(target_prov) = coastal_target {
                // Assign warships to beachhead operation targeting the specific province
                let target_pid = target_prov.id;
                let target_prov_name = target_prov.name.clone();
                if let Some(nation) = game.get_nation_mut(nation_id) {
                    for ship in &mut nation.military.warships {
                        ship.operation = Some(crate::military::naval::NavalOperation::Beachhead(
                            target_pid,
                        ));
                    }
                }
                let reason_text = if !any_land_adjacent {
                    format!(
                        "Naval superiority ({} vs enemy {}) and no land-adjacent provinces; launching amphibious assault",
                        our_naval_fp, max_enemy_naval_fp
                    )
                } else {
                    format!(
                        "Naval superiority ({} vs enemy {}); every land-adjacent enemy province outstrips army size {} * {:.1}",
                        our_naval_fp, max_enemy_naval_fp, our_army_size, adj_strength_ratio
                    )
                };
                actions.push(super::AiAction {
                    text: format!(
                        "{} launches amphibious invasion targeting {}",
                        nation_name, target_prov_name
                    ),
                    reason: reason_text,
                    is_non_action: false,
                    nation_id,
                });

                if game.ai_debug {
                    eprintln!(
                        "[AI:{}:naval] Assigning warships to beachhead against {} (any_land_adj={}, any_soft={})",
                        nation_name, enemy_id.0, any_land_adjacent, any_soft_land_target,
                    );
                }
            }
        }
    }

    advance_beachhead_fleets(game, nation_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::{test_game_with_ai, test_game_with_ai_and_minor};
    use crate::map::UnitId;

    #[test]
    fn ai_builds_warship_with_arms() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Arms, 4);

        assert!(build_one_warship(&mut game, NationId(2)));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            1,
            "AI should build a warship when it has sufficient materials"
        );
    }

    #[test]
    fn ai_produces_arms_from_steel_for_warships() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Steel, 5);
        // No arms at all

        // With fabric=4 lumber=10 steel=5 the AI picks ShipOfTheLine
        // (fabric≥3, lumber≥8, effective_arms=5≥5, coal=0). It converts all
        // 5 steel → 5 arms to meet ShipOfTheLine's arms_cost=5.
        assert!(build_one_warship(&mut game, NationId(2)));
        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.warship_count(),
            1,
            "AI should produce arms from steel and build a warship"
        );
        // All 5 steel consumed (converted to arms for ShipOfTheLine).
        assert_eq!(ai.material_amount(MaterialType::Steel), 0);
    }

    #[test]
    fn ai_does_not_build_warship_without_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        // No materials at all

        assert!(!build_one_warship(&mut game, NationId(2)));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            0,
            "AI should not build warships without materials"
        );
    }

    #[test]
    fn warship_builds_unbounded_while_materials_last() {
        // Card #112: there is no hard cap. Given sufficient materials for
        // exactly 5 ShipsOfTheLine (fabric=3, lumber=8, arms=5 each),
        // `build_one_warship` should keep building until materials run out.
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.economy.treasury = Money::dollars(5_000);
        ai.add_material(MaterialType::Fabric, 15); // 5 × 3
        ai.add_material(MaterialType::Lumber, 40); // 5 × 8
        ai.add_material(MaterialType::Arms, 25); // 5 × 5

        for _ in 0..5 {
            assert!(build_one_warship(&mut game, NationId(2)));
        }
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            5,
            "Warships should build as long as materials are available"
        );
    }

    #[test]
    fn ai_produces_partial_arms_from_steel() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Arms, 1); // have 1, need 2
        ai.add_material(MaterialType::Steel, 1); // can produce 1 more

        assert!(build_one_warship(&mut game, NationId(2)));
        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.warship_count(),
            1,
            "AI should produce 1 arms from steel to supplement existing 1 arms"
        );
        assert_eq!(ai.material_amount(MaterialType::Steel), 0);
    }

    #[test]
    fn ai_does_not_produce_arms_when_no_steel() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        // No arms and no steel

        assert!(!build_one_warship(&mut game, NationId(2)));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            0,
            "AI should not build warship without arms or steel"
        );
    }

    #[test]
    fn economic_ai_builds_merchant_ships() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Economic);
        ai.economy.treasury = Money::dollars(3_000); // below $5K threshold: cap is 3
        ai.add_material(MaterialType::Fabric, 10);
        ai.add_material(MaterialType::Lumber, 20);

        // Build ships up to 3 for Economic personality
        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            1,
            "Should build 1 ship per call"
        );

        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            2,
        );

        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            3,
        );

        // Should not build more than 3
        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            3,
            "Economic AI should cap at 3 ships"
        );
    }

    #[test]
    fn balanced_ai_only_builds_one_merchant_ship() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.economy.treasury = Money::dollars(3_000); // below $5K threshold: cap is 1
        ai.add_material(MaterialType::Fabric, 10);
        ai.add_material(MaterialType::Lumber, 20);

        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            1,
        );

        // Should not build more (has cargo capacity > 0)
        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            1,
            "Balanced AI should only build 1 ship (has cargo capacity)"
        );
    }

    #[test]
    fn ai_naval_strategy_builds_ships_when_outmatched() {
        let mut game = test_game_with_ai_and_minor();

        // Put AI at war with minor nation
        game.world.diplomacy.declare_war(NationId(2), NationId(3));

        // Give the minor nation 2 warships (more than AI's 0)
        let minor = game.get_nation_mut(NationId(3)).unwrap();
        minor
            .military
            .warships
            .push(Ship::new(UnitId(50001), ShipType::Frigate, NationId(3), 35));
        minor
            .military
            .warships
            .push(Ship::new(UnitId(50002), ShipType::Frigate, NationId(3), 35));

        // Give AI materials to build a warship (2 fabric + 5 lumber + 2 arms)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Arms, 4);
        // Verify AI has no warships initially
        assert_eq!(ai.warship_count(), 0);

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.warship_count(),
            1,
            "AI should build a warship when outmatched at sea"
        );
        assert!(
            actions
                .iter()
                .any(|a| a.text.contains("warships") || a.text.contains("naval")),
            "Should report shipbuilding action"
        );
    }

    #[test]
    fn ai_does_not_launch_beachhead_when_soft_overland_target_exists() {
        // AI shares a land border with a weakly-defended enemy province.
        // Even with naval superiority, it should not set Beachhead — the
        // overland attack is preferable.
        use crate::ai::common::test_helpers::test_game_with_adjacent_provinces;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let mut game = test_game_with_adjacent_provinces();
        // Mark the AI's border province coastal so "we_have_coast" is true.
        game.world.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(2) {
                p.coastal = true;
                p.ocean_coastal = true;
            }
        });
        // Make the enemy province ocean-coastal too, so it would be a viable beachhead.
        game.world.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(3) {
                p.coastal = true;
                p.ocean_coastal = true;
            }
        });

        // Give AI 5 army units and 3 warships (naval superiority).
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        for i in 0..5 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(9100 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..3 {
            ai.military.warships.push(Ship::new(
                UnitId(9200 + i),
                ShipType::Frigate,
                NationId(2),
                35,
            ));
        }
        // Enemy has no warships and a small garrison (garrison_count=3 by default).

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.military.warships.iter().all(|s| !matches!(
                s.operation,
                Some(crate::military::naval::NavalOperation::Beachhead(_))
            )),
            "AI should not assign Beachhead when a soft overland target is available"
        );
        assert!(
            actions
                .iter()
                .all(|a| !a.text.contains("amphibious invasion")),
            "AI should not announce amphibious invasion"
        );
    }

    #[test]
    fn ai_launches_beachhead_when_all_adjacent_too_hard() {
        // AI has a small army, naval superiority, and a coastal enemy
        // province. The only land-adjacent enemy province is heavily
        // defended — bigger than army * naval_min_adjacent_strength_ratio.
        // Expect: Beachhead assigned against a coastal target.
        use crate::ai::common::test_helpers::test_game_with_adjacent_provinces;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let mut game = test_game_with_adjacent_provinces();
        // Make the AI's border province coastal (required for embark).
        game.world.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(2) {
                p.coastal = true;
                p.ocean_coastal = true;
            }
            if p.id == ProvinceId(3) {
                // The land-adjacent enemy province is ALSO ocean-coastal — that
                // keeps it as the beachhead candidate, and we over-garrison
                // it so it fails the strength check.
                p.coastal = true;
                p.ocean_coastal = true;
                p.garrison_count = 20;
            }
        });

        // AI: small army (5), naval superiority.
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        for i in 0..5 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(9500 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..3 {
            ai.military.warships.push(Ship::new(
                UnitId(9600 + i),
                ShipType::Frigate,
                NationId(2),
                35,
            ));
        }
        // Enemy stacked with a fat garrison already (20). Attacker army=5,
        // ratio 1.5 → cap = 8; defenders (20) > 8 → too hard.

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.military.warships.iter().any(|s| matches!(
                s.operation,
                Some(crate::military::naval::NavalOperation::Beachhead(_))
            )),
            "AI should assign Beachhead when every land-adjacent target is too heavily defended"
        );
        assert!(
            actions
                .iter()
                .any(|a| a.text.contains("amphibious invasion")),
            "AI should announce amphibious invasion"
        );
    }

    #[test]
    fn ai_clears_stale_beachhead_in_peacetime() {
        // Regression for F-008: when the nation has no active wars, a
        // leftover Beachhead op from a prior war must still be cleared.
        use crate::ai::common::test_helpers::test_game_with_adjacent_provinces;
        let mut game = test_game_with_adjacent_provinces();
        // End the war seeded by the test helper.
        game.world.diplomacy.make_peace(NationId(2), NationId(3));

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        let mut stale_ship = Ship::new(UnitId(9800), ShipType::Frigate, NationId(2), 35);
        stale_ship.operation = Some(crate::military::naval::NavalOperation::Beachhead(
            ProvinceId(3),
        ));
        ai.military.warships.push(stale_ship);

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.military.warships.iter().all(|s| !matches!(
                s.operation,
                Some(crate::military::naval::NavalOperation::Beachhead(_))
            )),
            "peacetime stale Beachhead ops must be cleared"
        );
    }

    #[test]
    fn ai_clears_stale_beachhead_when_outmatched_at_sea() {
        // Regression for F-001: stale clearing must run even when the AI
        // returns early to build more ships.
        use crate::ai::common::test_helpers::test_game_with_adjacent_provinces;
        let mut game = test_game_with_adjacent_provinces();
        game.world.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(2) {
                p.coastal = true;
                p.ocean_coastal = true;
            }
        });

        // AI has a stale Beachhead op and zero warship firepower.
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        let mut stale_ship = Ship::new(UnitId(9700), ShipType::Frigate, NationId(2), 35);
        stale_ship.operation = Some(crate::military::naval::NavalOperation::Beachhead(
            ProvinceId(3),
        ));
        ai.military.warships.push(stale_ship);

        // Give enemy several strong warships so max_enemy_naval_fp > our_naval_fp.
        let enemy = game.get_nation_mut(NationId(3)).unwrap();
        for i in 0..3 {
            enemy.military.warships.push(Ship::new(
                UnitId(9700 + 100 + i),
                ShipType::ShipOfTheLine,
                NationId(3),
                65,
            ));
        }

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.military.warships.iter().all(|s| !matches!(
                s.operation,
                Some(crate::military::naval::NavalOperation::Beachhead(_))
            )),
            "Stale Beachhead must be cleared even on outmatched-at-sea turns"
        );
    }

    #[test]
    fn ai_clears_stale_beachhead_when_target_becomes_land_adjacent() {
        // Previous turn queued Beachhead on prov 3. This turn prov 3 is
        // land-adjacent to our territory — the op should be cleared.
        use crate::ai::common::test_helpers::test_game_with_adjacent_provinces;
        let mut game = test_game_with_adjacent_provinces();
        game.world.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(2) {
                p.coastal = true;
                p.ocean_coastal = true;
            }
        });

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        let mut stale_ship = Ship::new(UnitId(9300), ShipType::Frigate, NationId(2), 35);
        stale_ship.operation = Some(crate::military::naval::NavalOperation::Beachhead(
            ProvinceId(3),
        ));
        ai.military.warships.push(stale_ship);

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.military.warships.iter().all(|s| !matches!(
                s.operation,
                Some(crate::military::naval::NavalOperation::Beachhead(_))
            )),
            "stale Beachhead against a now-reachable target must be cleared"
        );
    }

    #[test]
    fn ai_naval_strategy_does_nothing_when_not_at_war() {
        let mut game = test_game_with_ai();
        // Not at war — naval strategy should do nothing
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.add_material(MaterialType::Fabric, 10);
        ai.add_material(MaterialType::Lumber, 20);
        ai.add_material(MaterialType::Arms, 10);

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        assert!(
            actions.is_empty(),
            "Naval strategy should do nothing when not at war"
        );
    }

    // ── Merchant-navy demand-based growth tests (card #469) ─────────

    #[test]
    fn wants_more_merchant_cargo_false_with_no_chains() {
        // Test nation has no buildings → no projected demand → trigger off,
        // even with zero merchant ships.
        let game = test_game_with_ai();
        assert!(!wants_more_merchant_cargo(&game, NationId(2)));
    }

    #[test]
    fn wants_more_merchant_cargo_true_when_demand_exceeds_capacity() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Add a sized lumber mill so projected_resource_needs returns a
        // non-zero Timber demand that exceeds zero cargo capacity.
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 5));
        assert!(wants_more_merchant_cargo(&game, NationId(2)));
    }

    #[test]
    fn merchant_navy_material_reserve_zero_when_no_demand() {
        let game = test_game_with_ai();
        assert_eq!(
            merchant_navy_material_reserve(&game, NationId(2)),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn merchant_navy_material_reserve_matches_best_tech_free_hull() {
        // Trader and Indiaman both have prerequisite_tech = nil. Indiaman
        // has more cargo (4 vs 2) and ranks first in MERCHANT_PRIORITY,
        // so the reserve == Indiaman cost (3 fabric, 7 lumber, 0 steel, 0 coal).
        use crate::economy::buildings::{Building, BuildingType};
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 5));
        let (fabric, lumber, steel, coal) = merchant_navy_material_reserve(&game, NationId(2));
        assert_eq!((fabric, lumber, steel, coal), (3, 7, 0, 0));
    }

    #[test]
    fn ai_build_merchant_ships_grows_past_static_cap_when_demand_bound() {
        // Balanced personality static cap is 1 below $5K treasury — but
        // when projected demand exceeds cargo capacity, the AI should
        // build past that cap.
        use crate::economy::buildings::{Building, BuildingType};
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.economy.treasury = Money::dollars(3_000); // below $5K threshold
        // Big LumberMill → high projected Timber demand → cargo-bound.
        ai.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 50));
        ai.add_material(MaterialType::Fabric, 20);
        ai.add_material(MaterialType::Lumber, 40);

        // First build creates the first ship.
        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            1,
        );
        // Second build still proceeds even though the static Balanced cap
        // is 1 — because demand still exceeds new capacity.
        ai_build_merchant_ships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().merchant_ship_count(),
            2,
            "demand-bound AI should keep building past static cap"
        );
    }
}
