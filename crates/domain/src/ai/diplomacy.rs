#![allow(unused_labels)]
use crate::game_state::GameState;
use crate::types::*;

#[cfg(test)]
use super::common::lua_or;
use super::common::{AiPersonality, PersonalityConfig, get_personality};

/// AI builds trade consulates with Minor Nations, prioritizing those with
/// the most tradeable resources (since trade increases relation scores).
#[cfg(test)]
pub(crate) fn ai_build_consulates(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);
    let cost = Money::dollars(game.game_data.game_config.consulate_cost);
    let treasury_threshold = Money::dollars(game.game_data.game_config.ai_consulate_treasury_threshold);

    // ── Read Lua config (feature-gated) ──────────────────────
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
    #[cfg(not(feature = "lua"))]
    let _lua_cfg: Option<()> = None;

    let defaults = PersonalityConfig::for_personality(personality);
    let max_per_turn = lua_or(lua_cfg.as_ref().and_then(|c| c.consulate_max_per_turn), defaults.consulate_max_per_turn);

    // Check treasury threshold
    let treasury = match game.get_nation(nation_id) {
        Some(n) => n.economy.treasury,
        None => return,
    };
    if treasury < treasury_threshold {
        return;
    }

    // Score minor nations by trade potential: count tradeable resource tiles
    // in their provinces. Prioritize nations with more resources to trade.
    let mut candidates: Vec<(NationId, u32)> = game
        .world.nations
        .iter()
        .filter(|n| !n.is_great_power() && !n.province_ids.is_empty() && !n.diplomacy.is_in_anarchy)
        .filter(|n| {
            !game
                .world.diplomacy
                .get_relation(nation_id, n.id)
                .is_some_and(|r| r.has_consulate)
        })
        .map(|n| {
            let trade_potential: u32 = game
                .world.provinces
                .iter()
                .filter(|p| p.owner == n.id)
                .flat_map(|p| &p.tiles)
                .filter_map(|coord| {
                    game.world.hex_map
                        .get_tile(*coord)
                        .and_then(|t| t.calculate_yield())
                })
                .filter(|y| y.resource.is_tradeable())
                .map(|y| y.quantity)
                .sum();
            (n.id, trade_potential)
        })
        .filter(|(_, potential)| *potential > 0) // Only build where there's something to trade
        .collect();

    // Sort by trade potential descending — richest trade partners first
    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let mut built = 0;
    for (mn_id, _trade_potential) in candidates {
        if built >= max_per_turn {
            break;
        }

        let treasury = match game.get_nation(nation_id) {
            Some(n) => n.economy.treasury,
            None => return,
        };
        if treasury.checked_sub(cost).is_none() {
            break;
        }

        // Build consulate
        if game.world.diplomacy.build_consulate(nation_id, mn_id).is_ok() {
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.economy.treasury -= cost;
            }
            game.transient.pending_ai_cash_spending.push((
                nation_id,
                crate::economy::ledger::CashSink::AiDiplomacyConsulate,
                cost,
                Some(mn_id),
            ));
            built += 1;
        }
    }
}

/// AI manages diplomatic relations: proposes treaties, sends grants.
///
/// - **Diplomatic**: proposes pacts with all Minor Nations it has embassies with,
///   proposes alliances with non-threatening GPs, sends grants to MNs with embassies.
/// - **Aggressive**: rarely proposes treaties, breaks alliances more easily.
/// - **Economic**: proposes pacts for trade security, sends grants.
/// - **All AI**: send small grants ($500) to Minor Nations with embassies to improve relations.
pub fn ai_manage_diplomacy(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<super::AiAction>,
) {
    // Auto-make peace with any nation that has 0 provinces left.
    // There is nothing left to fight over, so continuing a war is pointless.
    {
        let war_targets: Vec<NationId> = game
            .world.nations
            .iter()
            .filter(|n| {
                n.id != nation_id
                    && !n.diplomacy.is_in_anarchy
                    && n.province_ids.is_empty()
                    && game
                        .world.diplomacy
                        .get_relation(nation_id, n.id)
                        .is_some_and(|r| r.at_war)
            })
            .map(|n| n.id)
            .collect();

        for target_id in war_targets {
            // Skip peace if we have pending attacks against provinces owned by this nation
            let has_pending_attack = game.transient.pending_attacks.iter().any(|(attacker, prov_id)| {
                *attacker == nation_id
                    && game
                        .get_province(*prov_id)
                        .is_some_and(|p| p.owner == target_id)
            });
            if has_pending_attack {
                continue;
            }
            game.world.diplomacy.queue_peace(nation_id, target_id);
            let nation_name = game
                .get_nation(nation_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let target_name = game
                .get_nation(target_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            actions.push(super::AiAction {
                text: format!(
                    "{} made peace with {} (no provinces remaining)",
                    nation_name, target_name
                ),
                reason: format!(
                    "{} has 0 provinces left; nothing to fight over",
                    target_name
                ),
                is_non_action: false,
                nation_id,
            });
        }
    }

    let personality = get_personality(game, nation_id);
    let turn_number = game.turn.0;

    if game.ai_debug {
        let nation_name = game
            .get_nation(nation_id)
            .map(|n| n.name.as_str())
            .unwrap_or("?");
        eprintln!(
            "[AI:{}:diplomacy] personality={}, turn={}",
            nation_name, personality, turn_number
        );
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

    // Determine behavior parameters based on personality (Lua overrides Rust defaults)
    let propose_pact_chance: bool = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.propose_pacts) {
            break 'val v;
        }
        // All personalities propose NAPs with minor nations where they have an
        // embassy. Aggression is reserved for Great Powers; minor-nation NAPs
        // protect trade partners from poaching.
        true
    };
    let propose_alliance_chance: bool = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.propose_alliances) {
            break 'val v;
        }
        matches!(
            personality,
            AiPersonality::Diplomatic | AiPersonality::Balanced | AiPersonality::Economic
        )
    };
    let pc = PersonalityConfig::for_personality(personality);
    let grant_amount: i64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.grant_amount) {
            break 'val v;
        }
        pc.grant_amount_dollars as i64
    };
    let grant_every_n_turns: u32 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.grant_interval) {
            break 'val v;
        }
        pc.grant_interval_turns
    };

    // Phase 1: Propose non-aggression pacts with Minor Nations that have embassies
    let mut nap_proposed_this_turn = false;
    let mut nap_decline_summary: Option<String> = None;
    if propose_pact_chance {
        // Only propose pacts with minor nations that still have provinces
        let minor_ids: Vec<NationId> = game
            .world.nations
            .iter()
            .filter(|n| !n.is_great_power() && !n.province_ids.is_empty() && !n.diplomacy.is_in_anarchy)
            .map(|n| n.id)
            .collect();

        let mut embassy_partners = 0usize;
        let mut existing_pacts = 0usize;
        for mn_id in &minor_ids {
            let has_embassy = game
                .world.diplomacy
                .get_relation(nation_id, *mn_id)
                .is_some_and(|r| r.has_embassy);
            if has_embassy {
                embassy_partners += 1;
                if game.world.diplomacy.has_treaty(
                    nation_id,
                    *mn_id,
                    crate::events::TreatyType::NonAggressionPact,
                ) {
                    existing_pacts += 1;
                }
            }
        }

        for mn_id in minor_ids {
            let has_embassy = game
                .world.diplomacy
                .get_relation(nation_id, mn_id)
                .is_some_and(|r| r.has_embassy);
            if !has_embassy {
                continue;
            }

            let already_has_pact = game.world.diplomacy.has_treaty(
                nation_id,
                mn_id,
                crate::events::TreatyType::NonAggressionPact,
            );
            if already_has_pact {
                continue;
            }

            if game.world.diplomacy.propose_pact(nation_id, mn_id).is_ok() {
                nap_proposed_this_turn = true;
                let nation_name = game
                    .get_nation(nation_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let mn_name = game
                    .get_nation(mn_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                actions.push(super::AiAction {
                    text: format!(
                        "{} signed a non-aggression pact with {}",
                        nation_name, mn_name
                    ),
                    reason: format!(
                        "{:?} personality favors non-aggression pacts with embassy partners",
                        personality
                    ),
                    is_non_action: false,
                    nation_id,
                });
                let turn = game.turn;
                let entry = crate::events::HistoryEvent::NonAggressionPactSigned {
                    signer: nation_id,
                    partner: mn_id,
                };
                if !game
                    .archive.history
                    .iter()
                    .any(|(t, ev)| *t == turn && *ev == entry)
                {
                    game.archive.history.push((turn, entry));
                }
            }
        }

        if !nap_proposed_this_turn {
            nap_decline_summary = Some(if embassy_partners == 0 {
                "no minor nations with embassy connection".to_string()
            } else if existing_pacts >= embassy_partners {
                format!(
                    "all {} embassy partners already have pacts",
                    embassy_partners
                )
            } else {
                format!(
                    "{} embassy partners but none accepted (propose_pact failed)",
                    embassy_partners
                )
            });
        }
    } else {
        nap_decline_summary = Some(format!(
            "{:?} personality does not pursue non-aggression pacts",
            personality
        ));
    }

    if !nap_proposed_this_turn && let Some(why) = nap_decline_summary {
        let nation_name = game
            .get_nation(nation_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        actions.push(super::AiAction {
            text: format!(
                "{} did not sign any non-aggression pact this turn",
                nation_name
            ),
            reason: why,
            is_non_action: true,
            nation_id,
        });
    }

    // Phase 2: Propose alliances with other Great Powers
    // Wait until turn 10+ so diplomatic history develops, and limit alliances
    let mut alliance_proposed_this_turn = false;
    let mut alliance_decline_summary: Option<String> = None;
    if propose_alliance_chance && turn_number >= 10 {
        let max_alliances: usize = 'val: {
            #[cfg(feature = "lua")]
            if let Some(v) = lua_cfg.as_ref().and_then(|c| c.max_alliances) {
                break 'val v;
            }
            2
        };

        // Count existing alliances to cap
        let existing_alliances: usize = game
            .world.nations
            .iter()
            .filter(|n| {
                n.is_great_power()
                    && n.id != nation_id
                    && game.world.diplomacy.has_treaty(
                        nation_id,
                        n.id,
                        crate::events::TreatyType::Alliance,
                    )
            })
            .count();

        if existing_alliances >= max_alliances {
            alliance_decline_summary = Some(format!(
                "already holds {}/{} alliances; cap reached",
                existing_alliances, max_alliances
            ));
        } else {
            let gp_ids: Vec<NationId> = game
                .world.nations
                .iter()
                .filter(|n| {
                    n.is_great_power()
                        && n.id != nation_id
                        && n.id != game.human_player_nation
                        && !n.diplomacy.is_in_anarchy
                })
                .map(|n| n.id)
                .collect();

            let mut alliances_formed = existing_alliances;
            // Track decline reasons for the best-considered candidate so we can
            // surface a summary non-action if nothing is proposed this turn.
            let mut best_decline: Option<(String, String)> = None; // (gp_name, reason)
            let mut considered_any = false;
            for gp_id in gp_ids {
                considered_any = true;
                // Re-check cap inside loop to prevent forming more than max total
                if alliances_formed >= max_alliances {
                    break;
                }

                let gp_name_for_decline = game
                    .get_nation(gp_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();

                let at_war = game
                    .world.diplomacy
                    .get_relation(nation_id, gp_id)
                    .is_some_and(|r| r.at_war);
                if at_war {
                    best_decline.get_or_insert((
                        gp_name_for_decline.clone(),
                        "currently at war".to_string(),
                    ));
                    continue;
                }

                let already_allied = game.world.diplomacy.has_treaty(
                    nation_id,
                    gp_id,
                    crate::events::TreatyType::Alliance,
                );
                if already_allied {
                    continue;
                }

                // Skip nations with low standing (<50) — AI is less likely to accept treaties
                let partner_standing = game.world.diplomacy.get_standing(gp_id);
                if partner_standing < 50 {
                    best_decline.get_or_insert((
                        gp_name_for_decline.clone(),
                        format!("partner standing {} < 50", partner_standing),
                    ));
                    continue;
                }

                // Only propose if score is positive (non-threatening)
                let score = game
                    .world.diplomacy
                    .get_relation(nation_id, gp_id)
                    .map(|r| r.score)
                    .unwrap_or(0);
                if score < 0 {
                    best_decline.get_or_insert((
                        gp_name_for_decline.clone(),
                        format!("relation score {} is negative", score),
                    ));
                    continue;
                }

                // Create pending proposal — evaluated at end of turn by
                // resolve_diplomatic_proposals()
                if game
                    .world.diplomacy
                    .propose_treaty(
                        nation_id,
                        gp_id,
                        crate::events::TreatyType::Alliance,
                        game.turn,
                    )
                    .is_ok()
                {
                    // Count as "formed" for the max-alliance cap within this turn's
                    // proposal loop so we don't over-propose.
                    alliances_formed += 1;
                    alliance_proposed_this_turn = true;
                    let nation_name = game
                        .get_nation(nation_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let gp_name = game
                        .get_nation(gp_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    actions.push(super::AiAction {
                        text: format!("{} proposes an alliance with {}", nation_name, gp_name),
                        reason: format!(
                            "{:?} personality; relationship score {} and partner standing {}",
                            personality, score, partner_standing
                        ),
                        is_non_action: false,
                        nation_id,
                    });
                }
            }

            if !alliance_proposed_this_turn && alliance_decline_summary.is_none() {
                alliance_decline_summary = Some(match (best_decline, considered_any) {
                    (Some((name, why)), _) => format!("closest candidate {} — {}", name, why),
                    (None, true) => "no non-allied Great Powers remain".to_string(),
                    (None, false) => "no eligible Great Power candidates".to_string(),
                });
            }
        } // end else (existing_alliances < max_alliances)
    } else if !propose_alliance_chance {
        alliance_decline_summary = Some(format!(
            "{:?} personality does not pursue alliances",
            personality
        ));
    } else {
        alliance_decline_summary = Some(format!(
            "turn {} < 10 — diplomatic history too short to justify alliance",
            turn_number
        ));
    }

    // Emit a single summary non-action when no alliance was proposed this turn.
    if !alliance_proposed_this_turn && let Some(why) = alliance_decline_summary {
        let nation_name = game
            .get_nation(nation_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        actions.push(super::AiAction {
            text: format!("{} did not propose any alliance this turn", nation_name),
            reason: why,
            is_non_action: true,
            nation_id,
        });
    }

    // Phase 3: Send cash grants to Minor Nations with embassies
    // Wealthy AIs send much larger grants to burn excess treasury.
    if grant_amount > 0
        && grant_every_n_turns > 0
        && turn_number.is_multiple_of(grant_every_n_turns)
    {
        let treasury_val = game
            .get_nation(nation_id)
            .map(|n| n.economy.treasury.as_dollars())
            .unwrap_or(0);
        let wealth_multiplier = if treasury_val > 100_000 {
            20 // Very wealthy: grant 20x more
        } else if treasury_val > 50_000 {
            10
        } else if treasury_val > 20_000 {
            5
        } else {
            1
        };
        let ai_standing = game.world.diplomacy.get_standing(nation_id);
        let adjusted_grant = if ai_standing > 80 {
            (grant_amount + grant_amount / 2) * wealth_multiplier
        } else {
            grant_amount * wealth_multiplier
        };
        let grant = Money::dollars(adjusted_grant);
        let minor_ids: Vec<NationId> = game
            .world.nations
            .iter()
            .filter(|n| !n.is_great_power() && !n.province_ids.is_empty() && !n.diplomacy.is_in_anarchy)
            .map(|n| n.id)
            .collect();

        for mn_id in minor_ids {
            let has_embassy = game
                .world.diplomacy
                .get_relation(nation_id, mn_id)
                .is_some_and(|r| r.has_embassy);
            if !has_embassy {
                continue;
            }

            let can_afford = game
                .get_nation(nation_id)
                .is_some_and(|n| n.economy.treasury.checked_sub(grant).is_some());
            if !can_afford {
                break;
            }

            game.world.diplomacy.send_grant(nation_id, mn_id, grant);
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.economy.treasury -= grant;
            }
            game.transient.pending_ai_cash_spending.push((
                nation_id,
                crate::economy::ledger::CashSink::AiGrant,
                grant,
                Some(mn_id),
            ));
        }
    }
}

/// AI pre-election strategy: when approaching a decade election (within 4 turns),
/// send larger cash grants to Minor Nation partners to boost relationship scores,
/// and build more consulates/embassies to expand influence.
///
/// - **Diplomatic** personality: sends double grants and builds embassies aggressively
/// - **Balanced/Economic**: sends standard grants
/// - **Aggressive**: does nothing extra
pub fn ai_pre_election_strategy(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<super::AiAction>,
) {
    // Only activate within 4 turns of a decade election
    if !game.turn.is_near_decade_election(4) {
        return;
    }

    let personality = get_personality(game, nation_id);

    // Aggressive nations don't care about elections
    if personality == AiPersonality::Aggressive {
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

    let pc = PersonalityConfig::for_personality(personality);
    let grant_amount: Money = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.grant_amount) {
            break 'val Money::dollars(v);
        }
        Money::dollars(pc.election_grant_dollars as i64)
    };

    // Send grants to all MNs with embassies to boost relationship before the vote
    let minor_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| !n.is_great_power() && !n.province_ids.is_empty() && !n.diplomacy.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    for mn_id in &minor_ids {
        let has_embassy = game
            .world.diplomacy
            .get_relation(nation_id, *mn_id)
            .is_some_and(|r| r.has_embassy);
        if !has_embassy {
            continue;
        }

        let can_afford = game
            .get_nation(nation_id)
            .is_some_and(|n| n.economy.treasury.checked_sub(grant_amount).is_some());
        if !can_afford {
            break;
        }

        game.world.diplomacy.send_grant(nation_id, *mn_id, grant_amount);
        if let Some(nation) = game.get_nation_mut(nation_id) {
            nation.economy.treasury -= grant_amount;
        }
        game.transient.pending_ai_cash_spending.push((
            nation_id,
            crate::economy::ledger::CashSink::AiGrant,
            grant_amount,
            Some(*mn_id),
        ));
    }

    // All personalities try to build embassies with MNs that have consulates,
    // but with different treasury thresholds based on personality.
    let embassy_treasury_threshold: Money = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.embassy_treasury_threshold) {
            break 'val Money::dollars(v);
        }
        Money::dollars(pc.embassy_treasury_threshold_dollars as i64)
    };
    let embassy_cost = Money::dollars(5000);
    let treasury_ok = game
        .get_nation(nation_id)
        .is_some_and(|n| n.economy.treasury >= embassy_treasury_threshold);
    if treasury_ok {
        for mn_id in &minor_ids {
            let has_consulate_no_embassy = game
                .world.diplomacy
                .get_relation(nation_id, *mn_id)
                .is_some_and(|r| r.has_consulate && !r.has_embassy);
            if !has_consulate_no_embassy {
                continue;
            }

            let can_afford = game
                .get_nation(nation_id)
                .is_some_and(|n| n.economy.treasury.checked_sub(embassy_cost).is_some());
            if !can_afford {
                break;
            }

            if game.world.diplomacy.build_embassy(nation_id, *mn_id).is_ok() {
                if let Some(nation) = game.get_nation_mut(nation_id) {
                    nation.economy.treasury -= embassy_cost;
                }
                game.transient.pending_ai_cash_spending.push((
                    nation_id,
                    crate::economy::ledger::CashSink::AiDiplomacyEmbassy,
                    embassy_cost,
                    Some(*mn_id),
                ));
                let nation_name = game
                    .get_nation(nation_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let mn_name = game
                    .get_nation(*mn_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                actions.push(super::AiAction {
                    text: format!(
                        "{} built an embassy in {} ahead of the election",
                        nation_name, mn_name
                    ),
                    reason: format!(
                        "election approaching at turn {}; {:?} personality building influence",
                        game.turn.0, personality
                    ),
                    is_non_action: false,
                    nation_id,
                });
            }
        }
    }
}

/// Minor Nation bonus trade: when a MN has an embassy with a GP and relationship > 50,
/// it offers 1 extra of each resource type the MN has.
///
/// This function should be called during the trade phase to add bonus resources
/// for preferred trade partners.
pub fn minor_nation_bonus_trade(game: &mut GameState) {
    // Collect MN/GP pairs that qualify
    let mut bonus_pairs: Vec<(NationId, NationId)> = Vec::new();

    let minor_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| n.id)
        .collect();
    let gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    for &mn_id in &minor_ids {
        for &gp_id in &gp_ids {
            let qualifies = game
                .world.diplomacy
                .get_relation(mn_id, gp_id)
                .is_some_and(|r| r.has_embassy && r.score > 50);
            if qualifies {
                bonus_pairs.push((mn_id, gp_id));
            }
        }
    }

    // For each qualifying pair, give 1 bonus of each resource type the GP has
    // (simulating the MN offering bonus trade goods to preferred partners)
    for (_, gp_id) in bonus_pairs {
        let resources = [
            ResourceType::Timber,
            ResourceType::Iron,
            ResourceType::Coal,
            ResourceType::Grain,
            ResourceType::Cotton,
            ResourceType::Wool,
        ];
        if let Some(gp) = game.get_nation_mut(gp_id) {
            for resource in &resources {
                gp.add_resource(*resource, 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::{test_game_with_ai, test_game_with_ai_and_minor};
    use crate::hex::HexCoord;
    use crate::map::Province;
    use crate::nation::{Nation, NationColor};

    /// Set a tile with tradeable resource (Forest + Timber) at the given coord,
    /// assigned to the given province. Required so ai_build_consulates sees trade potential.
    fn set_tradeable_tile(game: &mut GameState, coord: HexCoord, province_id: ProvinceId) {
        let mut tile = crate::map::tile::Tile::with_province(TerrainType::Forest, province_id);
        tile.set_resource(ResourceType::Timber);
        game.world.hex_map.set_tile(coord, tile);
    }

    #[test]
    fn diplomatic_ai_builds_more_consulates() {
        let mut game = test_game_with_ai_and_minor();
        // Add more minor nations for the test
        let province4 = Province::new(
            ProvinceId(4),
            "Minor Capital 2".to_string(),
            NationId(4),
            HexCoord::new(7, 7),
            vec![HexCoord::new(7, 7)],
            3,
        );
        let province5 = Province::new(
            ProvinceId(5),
            "Minor Capital 3".to_string(),
            NationId(5),
            HexCoord::new(8, 8),
            vec![HexCoord::new(8, 8)],
            3,
        );
        let province6 = Province::new(
            ProvinceId(6),
            "Minor Capital 4".to_string(),
            NationId(6),
            HexCoord::new(9, 9),
            vec![HexCoord::new(9, 9)],
            3,
        );
        let province7 = Province::new(
            ProvinceId(7),
            "Minor Capital 5".to_string(),
            NationId(7),
            HexCoord::new(4, 4),
            vec![HexCoord::new(4, 4)],
            3,
        );
        game.world.provinces.push(province4);
        game.world.provinces.push(province5);
        game.world.provinces.push(province6);
        game.world.provinces.push(province7);
        game.world.nations.push(Nation::new(
            NationId(4),
            "Minor2".to_string(),
            NationColor::Brown,
            NationType::MinorNation,
            ProvinceId(4),
        ));
        game.world.nations.push(Nation::new(
            NationId(5),
            "Minor3".to_string(),
            NationColor::Pink,
            NationType::MinorNation,
            ProvinceId(5),
        ));
        game.world.nations.push(Nation::new(
            NationId(6),
            "Minor4".to_string(),
            NationColor::Teal,
            NationType::MinorNation,
            ProvinceId(6),
        ));
        game.world.nations.push(Nation::new(
            NationId(7),
            "Minor5".to_string(),
            NationColor::Olive,
            NationType::MinorNation,
            ProvinceId(7),
        ));

        // Set tradeable tiles so consulate builder sees trade potential
        set_tradeable_tile(&mut game, HexCoord::new(5, 5), ProvinceId(3));
        set_tradeable_tile(&mut game, HexCoord::new(7, 7), ProvinceId(4));
        set_tradeable_tile(&mut game, HexCoord::new(8, 8), ProvinceId(5));
        set_tradeable_tile(&mut game, HexCoord::new(9, 9), ProvinceId(6));
        set_tradeable_tile(&mut game, HexCoord::new(4, 4), ProvinceId(7));

        // Set Diplomatic personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Diplomatic);
        ai.economy.treasury = Money::dollars(10000);

        ai_build_consulates(&mut game, NationId(2));

        // Count consulates built
        let consulate_count = [
            NationId(3),
            NationId(4),
            NationId(5),
            NationId(6),
            NationId(7),
        ]
        .iter()
        .filter(|&&mn_id| {
            game.world.diplomacy
                .get_relation(NationId(2), mn_id)
                .is_some_and(|r| r.has_consulate)
        })
        .count();

        assert_eq!(
            consulate_count, 4,
            "Diplomatic AI should build up to 4 consulates"
        );
    }

    #[test]
    fn balanced_ai_builds_fewer_consulates() {
        let mut game = test_game_with_ai_and_minor();
        // Add more minor nations
        game.world.provinces.push(Province::new(
            ProvinceId(4),
            "Minor2 Capital".to_string(),
            NationId(4),
            HexCoord::new(7, 7),
            vec![HexCoord::new(7, 7)],
            3,
        ));
        game.world.provinces.push(Province::new(
            ProvinceId(5),
            "Minor3 Capital".to_string(),
            NationId(5),
            HexCoord::new(8, 8),
            vec![HexCoord::new(8, 8)],
            3,
        ));
        game.world.provinces.push(Province::new(
            ProvinceId(6),
            "Minor4 Capital".to_string(),
            NationId(6),
            HexCoord::new(9, 9),
            vec![HexCoord::new(9, 9)],
            3,
        ));
        game.world.nations.push(Nation::new(
            NationId(4),
            "Minor2".to_string(),
            NationColor::Brown,
            NationType::MinorNation,
            ProvinceId(4),
        ));
        game.world.nations.push(Nation::new(
            NationId(5),
            "Minor3".to_string(),
            NationColor::Pink,
            NationType::MinorNation,
            ProvinceId(5),
        ));
        game.world.nations.push(Nation::new(
            NationId(6),
            "Minor4".to_string(),
            NationColor::Teal,
            NationType::MinorNation,
            ProvinceId(6),
        ));

        // Set tradeable tiles so consulate builder sees trade potential
        set_tradeable_tile(&mut game, HexCoord::new(5, 5), ProvinceId(3));
        set_tradeable_tile(&mut game, HexCoord::new(7, 7), ProvinceId(4));
        set_tradeable_tile(&mut game, HexCoord::new(8, 8), ProvinceId(5));
        set_tradeable_tile(&mut game, HexCoord::new(9, 9), ProvinceId(6));

        // Set Balanced personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.economy.treasury = Money::dollars(10000);

        ai_build_consulates(&mut game, NationId(2));

        // Count consulates built
        let consulate_count = [NationId(3), NationId(4), NationId(5), NationId(6)]
            .iter()
            .filter(|&&mn_id| {
                game.world.diplomacy
                    .get_relation(NationId(2), mn_id)
                    .is_some_and(|r| r.has_consulate)
            })
            .count();

        assert_eq!(
            consulate_count, 2,
            "Balanced AI should build up to 2 consulates"
        );
    }

    #[test]
    fn ai_does_not_build_consulates_when_poor() {
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.economy.treasury = Money::dollars(1000); // Below $2,000 threshold

        ai_build_consulates(&mut game, NationId(2));

        let has_consulate = game
            .world.diplomacy
            .get_relation(NationId(2), NationId(3))
            .is_some_and(|r| r.has_consulate);
        assert!(
            !has_consulate,
            "AI should not build consulates when treasury < $2,000"
        );
    }

    #[test]
    fn diplomatic_ai_proposes_pacts_with_embassy_nations() {
        let mut game = test_game_with_ai_and_minor();
        let ai_id = NationId(2);
        let mn_id = NationId(3);

        // Set AI to Diplomatic personality
        game.get_nation_mut(ai_id).unwrap().diplomacy.ai_personality = Some(AiPersonality::Diplomatic);

        // Build consulate and embassy for the AI with the minor nation
        game.world.diplomacy.build_consulate(ai_id, mn_id).unwrap();
        game.world.diplomacy.build_embassy(ai_id, mn_id).unwrap();

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        // Diplomatic AI should propose a pact
        assert!(
            game.world.diplomacy
                .has_treaty(ai_id, mn_id, crate::events::TreatyType::NonAggressionPact),
            "Diplomatic AI should propose pact with Minor Nation it has embassy with"
        );
        assert!(
            actions
                .iter()
                .any(|a| a.text.contains("non-aggression pact")),
            "Should report pact in actions"
        );
    }

    #[test]
    fn aggressive_ai_proposes_nap_with_embassy_minor() {
        // Aggressive personalities still sign NAPs with minor nations where
        // they have an embassy — the aggression is reserved for Great Powers,
        // and minor-nation NAPs protect their trade partners from poaching.
        let mut game = test_game_with_ai_and_minor();
        let ai_id = NationId(2);
        let mn_id = NationId(3);

        game.get_nation_mut(ai_id).unwrap().diplomacy.ai_personality = Some(AiPersonality::Aggressive);

        game.world.diplomacy.build_consulate(ai_id, mn_id).unwrap();
        game.world.diplomacy.build_embassy(ai_id, mn_id).unwrap();

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        assert!(
            game.world.diplomacy
                .has_treaty(ai_id, mn_id, crate::events::TreatyType::NonAggressionPact),
            "Aggressive AI should propose NAP with embassy minor nation"
        );
    }

    #[test]
    fn aggressive_ai_does_not_propose_alliances_or_grants() {
        // Aggressive still refuses alliances and grants (unchanged behaviour).
        let mut game = test_game_with_ai_and_minor();
        let ai_id = NationId(2);
        let mn_id = NationId(3);

        game.get_nation_mut(ai_id).unwrap().diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        game.world.diplomacy.build_consulate(ai_id, mn_id).unwrap();
        game.world.diplomacy.build_embassy(ai_id, mn_id).unwrap();

        let score_before = game.world.diplomacy.get_relation(ai_id, mn_id).unwrap().score;

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        let score_after = game.world.diplomacy.get_relation(ai_id, mn_id).unwrap().score;
        // NAP adds +10 to relationship; grants would add another +5. Assert
        // only the NAP increment occurred (no grants).
        assert_eq!(
            score_after - score_before,
            10,
            "Aggressive AI should gain +10 from NAP but not +5 from a grant"
        );
    }

    #[test]
    fn diplomatic_ai_sends_grants() {
        let mut game = test_game_with_ai_and_minor();
        let ai_id = NationId(2);
        let mn_id = NationId(3);

        // Set AI to Diplomatic personality
        game.get_nation_mut(ai_id).unwrap().diplomacy.ai_personality = Some(AiPersonality::Diplomatic);
        // Set turn to multiple of 4 so Diplomatic AI sends grants
        game.turn = TurnNumber::new(4);

        // Build consulate and embassy
        game.world.diplomacy.build_consulate(ai_id, mn_id).unwrap();
        game.world.diplomacy.build_embassy(ai_id, mn_id).unwrap();

        let score_before = game.world.diplomacy.get_relation(ai_id, mn_id).unwrap().score;

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        let score_after = game.world.diplomacy.get_relation(ai_id, mn_id).unwrap().score;

        // Score should have improved (pact gives +10, grant gives +5)
        assert!(
            score_after > score_before,
            "AI grant should improve relationship score (before: {}, after: {})",
            score_before,
            score_after
        );

        // Treasury should have decreased by $500 for the grant
        assert!(
            game.get_nation(ai_id).unwrap().economy.treasury < Money::dollars(10000),
            "AI treasury should decrease after sending grant"
        );
    }

    #[test]
    fn diplomatic_ai_proposes_alliances_with_gps() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Set AI to Diplomatic personality
        game.get_nation_mut(ai_id).unwrap().diplomacy.ai_personality = Some(AiPersonality::Diplomatic);

        // Add a third GP that is AI-controlled (non-human, non-current-AI)
        let mut gp3 = Nation::new(
            NationId(4),
            "ThirdPower".to_string(),
            NationColor::Green,
            NationType::GreatPower,
            ProvinceId(2),
        );
        gp3.diplomacy.ai_personality = Some(AiPersonality::Diplomatic); // Diplomatic bias +0.4 makes acceptance likely
        gp3.economy.treasury = Money::dollars(10000);
        game.world.nations.push(gp3);

        // Initialize GP embassies (so they have embassies with each other)
        let gp_ids: Vec<NationId> = game
            .world.nations
            .iter()
            .filter(|n| n.is_great_power())
            .map(|n| n.id)
            .collect();
        game.world.diplomacy.initialize_great_powers(&gp_ids);

        // Give positive relationship so the alliance evaluation passes
        game.world.diplomacy
            .ensure_relation(ai_id, NationId(4))
            .improve_score(30);

        // Advance turn to 10+ so alliance proposals are allowed
        game.turn = TurnNumber(10);

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        // Diplomatic AI should create a pending alliance proposal with ThirdPower
        // (alliance is no longer formed inline — resolved at end of turn)
        assert!(
            game.world.diplomacy
                .pending_proposals
                .iter()
                .any(|p| p.from == ai_id
                    && p.to == NationId(4)
                    && p.proposal_type == crate::events::TreatyType::Alliance),
            "Diplomatic AI should propose alliance with non-threatening GP"
        );
    }

    #[test]
    fn ai_pre_election_grants_within_4_turns() {
        let mut game = test_game_with_ai_and_minor();
        // Set turn to 3 turns before an election at 1825 Q1 (turn 41).
        // Turn 38 = 1824 Q2 (within 4 turns of turn 41)
        game.turn = TurnNumber::from_year_quarter(1824, 2);

        // Give AI a Balanced personality (not Aggressive, so it will send grants)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.economy.treasury = Money::dollars(10000);

        // Set up embassy between AI(2) and MN(3)
        game.world.diplomacy
            .build_consulate(NationId(2), NationId(3))
            .unwrap();
        game.world.diplomacy
            .build_embassy(NationId(2), NationId(3))
            .unwrap();

        let score_before = game
            .world.diplomacy
            .get_relation(NationId(2), NationId(3))
            .unwrap()
            .score;

        let mut actions = Vec::new();
        ai_pre_election_strategy(&mut game, NationId(2), &mut actions);

        let score_after = game
            .world.diplomacy
            .get_relation(NationId(2), NationId(3))
            .unwrap()
            .score;
        assert!(
            score_after > score_before,
            "Pre-election grants should improve relationship: before={}, after={}",
            score_before,
            score_after
        );

        // Treasury should have decreased
        let treasury_after = game.get_nation(NationId(2)).unwrap().economy.treasury;
        assert!(
            treasury_after < Money::dollars(10000),
            "Treasury should decrease from pre-election grants"
        );
    }

    #[test]
    fn ai_pre_election_does_nothing_when_far_from_election() {
        let mut game = test_game_with_ai_and_minor();
        // Set turn to 1820 Q1 — far from any election
        game.turn = TurnNumber::from_year_quarter(1820, 1);

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.economy.treasury = Money::dollars(10000);

        // Set up embassy
        game.world.diplomacy
            .build_consulate(NationId(2), NationId(3))
            .unwrap();
        game.world.diplomacy
            .build_embassy(NationId(2), NationId(3))
            .unwrap();

        let treasury_before = game.get_nation(NationId(2)).unwrap().economy.treasury;
        let mut actions = Vec::new();
        ai_pre_election_strategy(&mut game, NationId(2), &mut actions);

        let treasury_after = game.get_nation(NationId(2)).unwrap().economy.treasury;
        assert_eq!(
            treasury_before, treasury_after,
            "Pre-election strategy should do nothing when far from election"
        );
    }

    #[test]
    fn ai_aggressive_ignores_pre_election() {
        let mut game = test_game_with_ai_and_minor();
        // Set turn near election
        game.turn = TurnNumber::from_year_quarter(1824, 2);

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        ai.economy.treasury = Money::dollars(10000);

        // Set up embassy
        game.world.diplomacy
            .build_consulate(NationId(2), NationId(3))
            .unwrap();
        game.world.diplomacy
            .build_embassy(NationId(2), NationId(3))
            .unwrap();

        let treasury_before = game.get_nation(NationId(2)).unwrap().economy.treasury;
        let mut actions = Vec::new();
        ai_pre_election_strategy(&mut game, NationId(2), &mut actions);

        let treasury_after = game.get_nation(NationId(2)).unwrap().economy.treasury;
        assert_eq!(
            treasury_before, treasury_after,
            "Aggressive AI should ignore pre-election strategy"
        );
    }

    #[test]
    fn no_alliances_formed_before_turn_10() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Set AI to Diplomatic personality (the only one that proposes alliances)
        game.get_nation_mut(ai_id).unwrap().diplomacy.ai_personality = Some(AiPersonality::Diplomatic);

        // Add a third GP that is AI-controlled
        let mut gp3 = Nation::new(
            NationId(4),
            "ThirdPower".to_string(),
            NationColor::Green,
            NationType::GreatPower,
            ProvinceId(2),
        );
        gp3.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        gp3.economy.treasury = Money::dollars(10000);
        game.world.nations.push(gp3);

        // Initialize GP embassies
        let gp_ids: Vec<NationId> = game
            .world.nations
            .iter()
            .filter(|n| n.is_great_power())
            .map(|n| n.id)
            .collect();
        game.world.diplomacy.initialize_great_powers(&gp_ids);

        // Turn 1: alliances should NOT be proposed (turn < 10)
        game.turn = TurnNumber(1);

        let mut actions = Vec::new();
        ai_manage_diplomacy(&mut game, ai_id, &mut actions);

        let alliance_count: usize = game
            .world.nations
            .iter()
            .filter(|n| {
                n.is_great_power()
                    && n.id != ai_id
                    && game
                        .world.diplomacy
                        .has_treaty(ai_id, n.id, crate::events::TreatyType::Alliance)
            })
            .count();
        assert_eq!(
            alliance_count, 0,
            "No alliances should form before turn 10; got {}",
            alliance_count
        );
    }

    #[test]
    fn alliances_capped_at_two() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Set AI to Diplomatic personality
        game.get_nation_mut(ai_id).unwrap().diplomacy.ai_personality = Some(AiPersonality::Diplomatic);

        // Add multiple AI-controlled Great Powers
        for i in 4..=7 {
            let mut gp = Nation::new(
                NationId(i),
                format!("Power{}", i),
                NationColor::Green,
                NationType::GreatPower,
                ProvinceId(2),
            );
            gp.diplomacy.ai_personality = Some(AiPersonality::Balanced);
            gp.economy.treasury = Money::dollars(10000);
            game.world.nations.push(gp);
        }

        // Initialize GP embassies
        let gp_ids: Vec<NationId> = game
            .world.nations
            .iter()
            .filter(|n| n.is_great_power())
            .map(|n| n.id)
            .collect();
        game.world.diplomacy.initialize_great_powers(&gp_ids);

        // Set turn to 40 (well past the turn 10 threshold)
        game.turn = TurnNumber(40);

        // Run diplomacy multiple times to give it every chance to form alliances
        for _ in 0..5 {
            let mut actions = Vec::new();
            ai_manage_diplomacy(&mut game, ai_id, &mut actions);
        }

        let alliance_count: usize = game
            .world.nations
            .iter()
            .filter(|n| {
                n.is_great_power()
                    && n.id != ai_id
                    && game
                        .world.diplomacy
                        .has_treaty(ai_id, n.id, crate::events::TreatyType::Alliance)
            })
            .count();
        assert!(
            alliance_count <= 2,
            "Diplomatic AI should have at most 2 alliances; got {}",
            alliance_count
        );
    }
}
