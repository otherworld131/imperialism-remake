#![allow(unused_labels)]
use crate::game_state::GameState;
use crate::military::ships::{Ship, ShipType};
use crate::types::*;

use super::common::{AiPersonality, get_personality, next_unit_id};

/// Try to build one Frigate for `nation_id`. Returns `true` if a ship was
/// added to the nation's warships. No cap check — the caller decides when
/// to invoke this (e.g. the scored-spending rotation in
/// `ai_scored_spending`, or the "outmatched at sea" branch in
/// `ai_naval_strategy`). If fabric/lumber are sufficient but arms are
/// short and steel is available, converts steel → arms first.
///
/// Trello card #112: the hard warship caps were removed. Warship growth
/// is now driven by the scored-spending alternation (backlog climbs each
/// turn navy is skipped) and gated by material availability only.
pub(crate) fn build_one_warship(game: &mut GameState, nation_id: NationId) -> bool {
    let costs = ShipType::Frigate.stats();
    let fabric_need = costs.fabric_cost;
    let lumber_need = costs.lumber_cost;
    let arms_need = costs.arms_cost;

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return false,
    };

    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);
    let arms_have = nation.material_amount(MaterialType::Arms);
    let steel_have = nation.material_amount(MaterialType::Steel);

    // If we have the fabric and lumber but need arms, produce arms from steel.
    if fabric_have >= fabric_need
        && lumber_have >= lumber_need
        && arms_have < arms_need
        && steel_have > 0
    {
        let arms_needed = arms_need - arms_have;
        let arms_to_produce = arms_needed.min(steel_have);
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return false;
        };
        nation.consume_material(MaterialType::Steel, arms_to_produce);
        nation.add_material(MaterialType::Arms, arms_to_produce);
    }

    // Re-check after possible arms production.
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return false,
    };
    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);
    let arms_have = nation.material_amount(MaterialType::Arms);

    if fabric_have >= fabric_need && lumber_have >= lumber_need && arms_have >= arms_need {
        let uid = next_unit_id();
        let ship = Ship::new(uid, ShipType::Frigate, nation_id);
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return false;
        };
        nation.consume_material(MaterialType::Fabric, fabric_need);
        nation.consume_material(MaterialType::Lumber, lumber_need);
        nation.consume_material(MaterialType::Arms, arms_need);
        nation.warships.push(ship);
        nation.warships_built += 1;
        return true;
    }
    false
}

/// True if `nation_id` has the raw materials on hand (or can produce the
/// arms from steel) to build one Frigate right now. Used by the
/// scored-spending system to gate the Warship category.
pub(crate) fn can_build_warship(game: &GameState, nation_id: NationId) -> bool {
    let Some(nation) = game.get_nation(nation_id) else {
        return false;
    };
    let costs = ShipType::Frigate.stats();
    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);
    let arms_have = nation.material_amount(MaterialType::Arms);
    let steel_have = nation.material_amount(MaterialType::Steel);
    if fabric_have < costs.fabric_cost || lumber_have < costs.lumber_cost {
        return false;
    }
    arms_have >= costs.arms_cost || (arms_have + steel_have) >= costs.arms_cost
}

pub(crate) fn ai_build_merchant_ships(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let treasury = nation.treasury;

    // ── Read Lua config (feature-gated) ──────────────────────
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
    #[cfg(not(feature = "lua"))]
    let _lua_cfg: Option<()> = None;

    // Ship cap depends on personality; wealthy nations always aim for 5
    let max_ships: usize = if treasury > Money::dollars(5_000) {
        5
    } else {
        'val: {
            #[cfg(feature = "lua")]
            if let Some(v) = lua_cfg.as_ref().and_then(|c| c.max_merchant_ships) {
                break 'val v;
            }
            match personality {
                AiPersonality::Economic => 3,
                _ => 1,
            }
        }
    };

    // For non-Economic with low treasury, only build if cargo capacity is 0
    if personality != AiPersonality::Economic
        && treasury <= Money::dollars(5_000)
        && nation.total_cargo_capacity() > 0
    {
        return;
    }

    if nation.merchant_ship_count() >= max_ships {
        return;
    }

    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);

    // Try to build Trader (2 fabric + 4 lumber)
    if fabric_have >= 2 && lumber_have >= 4 {
        let uid = next_unit_id();
        let ship = Ship::new(uid, ShipType::Trader, nation_id);
        let Some(nation) = game.get_nation_mut(nation_id) else {
            return;
        };
        nation.consume_material(MaterialType::Fabric, 2);
        nation.consume_material(MaterialType::Lumber, 4);
        nation.merchant_fleet.push(ship);
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
            n.warships
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
                crate::map::provinces_are_adjacent(&game.hex_map, our_p, target_prov)
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
        for ship in &mut nation.warships {
            if let Some(crate::military::naval::NavalOperation::Beachhead(pid)) = ship.operation
                && stale_targets.contains(&pid)
            {
                ship.operation = None;
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
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, _personality));
    #[cfg(not(feature = "lua"))]
    let _lua_cfg: Option<()> = None;

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let our_naval_fp = nation.total_naval_firepower();
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
        .nations
        .iter()
        .filter(|n| n.id != nation_id)
        .filter(|n| {
            game.diplomacy
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
        .map(|n| n.total_naval_firepower())
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
        #[cfg(feature = "lua")]
        let min_army_for_invasion: usize = lua_cfg
            .as_ref()
            .and_then(|c| c.min_army_naval_invasion)
            .unwrap_or(4);
        #[cfg(not(feature = "lua"))]
        let min_army_for_invasion: usize = 4;

        // Lua-tunable "too hard" ratio: an adjacent enemy province counts as
        // a viable overland target if its defenders are <= army * ratio.
        #[cfg(feature = "lua")]
        let adj_strength_ratio: f64 = lua_cfg
            .as_ref()
            .and_then(|c| c.naval_min_adjacent_strength_ratio)
            .unwrap_or(1.5);
        #[cfg(not(feature = "lua"))]
        let adj_strength_ratio: f64 = 1.5;

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
            for enemy_prov in game.provinces.iter().filter(|p| p.owner == enemy_id) {
                let is_land_adj = our_province_ids.iter().any(|&our_pid| {
                    game.get_province(our_pid).is_some_and(|our_prov| {
                        crate::map::provinces_are_adjacent(&game.hex_map, our_prov, enemy_prov)
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

            // Find coastal enemy province to target
            let coastal_target = game
                .provinces
                .iter()
                .find(|p| p.owner == enemy_id && p.coastal);

            if let Some(target_prov) = coastal_target {
                // Assign warships to beachhead operation targeting the specific province
                let target_pid = target_prov.id;
                let target_prov_name = target_prov.name.clone();
                if let Some(nation) = game.get_nation_mut(nation_id) {
                    for ship in &mut nation.warships {
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
        ai.ai_personality = Some(AiPersonality::Balanced);
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
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.add_material(MaterialType::Fabric, 4);
        ai.add_material(MaterialType::Lumber, 10);
        ai.add_material(MaterialType::Steel, 5);
        // No arms at all

        assert!(build_one_warship(&mut game, NationId(2)));
        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.warship_count(),
            1,
            "AI should produce arms from steel and build a warship"
        );
        // Steel should be consumed: 2 for arms production
        assert_eq!(ai.material_amount(MaterialType::Steel), 3);
    }

    #[test]
    fn ai_does_not_build_warship_without_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
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
        // Card #112: there is no hard cap. Given sufficient materials,
        // `build_one_warship` should keep producing Frigates.
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.treasury = Money::dollars(5_000);
        ai.add_material(MaterialType::Fabric, 20);
        ai.add_material(MaterialType::Lumber, 40);
        ai.add_material(MaterialType::Arms, 20);

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
        ai.ai_personality = Some(AiPersonality::Balanced);
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
        ai.ai_personality = Some(AiPersonality::Balanced);
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
        ai.ai_personality = Some(AiPersonality::Economic);
        ai.treasury = Money::dollars(3_000); // below $5K threshold: cap is 3
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
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.treasury = Money::dollars(3_000); // below $5K threshold: cap is 1
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
        game.diplomacy.declare_war(NationId(2), NationId(3));

        // Give the minor nation 2 warships (more than AI's 0)
        let minor = game.get_nation_mut(NationId(3)).unwrap();
        minor
            .warships
            .push(Ship::new(UnitId(50001), ShipType::Frigate, NationId(3)));
        minor
            .warships
            .push(Ship::new(UnitId(50002), ShipType::Frigate, NationId(3)));

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
        game.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(2) {
                p.coastal = true;
            }
        });
        // Make the enemy province coastal too, so it would be a viable beachhead.
        game.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(3) {
                p.coastal = true;
            }
        });

        // Give AI 5 army units and 3 warships (naval superiority).
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(9100 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..3 {
            ai.warships
                .push(Ship::new(UnitId(9200 + i), ShipType::Frigate, NationId(2)));
        }
        // Enemy has no warships and a small garrison (garrison_count=3 by default).

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.warships.iter().all(|s| !matches!(
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
        game.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(2) {
                p.coastal = true;
            }
            if p.id == ProvinceId(3) {
                // The land-adjacent enemy province is ALSO coastal — that
                // keeps it as the beachhead candidate, and we over-garrison
                // it so it fails the strength check.
                p.coastal = true;
                p.garrison_count = 20;
            }
        });

        // AI: small army (5), naval superiority.
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(9500 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..3 {
            ai.warships
                .push(Ship::new(UnitId(9600 + i), ShipType::Frigate, NationId(2)));
        }
        // Enemy stacked with a fat garrison already (20). Attacker army=5,
        // ratio 1.5 → cap = 8; defenders (20) > 8 → too hard.

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.warships.iter().any(|s| matches!(
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
        game.diplomacy.make_peace(NationId(2), NationId(3));

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        let mut stale_ship = Ship::new(UnitId(9800), ShipType::Frigate, NationId(2));
        stale_ship.operation = Some(crate::military::naval::NavalOperation::Beachhead(
            ProvinceId(3),
        ));
        ai.warships.push(stale_ship);

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.warships.iter().all(|s| !matches!(
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
        game.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(2) {
                p.coastal = true;
            }
        });

        // AI has a stale Beachhead op and zero warship firepower.
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        let mut stale_ship = Ship::new(UnitId(9700), ShipType::Frigate, NationId(2));
        stale_ship.operation = Some(crate::military::naval::NavalOperation::Beachhead(
            ProvinceId(3),
        ));
        ai.warships.push(stale_ship);

        // Give enemy several strong warships so max_enemy_naval_fp > our_naval_fp.
        let enemy = game.get_nation_mut(NationId(3)).unwrap();
        for i in 0..3 {
            enemy.warships.push(Ship::new(
                UnitId(9700 + 100 + i),
                ShipType::ShipOfTheLine,
                NationId(3),
            ));
        }

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.warships.iter().all(|s| !matches!(
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
        game.provinces.iter_mut().for_each(|p| {
            if p.id == ProvinceId(2) {
                p.coastal = true;
            }
        });

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        let mut stale_ship = Ship::new(UnitId(9300), ShipType::Frigate, NationId(2));
        stale_ship.operation = Some(crate::military::naval::NavalOperation::Beachhead(
            ProvinceId(3),
        ));
        ai.warships.push(stale_ship);

        let mut actions = Vec::new();
        ai_naval_strategy(&mut game, NationId(2), &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.warships.iter().all(|s| !matches!(
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
}
