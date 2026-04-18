#![allow(unused_labels)]
use crate::game_state::GameState;
#[cfg(test)]
use crate::military::units::ArmyUnit;
use crate::military::units::ArmyUnitType;
use crate::types::*;

#[cfg(test)]
use super::common::next_unit_id;
use super::common::{AiPersonality, get_personality};

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
        let army_count = n.map(|n| n.army.len()).unwrap_or(0);
        let treasury = n.map(|n| n.treasury.as_dollars()).unwrap_or(0);
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

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let army_count = nation.army.len();
    let treasury = nation.treasury;
    let capital = nation.capital_province_id;
    let nation_name = nation.name.clone();

    // Deterministic per-nation seed for unit-type variety
    let variety_seed = (turn_number as usize).wrapping_mul(nation_id.0 as usize + 7);

    // Thresholds vary by personality (Lua overrides Rust defaults)
    let tier1_max: usize = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.tier1_army_max) {
            break 'val v;
        }
        match personality {
            AiPersonality::Aggressive => 4,
            AiPersonality::Diplomatic => 2,
            AiPersonality::Economic => 3,
            AiPersonality::Balanced => 3,
        }
    };
    let tier1_treasury: Money = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.tier1_treasury) {
            break 'val Money::dollars(v);
        }
        match personality {
            AiPersonality::Aggressive => Money::dollars(1500),
            AiPersonality::Diplomatic => Money::dollars(3000),
            AiPersonality::Economic => Money::dollars(2500),
            AiPersonality::Balanced => Money::dollars(2000),
        }
    };
    let tier2_max: usize = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.tier2_army_max) {
            break 'val v;
        }
        match personality {
            AiPersonality::Aggressive => 7,
            AiPersonality::Diplomatic => 4,
            AiPersonality::Economic => 5,
            AiPersonality::Balanced => 5,
        }
    };
    let tier2_treasury: Money = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.tier2_treasury) {
            break 'val Money::dollars(v);
        }
        match personality {
            AiPersonality::Aggressive => Money::dollars(3000),
            AiPersonality::Diplomatic => Money::dollars(8000),
            AiPersonality::Economic => Money::dollars(6000),
            AiPersonality::Balanced => Money::dollars(5000),
        }
    };
    let tier3_treasury: Money = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.tier3_treasury) {
            break 'val Money::dollars(v);
        }
        match personality {
            AiPersonality::Aggressive => Money::dollars(6000),
            AiPersonality::Diplomatic => Money::dollars(15000),
            AiPersonality::Economic => Money::dollars(12000),
            AiPersonality::Balanced => Money::dollars(10000),
        }
    };

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
            nation.treasury -= cost;
            let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
            nation.army.push(unit);
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
            nation.treasury -= build_cost;
            let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
            nation.army.push(unit);
            actions.push(super::AiAction {
                text: format!("{} has been expanding its military forces", nation_name),
                reason: format!(
                    "Tier 2 build: army={}/{} cap, treasury=${}",
                    army_count + 1,
                    tier2_max,
                    treasury.as_dollars()
                ),
                is_non_action: false,
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
            match personality {
                AiPersonality::Aggressive => 15,
                AiPersonality::Diplomatic => 8,
                AiPersonality::Economic => 10,
                AiPersonality::Balanced => 12,
            }
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
                if nation.army.len() >= tier3_max {
                    break;
                }
                let unit_type = tier3_options[(variety_seed.wrapping_add(i)) % tier3_options.len()];
                let cost = if unit_type == ArmyUnitType::LightArtillery {
                    Money::dollars(2000)
                } else {
                    Money::dollars(1000)
                };
                if let Some(remaining) = nation.treasury.checked_sub(cost) {
                    nation.treasury = remaining;
                    let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
                    nation.army.push(unit);
                    if i == 0 {
                        actions.push(super::AiAction {
                            text: format!("{} has been expanding its military forces", nation_name),
                            reason: format!(
                                "Tier 3 advanced build: army={}/{} cap, building {:?}",
                                nation.army.len(),
                                tier3_max,
                                unit_type
                            ),
                            is_non_action: false,
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
                if let Some(remaining) = nation.treasury.checked_sub(cost) {
                    nation.treasury = remaining;
                    let unit = ArmyUnit::new(next_unit_id(), unit_type, nation_id, capital);
                    nation.army.push(unit);
                    actions.push(super::AiAction {
                        text: format!("{} has been expanding its military forces", nation_name),
                        reason: format!(
                            "Tier 4 uncapped expansion: army={}, treasury=${}",
                            nation.army.len(),
                            treasury.as_dollars()
                        ),
                        is_non_action: false,
                    });
                }
            }
        }
    }
}

/// Estimate the defensive firepower the AI would face attacking the target's
/// strongest province. Approximates `resolve_battle_with_targeting()` formula:
///
///   defender_fp = total_fp(units) * 1.2 * (1 + terrain) * (1 + effective_fort) + militia_count * 8
///
/// Uses the **strongest single province garrison** (not sum of all provinces),
/// since the AI attacks one province at a time. The field army is combined with
/// garrison base FP before applying multipliers (matching combat).
///
/// Note: General force-level bonus (5% per medal) is intentionally omitted —
/// it requires knowing the defending force composition and is a minor effect.
fn estimate_target_defense(game: &GameState, attacker_id: NationId, target_id: NationId) -> f64 {
    let target = match game.get_nation(target_id) {
        Some(n) => n,
        None => return 0.0,
    };

    // Check if attacker has siege artillery (reduces fort bonus by 50%)
    let attacker_has_siege = game
        .get_nation(attacker_id)
        .map(|n| {
            n.army.iter().any(|u| {
                u.unit_type == ArmyUnitType::SiegeArtillery
                    || u.unit_type == ArmyUnitType::RailroadGun
            })
        })
        .unwrap_or(false);

    // Field army base firepower (defenders concentrate in the attacked province,
    // so the full field army participates in combat alongside the garrison)
    let army_fp: f64 = target.total_military_firepower();

    // Evaluate each province and find the strongest defensive position.
    // In combat, all defending units (field army + garrison) are combined into
    // one force, then: total_fp * 1.2 * (1 + terrain) * (1 + fort) + militia * 8
    let is_minor = !target.is_great_power();
    let mut best_defense = 0.0f64;
    for &pid in &target.province_ids {
        if let Some(prov) = game.get_province(pid) {
            let militia_count = prov.garrison_count as f64;

            // Garrison base FP: militia (FP 1 each) + garrison artillery (FP 4)
            let mut garrison_base = militia_count * 1.0;
            if is_minor && pid == target.capital_province_id {
                garrison_base += 4.0; // GarrisonArtillery
            }

            // Combined base FP (field army + garrison, as in combat)
            let combined_base = army_fp + garrison_base;

            // Apply multipliers to combined base (mirrors resolve_battle_with_targeting)
            let mut multiplied = combined_base * 1.2; // 1.2x defender bonus

            // Terrain bonus on capital tile
            if let Some(tile) = game.hex_map.get_tile(prov.capital_tile) {
                let terrain_bonus = crate::military::combat::terrain_defense_bonus(tile.terrain());
                multiplied *= 1.0 + terrain_bonus;

                // Fort bonus (reduced by siege artillery, matching combat)
                if tile.infrastructure.has_fort {
                    let fort_bonus = crate::military::combat::effective_fort_bonus(
                        tile.infrastructure.fort_level,
                        attacker_has_siege,
                    );
                    multiplied *= 1.0 + fort_bonus;
                }
            }

            // Militia bonus added AFTER multipliers (matches combat formula)
            let prov_defense = multiplied + militia_count * 8.0;

            if prov_defense > best_defense {
                best_defense = prov_defense;
            }
        }
    }

    // If target has no provinces tracked in province_ids (test fixtures),
    // fall back to just the field army with defender bonus
    if best_defense == 0.0 && army_fp > 0.0 {
        best_defense = army_fp * 1.2;
    }

    best_defense
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

        // ── Per-personality defaults ────────────────────────────
        let (default_cooldown, default_army_min, default_threshold, default_opportunism) =
            match personality {
                AiPersonality::Aggressive => (8u32, 3usize, 0.3f64, 1.2f64),
                AiPersonality::Balanced => (12, 4, 0.5, 1.0),
                AiPersonality::Economic => (15, 5, 0.6, 0.8),
                AiPersonality::Diplomatic => (20, 8, 0.9, 0.6),
            };

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
            .unwrap_or(default_cooldown);
        #[cfg(not(feature = "lua"))]
        let war_cooldown = default_cooldown;

        #[cfg(feature = "lua")]
        let army_min_for_war = lua_cfg
            .as_ref()
            .and_then(|c| c.army_min_for_war)
            .unwrap_or(default_army_min);
        #[cfg(not(feature = "lua"))]
        let army_min_for_war = default_army_min;

        #[cfg(feature = "lua")]
        let war_threshold = lua_cfg
            .as_ref()
            .and_then(|c| c.war_threshold)
            .unwrap_or(default_threshold);
        #[cfg(not(feature = "lua"))]
        let war_threshold = default_threshold;

        #[cfg(feature = "lua")]
        let opportunism_weight = lua_cfg
            .as_ref()
            .and_then(|c| c.opportunism_weight)
            .unwrap_or(default_opportunism);
        #[cfg(not(feature = "lua"))]
        let opportunism_weight = default_opportunism;

        let attacker_name = game
            .get_nation(ai_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        // ── 1. Cooldown check ──────────────────────────────────
        let war_prefix = format!("{} declared war on", attacker_name);
        let last_war_turn: Option<u32> = game
            .history
            .iter()
            .filter(|(_, msg)| msg.starts_with(&war_prefix))
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
            });
            continue;
        }

        // ── 2. Military readiness ──────────────────────────────
        let ai_army = game.get_nation(ai_id).map(|n| n.army.len()).unwrap_or(0);
        if ai_army < army_min_for_war {
            actions.push(super::AiAction {
                text: format!("{} did not declare war this turn", attacker_name),
                reason: format!(
                    "army too small: {} units < minimum {} for war",
                    ai_army, army_min_for_war
                ),
                is_non_action: true,
            });
            continue;
        }

        // ── 3. Standing check ──────────────────────────────────
        let standing = game.diplomacy.get_standing(ai_id);
        if standing < 30 {
            actions.push(super::AiAction {
                text: format!("{} did not declare war this turn", attacker_name),
                reason: format!(
                    "diplomatic standing too low: {}/30 — a pariah nation cannot afford another war",
                    standing
                ),
                is_non_action: true,
            });
            continue;
        }

        // ── 4. Target evaluation ───────────────────────────────
        let ai_provinces = game
            .get_nation(ai_id)
            .map(|n| n.province_count())
            .unwrap_or(0);

        // Collect AI warehouse resources for need scoring
        let ai_resources: std::collections::HashSet<ResourceType> = game
            .get_nation(ai_id)
            .map(|n| {
                n.warehouse
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
            province_bonus: f64,
            at_war_bonus: f64,
            coalition_factor: f64,
            relationship_penalty: f64,
        }

        let mut best: Option<Candidate> = None;

        // Snapshot nation IDs and info to avoid borrow issues
        let nation_infos: Vec<(NationId, String, usize, usize, ProvinceId)> = game
            .nations
            .iter()
            .map(|n| {
                let prov_count = game.provinces.iter().filter(|p| p.owner == n.id).count();
                (
                    n.id,
                    n.name.clone(),
                    n.army.len(),
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
            // Skip human player
            if target_id == game.human_player_nation {
                continue;
            }
            // Skip already at war
            if game
                .diplomacy
                .get_relation(ai_id, target_id)
                .map(|r| r.at_war)
                .unwrap_or(false)
            {
                continue;
            }
            // Skip allies
            if game
                .diplomacy
                .has_treaty(ai_id, target_id, crate::events::TreatyType::Alliance)
            {
                continue;
            }
            // Skip conquered (0 provinces)
            if target_provinces == 0 {
                continue;
            }
            // Skip anarchic nations (already free to invade, no war declaration needed)
            if game.get_nation(target_id).is_some_and(|n| n.is_in_anarchy) {
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
                        n.army
                            .iter()
                            .filter(|u| {
                                u.unit_type.category()
                                    == crate::military::units::UnitCategory::Artillery
                            })
                            .count()
                    })
                    .unwrap_or(0);

                let default_min_artillery: usize = match personality {
                    AiPersonality::Aggressive | AiPersonality::Balanced => 2,
                    AiPersonality::Economic | AiPersonality::Diplomatic => 3,
                };

                #[cfg(feature = "lua")]
                let min_artillery = lua_cfg
                    .as_ref()
                    .and_then(|c| c.min_artillery_for_minor_war)
                    .unwrap_or(default_min_artillery);
                #[cfg(not(feature = "lua"))]
                let min_artillery = default_min_artillery;

                if artillery_count < min_artillery {
                    continue;
                }
            }

            // ── need_score ─────────────────────────────────────
            let base_need = (target_provinces as f64 / 5.0).min(1.0);
            // Resource bonus: check target's province tiles for resources the AI lacks
            let target_tile_resources: std::collections::HashSet<ResourceType> = game
                .provinces
                .iter()
                .filter(|p| p.owner == target_id)
                .flat_map(|p| {
                    p.tiles.iter().filter_map(|&coord| {
                        game.hex_map
                            .get_tile(coord)
                            .and_then(|t| t.resource_deposit())
                    })
                })
                .collect();
            let missing_count = target_tile_resources
                .iter()
                .filter(|r| !ai_resources.contains(r))
                .count();
            let resource_bonus = (missing_count as f64 * 0.15).min(0.4);
            let need_score = (base_need + resource_bonus).min(1.0);

            // ── opportunity_score ──────────────────────────────
            // Compare attacker firepower to estimated total defense (including garrison)
            let ai_fp = game
                .get_nation(ai_id)
                .map(|n| n.total_military_firepower())
                .unwrap_or(0.0);
            let target_defense = estimate_target_defense(game, ai_id, target_id);
            let army_ratio = if target_defense > 0.0 {
                (1.0 - (target_defense / (ai_fp + 1.0))).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let province_bonus = if ai_provinces > target_provinces {
                0.2
            } else {
                0.0
            };
            // Check if target is at war with someone else
            let target_at_war_with_other = nation_infos.iter().any(|&(other_id, _, _, _, _)| {
                other_id != ai_id
                    && other_id != target_id
                    && game
                        .diplomacy
                        .get_relation(target_id, other_id)
                        .map(|r| r.at_war)
                        .unwrap_or(false)
            });
            let at_war_bonus = if target_at_war_with_other { 0.3 } else { 0.0 };
            let opportunity_score = (army_ratio + province_bonus + at_war_bonus).clamp(0.0, 1.0);

            // ── coalition_factor ────────────────────────────────
            // Evaluate hypothetical coalition strengths (us+allies vs them+allies)
            let hypothetical = super::assessment::evaluate_hypothetical_war(
                game,
                ai_id,
                target_id,
                #[cfg(feature = "lua")]
                lua_cfg.as_ref(),
            );
            let coalition_factor = hypothetical.power_ratio.clamp(0.0, 2.0) / 2.0;

            // ── relationship_penalty ──────────────────────────
            let mut relationship_penalty = 0.0f64;
            if let Some(rel) = game.diplomacy.get_relation(ai_id, target_id) {
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

            // Conflicting alliance penalty: if any of our allies are also allied with target
            let our_allies = game.diplomacy.get_allies(ai_id);
            let target_allies = game.diplomacy.get_allies(target_id);
            let conflicted = our_allies.iter().any(|a| target_allies.contains(a));
            if conflicted {
                relationship_penalty += 0.5;
            }

            // Pact-defense risk: if target minor has NAPs with other nations,
            // each pact holder may choose to intervene militarily.
            // Penalty scales with protector's military strength relative to ours —
            // a weak protector is less of a deterrent than a strong one.
            if !target_is_gp {
                let protectors: Vec<NationId> = game
                    .diplomacy
                    .get_pact_holders(target_id)
                    .into_iter()
                    .filter(|&pid| pid != ai_id)
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
            // Coalition factor modulates opportunity: weak coalition dampens opportunity
            let combined_score = need_score
                + opportunity_score * opportunism_weight * coalition_factor
                - relationship_penalty;

            if best
                .as_ref()
                .map(|b| combined_score > b.combined_score)
                .unwrap_or(true)
            {
                best = Some(Candidate {
                    target_id,
                    combined_score,
                    need_score,
                    opportunity_score,
                    base_need,
                    resource_bonus,
                    missing_count,
                    army_ratio,
                    province_bonus,
                    at_war_bonus,
                    coalition_factor,
                    relationship_penalty,
                });
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
                        "best candidate {} scored combined={:.2} (need={:.2}, opp={:.2}) < threshold {:.2}; relationship_penalty={:.2}",
                        target_name,
                        c.combined_score,
                        c.need_score,
                        c.opportunity_score,
                        war_threshold,
                        c.relationship_penalty,
                    ),
                    is_non_action: true,
                });
                continue;
            }
            None => {
                // No eligible candidates (all at war, allied, anarchic, dogpiled, or no targets)
                actions.push(super::AiAction {
                    text: format!(
                        "{} did not declare war this turn",
                        attacker_name
                    ),
                    reason: "no eligible targets (already at war, allied, anarchic, or dogpile-prevented)".to_string(),
                    is_non_action: true,
                });
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
                    });
                    continue;
                }
            }
        }

        // ── 8. Declare war ─────────────────────────────────────
        // Find the target's weakest province (fewest tiles)
        let target_capital = nation_infos
            .iter()
            .find(|(id, _, _, _, _)| *id == target_id)
            .map(|(_, _, _, _, cap)| *cap)
            .unwrap_or(ProvinceId(0));

        let attack_province = game
            .provinces
            .iter()
            .filter(|p| p.owner == target_id)
            .min_by_key(|p| p.tiles.len())
            .map(|p| p.id)
            .unwrap_or(target_capital);

        // Final guard: target must still own at least one province
        if game.provinces.iter().all(|p| p.owner != target_id) {
            continue;
        }

        if game.ai_debug {
            eprintln!(
                "[AI:{}:war] Declaring war on {} (army={}, standing={}, score={:.2})",
                attacker_name, target_name, ai_army, standing, candidate.combined_score,
            );
        }
        game.diplomacy.declare_war(ai_id, target_id);
        game.pending_attacks.push((ai_id, attack_province));
        targeted_this_round.push(target_id);
        actions.push(super::AiAction {
            text: format!("{} has declared war on {}!", attacker_name, target_name),
            reason: format!(
                "combined={:.2} > threshold={:.2}. \
                 need={:.2} = base_need {:.2} (target provinces/5) + resource_bonus {:.2} ({} missing resources). \
                 opportunity={:.2} = army_ratio {:.2} (firepower advantage) + province_bonus {:.2} (larger empire) + at_war_bonus {:.2} (target already at war). \
                 opportunism_weight={:.2}, coalition_factor={:.2} (ally power ratio), relationship_penalty={:.2} (standing/treaties/pact-defense risk).",
                candidate.combined_score,
                war_threshold,
                candidate.need_score,
                candidate.base_need,
                candidate.resource_bonus,
                candidate.missing_count,
                candidate.opportunity_score,
                candidate.army_ratio,
                candidate.province_bonus,
                candidate.at_war_bonus,
                opportunism_weight,
                candidate.coalition_factor,
                candidate.relationship_penalty,
            ),
            is_non_action: false,
        });
        let turn = game.turn;
        game.history.push((
            turn,
            format!("{} declared war on {}", attacker_name, target_name),
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

    let army_size = nation.army.len();

    // Find nations we are at war with, plus anarchic nations (free to invade)
    let enemies: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.id != nation_id)
        .filter(|n| {
            n.is_in_anarchy
                || game
                    .diplomacy
                    .get_relation(nation_id, n.id)
                    .map(|r| r.at_war)
                    .unwrap_or(false)
        })
        .map(|n| n.id)
        .collect();

    // If at war and has >= 4 army units, attack enemy's weakest province
    // Smarter targeting: prefer provinces with fewer defenders, valuable resources,
    // and avoid attacking when outnumbered.
    if !enemies.is_empty() && army_size >= 4 {
        // Score each enemy province — lower score = better target
        let mut candidates: Vec<(ProvinceId, i32)> = Vec::new();
        let attacker_province_ids: Vec<ProvinceId> = game
            .get_nation(nation_id)
            .map(|n| n.province_ids.clone())
            .unwrap_or_default();
        for &enemy_id in &enemies {
            let enemy_is_gp = game
                .get_nation(enemy_id)
                .map(|n| n.is_great_power())
                .unwrap_or(false);
            // Count enemy army units in each province
            let enemy_army: Vec<(ProvinceId, usize)> = {
                let mut counts: Vec<(ProvinceId, usize)> = Vec::new();
                if let Some(en) = game.get_nation(enemy_id) {
                    for unit in &en.army {
                        if let Some(entry) = counts.iter_mut().find(|(p, _)| *p == unit.position) {
                            entry.1 += 1;
                        } else {
                            counts.push((unit.position, 1));
                        }
                    }
                }
                counts
            };

            // Pre-compute which of our provinces are adjacent to enemy territory
            let our_provinces: Vec<&crate::map::Province> = game
                .provinces
                .iter()
                .filter(|p| attacker_province_ids.contains(&p.id))
                .collect();

            for prov in &game.provinces {
                if prov.owner == enemy_id {
                    // Adjacency check: only attack provinces reachable by land
                    // (adjacent to one of our provinces) or via a naval landing site.
                    let is_adjacent = our_provinces
                        .iter()
                        .any(|ours| crate::map::provinces_are_adjacent(&game.hex_map, ours, prov));
                    let has_landing = game
                        .pending_landings
                        .iter()
                        .any(|(nid, pid, _)| *nid == nation_id && *pid == prov.id);
                    if !is_adjacent && !has_landing {
                        continue;
                    }

                    let tile_count = prov.tiles.len();
                    // Use actual garrison count from the province
                    let garrison_size = prov.garrison_count as usize;
                    // Estimated defender strength: garrison + stationed army
                    let stationed = enemy_army
                        .iter()
                        .find(|(p, _)| *p == prov.id)
                        .map(|(_, c)| *c)
                        .unwrap_or(0);
                    let total_defenders = garrison_size + stationed;

                    // For GP enemies, be more aggressive: attack if we have at
                    // least 2/3 of their defenders (wars stagnate otherwise
                    // because neither side ever attacks).
                    // For minor nations, keep the conservative check.
                    let dominated = if enemy_is_gp {
                        // Allow attacking GP provinces when we have a reasonable
                        // force, even if not strictly outnumbering defenders.
                        total_defenders > army_size + army_size / 2
                    } else {
                        total_defenders > army_size
                    };
                    if dominated {
                        continue;
                    }

                    // Score: fewer tiles = weaker (lower score = better)
                    // Bonus: check for valuable terrain (mountains/hills may have
                    // mineral deposits worth targeting)
                    let mut score = tile_count as i32 + stationed as i32 * 3;

                    // Penalize terrain defense (mountains are hard to attack)
                    let capital_terrain = game
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

                    candidates.push((prov.id, score));
                }
            }
        }

        // Sort by score ascending (best target first)
        candidates.sort_by_key(|&(_, score)| score);

        if let Some(&(target_prov, _)) = candidates.first() {
            // Only queue if not already pending
            let already_pending = game
                .pending_attacks
                .iter()
                .any(|(a, p)| *a == nation_id && *p == target_prov);
            if !already_pending {
                game.pending_attacks.push((nation_id, target_prov));
            }
        }
    }
}

/// Upgrade units when tech prerequisites have been researched.
fn ai_upgrade_units(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let researched = nation.researched_techs.clone();

    // Collect upgrade info: (index, new_type)
    let upgrades: Vec<(usize, ArmyUnitType)> = nation
        .army
        .iter()
        .enumerate()
        .filter_map(|(i, unit)| {
            unit.unit_type.upgrade_to().and_then(|new_type| {
                let prereq = new_type.stats().prerequisite_tech;
                match prereq {
                    // If the upgrade requires a tech, check it's researched
                    Some(ref tech_name) => {
                        let has_tech = game
                            .game_data
                            .tech_tree
                            .all_techs()
                            .iter()
                            .any(|t| t.name == *tech_name && researched.contains(&t.id));
                        if has_tech { Some((i, new_type)) } else { None }
                    }
                    // No tech prereq: always upgrade
                    None => Some((i, new_type)),
                }
            })
        })
        .collect();

    // Apply upgrades
    if let Some(nation) = game.get_nation_mut(nation_id) {
        for (idx, new_type) in upgrades {
            if idx < nation.army.len() {
                nation.army[idx].unit_type = new_type;
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
        ai.treasury = Money::dollars(3000);
        // AI starts with 0 army units

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.army.len(), 1, "AI should build 1 Regulars unit");
        assert_eq!(ai.army[0].unit_type, ArmyUnitType::Regulars);
        assert_eq!(ai.army[0].owner, NationId(2));
        assert_eq!(ai.army[0].position, ProvinceId(2)); // capital
        assert_eq!(
            ai.treasury,
            Money::dollars(2500),
            "Treasury should be reduced by $500"
        );
    }

    #[test]
    fn ai_does_not_build_military_when_poor() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(1000); // < $2,000 threshold

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.army.is_empty(),
            "AI should not build army units when treasury <= $2,000"
        );
    }

    #[test]
    fn ai_builds_unit_when_army_small_for_territory() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give large treasury so AI can spend on both infrastructure and military
        ai.treasury = Money::dollars(50000);
        ai.ai_personality = Some(AiPersonality::Aggressive);
        // Give AI enough provinces that 3 army isn't enough (deficit scoring)
        for i in 10..15 {
            ai.add_province(ProvinceId(i));
        }
        // Give AI 3 existing army units
        for i in 0..3 {
            ai.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.army.len() >= 4,
            "AI should have built at least a 4th unit, has {}",
            ai.army.len()
        );
        assert!(
            ai.treasury < Money::dollars(50000),
            "Treasury should be reduced after building a unit"
        );
    }

    #[test]
    fn ai_builds_more_units_when_territory_large() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(20000);
        ai.ai_personality = Some(AiPersonality::Aggressive);
        // Give AI many provinces so it needs a large army
        for i in 10..20 {
            ai.add_province(ProvinceId(i));
        }

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.army.len() >= 2,
            "AI with large territory should build multiple units, has {}",
            ai.army.len()
        );
        assert!(
            ai.treasury < Money::dollars(20000),
            "Treasury should be reduced after building units"
        );
    }

    #[test]
    fn ai_military_units_have_unique_ids() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(50000);

        // Run multiple turns to build several units
        let mut actions = Vec::new();
        for _ in 0..5 {
            ai_build_military(&mut game, NationId(2), &mut actions);
        }

        let ai = game.get_nation(NationId(2)).unwrap();
        let ids: Vec<UnitId> = ai.army.iter().map(|u| u.id).collect();
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
        ai.ai_personality = Some(AiPersonality::Aggressive);
        for i in 0..10 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..8 {
            ai.army.push(ArmyUnit::new(
                UnitId(5100 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        game.turn = TurnNumber::new(10);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // AI should declare war on the minor — it has provinces, low army, no relationship
        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        assert!(rel.is_some(), "Relation between AI and minor should exist");
        assert!(
            rel.unwrap().at_war,
            "AI should be at war with the minor nation"
        );
        // Should have queued a pending attack
        assert!(
            game.pending_attacks
                .iter()
                .any(|(attacker, _)| *attacker == NationId(2)),
            "AI should queue an attack on the minor"
        );
    }

    #[test]
    fn ai_does_not_redeclare_war_on_existing_enemy() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(10);
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        // Pre-set war
        game.diplomacy.declare_war(NationId(2), NationId(3));

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Should not have queued any attack (already at war, and no other targets)
        assert!(
            game.pending_attacks.is_empty(),
            "AI should not queue attack via ai_declare_wars if already at war"
        );
    }

    #[test]
    fn ai_respects_war_cooldown() {
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced); // cooldown = 12
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        // Simulate a recent war declaration in history
        game.turn = TurnNumber::new(15);
        game.history.push((
            TurnNumber::new(10),
            "AINation declared war on SomeNation".to_string(),
        ));

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Turn 15 - Turn 10 = 5 turns < cooldown of 12, so should NOT declare war
        let at_war = game
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
        ai.ai_personality = Some(AiPersonality::Aggressive);
        // Aggressive needs artillery for minor targets and enough firepower
        // to overcome garrison defense (≈37 FP).
        // 10 Regulars (20) + 8 LA (24) = 44 FP vs 37 defense.
        for i in 0..10 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..8 {
            ai.army.push(ArmyUnit::new(
                UnitId(5100 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        game.turn = TurnNumber::new(5);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        assert!(
            rel.is_some() && rel.unwrap().at_war,
            "Aggressive AI should declare war with low threshold and small army"
        );
    }

    #[test]
    fn diplomatic_ai_needs_high_score_to_declare_war() {
        let mut game = test_game_with_ai_and_minor();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Diplomatic);
        // Diplomatic needs >= 8 army units; give exactly 8
        for i in 0..8 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        // Establish a consulate + positive relations to raise relationship_penalty
        let _ = game.diplomacy.build_consulate(NationId(2), NationId(3));
        // Improve score to make it harder to declare war
        if let Some(rel) = game.diplomacy.get_relation_mut(NationId(2), NationId(3)) {
            rel.improve_score(40);
        }
        game.turn = TurnNumber::new(5);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Diplomatic threshold is 0.9, the relationship penalty from consulate (+0.1)
        // and positive relations (+0.4) should push the score below threshold.
        // The minor only has 1 province so need_score is low (0.2).
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            !at_war,
            "Diplomatic AI should not declare war when relationship penalty is high"
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
        game.provinces.push(province3);

        let mut gp3 = Nation::new(
            NationId(3),
            "WeakGP".to_string(),
            NationColor::Gray,
            NationType::GreatPower,
            ProvinceId(3),
        );
        gp3.treasury = Money::dollars(1000);
        // WeakGP has 0 army units — very vulnerable
        game.nations.push(gp3);

        // Give the AI attacker a strong army (enough to overcome garrison defense)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Aggressive);
        for i in 0..6 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..4 {
            ai.army.push(ArmyUnit::new(
                UnitId(5100 + i),
                ArmyUnitType::LightArtillery,
                NationId(2),
                ProvinceId(2),
            ));
        }
        game.turn = TurnNumber::new(10);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // AI should target the weak GP (NationId(3)), not the human (NationId(1))
        let at_war_with_gp3 = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        let at_war_with_human = game
            .diplomacy
            .get_relation(NationId(2), NationId(1))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            at_war_with_gp3,
            "AI should be able to declare war on a weak Great Power"
        );
        assert!(
            !at_war_with_human,
            "AI should never target the human player"
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
        ai_nation.treasury = Money::dollars(10000);
        for i in 0..4 {
            ai_nation
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

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
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
            ai_debug: false,
        };

        // Put AI at war with minor
        game.diplomacy.declare_war(NationId(2), NationId(3));

        // Give AI enough army units
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..6 {
            ai.army.push(ArmyUnit::new(
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
        let attack = game.pending_attacks.iter().find(|(a, _)| *a == NationId(2));
        assert!(attack.is_some(), "AI should queue an attack");
        let (_, target) = attack.unwrap();
        assert_eq!(
            *target,
            ProvinceId(3),
            "AI should target the smaller/weaker province (1 tile vs 5 tiles)"
        );
    }
}
