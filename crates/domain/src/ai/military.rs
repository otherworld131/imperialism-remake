#![allow(unused_labels)]
use crate::game_state::GameState;
#[cfg(test)]
use crate::military::units::ArmyUnit;
use crate::military::units::ArmyUnitType;
use crate::types::*;

#[cfg(test)]
use super::common::AiPersonality;
#[cfg(test)]
use super::common::lua_or;
#[cfg(test)]
use super::common::next_unit_id;
use super::common::{PersonalityConfig, get_personality};

/// Build military units when the nation has sufficient treasury.
/// Personality affects thresholds and unit preferences:
///
/// - **Aggressive**: lower thresholds, prefer artillery
/// - **Diplomatic**: higher thresholds, fewer units
/// - **Economic**: moderate thresholds
/// - **Balanced**: default behavior
#[cfg(test)]
pub(crate) fn ai_build_military(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<super::AiAction>,
) {
    let personality = get_personality(game, nation_id);
    let turn_number = game.turn.0;

    if game.ai_debug {
        let n = game.get_nation(nation_id);
        let nation_name = n.map(|n| n.name.as_str()).unwrap_or("?");
        let army_count = n.map(|n| n.field_army_count()).unwrap_or(0);
        let treasury = n.map(|n| n.economy.treasury.as_dollars()).unwrap_or(0);
        eprintln!(
            "[AI:{}:military] army={}, treasury=${}, personality={}",
            nation_name, army_count, treasury, personality
        );
    }

    // ── Read Lua config (feature-gated) ──────────────────────
    // Must happen before the mutable borrow of game below.
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
    #[cfg(not(feature = "lua"))]
    let _lua_cfg: Option<()> = None;

    let defaults = PersonalityConfig::for_personality(personality);

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // "Army count" for tier-based build decisions is the *field army* —
    // garrison militia do not count toward tier caps (they can't project
    // power anyway).
    let army_count = nation.field_army_count();
    let treasury = nation.economy.treasury;
    let capital = nation.capital_province_id;
    let nation_name = nation.name.clone();

    // Deterministic per-nation seed for unit-type variety
    let variety_seed = (turn_number as usize).wrapping_mul(nation_id.0 as usize + 7);

    // Thresholds: Lua overrides personality defaults via lua_or.
    let tier1_max = lua_or(
        lua_cfg.as_ref().and_then(|c| c.tier1_army_max),
        defaults.tier1_army_max,
    );
    let tier1_treasury = lua_or(
        lua_cfg
            .as_ref()
            .and_then(|c| c.tier1_treasury.map(Money::dollars)),
        defaults.tier1_treasury,
    );
    let tier2_max = lua_or(
        lua_cfg.as_ref().and_then(|c| c.tier2_army_max),
        defaults.tier2_army_max,
    );
    let tier2_treasury = lua_or(
        lua_cfg
            .as_ref()
            .and_then(|c| c.tier2_treasury.map(Money::dollars)),
        defaults.tier2_treasury,
    );
    let tier3_treasury = lua_or(
        lua_cfg
            .as_ref()
            .and_then(|c| c.tier3_treasury.map(Money::dollars)),
        defaults.tier3_treasury,
    );

    if army_count < tier1_max && treasury > tier1_treasury {
        // Tier 1: pick from basic unit types with personality bias + variety
        let tier1_options: &[ArmyUnitType] = match personality {
            AiPersonality::Aggressive => &[
                ArmyUnitType::Regulars,
                ArmyUnitType::Regulars,
                ArmyUnitType::Grenadiers,
            ],
            AiPersonality::Diplomatic => &[
                ArmyUnitType::Regulars,
                ArmyUnitType::Regulars,
                ArmyUnitType::Regulars,
            ],
            AiPersonality::Economic => &[
                ArmyUnitType::Regulars,
                ArmyUnitType::Regulars,
                ArmyUnitType::Grenadiers,
            ],
            AiPersonality::Balanced => &[
                ArmyUnitType::Regulars,
                ArmyUnitType::Grenadiers,
                ArmyUnitType::Regulars,
            ],
        };
        let unit_type = tier1_options[variety_seed % tier1_options.len()];
        let cost = match unit_type {
            ArmyUnitType::Grenadiers => Money::dollars(1000),
            _ => Money::dollars(500),
        };
        if treasury > tier1_treasury + cost {
            nation.economy.treasury -= cost;
            let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
            nation.military.army.push(unit);
            actions.push(super::AiAction {
                text: format!("{} has been expanding its military forces", nation_name),
                reason: format!(
                    "Tier 1 build: army={}/{} cap, treasury=${}, personality={}",
                    army_count + 1,
                    tier1_max,
                    treasury.as_dollars(),
                    personality
                ),
                is_non_action: false,
                nation_id,
            });
        }
    } else if army_count < tier2_max && treasury > tier2_treasury {
        // Tier 2: mix of grenadiers and artillery with personality + variety
        let tier2_options: &[ArmyUnitType] = match personality {
            AiPersonality::Aggressive => &[
                ArmyUnitType::LightArtillery,
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
            ],
            AiPersonality::Diplomatic => &[
                ArmyUnitType::Grenadiers,
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
            ],
            AiPersonality::Economic => &[
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
                ArmyUnitType::Grenadiers,
            ],
            AiPersonality::Balanced => &[
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
                ArmyUnitType::Grenadiers,
            ],
        };
        let unit_type = tier2_options[variety_seed % tier2_options.len()];
        let build_cost = if unit_type == ArmyUnitType::LightArtillery {
            Money::dollars(2000)
        } else {
            Money::dollars(1000)
        };
        if treasury > tier2_treasury + build_cost {
            nation.economy.treasury -= build_cost;
            let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
            nation.military.army.push(unit);
            actions.push(super::AiAction {
                text: format!("{} has been expanding its military forces", nation_name),
                reason: format!(
                    "Tier 2 build: army={}/{} cap, treasury=${}",
                    army_count + 1,
                    tier2_max,
                    treasury.as_dollars()
                ),
                is_non_action: false,
                nation_id,
            });
        }
    } else if army_count >= tier2_max && treasury > tier3_treasury {
        // Tier 3: advanced units with some variety
        // Cap total army size to prevent runaway military buildup
        let tier3_max: usize = 'val: {
            #[cfg(feature = "lua")]
            if let Some(v) = lua_cfg.as_ref().and_then(|c| c.tier3_army_max) {
                break 'val v;
            }
            defaults.tier3_army_max
        };
        if army_count < tier3_max {
            let tier3_options: &[ArmyUnitType] = match personality {
                AiPersonality::Aggressive => &[
                    ArmyUnitType::LightArtillery,
                    ArmyUnitType::LightArtillery,
                    ArmyUnitType::Grenadiers,
                ],
                _ => &[
                    ArmyUnitType::LightArtillery,
                    ArmyUnitType::Grenadiers,
                    ArmyUnitType::LightArtillery,
                ],
            };
            // Build up to 2 units per turn when treasury is very high (> $20,000)
            let units_to_build = if treasury > Money::dollars(20_000) {
                2
            } else {
                1
            };
            for i in 0..units_to_build {
                if nation.field_army_count() >= tier3_max {
                    break;
                }
                let unit_type = tier3_options[(variety_seed.wrapping_add(i)) % tier3_options.len()];
                let cost = if unit_type == ArmyUnitType::LightArtillery {
                    Money::dollars(2000)
                } else {
                    Money::dollars(1000)
                };
                if let Some(remaining) = nation.economy.treasury.checked_sub(cost) {
                    nation.economy.treasury = remaining;
                    let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
                    nation.military.army.push(unit);
                    if i == 0 {
                        actions.push(super::AiAction {
                            text: format!("{} has been expanding its military forces", nation_name),
                            reason: format!(
                                "Tier 3 advanced build: army={}/{} cap, building {:?}",
                                nation.field_army_count(),
                                tier3_max,
                                unit_type
                            ),
                            is_non_action: false,
                            nation_id,
                        });
                    }
                } else {
                    break;
                }
            }
        } else {
            // Tier 4: uncapped expansion when treasury is very high.
            // Nations with massive wealth keep building past tier3 cap.
            let tier4_treasury: Money = 'val: {
                #[cfg(feature = "lua")]
                if let Some(v) = lua_cfg.as_ref().and_then(|c| c.tier4_treasury) {
                    break 'val Money::dollars(v);
                }
                Money::dollars(30_000)
            };
            if treasury > tier4_treasury {
                let unit_type = ArmyUnitType::LightArtillery;
                let cost = Money::dollars(2000);
                if let Some(remaining) = nation.economy.treasury.checked_sub(cost) {
                    nation.economy.treasury = remaining;
                    let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
                    nation.military.army.push(unit);
                    actions.push(super::AiAction {
                        text: format!("{} has been expanding its military forces", nation_name),
                        reason: format!(
                            "Tier 4 uncapped expansion: army={}, treasury=${}",
                            nation.field_army_count(),
                            treasury.as_dollars()
                        ),
                        is_non_action: false,
                        nation_id,
                    });
                }
            }
        }
    }
}

/// Compute the (attacker, target) coalition firepower used by the war-decision
/// `army_ratio`. Both sides include allies; for minor targets the defending
/// coalition also includes NAP pact-defense holders. Cards #115/#116:
/// target-side allies that are currently at war with someone other than the
/// attacker are half-credited (tied up in another conflict). Anarchic nations
/// are excluded from both coalitions — an anarchic state has no offensive
/// military capability and cannot reinforce or be reinforced. The target
/// itself contributes its raw firepower regardless (anarchy on the target is
/// rare and the loop above already gates anarchic targets out).
pub(crate) fn coalition_firepower_for_war_decision(
    game: &GameState,
    attacker: NationId,
    target: NationId,
) -> (f64, f64) {
    let is_anarchic = |id: NationId| {
        game.get_nation(id)
            .is_some_and(|n| n.diplomacy.is_in_anarchy)
    };

    let our_coalition = super::assessment::collect_hypothetical_coalition(game, attacker, target);
    let target_coalition =
        super::assessment::collect_target_hypothetical_coalition(game, attacker, target);
    let our_military: f64 = our_coalition
        .iter()
        .filter(|&&id| id == attacker || !is_anarchic(id))
        .map(|&id| super::assessment::nation_military_score(game, id, 0.0))
        .sum();
    let target_military: f64 = target_coalition
        .iter()
        .filter(|&&id| id == target || !is_anarchic(id))
        .map(|&id| {
            let raw = super::assessment::nation_military_score(game, id, 0.0);
            if id == target {
                return raw;
            }
            if game
                .world
                .diplomacy
                .is_at_war_with_anyone_except(id, attacker)
            {
                raw * 0.5
            } else {
                raw
            }
        })
        .sum();
    (our_military, target_military)
}

/// Unified war-declaration logic with cooldown + need/opportunity scoring.
///
/// Every turn, each AI nation evaluates ALL other nations (minor and GP) as
/// potential targets using a combined score of need, opportunity, and
/// relationship penalty. Personality affects cooldown, thresholds, army
/// minimums, and opportunism weighting.
pub(crate) fn ai_declare_wars(
    game: &mut GameState,
    ai_nation_ids: &[NationId],
    actions: &mut Vec<super::AiAction>,
) {
    let turn_number = game.turn.0;

    // Anti-dogpile: track targets selected this round so multiple AIs
    // don't pile onto the same nation in a single turn.
    let mut targeted_this_round: Vec<NationId> = Vec::new();

    for &ai_id in ai_nation_ids {
        let personality = get_personality(game, ai_id);

        let pc = PersonalityConfig::for_personality(personality);

        // ── Read Lua overrides (feature-gated) ─────────────────
        #[cfg(feature = "lua")]
        let lua_cfg = game
            .game_data
            .lua_engine
            .as_ref()
            .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
        #[cfg(not(feature = "lua"))]
        let _lua_cfg: Option<()> = None;

        #[cfg(feature = "lua")]
        let war_cooldown = lua_cfg
            .as_ref()
            .and_then(|c| c.war_cooldown)
            .unwrap_or(pc.war_cooldown);
        #[cfg(not(feature = "lua"))]
        let war_cooldown = pc.war_cooldown;

        #[cfg(feature = "lua")]
        let army_min_for_war = lua_cfg
            .as_ref()
            .and_then(|c| c.army_min_for_war)
            .unwrap_or(pc.army_min_for_war);
        #[cfg(not(feature = "lua"))]
        let army_min_for_war = pc.army_min_for_war;

        #[cfg(feature = "lua")]
        let war_threshold = lua_cfg
            .as_ref()
            .and_then(|c| c.war_threshold)
            .unwrap_or(pc.war_threshold);
        #[cfg(not(feature = "lua"))]
        let war_threshold = pc.war_threshold;

        #[cfg(feature = "lua")]
        let opportunism_weight = lua_cfg
            .as_ref()
            .and_then(|c| c.opportunism_weight)
            .unwrap_or(pc.opportunism_weight);
        #[cfg(not(feature = "lua"))]
        let opportunism_weight = pc.opportunism_weight;

        // Opportunity gate + resource-bonus tunables (Lua-overridable)
        #[cfg(feature = "lua")]
        let min_opp_start = lua_cfg
            .as_ref()
            .and_then(|c| c.min_opportunity_start)
            .unwrap_or(pc.opp_start);
        #[cfg(feature = "lua")]
        let min_opp_end = lua_cfg
            .as_ref()
            .and_then(|c| c.min_opportunity_end)
            .unwrap_or(pc.opp_end);
        #[cfg(feature = "lua")]
        let opp_decay_turns = lua_cfg
            .as_ref()
            .and_then(|c| c.min_opportunity_decay_turns)
            .unwrap_or(pc.opp_decay_turns);
        #[cfg(feature = "lua")]
        let resource_bonus_per_missing = lua_cfg
            .as_ref()
            .and_then(|c| c.resource_bonus_per_missing)
            .unwrap_or(pc.res_per_missing);
        #[cfg(feature = "lua")]
        let resource_bonus_cap = lua_cfg
            .as_ref()
            .and_then(|c| c.resource_bonus_cap)
            .unwrap_or(pc.res_cap);
        #[cfg(not(feature = "lua"))]
        let (min_opp_start, min_opp_end, opp_decay_turns) =
            (pc.opp_start, pc.opp_end, pc.opp_decay_turns);
        #[cfg(not(feature = "lua"))]
        let (resource_bonus_per_missing, resource_bonus_cap) = (pc.res_per_missing, pc.res_cap);

        // Linear decay of the opportunity gate. Turns are 1-based (turn 1 is
        // the first turn of the game), so subtract 1 to make turn 1 = start
        // and turn (1 + decay_turns) = end. Attacking a peer is a risky bet
        // early on; later the bar relaxes as real power imbalances emerge.
        //
        // Defensively clamp `end <= start` here too. `LuaAiConfig::sanitize`
        // enforces this when both fields are set in Lua, but a script that
        // overrides only `end` (letting `start` fall back to the per-personality
        // default) can still produce an inverted pair after fallback. This
        // second clamp guarantees the floor is monotonically non-increasing.
        let min_opp_end = min_opp_end.min(min_opp_start);
        let effective_turn = turn_number.saturating_sub(1);
        let decay_t = (effective_turn as f64 / opp_decay_turns.max(1) as f64).min(1.0);
        let min_opportunity_for_war = min_opp_start - (min_opp_start - min_opp_end) * decay_t;

        let attacker_name = game
            .get_nation(ai_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        // ── 1. Cooldown check ──────────────────────────────────
        let last_war_turn: Option<u32> = game
            .archive
            .history
            .iter()
            .filter(|(_, ev)| {
                matches!(
                    ev,
                    crate::events::HistoryEvent::WarDeclared { attacker, .. } if *attacker == ai_id
                )
            })
            .map(|(t, _)| t.0)
            .max();
        if let Some(last) = last_war_turn
            && turn_number.saturating_sub(last) < war_cooldown
        {
            actions.push(super::AiAction {
                text: format!("{} did not declare war this turn", attacker_name),
                reason: format!(
                    "war cooldown active: last war {} turns ago, cooldown is {} turns",
                    turn_number.saturating_sub(last),
                    war_cooldown
                ),
                is_non_action: true,
                nation_id: ai_id,
            });
            continue;
        }

        // ── 2. Military readiness ──────────────────────────────
        // Only field units count for war readiness — garrisons stay home.
        let ai_army = game
            .get_nation(ai_id)
            .map(|n| n.field_army_count())
            .unwrap_or(0);
        if ai_army < army_min_for_war {
            actions.push(super::AiAction {
                text: format!("{} did not declare war this turn", attacker_name),
                reason: format!(
                    "army too small: {} units < minimum {} for war",
                    ai_army, army_min_for_war
                ),
                is_non_action: true,
                nation_id: ai_id,
            });
            continue;
        }

        // ── 3. Standing check ──────────────────────────────────
        let standing = game.world.diplomacy.get_standing(ai_id);
        if standing < 30 {
            actions.push(super::AiAction {
                text: format!("{} did not declare war this turn", attacker_name),
                reason: format!(
                    "diplomatic standing too low: {}/30 — a pariah nation cannot afford another war",
                    standing
                ),
                is_non_action: true,
                nation_id: ai_id,
            });
            continue;
        }

        // ── 3b. Already at war check ──────────────────────────
        if game.world.diplomacy.is_at_war_with_anyone(ai_id) {
            actions.push(super::AiAction {
                text: format!("{} did not declare war this turn", attacker_name),
                reason: "already at war — cannot open a second front".to_string(),
                is_non_action: true,
                nation_id: ai_id,
            });
            continue;
        }

        // ── 4. Target evaluation ───────────────────────────────
        // Collect AI warehouse resources for need scoring
        let ai_resources: std::collections::HashSet<ResourceType> = game
            .get_nation(ai_id)
            .map(|n| {
                n.economy
                    .warehouse
                    .iter()
                    .filter(|(_, qty)| **qty > 0)
                    .map(|(r, _)| *r)
                    .collect()
            })
            .unwrap_or_default();

        struct Candidate {
            target_id: NationId,
            combined_score: f64,
            need_score: f64,
            opportunity_score: f64,
            // Sub-components captured for reason text
            base_need: f64,
            resource_bonus: f64,
            missing_count: usize,
            army_ratio: f64,
            at_war_bonus: f64,
            relationship_penalty: f64,
        }

        let mut best: Option<Candidate> = None;
        // Best candidate that *failed* the early-game opportunity gate.
        // If no eligible candidate survives, we surface this one in the
        // news feed so the player understands why nobody is attacking yet.
        let mut best_gated: Option<Candidate> = None;

        // Snapshot nation IDs and info to avoid borrow issues
        let nation_infos: Vec<(NationId, String, usize, usize, ProvinceId)> = game
            .world
            .nations
            .iter()
            .map(|n| {
                let prov_count = game
                    .world
                    .provinces
                    .iter()
                    .filter(|p| p.owner == n.id)
                    .count();
                (
                    n.id,
                    n.name.clone(),
                    n.military.army.len(),
                    prov_count,
                    n.capital_province_id,
                )
            })
            .collect();

        for &(target_id, ref _target_name, _target_army, target_provinces, _target_capital) in
            &nation_infos
        {
            // Skip self
            if target_id == ai_id {
                continue;
            }
            // Skip already at war
            if game
                .world
                .diplomacy
                .get_relation(ai_id, target_id)
                .map(|r| r.at_war)
                .unwrap_or(false)
            {
                continue;
            }
            // Skip allies
            if game.world.diplomacy.has_treaty(
                ai_id,
                target_id,
                crate::events::TreatyType::Alliance,
            ) {
                continue;
            }
            // Skip conquered (0 provinces)
            if target_provinces == 0 {
                continue;
            }
            // Skip anarchic nations (already free to invade, no war declaration needed)
            if game
                .get_nation(target_id)
                .is_some_and(|n| n.diplomacy.is_in_anarchy)
            {
                continue;
            }
            // Anti-dogpile: skip if another AI targeted this nation this round
            if targeted_this_round.contains(&target_id) {
                continue;
            }

            // Minor nation artillery gate: require sufficient artillery to breach
            // garrison defenses (original game required 2-3 Light Artillery)
            let target_is_gp = game
                .get_nation(target_id)
                .is_some_and(|n| n.is_great_power());
            if !target_is_gp {
                let artillery_count = game
                    .get_nation(ai_id)
                    .map(|n| {
                        n.military
                            .army
                            .iter()
                            .filter(|u| {
                                u.unit_type.category()
                                    == crate::military::units::UnitCategory::Artillery
                            })
                            .count()
                    })
                    .unwrap_or(0);

                let pc_inner = PersonalityConfig::for_personality(personality);
                #[cfg(feature = "lua")]
                let min_artillery = lua_cfg
                    .as_ref()
                    .and_then(|c| c.min_artillery_for_minor_war)
                    .unwrap_or(pc_inner.min_artillery_for_minor_war);
                #[cfg(not(feature = "lua"))]
                let min_artillery = pc_inner.min_artillery_for_minor_war;

                if artillery_count < min_artillery {
                    continue;
                }
            }

            // ── need_score ─────────────────────────────────────
            let base_need = (target_provinces as f64 / 5.0).min(1.0);
            // Resource bonus: check target's province tiles for resources the AI lacks
            let target_tile_resources: std::collections::HashSet<ResourceType> = game
                .world
                .provinces
                .iter()
                .filter(|p| p.owner == target_id)
                .flat_map(|p| {
                    p.tiles.iter().filter_map(|&coord| {
                        game.world
                            .hex_map
                            .get_tile(coord)
                            .and_then(|t| t.resource_deposit())
                    })
                })
                .collect();
            let missing_count = target_tile_resources
                .iter()
                .filter(|r| !ai_resources.contains(r))
                .count();
            let resource_bonus =
                (missing_count as f64 * resource_bonus_per_missing).min(resource_bonus_cap);
            let need_score = (base_need + resource_bonus).min(1.0);

            // ── opportunity_score ──────────────────────────────
            // Card #115/#116: army_ratio compares like-for-like coalition
            // firepower. Both sides include allies, both sides are raw FP
            // (no defender bonus, no terrain/fort multipliers — those are
            // tactical, not strategic, and inflating the defender's score
            // here was making army_ratio always clamp to 0). For minor
            // targets, the target coalition also includes NAP pact-defense
            // holders (any of them might intervene).
            let (our_military, target_military) =
                coalition_firepower_for_war_decision(game, ai_id, target_id);
            // Symmetric advantage ratio: 0 for parity, 1 for unopposed,
            // clamps to 0 when defender (with allies) is stronger.
            let army_ratio = if our_military + target_military > 0.0 {
                ((our_military - target_military) / (our_military + target_military))
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            // Check if target is at war with someone else
            let target_at_war_with_other = nation_infos.iter().any(|&(other_id, _, _, _, _)| {
                other_id != ai_id
                    && other_id != target_id
                    && game
                        .world
                        .diplomacy
                        .get_relation(target_id, other_id)
                        .map(|r| r.at_war)
                        .unwrap_or(false)
            });
            let at_war_bonus = if target_at_war_with_other { 0.3 } else { 0.0 };
            let opportunity_score = (army_ratio + at_war_bonus).clamp(0.0, 1.0);

            // ── relationship_penalty ──────────────────────────
            let mut relationship_penalty = 0.0f64;
            if let Some(rel) = game.world.diplomacy.get_relation(ai_id, target_id) {
                if rel.score > 0 {
                    relationship_penalty += (rel.score as f64 / 100.0).min(0.5);
                }
                if rel.has_consulate {
                    relationship_penalty += 0.1;
                }
                if rel.has_embassy {
                    relationship_penalty += 0.2;
                }
                if rel.has_treaty(crate::events::TreatyType::NonAggressionPact) {
                    relationship_penalty += 0.4;
                }
            }

            // Conflicting alliance penalty: if any of our (non-anarchic)
            // allies are also allied with the target. Anarchic shared allies
            // would not actually intervene, so they should not register as a
            // diplomatic obstacle either — matches the coalition-firepower
            // and pact-defense filters.
            let is_active = |id: NationId| {
                !game
                    .get_nation(id)
                    .is_some_and(|n| n.diplomacy.is_in_anarchy)
            };
            let our_allies: Vec<NationId> = game
                .world
                .diplomacy
                .get_allies(ai_id)
                .into_iter()
                .filter(|&id| is_active(id))
                .collect();
            let target_allies: Vec<NationId> = game
                .world
                .diplomacy
                .get_allies(target_id)
                .into_iter()
                .filter(|&id| is_active(id))
                .collect();
            let conflicted = our_allies.iter().any(|a| target_allies.contains(a));
            if conflicted {
                relationship_penalty += 0.5;
            }

            // Pact-defense risk: if target minor has NAPs with other nations,
            // each pact holder may choose to intervene militarily.
            // Penalty scales with protector's military strength relative to ours —
            // a weak protector is less of a deterrent than a strong one.
            // Anarchic protectors are skipped: a collapsed government cannot
            // mount a defense (matches the coalition firepower exclusion).
            if !target_is_gp {
                let ai_fp = game
                    .get_nation(ai_id)
                    .map(|n| n.total_military_firepower())
                    .unwrap_or(0.0);
                let protectors: Vec<NationId> = game
                    .world
                    .diplomacy
                    .get_pact_holders(target_id)
                    .into_iter()
                    .filter(|&pid| {
                        pid != ai_id
                            && !game
                                .get_nation(pid)
                                .is_some_and(|n| n.diplomacy.is_in_anarchy)
                    })
                    .collect();
                for &protector_id in &protectors {
                    let protector_fp = game
                        .get_nation(protector_id)
                        .map(|n| n.total_military_firepower())
                        .unwrap_or(0.0);
                    // Scale: protector at equal strength = 0.4, double = 0.6, half = 0.2
                    let ratio = if ai_fp > 0.0 {
                        (protector_fp / ai_fp).clamp(0.0, 2.0)
                    } else {
                        1.0
                    };
                    relationship_penalty += ratio * 0.4;
                }
            }

            relationship_penalty = relationship_penalty.clamp(0.0, 2.5);

            // ── combined_score ─────────────────────────────────
            // Coalition strength is now baked into army_ratio (Fix #115),
            // so combined_score is a clean weighted sum of need + opportunity
            // minus relationship penalties.
            let combined_score =
                need_score + opportunity_score * opportunism_weight - relationship_penalty;

            let candidate_snapshot = Candidate {
                target_id,
                combined_score,
                need_score,
                opportunity_score,
                base_need,
                resource_bonus,
                missing_count,
                army_ratio,
                at_war_bonus,
                relationship_penalty,
            };

            // Early-game opportunity gate: an attacker at military parity
            // and equal empire size has no realistic path to victory —
            // skip regardless of need. Trade covers resource shortages.
            if opportunity_score < min_opportunity_for_war {
                if best_gated
                    .as_ref()
                    .map(|b| combined_score > b.combined_score)
                    .unwrap_or(true)
                {
                    best_gated = Some(candidate_snapshot);
                }
                continue;
            }

            if best
                .as_ref()
                .map(|b| combined_score > b.combined_score)
                .unwrap_or(true)
            {
                best = Some(candidate_snapshot);
            }
        }

        // ── 5-6. Best target + threshold check ────────────────
        let candidate = match best {
            Some(c) if c.combined_score > war_threshold => c,
            Some(c) => {
                // Considered a best candidate but scored below threshold —
                // emit a non-action summarizing why we did not declare war.
                let target_name = nation_infos
                    .iter()
                    .find(|(id, _, _, _, _)| *id == c.target_id)
                    .map(|(_, name, _, _, _)| name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                actions.push(super::AiAction {
                    text: format!(
                        "{} did not declare war this turn",
                        attacker_name
                    ),
                    reason: format!(
                        "best candidate {} scored combined {:.2} < threshold {:.2}\n  \
                         need {:.2} = base_need {:.2} (target provinces / 5) + resource_bonus {:.2} ({} missing resources)\n  \
                         opportunity {:.2} = army_ratio {:.2} + at_war_bonus {:.2}\n  \
                         combined = need + opportunity \u{00d7} opportunism_weight {:.2} \u{2212} relationship_penalty {:.2}\n  \
                         \u{2192} combined below threshold, war not declared",
                        target_name,
                        c.combined_score,
                        war_threshold,
                        c.need_score,
                        c.base_need,
                        c.resource_bonus,
                        c.missing_count,
                        c.opportunity_score,
                        c.army_ratio,
                        c.at_war_bonus,
                        opportunism_weight,
                        c.relationship_penalty,
                    ),
                    is_non_action: true,
                    nation_id: ai_id,
                });
                continue;
            }
            None => {
                // No candidate cleared the opportunity gate — surface the
                // strongest gated candidate so the player understands why
                // nobody is attacking yet.
                if let Some(c) = best_gated {
                    let target_name = nation_infos
                        .iter()
                        .find(|(id, _, _, _, _)| *id == c.target_id)
                        .map(|(_, name, _, _, _)| name.clone())
                        .unwrap_or_else(|| "Unknown".to_string());
                    // Card #97: surface this explanation in the default
                    // news feed so the player understands why day-1
                    // declarations are rare. Non-actions are hidden by
                    // default in the UI; this headline is elevated to a
                    // regular visible event by setting is_non_action=false.
                    actions.push(super::AiAction {
                        text: format!(
                            "{} held back from war with {} this turn",
                            attacker_name, target_name
                        ),
                        reason: format!(
                            "blocked by early-game opportunity floor: \
                             opportunity {:.2} < floor {:.2} (decays {:.2} \u{2192} {:.2} over {} turns)\n  \
                             need {:.2} = base_need {:.2} + resource_bonus {:.2} ({} missing resources)\n  \
                             opportunity = army_ratio {:.2} + at_war_bonus {:.2}\n  \
                             \u{2192} attacking a peer at parity is too risky; trade fulfills resources without war",
                            c.opportunity_score,
                            min_opportunity_for_war,
                            min_opp_start,
                            min_opp_end,
                            opp_decay_turns,
                            c.need_score,
                            c.base_need,
                            c.resource_bonus,
                            c.missing_count,
                            c.army_ratio,
                            c.at_war_bonus,
                        ),
                        is_non_action: false,
                        nation_id: ai_id,
                    });
                } else {
                    // No eligible candidates (all at war, allied, anarchic, dogpiled, or no targets)
                    actions.push(super::AiAction {
                        text: format!(
                            "{} did not declare war this turn",
                            attacker_name
                        ),
                        reason: "no eligible targets (already at war, allied, anarchic, or dogpile-prevented)".to_string(),
                        is_non_action: true,
                        nation_id: ai_id,
                    });
                }
                continue;
            }
        };

        let target_id = candidate.target_id;
        let target_name = nation_infos
            .iter()
            .find(|(id, _, _, _, _)| *id == target_id)
            .map(|(_, name, _, _, _)| name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        // Debug output
        if game.ai_debug {
            eprintln!(
                "[AI:{}:war] Top candidate={} need={:.2} opp={:.2} combined={:.2} threshold={:.2}",
                attacker_name,
                target_name,
                candidate.need_score,
                candidate.opportunity_score,
                candidate.combined_score,
                war_threshold,
            );
        }

        // ── 7. Lua check (feature-gated) ──────────────────────
        #[cfg(feature = "lua")]
        {
            if let Some(rel) = game
                .world
                .diplomacy
                .get_relation(ai_id, target_id)
                .filter(|r| r.score != 0)
            {
                let relations = rel.score;
                if let Some(false) = super::lua_bridge::lua_evaluate_war(
                    game,
                    personality,
                    ai_id,
                    target_id,
                    relations,
                    candidate.need_score,
                    candidate.opportunity_score,
                ) {
                    if game.ai_debug {
                        eprintln!(
                            "[AI:war] Lua vetoed war on {} (relations={})",
                            target_name, relations
                        );
                    }
                    actions.push(super::AiAction {
                        text: format!("{} did not declare war this turn", attacker_name),
                        reason: format!(
                            "Lua script vetoed war on {} (relations={}, need={:.2}, opportunity={:.2})",
                            target_name,
                            relations,
                            candidate.need_score,
                            candidate.opportunity_score
                        ),
                        is_non_action: true,
                        nation_id: ai_id,
                    });
                    continue;
                }
            }
        }

        // ── 8. Declare war ─────────────────────────────────────
        // Find the target's weakest province (fewest tiles). Trello card #8:
        // Final guard: target must still own at least one province
        if game.world.provinces.iter().all(|p| p.owner != target_id) {
            continue;
        }

        if game.ai_debug {
            eprintln!(
                "[AI:{}:war] Declaring war on {} (army={}, standing={}, score={:.2})",
                attacker_name, target_name, ai_army, standing, candidate.combined_score,
            );
        }
        let turn = game.turn;
        game.world.diplomacy.declare_war_at(ai_id, target_id, turn);
        // When the defender is the human player, queue a WarDeclaration
        // notification proposal so the modal opens with a prominent alert.
        // Both Accept and Reject are dismissals — the war is already live.
        if target_id == game.human_player_nation {
            game.world
                .diplomacy
                .pending_proposals
                .push(crate::diplomacy::DiplomaticProposal {
                    from: ai_id,
                    to: target_id,
                    proposal_type: crate::events::TreatyType::WarDeclaration,
                    turn_proposed: turn,
                    attacker: None,
                    cascade_remaining: None,
                });
        }
        // Attack is NOT queued here. ai_declare_wars runs before the per-nation
        // loop (see ai/mod.rs), so ai_military_strategy will pick up the new
        // war on the same turn and apply the rest_health_threshold filter when
        // deciding whether to commit forces. This avoids sending wounded units
        // into a first-turn attack that bypasses the health gate.
        targeted_this_round.push(target_id);
        actions.push(super::AiAction {
            text: format!("{} has declared war on {}!", attacker_name, target_name),
            reason: format!(
                "combined {:.2} > threshold {:.2}\n  \
                 need {:.2} = base_need {:.2} (target provinces / 5) + resource_bonus {:.2} ({} missing resources)\n  \
                 opportunity {:.2} = army_ratio {:.2} (coalition firepower advantage, allies in other wars half-counted) + at_war_bonus {:.2} (target already at war)\n  \
                 combined = need + opportunity \u{00d7} opportunism_weight {:.2} \u{2212} relationship_penalty {:.2} (standing / treaties / pact-defense risk)",
                candidate.combined_score,
                war_threshold,
                candidate.need_score,
                candidate.base_need,
                candidate.resource_bonus,
                candidate.missing_count,
                candidate.opportunity_score,
                candidate.army_ratio,
                candidate.at_war_bonus,
                opportunism_weight,
                candidate.relationship_penalty,
            ),
            is_non_action: false,
            nation_id: ai_id,
        });
        let turn = game.turn;
        game.archive.history.push((
            turn,
            crate::events::HistoryEvent::WarDeclared {
                attacker: ai_id,
                defender: target_id,
                protectee: None,
            },
        ));
    }
}

/// Strategic military decisions for an AI nation.
///
/// - If at war and has >= 4 army units, queue an attack on the enemy's weakest province
/// - Upgrade units when tech allows (call unit_type.upgrade_to(), check if prereq tech is researched)
///
/// War declaration is handled separately by `ai_declare_wars`.
pub(crate) fn ai_military_strategy(
    game: &mut GameState,
    nation_id: NationId,
    _actions: &mut Vec<super::AiAction>,
) {
    // Phase 1: Upgrade units if possible
    ai_upgrade_units(game, nation_id);

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Count only FP-contributing field army (filters out Generals/FP-0 support
    // units). The nation-level gate below uses this to avoid queueing attacks
    // from an army that looks populous on paper but has no combat weight.
    let combat_unit_count = nation
        .field_army_iter()
        .filter(|u| u.unit_type.stats().firepower > 0)
        .count();

    // Find nations we are at war with, plus anarchic nations (free to invade)
    let enemies: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.id != nation_id)
        .filter(|n| {
            n.diplomacy.is_in_anarchy
                || game
                    .world
                    .diplomacy
                    .get_relation(nation_id, n.id)
                    .map(|r| r.at_war)
                    .unwrap_or(false)
        })
        .map(|n| n.id)
        .collect();

    // Phase 2: the attack decision is FP-based, not unit-count-based. For
    // each candidate province we compare *our forward FP* (units positioned
    // in, or being moved to, provinces adjacent to the target) to *their
    // local FP* (stationed field army + militia garrison + garrison
    // artillery, using raw FP without the 1.2× defender or +8 militia
    // entrenchment bonuses). Generals drop out of both sides because their
    // FP is zero. Aggressive personalities use a lower ratio (willing to
    // engage at less than 1:1 raw FP).
    let personality = get_personality(game, nation_id);
    let pc_tactical = PersonalityConfig::for_personality(personality);
    #[cfg(feature = "lua")]
    let (attack_fp_vs_minor, attack_fp_vs_gp, rest_health_threshold, capital_save_for_last_penalty) = {
        let cfg = game
            .game_data
            .lua_engine
            .as_ref()
            .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
        (
            cfg.as_ref()
                .and_then(|c| c.attack_fp_vs_minor)
                .unwrap_or(0.8),
            cfg.as_ref().and_then(|c| c.attack_fp_vs_gp).unwrap_or(1.0),
            cfg.as_ref()
                .and_then(|c| c.rest_health_threshold)
                .unwrap_or(50),
            cfg.as_ref()
                .and_then(|c| c.capital_save_for_last_penalty)
                .unwrap_or(pc_tactical.capital_save_for_last_penalty),
        )
    };
    #[cfg(not(feature = "lua"))]
    let (attack_fp_vs_minor, attack_fp_vs_gp, rest_health_threshold, capital_save_for_last_penalty) = (
        0.8f64,
        1.0f64,
        50u8,
        pc_tactical.capital_save_for_last_penalty,
    );

    // Attack only when we actually have a meaningful combat force.
    if !enemies.is_empty() && combat_unit_count >= 4 {
        // Score each enemy province — lower score = better target
        let mut candidates: Vec<(ProvinceId, i32)> = Vec::new();
        let attacker_province_ids: Vec<ProvinceId> = game
            .get_nation(nation_id)
            .map(|n| n.province_ids.clone())
            .unwrap_or_default();
        // F-006: we used to count pending_move destinations toward forward
        // FP, but `resolve_combat` excludes moved units from the attacker
        // cohort the same turn they move (see `moved_unit_ids` filter in
        // `processor.rs`), so redistributed units cannot actually fight
        // this turn. Forward FP now uses only current position — units
        // redistributed this turn will count in the NEXT turn's decision.
        for &enemy_id in &enemies {
            let enemy_is_gp = game
                .get_nation(enemy_id)
                .map(|n| n.is_great_power())
                .unwrap_or(false);
            // Stationed FP per enemy province from field-army units. We use
            // `effective_firepower` so damaged units contribute less — same
            // metric used everywhere else.
            let enemy_stationed_fp: Vec<(ProvinceId, f64)> = {
                let mut sums: Vec<(ProvinceId, f64)> = Vec::new();
                if let Some(en) = game.get_nation(enemy_id) {
                    for unit in en.field_army_iter() {
                        let fp = unit.effective_firepower();
                        if let Some(entry) = sums.iter_mut().find(|(p, _)| *p == unit.position) {
                            entry.1 += fp;
                        } else {
                            sums.push((unit.position, fp));
                        }
                    }
                }
                sums
            };

            // Pre-compute which of our provinces are adjacent to enemy territory
            let our_provinces: Vec<&crate::map::Province> = game
                .world
                .provinces
                .iter()
                .filter(|p| attacker_province_ids.contains(&p.id))
                .collect();

            // Card #8: does this enemy have at least one reachable non-capital
            // province? If so, the AI should save the capital for last to
            // avoid flipping the enemy into anarchy and handing the vacuum
            // to third parties.
            let enemy_capital_pid = game.get_nation(enemy_id).map(|n| n.capital_province_id);
            let has_reachable_non_capital = game.world.provinces.iter().any(|p| {
                if p.owner != enemy_id || Some(p.id) == enemy_capital_pid {
                    return false;
                }
                let adjacent = our_provinces
                    .iter()
                    .any(|ours| crate::map::provinces_are_adjacent(&game.world.hex_map, ours, p));
                let has_landing = game
                    .transient
                    .pending_landings
                    .iter()
                    .any(|(nid, pid, _)| *nid == nation_id && *pid == p.id);
                adjacent || has_landing
            });

            for prov in &game.world.provinces {
                if prov.owner == enemy_id {
                    // Adjacency check: only attack provinces reachable by land
                    // (adjacent to one of our provinces) or via a naval landing site.
                    let adjacent_owned_pids: Vec<ProvinceId> = our_provinces
                        .iter()
                        .filter(|ours| {
                            crate::map::provinces_are_adjacent(&game.world.hex_map, ours, prov)
                        })
                        .map(|ours| ours.id)
                        .collect();
                    let has_landing = game
                        .transient
                        .pending_landings
                        .iter()
                        .any(|(nid, pid, _)| *nid == nation_id && *pid == prov.id);
                    if adjacent_owned_pids.is_empty() && !has_landing {
                        continue;
                    }

                    let tile_count = prov.tiles.len();
                    // Defender local FP (raw — matches the retreat decision
                    // baseline). Militia contribute base FP 1 each;
                    // GarrisonArtillery contributes its base FP 4. No 1.2×
                    // defender multiplier, no +8 militia entrenchment bonus.
                    let garrison_size = prov.garrison_count as usize;
                    let stationed_fp = enemy_stationed_fp
                        .iter()
                        .find(|(p, _)| *p == prov.id)
                        .map(|(_, fp)| *fp)
                        .unwrap_or(0.0);
                    let garrison_artillery_fp = if game
                        .get_nation(enemy_id)
                        .is_some_and(|n| n.has_garrison_artillery_at(prov.id))
                    {
                        crate::military::units::ArmyUnitType::GarrisonArtillery
                            .stats()
                            .firepower as f64
                    } else {
                        0.0
                    };
                    let militia_base_fp = crate::military::units::ArmyUnitType::Minutemen
                        .stats()
                        .firepower as f64;
                    let their_local_fp = stationed_fp
                        + garrison_artillery_fp
                        + (garrison_size as f64) * militia_base_fp;

                    // Our forward FP: effective_firepower of our movable
                    // units whose current position is in a province adjacent
                    // to the target (land cohort). When a naval landing is
                    // pending, we also add a naval cohort computed the same
                    // way `resolve_combat` assembles it — units in coastal
                    // attacker-owned provinces (excluding already-adjacent
                    // ones) capped by beachhead capacity, highest FP first.
                    // Card #20: wounded units below `rest_health_threshold`
                    // are excluded from the attack cohort so they stay in place
                    // and heal via the end-of-turn rest-heal pass. The AI will
                    // only commit fresh troops.
                    let (our_land_fp, naval_candidates): (f64, Vec<(f64, ProvinceId)>) = game
                        .get_nation(nation_id)
                        .map(|n| {
                            let mut land_fp = 0.0;
                            let mut naval: Vec<(f64, ProvinceId)> = Vec::new();
                            for u in &n.military.army {
                                if !u.unit_type.can_move() {
                                    continue;
                                }
                                if u.health < rest_health_threshold {
                                    continue;
                                }
                                if adjacent_owned_pids.contains(&u.position) {
                                    land_fp += u.effective_firepower();
                                } else if has_landing {
                                    naval.push((u.effective_firepower(), u.position));
                                }
                            }
                            (land_fp, naval)
                        })
                        .unwrap_or((0.0, Vec::new()));

                    let our_naval_fp: f64 = if has_landing {
                        // Filter to coastal ports and cap by beachhead size.
                        let coastal_attacker_pids: std::collections::HashSet<ProvinceId> =
                            attacker_province_ids
                                .iter()
                                .copied()
                                .filter(|&pid| game.get_province(pid).is_some_and(|p| p.coastal))
                                .collect();
                        let beachhead_cap: usize =
                            game.get_nation(nation_id)
                                .map(|n| {
                                    use crate::military::naval::NavalOperation;
                                    let assigned: Vec<_> = n
                                        .military
                                        .warships
                                        .iter()
                                        .filter(|s| {
                                            s.operation == Some(NavalOperation::Beachhead(prov.id))
                                        })
                                        .cloned()
                                        .collect();
                                    crate::military::naval::beachhead_force_size(
                                        &assigned,
                                        &game.game_data,
                                    )
                                })
                                .unwrap_or(0) as usize;
                        let mut eligible: Vec<f64> = naval_candidates
                            .into_iter()
                            .filter(|(_, pos)| coastal_attacker_pids.contains(pos))
                            .map(|(fp, _)| fp)
                            .collect();
                        eligible
                            .sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
                        eligible.truncate(beachhead_cap);
                        eligible.iter().sum()
                    } else {
                        0.0
                    };

                    let our_forward_fp = our_land_fp + our_naval_fp;

                    // FP-based attack acceptance (card #99 phase 2).
                    let ratio = if enemy_is_gp {
                        attack_fp_vs_gp
                    } else {
                        attack_fp_vs_minor
                    };
                    if our_forward_fp < their_local_fp * ratio {
                        continue;
                    }

                    // Legacy score uses stationed unit count for tie-breaking;
                    // recompute a cheap integer proxy from the FP sum.
                    let stationed = enemy_stationed_fp
                        .iter()
                        .find(|(p, _)| *p == prov.id)
                        .map(|(_, fp)| (*fp as i32).max(0))
                        .unwrap_or(0);

                    // Score: fewer tiles = weaker (lower score = better)
                    // Bonus: check for valuable terrain (mountains/hills may have
                    // mineral deposits worth targeting)
                    let mut score = tile_count as i32 + stationed * 3;

                    // Penalize terrain defense (mountains are hard to attack)
                    let capital_terrain = game
                        .world
                        .hex_map
                        .get_tile(prov.capital_tile)
                        .map(|t| t.terrain());
                    if let Some(terrain) = capital_terrain {
                        match terrain {
                            TerrainType::Mountain => score += 5,
                            TerrainType::Hills => score += 2,
                            _ => {}
                        }
                    }

                    // Bonus for provinces with many tiles (valuable resources)
                    // but not so much that it outweighs defense difficulty
                    if tile_count >= 4 {
                        score -= 1; // Slightly prefer larger provinces (more valuable)
                    }

                    // Prefer attacking GP enemies (they are higher-value targets
                    // and wars should not stagnate)
                    if enemy_is_gp {
                        score -= 3;
                    }

                    // Card #8: save a minor nation's capital for last —
                    // capturing it triggers anarchy and hands a vacuum to
                    // third parties. Hard-skip the capital when any reachable
                    // non-capital exists. For GPs apply only a soft penalty
                    // (anarchy semantics differ; GPs rarely collapse).
                    if Some(prov.id) == enemy_capital_pid && has_reachable_non_capital {
                        if !enemy_is_gp {
                            continue; // hard skip: pick non-capital instead
                        } else {
                            score += capital_save_for_last_penalty;
                        }
                    }

                    candidates.push((prov.id, score));
                }
            }
        }

        // Sort by score ascending (best target first)
        candidates.sort_by_key(|&(_, score)| score);

        if let Some(&(target_prov, _)) = candidates.first() {
            // Only queue if not already pending
            let already_pending = game
                .transient
                .pending_attacks
                .iter()
                .any(|(a, p)| *a == nation_id && *p == target_prov);
            if !already_pending {
                game.transient
                    .pending_attacks
                    .push((nation_id, target_prov));
            }
        }
    }
}

/// Upgrade units when tech prerequisites have been researched.
///
/// F-003 (round-1 review): use `required_tech()` for gating, the same
/// source consulted by the recruit menu and `upgrade_player_unit`. The
/// `UnitStats.prerequisite_tech` field carries a different (project-
/// specific) name set in some rows and used to silently disagree with
/// the canonical tech-tree wiring.
///
/// Garrison units (Minutemen / Militia / Conscript / GarrisonArtillery)
/// are persistent province defenders — they are auto-spawned and capped
/// per province by `regenerate_garrisons`. Auto-upgrading them would
/// drain the Minutemen pool every turn (Minutemen → Militia → Conscript
/// is free), and `regenerate_garrisons` would keep re-seeding fresh
/// Minutemen up to the cap, accumulating Conscripts without bound. Only
/// field-army units (Infantry/Cavalry/Artillery/Special) auto-upgrade.
fn ai_upgrade_units(game: &mut GameState, nation_id: NationId) {
    use crate::military::units::UnitCategory;

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let researched = nation.researched_techs.clone();

    // Collect upgrade info: (index, new_type)
    let upgrades: Vec<(usize, ArmyUnitType)> = nation
        .military
        .army
        .iter()
        .enumerate()
        .filter_map(|(i, unit)| {
            if unit.unit_type.category() == UnitCategory::Garrison {
                return None;
            }
            unit.unit_type
                .upgrade_to()
                .and_then(|new_type| match new_type.required_tech() {
                    Some(tech_name) => {
                        let has_tech = game
                            .game_data
                            .tech_tree
                            .all_techs()
                            .iter()
                            .any(|t| t.name == tech_name && researched.contains(&t.id));
                        if has_tech { Some((i, new_type)) } else { None }
                    }
                    None => Some((i, new_type)),
                })
        })
        .collect();

    // Apply upgrades
    if let Some(nation) = game.get_nation_mut(nation_id) {
        for (idx, new_type) in upgrades {
            if idx < nation.military.army.len() {
                nation.military.army[idx].unit_type = new_type;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::{test_game_with_ai, test_game_with_ai_and_minor};
    use crate::ai::run_ai_turns;
    use crate::map::UnitId;

    // ── Military building ────────────────────────────────────

    #[test]
    fn ai_builds_regulars_when_army_small_and_can_afford() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(3000);
        // AI starts with 0 FIELD army units (only starting garrison militia).
        // Give the human one rival field unit so the threat-gated military
        // scoring (added with the turn-1 spending fix) has a real rival to
        // match. With one rival, after the AI builds its first Regular the
        // deficit closes and the AI stops — keeping this test single-build.
        let human = game.get_nation_mut(NationId(1)).unwrap();
        human
            .military
            .army
            .push(crate::military::units::ArmyUnit::new(
                UnitId(2_000_000),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            ));

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.field_army_count(),
            1,
            "AI should build 1 Regulars unit (militia excluded)"
        );
        let built = ai
            .field_army_iter()
            .next()
            .expect("one field unit expected");
        assert_eq!(built.unit_type, ArmyUnitType::Regulars);
        assert_eq!(built.owner, NationId(2));
        assert_eq!(built.position, ProvinceId(2)); // capital
        // Card #420 follow-up: the AI now spends the unit's actual stat
        // cost ($100 for Regulars per the manual) instead of the old
        // hardcoded $500 fallback in `execute_military`.
        assert_eq!(
            ai.economy.treasury,
            Money::dollars(2900),
            "Treasury should be reduced by Regulars stats().cost = $100"
        );
    }

    /// Card #210 follow-up: with no rival fielding any army, the AI must
    /// not build a field unit on turn 1 even when it can afford to. The
    /// territorial-floor `provinces * 1.5` used to fire here; the threat
    /// gate disables it when `strongest_rival_army == 0`.
    #[test]
    fn ai_does_not_build_field_army_on_turn_1_absent_rivals() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(10_000);
        // No rival armies anywhere; no consulates / embassies; turn 1.
        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.field_army_count(),
            0,
            "AI must not raise a field army on turn 1 when no rival is projecting power",
        );
    }

    #[test]
    fn ai_does_not_build_military_when_poor() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(1000); // < $2,000 threshold

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.field_army_count(),
            0,
            "AI should not build field army units when treasury <= $2,000"
        );
    }

    #[test]
    fn ai_builds_unit_when_army_small_for_territory() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give large treasury so AI can spend on both infrastructure and military
        ai.economy.treasury = Money::dollars(50000);
        ai.diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        // Give AI enough provinces that 3 army isn't enough (deficit scoring)
        for i in 10..15 {
            ai.add_province(ProvinceId(i));
        }
        // Give AI 3 existing army units
        for i in 0..3 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.military.army.len() >= 4,
            "AI should have built at least a 4th unit, has {}",
            ai.military.army.len()
        );
        assert!(
            ai.economy.treasury < Money::dollars(50000),
            "Treasury should be reduced after building a unit"
        );
    }

    #[test]
    fn ai_builds_more_units_when_territory_large() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(20000);
        ai.diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        // Give AI many provinces so it needs a large army
        for i in 10..20 {
            ai.add_province(ProvinceId(i));
        }

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.military.army.len() >= 2,
            "AI with large territory should build multiple units, has {}",
            ai.military.army.len()
        );
        assert!(
            ai.economy.treasury < Money::dollars(20000),
            "Treasury should be reduced after building units"
        );
    }

    #[test]
    fn ai_military_units_have_unique_ids() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(50000);

        // Run multiple turns to build several units
        let mut actions = Vec::new();
        for _ in 0..5 {
            ai_build_military(&mut game, NationId(2), &mut actions);
        }

        let ai = game.get_nation(NationId(2)).unwrap();
        let ids: Vec<UnitId> = ai.military.army.iter().map(|u| u.id).collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "AI army units must have unique IDs");
            }
        }
    }

    // ── War declaration ──────────────────────────────────────

    #[test]
    fn ai_declares_war_when_target_vulnerable() {
        let mut game = test_game_with_ai_and_minor();
        // Give AI a large army with artillery to overcome minor garrison defense.
        // Defense estimate ≈ 37 FP, so AI needs overwhelming force.
        // Use Aggressive personality (threshold 0.3) since a nation with this much
        // military is realistically aggressive.
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        for i in 0..10 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..8 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5100 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        // Card #4: AI no longer skips the human player. Give the human a
        // strong defending army so AI scoring picks the unarmed adjacent minor
        // — the test's intent is "AI declares war on the vulnerable minor".
        let human = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..40 {
            human.military.army.push(ArmyUnit::new(
                UnitId(7000 + i),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            ));
        }
        game.turn = TurnNumber::new(10);

        let mut actions = Vec::new();
        // War declarations run first, then ai_military_strategy picks up the
        // new war and queues the attack (matching the production flow in mod.rs).
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);
        ai_military_strategy(&mut game, NationId(2), &mut actions);

        // The human is heavily defended (40 Regulars above) so the unarmed
        // adjacent minor is unambiguously the more attractive target. This
        // verifies AI war-target scoring under Card #4 (no human exemption).
        let at_war_with_minor = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .is_some_and(|r| r.at_war);
        let at_war_with_human = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(1))
            .is_some_and(|r| r.at_war);
        assert!(
            at_war_with_minor,
            "AI should be at war with the vulnerable minor"
        );
        assert!(
            !at_war_with_human,
            "AI should not target the heavily-defended human GP when a softer target exists"
        );
        // Should have queued a pending attack against the minor
        assert!(
            game.transient
                .pending_attacks
                .iter()
                .any(|(attacker, target)| *attacker == NationId(2) && *target == ProvinceId(3)),
            "AI should queue an attack on the minor's province"
        );
    }

    #[test]
    fn ai_does_not_redeclare_war_on_existing_enemy() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(10);
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..5 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        // Pre-set war
        game.world.diplomacy.declare_war(NationId(2), NationId(3));

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Should not have queued any attack (already at war, and no other targets)
        assert!(
            game.transient.pending_attacks.is_empty(),
            "AI should not queue attack via ai_declare_wars if already at war"
        );
    }

    #[test]
    fn ai_respects_war_cooldown() {
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced); // cooldown = 12
        for i in 0..5 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        // Simulate a recent war declaration in history
        game.turn = TurnNumber::new(15);
        game.archive.history.push((
            TurnNumber::new(10),
            crate::events::HistoryEvent::WarDeclared {
                attacker: NationId(2),
                defender: NationId(99),
                protectee: None,
            },
        ));

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Turn 15 - Turn 10 = 5 turns < cooldown of 12, so should NOT declare war
        let at_war = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            !at_war,
            "AI should not declare war while within cooldown period"
        );
    }

    // ── Personality affects war declaration ──────────────────

    #[test]
    fn aggressive_ai_declares_war_easily() {
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        // Aggressive needs artillery for minor targets and enough firepower
        // to overcome garrison defense (≈37 FP).
        // 10 Regulars (20) + 8 LA (24) = 44 FP vs 37 defense.
        for i in 0..10 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..8 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5100 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        // Past the early-game opportunity floor decay (Aggressive decays by
        // turn 15): with marginal firepower advantage the gate is permissive.
        game.turn = TurnNumber::new(20);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Card #4: AI may target the minor or the human; either is fine — the
        // assertion is about the AI declaring war at all under low threshold.
        let at_war_with_someone = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(1))
            .is_some_and(|r| r.at_war)
            || game
                .world
                .diplomacy
                .get_relation(NationId(2), NationId(3))
                .is_some_and(|r| r.at_war);
        assert!(
            at_war_with_someone,
            "Aggressive AI should declare war with low threshold and small army"
        );
    }

    #[test]
    fn diplomatic_ai_needs_high_score_to_declare_war() {
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Diplomatic);
        // Diplomatic needs >= 8 army units; give exactly 8
        for i in 0..8 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        // Establish a consulate + positive relations to raise relationship_penalty
        let _ = game
            .world
            .diplomacy
            .build_consulate(NationId(2), NationId(3));
        // Improve score to make it harder to declare war
        if let Some(rel) = game
            .world
            .diplomacy
            .get_relation_mut(NationId(2), NationId(3))
        {
            rel.improve_score(40);
        }
        game.turn = TurnNumber::new(5);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Diplomatic threshold is 0.9, the relationship penalty from consulate (+0.1)
        // and positive relations (+0.4) should push the score below threshold.
        // The minor only has 1 province so need_score is low (0.2).
        let at_war = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            !at_war,
            "Diplomatic AI should not declare war when relationship penalty is high"
        );
    }

    // ── Card #97: early-game opportunity gate ────────────────

    #[test]
    fn opportunity_gate_blocks_day_one_war_at_parity() {
        // On day 1 with equal-strength armies and equal empires, the
        // early-game opportunity floor should block the war declaration and
        // emit a non-action citing the gate.
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        // Match the minor's field army so opportunity ~ 0 (army_ratio
        // compares raw field firepower like-for-like, no defender bonus).
        for i in 0..4 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..2 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5100 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        let minor = game.get_nation_mut(NationId(3)).unwrap();
        for i in 0..4 {
            minor.military.army.push(ArmyUnit::new(
                UnitId(6000 + i),
                ArmyUnitType::Regulars,
                NationId(3),
                ProvinceId(3),
            ));
        }
        for i in 0..2 {
            minor.military.army.push(ArmyUnit::new(
                UnitId(6100 + i),
                ArmyUnitType::LightArtillery,
                NationId(3),
                ProvinceId(3),
            ));
        }
        // Card #4: AI no longer skips the human player. Match the human's
        // field army to the minor's so the parity gate applies to that target
        // too — otherwise AI would declare war on the unarmed human instead.
        let human = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..4 {
            human.military.army.push(ArmyUnit::new(
                UnitId(7000 + i),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            ));
        }
        for i in 0..2 {
            human.military.army.push(ArmyUnit::new(
                UnitId(7100 + i),
                ArmyUnitType::LightArtillery,
                NationId(1),
                ProvinceId(1),
            ));
        }
        game.turn = TurnNumber::new(1);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // No war should be declared on either neighbor at parity.
        let at_war_with_minor = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        let at_war_with_human = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(1))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            !at_war_with_minor && !at_war_with_human,
            "Balanced AI at turn 1 with no firepower advantage must not declare war"
        );
        // After review fix for card #97, the gate-blocked explanation is a
        // visible headline (is_non_action=false). Check that *some* action
        // reason cites the gate. Both predicates are wrapped in parentheses
        // so the && binds tightly and the || is explicit.
        assert!(
            actions.iter().any(|a| {
                a.reason.contains("early-game opportunity floor")
                    || a.reason.contains("peer at parity")
            }),
            "action reason should cite the early-game opportunity gate: {:?}",
            actions.iter().map(|a| &a.reason).collect::<Vec<_>>()
        );
    }

    #[test]
    fn opportunity_gate_permits_war_with_overwhelming_advantage_early() {
        // Even on turn 0, overwhelming firepower clears the gate.
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        // Stack a huge army so army_ratio is near 1.0 and easily clears
        // Aggressive's turn-0 floor of 0.25.
        for i in 0..40 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..20 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5200 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        game.turn = TurnNumber::new(1);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Card #4: AI may target either the minor or the human GP — both are
        // valid given overwhelming firepower clears the early-game gate.
        let at_war_with_someone = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(1))
            .is_some_and(|r| r.at_war)
            || game
                .world
                .diplomacy
                .get_relation(NationId(2), NationId(3))
                .is_some_and(|r| r.at_war);
        assert!(
            at_war_with_someone,
            "Aggressive AI with overwhelming firepower should clear the gate even on turn 0"
        );
    }

    #[test]
    fn coalition_firepower_includes_allies_and_discounts_busy_ones() {
        // Card #115: target coalition firepower must include the target's
        // allies, AND allies tied up in another war must be half-credited.
        use crate::events::TreatyType;
        use crate::hex::HexCoord;
        use crate::map::Province;
        use crate::map::tile::Tile;
        use crate::nation::{Nation, NationColor};
        use crate::types::NationType;

        fn build_game(ally_at_war_with: Option<NationId>) -> GameState {
            let mut hex_map = crate::map::HexMap::new(10, 10);
            for (c, pid) in [
                (HexCoord::new(0, 0), ProvinceId(1)),
                (HexCoord::new(1, 0), ProvinceId(2)),
                (HexCoord::new(2, 0), ProvinceId(3)),
                (HexCoord::new(3, 0), ProvinceId(4)),
            ] {
                hex_map.set_tile(c, Tile::with_province(TerrainType::Grassland, pid));
            }
            let mut provinces = Vec::new();
            for (pid, name, owner, c) in [
                (ProvinceId(1), "Atk", NationId(1), HexCoord::new(0, 0)),
                (ProvinceId(2), "Tgt", NationId(2), HexCoord::new(1, 0)),
                (ProvinceId(3), "Ally", NationId(3), HexCoord::new(2, 0)),
                (ProvinceId(4), "Other", NationId(4), HexCoord::new(3, 0)),
            ] {
                provinces.push(Province::new(pid, name.into(), owner, c, vec![c], 4));
            }
            let mut atk = Nation::new(
                NationId(1),
                "Atk".into(),
                NationColor::Blue,
                NationType::GreatPower,
                ProvinceId(1),
            );
            atk.province_ids = vec![ProvinceId(1)];
            // Atk firepower is irrelevant for this test.
            let mut tgt = Nation::new(
                NationId(2),
                "Tgt".into(),
                NationColor::Red,
                NationType::GreatPower,
                ProvinceId(2),
            );
            tgt.province_ids = vec![ProvinceId(2)];
            for i in 0..4 {
                tgt.military.army.push(ArmyUnit::new(
                    UnitId(2000 + i),
                    ArmyUnitType::Regulars,
                    NationId(2),
                    ProvinceId(2),
                ));
            }
            let mut ally = Nation::new(
                NationId(3),
                "Ally".into(),
                NationColor::Green,
                NationType::GreatPower,
                ProvinceId(3),
            );
            ally.province_ids = vec![ProvinceId(3)];
            for i in 0..10 {
                ally.military.army.push(ArmyUnit::new(
                    UnitId(3000 + i),
                    ArmyUnitType::Regulars,
                    NationId(3),
                    ProvinceId(3),
                ));
            }
            let mut other = Nation::new(
                NationId(4),
                "Other".into(),
                NationColor::Yellow,
                NationType::GreatPower,
                ProvinceId(4),
            );
            other.province_ids = vec![ProvinceId(4)];

            let mut diplomacy = crate::diplomacy::DiplomacyState::new();
            diplomacy
                .ensure_relation(NationId(2), NationId(3))
                .add_treaty(TreatyType::Alliance);
            if let Some(opp) = ally_at_war_with {
                diplomacy.declare_war(NationId(3), opp);
            }

            crate::test_game_state! {
                turn: TurnNumber::new(10),
                difficulty: crate::types::Difficulty::Normal,
                map_key: "t".into(),
                hex_map: hex_map,
                provinces: provinces,
                nations: vec![atk, tgt, ally, other],
                human_player_nation: NationId(1),
                events: Vec::new(),
                game_data: crate::data::GameData::default(),
                diplomacy: diplomacy,
                pending_attacks: Vec::new(),
                pending_moves: Vec::new(),
                pending_landings: Vec::new(),
                history: Vec::new(),
                high_scores: Vec::new(),
                newspaper_archive: Vec::new(),
                battle_archive: Vec::new(),
                political_archive: Vec::new(),
                ai_debug: false,
                observer_mode: false,
                last_cash_flow: std::collections::HashMap::new(),
                last_resource_flow: std::collections::HashMap::new(),
                pending_ai_cash_spending: Vec::new(),
                pending_ai_cash_income: Vec::new(),
            next_unit_id: 6_000_000,}
        }

        let game_full = build_game(None);
        let (_, tgt_full) =
            coalition_firepower_for_war_decision(&game_full, NationId(1), NationId(2));

        let game_busy = build_game(Some(NationId(4)));
        let (_, tgt_busy) =
            coalition_firepower_for_war_decision(&game_busy, NationId(1), NationId(2));

        // Sanity: target alone is non-trivial (4 Regulars).
        let tgt_alone =
            super::super::assessment::nation_military_score(&game_full, NationId(2), 0.0);
        assert!(tgt_alone > 0.0, "target should have positive firepower");

        // The full-strength coalition adds the ally on top of the target.
        assert!(
            tgt_full > tgt_alone,
            "ally should add firepower: tgt_alone={tgt_alone}, tgt_full={tgt_full}"
        );

        // Discounted coalition is between target-alone and full-strength
        // (ally counts as half its firepower).
        assert!(
            tgt_busy < tgt_full && tgt_busy > tgt_alone,
            "busy ally should be half-credited: tgt_alone={tgt_alone}, \
             tgt_busy={tgt_busy}, tgt_full={tgt_full}"
        );
        // Halved-ally formula: tgt + ally*0.5 = (tgt_full + tgt_alone) / 2.
        let expected = (tgt_full + tgt_alone) / 2.0;
        assert!(
            (tgt_busy - expected).abs() < 1e-6,
            "expected discounted total {expected}, got {tgt_busy}"
        );
    }

    #[test]
    fn coalition_firepower_excludes_anarchic_allies() {
        // Review F-001: an anarchic state has no offensive military
        // capability and must not be counted as a coalition reinforcement,
        // even if the alliance treaty is still on paper.
        use crate::events::TreatyType;
        use crate::hex::HexCoord;
        use crate::map::Province;
        use crate::map::tile::Tile;
        use crate::nation::{Nation, NationColor};
        use crate::types::NationType;

        let mut hex_map = crate::map::HexMap::new(10, 10);
        for (c, pid) in [
            (HexCoord::new(0, 0), ProvinceId(1)),
            (HexCoord::new(1, 0), ProvinceId(2)),
            (HexCoord::new(2, 0), ProvinceId(3)),
        ] {
            hex_map.set_tile(c, Tile::with_province(TerrainType::Grassland, pid));
        }
        let mut provinces = Vec::new();
        for (pid, name, owner, c) in [
            (ProvinceId(1), "Atk", NationId(1), HexCoord::new(0, 0)),
            (ProvinceId(2), "Tgt", NationId(2), HexCoord::new(1, 0)),
            (ProvinceId(3), "Ally", NationId(3), HexCoord::new(2, 0)),
        ] {
            provinces.push(Province::new(pid, name.into(), owner, c, vec![c], 4));
        }
        let mut atk = Nation::new(
            NationId(1),
            "Atk".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        atk.province_ids = vec![ProvinceId(1)];
        let mut tgt = Nation::new(
            NationId(2),
            "Tgt".into(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        tgt.province_ids = vec![ProvinceId(2)];
        for i in 0..4 {
            tgt.military.army.push(ArmyUnit::new(
                UnitId(2000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        // Ally has a big army on paper but is anarchic (collapsed).
        let mut ally = Nation::new(
            NationId(3),
            "Ally".into(),
            NationColor::Green,
            NationType::GreatPower,
            ProvinceId(3),
        );
        ally.province_ids = vec![ProvinceId(3)];
        ally.diplomacy.is_in_anarchy = true;
        for i in 0..10 {
            ally.military.army.push(ArmyUnit::new(
                UnitId(3000 + i),
                ArmyUnitType::Regulars,
                NationId(3),
                ProvinceId(3),
            ));
        }

        let mut diplomacy = crate::diplomacy::DiplomacyState::new();
        diplomacy
            .ensure_relation(NationId(2), NationId(3))
            .add_treaty(TreatyType::Alliance);

        let game = crate::test_game_state! {
        turn: TurnNumber::new(10),
        difficulty: crate::types::Difficulty::Normal,
        map_key: "t".into(),
        hex_map: hex_map,
        provinces: provinces,
        nations: vec![atk, tgt, ally],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: crate::data::GameData::default(),
        diplomacy: diplomacy,
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        pending_landings: Vec::new(),
        history: Vec::new(),
        high_scores: Vec::new(),
        newspaper_archive: Vec::new(),
        battle_archive: Vec::new(),
        political_archive: Vec::new(),
        ai_debug: false,
        observer_mode: false,
        last_cash_flow: std::collections::HashMap::new(),
        last_resource_flow: std::collections::HashMap::new(),
        pending_ai_cash_spending: Vec::new(),
        pending_ai_cash_income: Vec::new(),
        next_unit_id: 6_000_000,};

        let (_, tgt_with_anarchic) =
            coalition_firepower_for_war_decision(&game, NationId(1), NationId(2));
        let tgt_alone = super::super::assessment::nation_military_score(&game, NationId(2), 0.0);

        assert_eq!(
            tgt_with_anarchic, tgt_alone,
            "anarchic ally must not contribute to coalition firepower; \
             tgt_alone={tgt_alone}, tgt_with_anarchic={tgt_with_anarchic}"
        );
    }

    #[test]
    fn ai_can_target_great_powers() {
        // Set up a game with two AI GPs and no minor nations
        let mut game = test_game_with_ai();
        // Add a second AI great power (NationId(1) is human, NationId(2) is AI)
        // We need a third nation that is a GP and AI-controlled
        use crate::hex::HexCoord;
        use crate::map::Province;
        use crate::nation::{Nation, NationColor};
        use crate::types::NationType;

        let province3 = Province::new(
            ProvinceId(3),
            "GP3 Land".to_string(),
            NationId(3),
            HexCoord::new(6, 6),
            vec![HexCoord::new(6, 6)],
            2,
        );
        game.world.provinces.push(province3);

        let mut gp3 = Nation::new(
            NationId(3),
            "WeakGP".to_string(),
            NationColor::Gray,
            NationType::GreatPower,
            ProvinceId(3),
        );
        gp3.economy.treasury = Money::dollars(1000);
        // WeakGP has 0 army units — very vulnerable
        game.world.nations.push(gp3);

        // Card #4: AI no longer skips the human player. Give the human a
        // strong defending army so the weak GP3 is unambiguously the more
        // attractive target — this test specifically checks that the AI
        // discriminates between strong and weak GP targets.
        let human = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..40 {
            human.military.army.push(ArmyUnit::new(
                UnitId(7000 + i),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            ));
        }

        // Give the AI attacker a strong army (enough to overcome garrison defense)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        for i in 0..6 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..4 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5100 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        game.turn = TurnNumber::new(10);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // AI should target the weak GP3, not the strongly-defended human GP.
        // This validates that the war-target scoring discriminates by
        // vulnerability now that the human-player exemption (Card #4) is gone.
        let at_war_with_gp3 = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        let at_war_with_human = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(1))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            at_war_with_gp3,
            "AI should declare war on the weak Great Power (GP3)"
        );
        assert!(
            !at_war_with_human,
            "AI should prefer the unarmed weak GP over a heavily-defended human GP"
        );
    }

    #[test]
    fn ai_war_declaration_on_human_queues_modal_proposal() {
        // When the AI declares war on the human player, a WarDeclaration
        // proposal must be pushed to the modal so the player gets a
        // prominent notification (the war is already in effect).
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        for i in 0..40 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..20 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5200 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        // Strip the minor so the human is the only candidate.
        if let Some(p) = game
            .world
            .provinces
            .iter_mut()
            .find(|p| p.id == ProvinceId(3))
        {
            p.owner = NationId(2);
        }
        if let Some(minor) = game.get_nation_mut(NationId(3)) {
            minor.province_ids.clear();
        }
        if let Some(ai) = game.get_nation_mut(NationId(2)) {
            ai.add_province(ProvinceId(3));
        }
        // Move the human's province adjacent to the AI so reachability/scoring works.
        if let Some(p) = game
            .world
            .provinces
            .iter_mut()
            .find(|p| p.id == ProvinceId(1))
        {
            p.tiles = vec![crate::hex::HexCoord::new(2, 3)];
            p.capital_tile = crate::hex::HexCoord::new(2, 3);
        }
        game.world.hex_map.set_tile(
            crate::hex::HexCoord::new(2, 3),
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        game.turn = TurnNumber::new(20);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        let at_war = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(1))
            .is_some_and(|r| r.at_war);
        assert!(
            at_war,
            "precondition: AI should declare war on the human in this setup"
        );

        let modal_proposal_count = game
            .world
            .diplomacy
            .pending_proposals
            .iter()
            .filter(|p| {
                p.proposal_type == crate::events::TreatyType::WarDeclaration
                    && p.from == NationId(2)
                    && p.to == NationId(1)
            })
            .count();
        assert_eq!(
            modal_proposal_count, 1,
            "AI war declaration on human must produce exactly one WarDeclaration modal proposal"
        );
    }

    #[test]
    fn ai_attacks_gp_capital_when_only_reachable_province() {
        // Regression for card #8: when attacking a GP where only the capital
        // is reachable (non-capital is landlocked elsewhere), the AI should
        // still target the capital rather than skipping it. This is the key
        // difference from minor-nation behavior (which hard-skips the capital
        // if any non-capital is reachable but can still target it when forced).
        use crate::hex::HexCoord;
        use crate::map::Province;
        use crate::map::tile::Tile;
        use crate::types::NationType;

        let mut hex_map = crate::map::HexMap::new(20, 20);
        // AI tile at (0,0)
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        // GP capital adjacent to AI at (1,0) — only reachable province
        hex_map.set_tile(
            HexCoord::new(1, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );
        // GP non-capital at (10,10) — far away, NOT reachable from AI
        hex_map.set_tile(
            HexCoord::new(10, 10),
            Tile::with_province(TerrainType::Grassland, ProvinceId(4)),
        );

        let ai_prov = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let gp_capital = Province::new(
            ProvinceId(3),
            "GP Capital".to_string(),
            NationId(3),
            HexCoord::new(1, 0),
            vec![HexCoord::new(1, 0)],
            3,
        );
        let gp_non_capital = Province::new(
            ProvinceId(4),
            "GP Hinterland".to_string(),
            NationId(3),
            HexCoord::new(10, 10),
            vec![HexCoord::new(10, 10)],
            3,
        );

        let mut ai_nation = crate::nation::Nation::new(
            NationId(2),
            "AILand".to_string(),
            crate::nation::NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        ai_nation.economy.treasury = Money::dollars(10000);
        ai_nation.diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        for i in 0..10 {
            ai_nation.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..5 {
            ai_nation.military.army.push(ArmyUnit::new(
                UnitId(5100 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..4 {
            ai_nation
                .military
                .civilians
                .push(crate::economy::civilians::Civilian::new(
                    UnitId(10000 + i),
                    crate::economy::civilians::CivilianType::Farmer,
                    NationId(2),
                ));
        }

        let mut gp_nation = crate::nation::Nation::new(
            NationId(3),
            "WeakGP".to_string(),
            crate::nation::NationColor::Gray,
            NationType::GreatPower,
            ProvinceId(3),
        );
        gp_nation.add_province(ProvinceId(4));
        gp_nation.economy.treasury = Money::dollars(500);

        let human_nation = crate::nation::Nation::new(
            NationId(1),
            "HumanNation".to_string(),
            crate::nation::NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(10),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![ai_prov, gp_capital, gp_non_capital],
        nations: vec![human_nation, ai_nation, gp_nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: crate::data::GameData::default(),
        diplomacy: crate::diplomacy::DiplomacyState::new(),
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        pending_landings: Vec::new(),
        history: Vec::new(),
        high_scores: Vec::new(),
        newspaper_archive: Vec::new(),
        battle_archive: Vec::new(),
        political_archive: Vec::new(),
        ai_debug: false,
        observer_mode: false,
        last_cash_flow: std::collections::HashMap::new(),
        last_resource_flow: std::collections::HashMap::new(),
        pending_ai_cash_spending: Vec::new(),
        pending_ai_cash_income: Vec::new(),
        next_unit_id: 6_000_000,};
        crate::military::combat::seed_militia_from_garrison_count(&mut game);

        let mut actions = Vec::new();
        // Matching production flow: war declarations then strategy
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);
        ai_military_strategy(&mut game, NationId(2), &mut actions);

        let at_war = game
            .world
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(at_war, "AI should declare war on the weak GP");

        // GP capital is the only reachable province → must be targeted
        let attack = game
            .transient
            .pending_attacks
            .iter()
            .find(|(a, _)| *a == NationId(2));
        assert!(
            attack.is_some(),
            "AI should queue an attack on the GP capital"
        );
        let (_, target) = attack.unwrap();
        assert_eq!(
            *target,
            ProvinceId(3),
            "GP capital should be targeted when it is the only reachable province"
        );
    }

    // ── Smart attack targeting ───────────────────────────────

    #[test]
    fn ai_targets_weaker_provinces() {
        use crate::hex::HexCoord;
        use crate::map::Province;
        use crate::map::tile::Tile;

        // Build a game with adjacent provinces so the adjacency check passes.
        // AI province at (0,0), minor provinces adjacent at (1,0) and (2,0).
        let mut hex_map = crate::map::HexMap::new(20, 20);
        // AI tile
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        // Minor province 1 tile (adjacent to AI at (0,0))
        hex_map.set_tile(
            HexCoord::new(1, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );
        // Minor province 2 tiles (adjacent to minor province 1 and farther from AI)
        for coord in [
            HexCoord::new(2, 0),
            HexCoord::new(3, 0),
            HexCoord::new(2, 1),
            HexCoord::new(3, 1),
            HexCoord::new(4, 0),
        ] {
            hex_map.set_tile(
                coord,
                Tile::with_province(TerrainType::Grassland, ProvinceId(4)),
            );
        }

        let province2 = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Small Minor".to_string(),
            NationId(3),
            HexCoord::new(1, 0),
            vec![HexCoord::new(1, 0)],
            3,
        );
        let province4 = Province::new(
            ProvinceId(4),
            "Big Minor".to_string(),
            NationId(3),
            HexCoord::new(2, 0),
            vec![
                HexCoord::new(2, 0),
                HexCoord::new(3, 0),
                HexCoord::new(2, 1),
                HexCoord::new(3, 1),
                HexCoord::new(4, 0),
            ],
            3,
        );

        let mut ai_nation = crate::nation::Nation::new(
            NationId(2),
            "AINation".to_string(),
            crate::nation::NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        ai_nation.economy.treasury = Money::dollars(10000);
        for i in 0..4 {
            ai_nation
                .military
                .civilians
                .push(crate::economy::civilians::Civilian::new(
                    UnitId(10000 + i),
                    crate::economy::civilians::CivilianType::Farmer,
                    NationId(2),
                ));
        }

        let mut minor_nation = crate::nation::Nation::new(
            NationId(3),
            "MinorLand".to_string(),
            crate::nation::NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(3),
        );
        minor_nation.add_province(ProvinceId(4));

        let human_nation = crate::nation::Nation::new(
            NationId(1),
            "HumanNation".to_string(),
            crate::nation::NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province2, province3, province4],
        nations: vec![human_nation, ai_nation, minor_nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: crate::data::GameData::default(),
        diplomacy: crate::diplomacy::DiplomacyState::new(),
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        pending_landings: Vec::new(),
        history: Vec::new(),
        high_scores: Vec::new(),
        newspaper_archive: Vec::new(),
        battle_archive: Vec::new(),
        political_archive: Vec::new(),
        ai_debug: false,
        observer_mode: false,
        last_cash_flow: std::collections::HashMap::new(),
        last_resource_flow: std::collections::HashMap::new(),
        pending_ai_cash_spending: Vec::new(),
        pending_ai_cash_income: Vec::new(),
        next_unit_id: 6_000_000,};

        // Put AI at war with minor
        game.world.diplomacy.declare_war(NationId(2), NationId(3));

        // Give AI enough army units
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..6 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(2),
            ));
        }

        let mut actions = Vec::new();
        ai_military_strategy(&mut game, NationId(2), &mut actions);

        // AI should prefer the smaller province (ProvinceId(3) with 1 tile)
        // over the larger one (ProvinceId(4) with 5 tiles)
        let attack = game
            .transient
            .pending_attacks
            .iter()
            .find(|(a, _)| *a == NationId(2));
        assert!(attack.is_some(), "AI should queue an attack");
        let (_, target) = attack.unwrap();
        assert_eq!(
            *target,
            ProvinceId(3),
            "AI should target the smaller/weaker province (1 tile vs 5 tiles)"
        );
    }
}
