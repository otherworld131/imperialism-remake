use crate::game_state::GameState;
use crate::military::ships::{Ship, ShipType};
use crate::types::*;

use super::common::{AiPersonality, get_personality, next_unit_id};

/// AI builds warships if it has fewer than the threshold and has the required materials.
///
/// - If AI has < 2 warships and has fabric + lumber + arms materials, build a Frigate.
/// - Aggressive AI builds up to 4 warships.
/// - If AI has steel but no arms, it produces arms from steel first.
pub(crate) fn ai_build_warships(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Wealthy nations invest in larger navies
    let max_warships: usize = if nation.treasury > Money::dollars(8_000) {
        match personality {
            AiPersonality::Aggressive => 6,
            _ => 4,
        }
    } else {
        match personality {
            AiPersonality::Aggressive => 4,
            _ => 2,
        }
    };

    if nation.warship_count() >= max_warships {
        return;
    }

    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);
    let arms_have = nation.material_amount(MaterialType::Arms);
    let steel_have = nation.material_amount(MaterialType::Steel);

    // If we have the fabric and lumber but need arms, produce arms from steel
    if fabric_have >= 2 && lumber_have >= 5 && arms_have < 2 && steel_have > 0 {
        let arms_needed = 2 - arms_have;
        let arms_to_produce = arms_needed.min(steel_have);
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_material(MaterialType::Steel, arms_to_produce);
        nation.add_material(MaterialType::Arms, arms_to_produce);
    }

    // Re-check after possible arms production
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    let fabric_have = nation.material_amount(MaterialType::Fabric);
    let lumber_have = nation.material_amount(MaterialType::Lumber);
    let arms_have = nation.material_amount(MaterialType::Arms);

    // Try to build a Frigate: 2 fabric + 5 lumber + 2 arms
    if fabric_have >= 2 && lumber_have >= 5 && arms_have >= 2 {
        let uid = next_unit_id();
        let ship = Ship::new(uid, ShipType::Frigate, nation_id);
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_material(MaterialType::Fabric, 2);
        nation.consume_material(MaterialType::Lumber, 5);
        nation.consume_material(MaterialType::Arms, 2);
        nation.warships.push(ship);
    }
}

pub(crate) fn ai_build_merchant_ships(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let treasury = nation.treasury;

    // Ship cap depends on personality; wealthy nations always aim for 5
    let max_ships: usize = if treasury > Money::dollars(5_000) {
        5
    } else {
        match personality {
            AiPersonality::Economic => 3,
            _ => 1,
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
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_material(MaterialType::Fabric, 2);
        nation.consume_material(MaterialType::Lumber, 4);
        nation.merchant_fleet.push(ship);
    }
}

/// AI naval strategy: build warships when outmatched, plan blockades, evaluate
/// beachhead viability for coastal attacks.
///
/// - If at war and enemy has more naval firepower: try to build additional warships
/// - If at war and AI has naval superiority: report blockade capability
/// - Estimate enemy strength (provinces × 4 for garrison + known army size)
/// - Prefer coastal attack targets when AI has naval superiority
pub fn ai_naval_strategy(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
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

    // If enemy has more naval firepower: try to build more warships
    if max_enemy_naval_fp > our_naval_fp {
        // Build additional warships beyond normal cap
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };

        let fabric_have = nation.material_amount(MaterialType::Fabric);
        let lumber_have = nation.material_amount(MaterialType::Lumber);
        let arms_have = nation.material_amount(MaterialType::Arms);
        let steel_have = nation.material_amount(MaterialType::Steel);

        // Try producing arms from steel if needed
        if fabric_have >= 2 && lumber_have >= 5 && arms_have < 2 && steel_have > 0 {
            let arms_needed = 2 - arms_have;
            let arms_to_produce = arms_needed.min(steel_have);
            let nation = game.get_nation_mut(nation_id).unwrap();
            nation.consume_material(MaterialType::Steel, arms_to_produce);
            nation.add_material(MaterialType::Arms, arms_to_produce);
        }

        // Re-check after possible arms production
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };
        let fabric_have = nation.material_amount(MaterialType::Fabric);
        let lumber_have = nation.material_amount(MaterialType::Lumber);
        let arms_have = nation.material_amount(MaterialType::Arms);

        if fabric_have >= 2 && lumber_have >= 5 && arms_have >= 2 {
            let uid = next_unit_id();
            let ship = Ship::new(uid, ShipType::Frigate, nation_id);
            let nation = game.get_nation_mut(nation_id).unwrap();
            nation.consume_material(MaterialType::Fabric, 2);
            nation.consume_material(MaterialType::Lumber, 5);
            nation.consume_material(MaterialType::Arms, 2);
            nation.warships.push(ship);
            actions.push(format!(
                "{} is building warships to counter enemy naval superiority",
                nation_name
            ));
        }
        return; // Focus on shipbuilding when outmatched
    }

    // If AI has naval superiority, announce blockade capability
    if our_naval_fp > 0 && our_naval_fp > max_enemy_naval_fp {
        // Blockade is applied automatically by the game engine.
        // AI reconnaissance: estimate enemy forces
        for &enemy_id in &enemies {
            let enemy_provinces = game
                .provinces
                .iter()
                .filter(|p| p.owner == enemy_id)
                .count();
            let enemy_army_size = game.get_nation(enemy_id).map(|n| n.army.len()).unwrap_or(0);
            let estimated_enemy_strength = enemy_provinces * 4 + enemy_army_size;

            // If AI has army superiority and naval superiority, prefer coastal targets
            let our_army_size = game
                .get_nation(nation_id)
                .map(|n| n.army.len())
                .unwrap_or(0);

            if our_army_size >= 4 && our_army_size > estimated_enemy_strength / 2 {
                // Look for coastal enemy provinces to prioritize in attacks
                // (The actual attack queueing happens in ai_military_strategy;
                // this just adds a headline for the report)
                let enemy_has_coastal = game
                    .provinces
                    .iter()
                    .any(|p| p.owner == enemy_id && p.is_coastal());

                if enemy_has_coastal {
                    actions.push(format!(
                        "{} is preparing amphibious operations against the enemy coast",
                        nation_name
                    ));
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

        ai_build_warships(&mut game, NationId(2));
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

        ai_build_warships(&mut game, NationId(2));
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

        ai_build_warships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            0,
            "AI should not build warships without materials"
        );
    }

    #[test]
    fn aggressive_ai_builds_up_to_four_warships() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Aggressive);
        ai.treasury = Money::dollars(5_000); // below $8K threshold: cap is 4
        ai.add_material(MaterialType::Fabric, 20);
        ai.add_material(MaterialType::Lumber, 40);
        ai.add_material(MaterialType::Arms, 20);

        for _ in 0..4 {
            ai_build_warships(&mut game, NationId(2));
        }
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            4,
            "Aggressive AI should build up to 4 warships"
        );

        // Should not build a 5th
        ai_build_warships(&mut game, NationId(2));
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            4,
            "Aggressive AI should cap at 4 warships"
        );
    }

    #[test]
    fn balanced_ai_caps_at_two_warships() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.treasury = Money::dollars(5_000); // below $8K threshold: cap is 2
        ai.add_material(MaterialType::Fabric, 20);
        ai.add_material(MaterialType::Lumber, 40);
        ai.add_material(MaterialType::Arms, 20);

        for _ in 0..3 {
            ai_build_warships(&mut game, NationId(2));
        }
        assert_eq!(
            game.get_nation(NationId(2)).unwrap().warship_count(),
            2,
            "Balanced AI should cap at 2 warships"
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

        ai_build_warships(&mut game, NationId(2));
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

        ai_build_warships(&mut game, NationId(2));
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
                .any(|a| a.contains("warships") || a.contains("naval")),
            "Should report shipbuilding action"
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
