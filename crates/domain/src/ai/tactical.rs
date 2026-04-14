#![allow(unused_labels)]
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

    // ── Read Lua config (feature-gated) ──────────────────────
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
    #[cfg(not(feature = "lua"))]
    let _lua_cfg: Option<()> = None;

    // Choose which province to fort based on personality / Lua fort_strategy
    let fort_strategy: String = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.fort_strategy.clone()) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => "capital".to_string(),
            AiPersonality::Aggressive => "border".to_string(),
            _ => "border".to_string(),
        }
    };

    let target_province = if fort_strategy == "capital" {
        // Fort the capital for defense
        if owned_provinces.contains(&capital_province_id) {
            capital_province_id
        } else {
            border_provinces[0]
        }
    } else {
        // "border" or any other value: first border province
        border_provinces[0]
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
///
/// Propose peace using coalition-aware assessment.
///
/// The AI evaluates each ongoing war through two lenses:
///   1. **Assessment**: relative coalition strength (military + provinces + economy + momentum)
///   2. **Worthiness**: whether continuing the war is still worthwhile (captures, losses, diminishing returns)
///
/// Peace is proposed when:
///   - `lost_enough`: heavy losses or low win_likelihood
///   - `won_enough`: captured enough and diminishing returns
///   - Stalemate: near-equal power for prolonged duration
///
/// For AI-to-AI wars, the proposal is evaluated inline (both decide in the same turn).
/// For AI-to-human wars, a `DiplomaticProposal` is created for the UI to display.
fn ai_propose_peace(
    game: &mut GameState,
    nation_id: NationId,
    personality: AiPersonality,
    actions: &mut Vec<String>,
) {
    use super::assessment::{
        evaluate_coalition_strength, evaluate_peace_proposal, evaluate_war_worthiness,
    };

    // ── Read Lua config (feature-gated) ──────────────────────
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
    #[cfg(not(feature = "lua"))]
    let _lua_cfg: Option<()> = None;

    let stalemate_duration: u32 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.peace_stalemate_duration) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 12,
            AiPersonality::Aggressive => 25,
            _ => 15,
        }
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

    for &enemy_id in &enemies {
        let enemy_name = game
            .get_nation(enemy_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();

        // Skip peace if we have pending attacks against provinces owned by this enemy
        let has_pending_attack = game.pending_attacks.iter().any(|(attacker, prov_id)| {
            *attacker == nation_id
                && game
                    .get_province(*prov_id)
                    .is_some_and(|p| p.owner == enemy_id)
        });
        if has_pending_attack {
            continue;
        }

        // ── Assess the war ──────────────────────────────────
        let assessment = evaluate_coalition_strength(
            game,
            nation_id,
            enemy_id,
            #[cfg(feature = "lua")]
            lua_cfg.as_ref(),
        );
        let worthiness = evaluate_war_worthiness(
            game,
            nation_id,
            enemy_id,
            personality,
            assessment.win_likelihood,
            #[cfg(feature = "lua")]
            lua_cfg.as_ref(),
        );

        // ── Decide whether to propose peace ─────────────────
        // Lua hook can override the decision
        let war_start = super::assessment::find_war_start_turn(game, &nation_name, &enemy_name);
        let war_duration = war_start
            .map(|start| game.turn.0.saturating_sub(start))
            .unwrap_or(0);

        let should_propose = 'decide: {
            #[cfg(feature = "lua")]
            if let Some(lua_result) = super::lua_bridge::lua_evaluate_peace(
                game,
                personality,
                nation_id,
                enemy_id,
                assessment.win_likelihood,
                worthiness.provinces_captured,
                worthiness.provinces_lost,
                war_duration,
            ) {
                break 'decide lua_result;
            }

            if worthiness.lost_enough {
                break 'decide true;
            }
            if worthiness.won_enough {
                break 'decide true;
            }
            // Stalemate: near-equal power for a long time
            assessment.win_likelihood > 0.4
                && assessment.win_likelihood < 0.6
                && war_duration > stalemate_duration
        };

        if !should_propose {
            continue;
        }

        if game.ai_debug {
            eprintln!(
                "[AI:{}:peace] Proposing peace with {} (win={:.2}, captured={}, lost={}, won_enough={}, lost_enough={})",
                nation_name,
                enemy_name,
                assessment.win_likelihood,
                worthiness.provinces_captured,
                worthiness.provinces_lost,
                worthiness.won_enough,
                worthiness.lost_enough,
            );
        }

        // ── Determine target type: AI GP, human, or minor nation ─
        let target_is_ai = game
            .get_nation(enemy_id)
            .is_some_and(|n| n.ai_personality.is_some());
        let target_is_human = enemy_id == game.human_player_nation;

        if target_is_ai {
            // AI-to-AI: evaluate inline — get the receiver's personality and decide
            let receiver_personality = super::common::get_personality(game, enemy_id);

            #[cfg(feature = "lua")]
            let receiver_lua_cfg = game
                .game_data
                .lua_engine
                .as_ref()
                .and_then(|e| super::lua_bridge::lua_get_config(e, receiver_personality));

            let accepted = evaluate_peace_proposal(
                game,
                nation_id,
                enemy_id,
                receiver_personality,
                #[cfg(feature = "lua")]
                receiver_lua_cfg.as_ref(),
            );

            if accepted {
                game.diplomacy.make_peace(nation_id, enemy_id);
                let reason = if worthiness.lost_enough {
                    " (heavy losses)"
                } else if worthiness.won_enough {
                    " (objectives achieved)"
                } else {
                    ""
                };
                actions.push(format!(
                    "{} has sued for peace with {}{}",
                    nation_name, enemy_name, reason
                ));
                let turn = game.turn;
                game.history.push((
                    turn,
                    format!("{} made peace with {}", nation_name, enemy_name),
                ));
            } else if game.ai_debug {
                eprintln!(
                    "[AI:{}:peace] {} rejected peace proposal",
                    nation_name, enemy_name,
                );
            }
        } else if target_is_human {
            // AI-to-human: create a pending proposal for the UI
            let _ = game.diplomacy.propose_peace(nation_id, enemy_id, game.turn);
            let reason = if worthiness.lost_enough {
                " (heavy losses)"
            } else if worthiness.won_enough {
                " (objectives achieved)"
            } else {
                ""
            };
            actions.push(format!(
                "{} proposes peace with {}{}",
                nation_name, enemy_name, reason
            ));
        } else {
            // AI-to-minor-nation: auto-accept (minor nations are passive)
            game.diplomacy.make_peace(nation_id, enemy_id);
            let reason = if worthiness.lost_enough {
                " (heavy losses)"
            } else if worthiness.won_enough {
                " (objectives achieved)"
            } else {
                ""
            };
            actions.push(format!(
                "{} has sued for peace with {}{}",
                nation_name, enemy_name, reason
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
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(4)),
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
    fn ai_proposes_peace_after_heavy_losses() {
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(25);

        // Record war declaration and province losses in history
        game.history.push((
            TurnNumber::new(1),
            "AINation declared war on EnemyLand".to_string(),
        ));
        // AI lost 2 provinces (meeting Balanced lost_enough_losses threshold)
        game.history.push((
            TurnNumber::new(10),
            "EnemyLand conquered Province A from AINation".to_string(),
        ));
        game.history.push((
            TurnNumber::new(15),
            "EnemyLand conquered Province B from AINation".to_string(),
        ));

        // Make AI weaker: enemy has more provinces
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

        // Should have made peace (AI-to-MinorNation: auto-accepted)
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(true);
        assert!(
            !at_war,
            "AI should propose peace after heavy losses (lost_enough triggered)"
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
    fn ai_proposes_peace_to_human_when_lost_heavily() {
        let mut game = test_game_with_ai_and_minor();

        // Give AI multiple provinces, then simulate heavy losses in history
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        // AI has 1 province (ProvinceId(2)) but lost 3 provinces in history
        game.history.push((
            TurnNumber::new(5),
            "AINation declared war on HumanNation".to_string(),
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

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        // AI-to-human: should create a pending proposal, NOT immediate peace
        assert!(
            actions
                .iter()
                .any(|a| a.contains("proposes peace") && a.contains("heavy losses")),
            "AI should propose peace when heavily losing; actions: {:?}",
            actions
        );

        // War is still active (human hasn't accepted yet)
        assert!(
            game.diplomacy.is_at_war(NationId(2), NationId(1)),
            "War should still be active until human accepts"
        );

        // But a pending peace proposal should exist
        assert!(
            game.diplomacy.pending_proposals.iter().any(|p| {
                p.from == NationId(2)
                    && p.to == NationId(1)
                    && p.proposal_type == crate::events::TreatyType::PeaceTreaty
            }),
            "Should have a pending peace proposal to human player"
        );
    }

    #[test]
    fn diplomatic_ai_proposes_peace_to_human_at_low_loss() {
        let mut game = test_game_with_ai_and_minor();

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Diplomatic);
        // Diplomatic AI has low lost_enough_losses threshold (1)
        // Simulate losing 1 province
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

        // Diplomatic AI should propose peace (lost_enough: 1 loss >= threshold of 1)
        assert!(
            actions.iter().any(|a| a.contains("proposes peace")),
            "Diplomatic AI should propose peace after 1 province loss; actions: {:?}",
            actions
        );
        // Should be a pending proposal to human
        assert!(
            game.diplomacy.pending_proposals.iter().any(|p| {
                p.from == NationId(2)
                    && p.to == NationId(1)
                    && p.proposal_type == crate::events::TreatyType::PeaceTreaty
            }),
            "Should have a pending peace proposal"
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
