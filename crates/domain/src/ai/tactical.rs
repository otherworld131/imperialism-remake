use crate::game_state::GameState;
use crate::types::*;

use super::common::{AiPersonality, get_personality};

/// AI tactical combat decisions: build forts, move units to threatened provinces,
/// and propose peace after prolonged losing wars.
///
/// - **Fort building**: If treasury > $5,000 and a border province exists (adjacent
///   to an enemy-owned province), build a fort on the capital tile of that province.
///   Aggressive AI builds forts on offensive staging provinces. Diplomatic AI builds
///   forts on the capital for defense.
///
/// - **Move units to threatened provinces**: If a province borders an enemy and has
///   no stationed army units, move one unit there from the capital.
///
/// - **Retreat from losing wars**: If at war for 20+ turns and has lost provinces
///   (owns fewer than started with), propose peace. Diplomatic AI: 10 turns.
///   Aggressive AI: 30 turns.
pub fn ai_tactical_decisions(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let personality = get_personality(game, nation_id);

    if game.ai_debug {
        let nation_name = game
            .get_nation(nation_id)
            .map(|n| n.name.as_str())
            .unwrap_or("?");
        let army_size = game
            .get_nation(nation_id)
            .map(|n| n.army.len())
            .unwrap_or(0);
        eprintln!(
            "[AI:{}:tactical] army={}, personality={}",
            nation_name, army_size, personality
        );
    }

    // Phase 1: Build forts on border provinces
    ai_build_forts(game, nation_id, personality, actions);

    // Phase 2: Move units to threatened (undefended border) provinces
    ai_move_units_to_threatened(game, nation_id);

    // Phase 3: Propose peace after prolonged losing war
    ai_propose_peace(game, nation_id, personality, actions);
}

/// Build a fort on a border province's capital tile if the AI can afford it.
///
/// A "border province" is one that has tiles adjacent to tiles belonging to a
/// province owned by a nation the AI is at war with.
///
/// - Aggressive AI: picks the province closest to the enemy (offensive staging)
/// - Diplomatic AI: always forts the national capital
/// - Others: pick the first border province found
fn ai_build_forts(
    game: &mut GameState,
    nation_id: NationId,
    personality: AiPersonality,
    actions: &mut Vec<String>,
) {
    use crate::map::infrastructure::build_fort;

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Need treasury > $5,000 to build a fort (level 1 costs $5,000)
    if nation.treasury <= Money::dollars(5000) {
        return;
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

    let nation = game.get_nation(nation_id).unwrap();
    let capital_province_id = nation.capital_province_id;
    let nation_name = nation.name.clone();
    let owned_provinces: Vec<ProvinceId> = nation.province_ids.clone();

    // Collect enemy-owned tiles for adjacency check
    let enemy_province_ids: Vec<ProvinceId> = game
        .provinces
        .iter()
        .filter(|p| enemies.contains(&p.owner))
        .map(|p| p.id)
        .collect();

    // Find which of our provinces border enemy territory
    let mut border_provinces: Vec<ProvinceId> = Vec::new();
    for &pid in &owned_provinces {
        if let Some(prov) = game.get_province(pid) {
            let is_border = prov.tiles.iter().any(|&tile_coord| {
                tile_coord.neighbors().iter().any(|neighbor| {
                    game.hex_map
                        .get_tile(*neighbor)
                        .and_then(|t| t.province_id)
                        .is_some_and(|npid| enemy_province_ids.contains(&npid))
                })
            });
            if is_border {
                border_provinces.push(pid);
            }
        }
    }

    if border_provinces.is_empty() {
        return;
    }

    // Choose which province to fort based on personality
    let target_province = match personality {
        AiPersonality::Diplomatic => {
            // Fort the capital for defense
            if owned_provinces.contains(&capital_province_id) {
                capital_province_id
            } else {
                border_provinces[0]
            }
        }
        AiPersonality::Aggressive => {
            // Fort the border province (offensive staging)
            border_provinces[0]
        }
        _ => {
            // Default: first border province
            border_provinces[0]
        }
    };

    // Get the capital tile of that province
    let fort_coord = match game.get_province(target_province) {
        Some(p) => p.capital_tile,
        None => return,
    };

    // Check if there's already a fort at max level
    let current_level = game
        .hex_map
        .get_tile(fort_coord)
        .map(|t| t.infrastructure.fort_level)
        .unwrap_or(0);
    if current_level >= 3 {
        return;
    }

    // Build the fort
    let new_level = current_level + 1;
    let cost = match crate::map::infrastructure::fort_cost(new_level) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Can we afford it?
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    if nation.treasury.checked_sub(cost).is_none() {
        return;
    }

    if build_fort(&mut game.hex_map, fort_coord).is_ok() {
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.treasury -= cost;
        actions.push(format!("{} has fortified its borders", nation_name));
    }
}

/// Move units to threatened provinces: provinces that border enemy territory
/// but have no army units stationed there.
fn ai_move_units_to_threatened(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Find enemies
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

    let owned_provinces: Vec<ProvinceId> = nation.province_ids.clone();

    // Collect enemy province IDs
    let enemy_province_ids: Vec<ProvinceId> = game
        .provinces
        .iter()
        .filter(|p| enemies.contains(&p.owner))
        .map(|p| p.id)
        .collect();

    // Find threatened provinces: border enemy, have no units stationed
    let nation = game.get_nation(nation_id).unwrap();
    let mut threatened: Vec<ProvinceId> = Vec::new();

    for &pid in &owned_provinces {
        // Check if any unit is stationed in this province
        let has_unit = nation.army.iter().any(|u| u.position == pid);
        if has_unit {
            continue;
        }

        // Check if this province borders enemy territory
        if let Some(prov) = game.get_province(pid) {
            let borders_enemy = prov.tiles.iter().any(|&tile_coord| {
                tile_coord.neighbors().iter().any(|neighbor| {
                    game.hex_map
                        .get_tile(*neighbor)
                        .and_then(|t| t.province_id)
                        .is_some_and(|npid| enemy_province_ids.contains(&npid))
                })
            });
            if borders_enemy {
                threatened.push(pid);
            }
        }
    }

    // For each threatened province, try to move a unit from the capital
    // (or any non-threatened province with units)
    for target_pid in threatened {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };

        // Find an available unit (not already being moved this turn, stationed
        // in a non-threatened province)
        let unit_idx = nation.army.iter().position(|u| {
            u.position != target_pid && !game.pending_moves.iter().any(|(_, uid, _)| *uid == u.id)
        });

        if let Some(idx) = unit_idx {
            let unit_id = nation.army[idx].id;
            game.pending_moves.push((nation_id, unit_id, target_pid));
        }
    }
}

/// If AI has been at war for a prolonged time and is losing (lost provinces),
/// propose peace.
///
/// War duration thresholds by personality:
/// - Diplomatic: 10 turns
/// - Balanced/Economic: 20 turns
/// - Aggressive: 30 turns
///
/// Province-loss-based retreat:
/// - If AI has lost >50% of its starting provinces, accept peace immediately
/// - Diplomatic AI retreats at 30% loss
fn ai_propose_peace(
    game: &mut GameState,
    nation_id: NationId,
    personality: AiPersonality,
    actions: &mut Vec<String>,
) {
    let turn_number = game.turn.0;

    let peace_threshold = match personality {
        AiPersonality::Diplomatic => 10u32,
        AiPersonality::Aggressive => 30,
        _ => 20,
    };

    // Province-loss threshold for immediate peace (fraction of starting provinces lost)
    let loss_threshold: f64 = match personality {
        AiPersonality::Diplomatic => 0.30,
        _ => 0.50,
    };

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

    let nation_name = game
        .get_nation(nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    let current_provinces = game
        .get_nation(nation_id)
        .map(|n| n.province_ids.len())
        .unwrap_or(0);

    // Pre-compute enemy names for efficient history scanning
    let enemies_with_names: Vec<(NationId, String)> = enemies
        .iter()
        .map(|&eid| {
            let name = game
                .get_nation(eid)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            (eid, name)
        })
        .collect();

    // Pre-compute search patterns (avoid re-creating strings in the loop)
    let loss_pattern = format!("from {}", nation_name);

    // Single pass over history to count lost provinces and find war start turns
    let mut provinces_lost_count = 0usize;
    let mut war_starts: Vec<(NationId, u32)> = Vec::new(); // (enemy_id, turn)
    for (turn_entry, desc) in &game.history {
        if desc.contains("conquered") && desc.contains(&loss_pattern) {
            provinces_lost_count += 1;
        }
        if desc.contains("declared war") && desc.contains(&nation_name) {
            for (enemy_id, enemy_name) in &enemies_with_names {
                if desc.contains(enemy_name.as_str()) {
                    war_starts.push((*enemy_id, turn_entry.0));
                }
            }
        }
    }

    let estimated_starting = current_provinces + provinces_lost_count;

    for (enemy_id, enemy_name) in &enemies_with_names {
        // Skip peace if we have pending attacks against provinces owned by this enemy
        let has_pending_attack = game.pending_attacks.iter().any(|(attacker, prov_id)| {
            *attacker == nation_id
                && game
                    .get_province(*prov_id)
                    .is_some_and(|p| p.owner == *enemy_id)
        });
        if has_pending_attack {
            continue;
        }

        // Immediate peace if province loss exceeds threshold
        if estimated_starting > 0 {
            let loss_ratio = provinces_lost_count as f64 / estimated_starting as f64;
            if loss_ratio >= loss_threshold {
                game.diplomacy.make_peace(nation_id, *enemy_id);
                actions.push(format!(
                    "{} has sued for peace with {} (heavy losses)",
                    nation_name, enemy_name
                ));
                let turn = game.turn;
                game.history.push((
                    turn,
                    format!("{} made peace with {}", nation_name, enemy_name),
                ));
                continue;
            }
        }

        // Look up war start turn from pre-computed data
        let war_start_turn = war_starts
            .iter()
            .filter(|(eid, _)| *eid == *enemy_id)
            .map(|(_, t)| *t)
            .min();

        let war_duration = match war_start_turn {
            Some(start) => turn_number.saturating_sub(start),
            None => 0,
        };

        if war_duration < peace_threshold {
            continue;
        }

        // Simple heuristic: if AI has 1 or fewer provinces, definitely losing
        // or if the enemy has more provinces than us
        let enemy_provinces = game
            .get_nation(*enemy_id)
            .map(|n| n.province_ids.len())
            .unwrap_or(0);

        let is_losing = current_provinces <= 1 || enemy_provinces > current_provinces;

        if is_losing {
            game.diplomacy.make_peace(nation_id, *enemy_id);
            actions.push(format!(
                "{} has sued for peace with {}",
                nation_name, enemy_name
            ));
            let turn = game.turn;
            game.history.push((
                turn,
                format!("{} made peace with {}", nation_name, enemy_name),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::{
        test_game_with_adjacent_provinces, test_game_with_ai_and_minor,
    };
    use crate::hex::HexCoord;
    use crate::map::{Province, UnitId};
    use crate::military::units::{ArmyUnit, ArmyUnitType};

    #[test]
    fn ai_builds_fort_on_border_province() {
        let mut game = test_game_with_adjacent_provinces();

        let mut actions = Vec::new();
        ai_build_forts(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        // Check that a fort was built on the AI province's capital tile
        let ai_capital_tile = HexCoord::new(0, 0);
        let tile = game.hex_map.get_tile(ai_capital_tile).unwrap();
        assert!(
            tile.infrastructure.has_fort,
            "AI should build a fort on border province capital tile"
        );
        assert_eq!(tile.infrastructure.fort_level, 1, "Fort should be level 1");

        // Treasury should be reduced by $5,000
        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.treasury,
            Money::dollars(15000),
            "Treasury should be reduced by $5,000 for fort"
        );

        assert!(
            actions.iter().any(|a| a.contains("fortified")),
            "Should report fort building"
        );
    }

    #[test]
    fn ai_does_not_build_fort_when_poor() {
        let mut game = test_game_with_adjacent_provinces();
        game.get_nation_mut(NationId(2)).unwrap().treasury = Money::dollars(3000);

        let mut actions = Vec::new();
        ai_build_forts(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        let ai_capital_tile = HexCoord::new(0, 0);
        let tile = game.hex_map.get_tile(ai_capital_tile).unwrap();
        assert!(
            !tile.infrastructure.has_fort,
            "AI should not build fort when too poor"
        );
    }

    #[test]
    fn ai_does_not_build_fort_when_not_at_war() {
        let mut game = test_game_with_adjacent_provinces();
        // Make peace
        game.diplomacy.make_peace(NationId(2), NationId(3));

        let mut actions = Vec::new();
        ai_build_forts(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        let ai_capital_tile = HexCoord::new(0, 0);
        let tile = game.hex_map.get_tile(ai_capital_tile).unwrap();
        assert!(
            !tile.infrastructure.has_fort,
            "AI should not build fort when not at war"
        );
    }

    #[test]
    fn ai_moves_unit_to_threatened_province() {
        let mut game = test_game_with_adjacent_provinces();

        // Give AI a unit stationed at a non-threatened location (capital)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.army.push(ArmyUnit::new(
            UnitId(9000),
            ArmyUnitType::Regulars,
            NationId(2),
            ProvinceId(2), // stationed in AI province
        ));

        // Add another province for the AI that is NOT a border province
        let safe_tile = HexCoord::new(0, 5);
        game.hex_map.set_tile(
            safe_tile,
            crate::map::tile::Tile::with_province(TerrainType::Farm, ProvinceId(4)),
        );
        let safe_province = Province::new(
            ProvinceId(4),
            "Safe Province".to_string(),
            NationId(2),
            safe_tile,
            vec![safe_tile],
            4,
        );
        game.provinces.push(safe_province);
        game.get_nation_mut(NationId(2))
            .unwrap()
            .add_province(ProvinceId(4));

        // Move the unit to the safe province so it's available to be moved
        game.get_nation_mut(NationId(2)).unwrap().army[0].position = ProvinceId(4);

        ai_move_units_to_threatened(&mut game, NationId(2));

        // Should have a pending move to the border province (ProvinceId(2))
        assert!(
            game.pending_moves
                .iter()
                .any(|(nation, _, dest)| *nation == NationId(2) && *dest == ProvinceId(2)),
            "AI should queue a move to the threatened border province"
        );
    }

    #[test]
    fn ai_does_not_move_units_when_not_at_war() {
        let mut game = test_game_with_adjacent_provinces();
        game.diplomacy.make_peace(NationId(2), NationId(3));

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.army.push(ArmyUnit::new(
            UnitId(9001),
            ArmyUnitType::Regulars,
            NationId(2),
            ProvinceId(2),
        ));

        ai_move_units_to_threatened(&mut game, NationId(2));

        assert!(
            game.pending_moves.is_empty(),
            "No moves should be queued when not at war"
        );
    }

    #[test]
    fn ai_proposes_peace_after_prolonged_losing_war() {
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(25);

        // Record war declaration in history at turn 1
        game.history.push((
            TurnNumber::new(1),
            "AINation declared war on EnemyLand".to_string(),
        ));

        // Make AI "losing": enemy has more provinces
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(4));
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(5));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        // Should have made peace
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(true);
        assert!(
            !at_war,
            "AI should propose peace after 24 turns of losing war (threshold 20 for Balanced)"
        );
        assert!(
            actions.iter().any(|a| a.contains("sued for peace")),
            "Should report peace proposal"
        );
    }

    #[test]
    fn diplomatic_ai_proposes_peace_earlier() {
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(15);

        game.history.push((
            TurnNumber::new(1),
            "AINation declared war on EnemyLand".to_string(),
        ));

        // Make AI "losing"
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(4));
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(5));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Diplomatic,
            &mut actions,
        );

        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(true);
        assert!(
            !at_war,
            "Diplomatic AI should propose peace after 14 turns (threshold 10)"
        );
    }

    #[test]
    fn aggressive_ai_fights_longer() {
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(25);

        game.history.push((
            TurnNumber::new(1),
            "AINation declared war on EnemyLand".to_string(),
        ));

        // Make AI "losing"
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(4));
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(5));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Aggressive,
            &mut actions,
        );

        // At turn 25 with war starting at turn 1: 24 turns of war < 30 threshold
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            at_war,
            "Aggressive AI should NOT propose peace at 24 turns (threshold is 30)"
        );
    }

    #[test]
    fn ai_accepts_peace_when_lost_over_50_percent_provinces() {
        let mut game = test_game_with_ai_and_minor();

        // Give AI multiple provinces, then simulate heavy losses in history
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        // AI has 1 province (ProvinceId(2)) but started with 4
        // Simulate losing 3 provinces in history
        game.history.push((
            TurnNumber::new(5),
            "AINation declared war on MinorLand".to_string(),
        ));
        game.history.push((
            TurnNumber::new(10),
            "HumanNation conquered Province A from AINation".to_string(),
        ));
        game.history.push((
            TurnNumber::new(12),
            "HumanNation conquered Province B from AINation".to_string(),
        ));
        game.history.push((
            TurnNumber::new(14),
            "HumanNation conquered Province C from AINation".to_string(),
        ));

        // Put AI at war with human
        game.diplomacy.declare_war(NationId(2), NationId(1));

        // AI has lost 3 of 4 provinces (75% > 50% threshold)
        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        // AI should sue for peace
        assert!(
            actions
                .iter()
                .any(|a| a.contains("sued for peace") && a.contains("heavy losses")),
            "AI should sue for peace when losing > 50%% of provinces; actions: {:?}",
            actions
        );

        // War should be over
        let rel = game.diplomacy.get_relation(NationId(2), NationId(1));
        assert!(
            rel.is_none() || !rel.unwrap().at_war,
            "Should no longer be at war after suing for peace"
        );
    }

    #[test]
    fn diplomatic_ai_accepts_peace_at_30_percent_loss() {
        let mut game = test_game_with_ai_and_minor();

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Diplomatic);
        // AI has 1 province, simulate losing 1 (so started with 2, lost 50% > 30%)
        game.history.push((
            TurnNumber::new(5),
            "AINation declared war on HumanNation".to_string(),
        ));
        game.history.push((
            TurnNumber::new(10),
            "HumanNation conquered Lost Province from AINation".to_string(),
        ));

        // Put at war
        game.diplomacy.declare_war(NationId(2), NationId(1));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Diplomatic,
            &mut actions,
        );

        // Diplomatic AI should sue for peace at 50% (> 30% threshold)
        assert!(
            actions.iter().any(|a| a.contains("sued for peace")),
            "Diplomatic AI should sue for peace at 50%% loss (threshold=30%%); actions: {:?}",
            actions
        );
    }

    #[test]
    fn ai_does_not_sue_for_peace_when_not_losing() {
        let mut game = test_game_with_ai_and_minor();

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        // AI has not lost any provinces — no conquest history against it

        // Put at war
        game.diplomacy.declare_war(NationId(2), NationId(1));
        game.turn = TurnNumber::new(50); // past any war duration threshold
        game.history.push((
            TurnNumber::new(1),
            "AINation declared war on HumanNation".to_string(),
        ));

        // Give AI more provinces than enemy so it doesn't feel like it's losing
        game.get_nation_mut(NationId(2))
            .unwrap()
            .add_province(ProvinceId(10));
        game.get_nation_mut(NationId(2))
            .unwrap()
            .add_province(ProvinceId(11));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        assert!(
            actions.is_empty(),
            "AI should not sue for peace when not losing; actions: {:?}",
            actions
        );
    }
}
