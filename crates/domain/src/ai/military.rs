#![allow(unused_labels)]
use crate::game_state::GameState;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;

use super::common::{AiPersonality, get_personality, next_unit_id};

/// Build military units when the nation has sufficient treasury.
/// Personality affects thresholds and unit preferences:
///
/// - **Aggressive**: lower thresholds, prefer artillery
/// - **Diplomatic**: higher thresholds, fewer units
/// - **Economic**: moderate thresholds
/// - **Balanced**: default behavior
pub(crate) fn ai_build_military(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<String>,
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
            actions.push(format!(
                "{} has been expanding its military forces",
                nation_name
            ));
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
            actions.push(format!(
                "{} has been expanding its military forces",
                nation_name
            ));
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
                        actions.push(format!(
                            "{} has been expanding its military forces",
                            nation_name
                        ));
                    }
                } else {
                    break;
                }
            }
        }
    }
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
    actions: &mut Vec<String>,
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
            continue;
        }

        // ── 2. Military readiness ──────────────────────────────
        let ai_army = game.get_nation(ai_id).map(|n| n.army.len()).unwrap_or(0);
        if ai_army < army_min_for_war {
            continue;
        }

        // ── 3. Standing check ──────────────────────────────────
        let standing = game.diplomacy.get_standing(ai_id);
        if standing < 30 {
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
            #[allow(dead_code)]
            need_score: f64,
            #[allow(dead_code)]
            opportunity_score: f64,
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

        for &(target_id, ref _target_name, target_army, target_provinces, _target_capital) in
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
            // Anti-dogpile: skip if another AI targeted this nation this round
            if targeted_this_round.contains(&target_id) {
                continue;
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
            let army_ratio = (1.0 - (target_army as f64 / (ai_army as f64 + 1.0))).clamp(0.0, 1.0);
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
            relationship_penalty = relationship_penalty.clamp(0.0, 1.0);

            // ── combined_score ─────────────────────────────────
            let combined_score =
                need_score + opportunity_score * opportunism_weight - relationship_penalty;

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
                });
            }
        }

        // ── 5-6. Best target + threshold check ────────────────
        let candidate = match best {
            Some(c) if c.combined_score > war_threshold => c,
            _ => continue,
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
        actions.push(format!(
            "{} has declared war on {}!",
            attacker_name, target_name
        ));
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
    _actions: &mut Vec<String>,
) {
    // Phase 1: Upgrade units if possible
    ai_upgrade_units(game, nation_id);

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let army_size = nation.army.len();

    // Find nations we are at war with
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

    // If at war and has >= 4 army units, attack enemy's weakest province
    // Smarter targeting: prefer provinces with fewer defenders, valuable resources,
    // and avoid attacking when outnumbered.
    if !enemies.is_empty() && army_size >= 4 {
        // Score each enemy province — lower score = better target
        let mut candidates: Vec<(ProvinceId, i32)> = Vec::new();
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

            for prov in &game.provinces {
                if prov.owner == enemy_id {
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
                            TerrainType::FertileHills | TerrainType::BarrenHills => score += 2,
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
    fn ai_builds_unit_when_army_has_3_units() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(15000);
        ai.ai_personality = Some(AiPersonality::Balanced);
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
            ai.treasury < Money::dollars(15000),
            "Treasury should be reduced after building a unit"
        );
    }

    #[test]
    fn ai_builds_advanced_unit_when_army_large() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(12000);
        // Give AI 5 existing army units
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.army.len(), 6, "AI should have built a 6th unit");
        // With variety, the unit type varies by personality and seed
        let unit_type = ai.army[5].unit_type;
        assert!(
            matches!(
                unit_type,
                ArmyUnitType::LightArtillery
                    | ArmyUnitType::StandardArtillery
                    | ArmyUnitType::SiegeArtillery
                    | ArmyUnitType::Grenadiers
            ),
            "6th unit should be a tier-3 type, got {:?}",
            unit_type
        );
        assert!(
            ai.treasury < Money::dollars(12000),
            "Treasury should be reduced after building a unit"
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
        // Give AI enough army units (Balanced army_min_for_war = 4)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Balanced);
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
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
        // Aggressive only needs 3 army units
        for i in 0..3 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
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

        // Give the AI attacker a strong army
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

        let mut game = test_game_with_ai_and_minor();

        // Add a second minor province with more tiles (stronger garrison estimate)
        let province4 = Province::new(
            ProvinceId(4),
            "Big Minor Province".to_string(),
            NationId(3),
            HexCoord::new(6, 6),
            vec![
                HexCoord::new(6, 6),
                HexCoord::new(7, 6),
                HexCoord::new(6, 7),
                HexCoord::new(7, 7),
                HexCoord::new(8, 6),
            ],
            3,
        );
        game.provinces.push(province4);
        game.get_nation_mut(NationId(3))
            .unwrap()
            .add_province(ProvinceId(4));

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
