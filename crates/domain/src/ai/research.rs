use crate::events::TechId;
use crate::game_state::GameState;
use crate::tech::tree::TechEffect;
use crate::types::*;

use super::common::AiPersonality;
use super::common::get_personality;

/// Returns true if a technology has military effects (upgrades units, unlocks military units/ships).
fn is_military_tech(effects: &[TechEffect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            TechEffect::UpgradeUnit { .. } | TechEffect::UnlockUnit(_) | TechEffect::UnlockShip(_)
        )
    })
}

/// Returns true if a technology has economic effects (buildings, terrain, infrastructure, civilians).
fn is_economic_tech(effects: &[TechEffect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            TechEffect::UnlockBuilding(_)
                | TechEffect::EnableTerrainImprovement { .. }
                | TechEffect::EnableInfrastructure(_)
                | TechEffect::EnableCivilian(_)
        )
    })
}

/// Pick a tech based on personality and research it if the nation can afford it.
///
/// - **Economic**: prefer the most expensive available tech (invest in the future)
/// - **Aggressive**: prefer military techs (unit upgrades, unit unlocks, ship unlocks)
/// - **Diplomatic**: prefer economic/trade techs (buildings, terrain, infrastructure)
/// - **Balanced**: pick the cheapest available tech (current behavior)
pub(crate) fn ai_research_tech(
    game: &mut GameState,
    nation_id: NationId,
    current_year: u32,
    actions: &mut Vec<super::AiAction>,
) {
    let personality = get_personality(game, nation_id);

    // Gather the nation's researched techs
    let researched: Vec<TechId> = match game.get_nation(nation_id) {
        Some(n) => n.researched_techs.clone(),
        None => return,
    };

    // Find available techs — collect owned data to avoid borrow conflicts
    let available: Vec<(TechId, Money, String, Vec<TechEffect>)> = game
        .game_data
        .tech_tree
        .available_techs(&researched, current_year)
        .iter()
        .map(|t| (t.id, t.cost, t.name.clone(), t.effects.clone()))
        .collect();
    if available.is_empty() {
        return;
    }

    // Attempt Lua override for tech selection
    #[cfg(feature = "lua")]
    {
        let lua_available = &available;
        // Drop the borrow on `available` (which borrows game.game_data) by
        // working only with the owned `lua_available` from here.
        if let Some(tech_id) = super::lua_bridge::lua_pick_tech(game, personality, lua_available) {
            // Verify against our owned copy
            if let Some((_, tech_cost, tech_name, _)) =
                lua_available.iter().find(|(id, _, _, _)| *id == tech_id)
            {
                let tech_cost = *tech_cost;
                let tech_name = tech_name.clone();
                let (did_spend, nation_name) = {
                    let nation = match game.get_nation_mut(nation_id) {
                        Some(n) => n,
                        None => return,
                    };
                    if let Some(remaining) = nation.treasury.checked_sub(tech_cost) {
                        nation.treasury = remaining;
                        nation.research_tech(tech_id);
                        (true, nation.name.clone())
                    } else {
                        (false, String::new())
                    }
                };
                if did_spend {
                    game.pending_ai_cash_spending.push((
                        nation_id,
                        crate::economy::ledger::CashSink::AiResearch,
                        tech_cost,
                        None,
                    ));
                    if game.ai_debug {
                        eprintln!(
                            "[AI:{}:research] Lua picked \"{}\" (cost=${})",
                            nation_name,
                            tech_name,
                            tech_cost.as_dollars()
                        );
                    }
                    actions.push(super::AiAction {
                        text: format!(
                            "Scientists in {} have discovered {}!",
                            nation_name, tech_name
                        ),
                        reason: format!(
                            "Lua/{} personality selected tech \"{}\" (cost=${})",
                            personality,
                            tech_name,
                            tech_cost.as_dollars()
                        ),
                        is_non_action: false,
                        nation_id,
                    });
                    let turn = game.turn;
                    let entry_text = format!("{} researched {}", nation_name, tech_name);
                    if !game
                        .history
                        .iter()
                        .any(|(t, text)| *t == turn && text == &entry_text)
                    {
                        game.history.push((turn, entry_text));
                    }
                    return;
                }
            }
        }
    }

    // Build a personality-ranked list of candidate techs, then pick from the
    // top few using a per-nation pseudo-random seed so different nations
    // (and different games) produce varied research paths.
    // available is Vec<(TechId, Money, String, Vec<TechEffect>)>
    let mut candidates = match personality {
        AiPersonality::Economic => {
            // Prefer the most expensive techs (long-term investment)
            let mut v = available.clone();
            v.sort_by(|a, b| b.1.cents().cmp(&a.1.cents()));
            v
        }
        AiPersonality::Aggressive => {
            // Prefer military techs sorted cheapest-first, then non-military cheapest-first
            let mut military: Vec<_> = available
                .iter()
                .filter(|t| is_military_tech(&t.3))
                .cloned()
                .collect();
            let mut other: Vec<_> = available
                .iter()
                .filter(|t| !is_military_tech(&t.3))
                .cloned()
                .collect();
            military.sort_by_key(|t| t.1.cents());
            other.sort_by_key(|t| t.1.cents());
            military.extend(other);
            military
        }
        AiPersonality::Diplomatic => {
            // Prefer economic/trade techs sorted cheapest-first, then others
            let mut econ: Vec<_> = available
                .iter()
                .filter(|t| is_economic_tech(&t.3))
                .cloned()
                .collect();
            let mut other: Vec<_> = available
                .iter()
                .filter(|t| !is_economic_tech(&t.3))
                .cloned()
                .collect();
            econ.sort_by_key(|t| t.1.cents());
            other.sort_by_key(|t| t.1.cents());
            econ.extend(other);
            econ
        }
        AiPersonality::Balanced => {
            // Cheapest first
            let mut v = available.clone();
            v.sort_by_key(|t| t.1.cents());
            v
        }
    };
    let _ = &mut candidates; // suppress unused_mut if needed

    if candidates.is_empty() {
        return;
    }

    // Extract (id, cost, name) for selection
    let all_candidates: Vec<(TechId, Money, String)> =
        candidates.iter().map(|t| (t.0, t.1, t.2.clone())).collect();

    // Pick from the top candidates using a deterministic per-nation seed
    // so that each nation gets a different research path each game.
    let top_n = all_candidates.len().min(3);
    let seed = (game.turn.0 as usize).wrapping_mul(nation_id.0 as usize + 7) % top_n;
    let (tech_id, tech_cost, ref tech_name) = all_candidates[seed];

    if game.ai_debug {
        let nation_name = game
            .get_nation(nation_id)
            .map(|n| n.name.as_str())
            .unwrap_or("?");
        eprintln!(
            "[AI:{}:research] personality={}, candidates={}, picked=\"{}\" (cost=${})",
            nation_name,
            personality,
            all_candidates.len(),
            tech_name,
            tech_cost.as_dollars()
        );
    }

    // Check if the nation can afford it
    let (did_spend, nation_name) = {
        let nation = match game.get_nation_mut(nation_id) {
            Some(n) => n,
            None => return,
        };
        if let Some(remaining) = nation.treasury.checked_sub(tech_cost) {
            nation.treasury = remaining;
            nation.research_tech(tech_id);
            (true, nation.name.clone())
        } else {
            (false, String::new())
        }
    };
    if did_spend {
        game.pending_ai_cash_spending.push((
            nation_id,
            crate::economy::ledger::CashSink::AiResearch,
            tech_cost,
            None,
        ));
        actions.push(super::AiAction {
            text: format!(
                "Scientists in {} have discovered {}!",
                nation_name, tech_name
            ),
            reason: format!(
                "{} personality preference, picked from {} candidates (cost=${})",
                personality,
                all_candidates.len(),
                tech_cost.as_dollars()
            ),
            is_non_action: false,
            nation_id,
        });
        let turn = game.turn;
        let entry_text = format!("{} researched {}", nation_name, tech_name);
        // Deduplicate: only push if this exact text doesn't already exist for this turn
        if !game
            .history
            .iter()
            .any(|(t, text)| *t == turn && text == &entry_text)
        {
            game.history.push((turn, entry_text));
        }
        return; // Successfully researched
    }

    // Second pass: if we couldn't afford the preferred tech and treasury is high,
    // try ANY available tech (cheapest first) to avoid hoarding cash.
    let treasury = match game.get_nation(nation_id) {
        Some(n) => n.treasury,
        None => return,
    };
    if treasury > Money::dollars(10_000) {
        let mut fallback_candidates = all_candidates;
        fallback_candidates.sort_by_key(|(_, cost, _)| cost.cents());
        for (cand_id, cand_cost, cand_name) in &fallback_candidates {
            let (did_spend, nation_name) = {
                let nation = match game.get_nation_mut(nation_id) {
                    Some(n) => n,
                    None => return,
                };
                if let Some(remaining) = nation.treasury.checked_sub(*cand_cost) {
                    nation.treasury = remaining;
                    nation.research_tech(*cand_id);
                    (true, nation.name.clone())
                } else {
                    (false, String::new())
                }
            };
            if did_spend {
                game.pending_ai_cash_spending.push((
                    nation_id,
                    crate::economy::ledger::CashSink::AiResearch,
                    *cand_cost,
                    None,
                ));
                actions.push(super::AiAction {
                    text: format!(
                        "Scientists in {} have discovered {}!",
                        nation_name, cand_name
                    ),
                    reason: format!(
                        "Fallback research path: treasury ${} > $10,000, picked cheapest affordable tech (cost=${})",
                        treasury.as_dollars(),
                        cand_cost.as_dollars()
                    ),
                    is_non_action: false,
                    nation_id,
                });
                let turn = game.turn;
                let entry_text = format!("{} researched {}", nation_name, cand_name);
                if !game
                    .history
                    .iter()
                    .any(|(t, text)| *t == turn && text == &entry_text)
                {
                    game.history.push((turn, entry_text));
                }
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::test_game_with_ai;
    use crate::ai::run_ai_turns;
    use crate::events::TechId;

    #[test]
    fn ai_does_not_spend_more_than_it_has() {
        let mut game = test_game_with_ai();
        // Pre-research the free techs so only paid techs remain
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.research_tech(TechId(1));
        ai.research_tech(TechId(2));
        // Set treasury to $500 (less than the cheapest paid tech at $1,000)
        ai.treasury = Money::dollars(500);

        // Move to year 1816 so Cotton Gin ($1,000) becomes available
        game.turn = TurnNumber::from_year_quarter(1816, 1);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should NOT have researched Cotton Gin since it can't afford it
        assert!(
            !ai.has_researched(TechId(3)),
            "AI should not research techs it cannot afford"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(500),
            "Treasury should be unchanged"
        );
    }

    #[test]
    fn economic_ai_prefers_expensive_tech() {
        let mut game = test_game_with_ai();
        // Set Economic personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Economic);
        ai.treasury = Money::dollars(50000);

        // At year 1821, multiple techs with different costs are available
        game.turn = TurnNumber::from_year_quarter(1821, 1);

        // Pre-research the free techs so only paid techs remain
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.research_tech(TechId(1));
        ai.research_tech(TechId(2));

        let mut actions = Vec::new();
        ai_research_tech(&mut game, NationId(2), 1821, &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Economic should pick the most expensive available tech
        assert!(
            ai.researched_techs.len() > 2,
            "Economic AI should have researched a tech"
        );
    }

    #[test]
    fn aggressive_ai_prefers_military_tech() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Aggressive);
        ai.treasury = Money::dollars(100000);

        // Set year to 1841 when Breech-Loading Rifles (military) is available
        game.turn = TurnNumber::from_year_quarter(1841, 1);

        // Pre-research Bessemer Converter (prerequisite for Breech-Loading Rifles)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.research_tech(TechId(1));
        ai.research_tech(TechId(2));
        ai.research_tech(TechId(11)); // Bessemer Converter

        let mut actions = Vec::new();
        ai_research_tech(&mut game, NationId(2), 1841, &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have picked Breech-Loading Rifles (TechId 13, military)
        // or Rifled Artillery (TechId 14, also military)
        let has_military = ai.has_researched(TechId(13)) || ai.has_researched(TechId(14));
        assert!(
            has_military,
            "Aggressive AI should prefer military techs (Breech-Loading Rifles or Rifled Artillery)"
        );
    }

    #[test]
    fn balanced_ai_picks_cheapest_tech() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        ai.treasury = Money::dollars(50000);

        // At 1815, two free techs (ID 1 and 2) are available
        let mut actions = Vec::new();
        ai_research_tech(&mut game, NationId(2), 1815, &mut actions);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should pick one of the free techs
        assert!(
            ai.has_researched(TechId(1)) || ai.has_researched(TechId(2)),
            "Balanced AI should pick the cheapest (free) tech"
        );
    }
}
