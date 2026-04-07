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

    // Thresholds vary by personality
    let (tier1_max, tier1_treasury, tier2_max, tier2_treasury, tier3_treasury) = match personality {
        AiPersonality::Aggressive => (
            4,
            Money::dollars(1500),
            7,
            Money::dollars(3000),
            Money::dollars(6000),
        ),
        AiPersonality::Diplomatic => (
            2,
            Money::dollars(3000),
            4,
            Money::dollars(8000),
            Money::dollars(15000),
        ),
        AiPersonality::Economic => (
            3,
            Money::dollars(2500),
            5,
            Money::dollars(6000),
            Money::dollars(12000),
        ),
        AiPersonality::Balanced => (
            3,
            Money::dollars(2000),
            5,
            Money::dollars(5000),
            Money::dollars(10000),
        ),
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
        let tier3_max = match personality {
            AiPersonality::Aggressive => 15,
            AiPersonality::Diplomatic => 8,
            AiPersonality::Economic => 10,
            AiPersonality::Balanced => 12,
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

/// Periodically, each AI Great Power considers declaring war on a Minor Nation.
/// Frequency and army threshold depend on personality:
///
/// - **Aggressive**: every 15 turns, needs >= 3 units
/// - **Diplomatic**: every 40 turns, needs >= 8 units
/// - **Economic**: every 30 turns, needs >= 5 units
/// - **Balanced**: every 20 turns, needs >= 4 units
pub(crate) fn ai_declare_wars(
    game: &mut GameState,
    ai_nation_ids: &[NationId],
    actions: &mut Vec<String>,
) {
    let turn_number = game.turn.0;

    // Collect minor nation IDs, their capitals, names, and tile counts.
    // Skip minor nations that have been fully conquered: check both
    // province_ids (ownership tracking) and actual province ownership.
    let minor_nations: Vec<(NationId, ProvinceId, String, usize)> = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .filter(|n| {
            // Must still actually own at least one province
            game.provinces.iter().any(|p| p.owner == n.id)
        })
        .map(|n| {
            let total_tiles: usize = game
                .provinces
                .iter()
                .filter(|p| p.owner == n.id)
                .map(|p| p.tiles.len())
                .sum();
            (n.id, n.capital_province_id, n.name.clone(), total_tiles)
        })
        .collect();

    if minor_nations.is_empty() {
        return;
    }

    // Track which minors are being targeted this round to avoid dogpiling
    let mut targeted_this_round: Vec<NationId> = Vec::new();

    // Also check which minors are already at war with any AI
    let already_targeted: Vec<NationId> = minor_nations
        .iter()
        .filter(|(mn_id, _, _, _)| {
            ai_nation_ids.iter().any(|&ai_id| {
                game.diplomacy
                    .get_relation(ai_id, *mn_id)
                    .map(|r| r.at_war)
                    .unwrap_or(false)
            })
        })
        .map(|(mn_id, _, _, _)| *mn_id)
        .collect();

    for &ai_id in ai_nation_ids {
        let personality = get_personality(game, ai_id);

        // War frequency and army threshold depend on personality
        let (war_interval, army_threshold) = match personality {
            AiPersonality::Aggressive => (25u32, 5),
            AiPersonality::Diplomatic => (40, 8),
            AiPersonality::Economic => (30, 5),
            AiPersonality::Balanced => (25, 4),
        };

        // Check if this is a war-consideration turn for this AI
        if !turn_number.is_multiple_of(war_interval) {
            continue;
        }

        // Only attack if AI has enough army units
        let army_size = game.get_nation(ai_id).map(|n| n.army.len()).unwrap_or(0);
        if army_size < army_threshold {
            continue;
        }

        // AI with low standing (<30) avoids declaring wars to limit diplomatic damage
        let standing = game.diplomacy.get_standing(ai_id);
        if standing < 30 {
            continue;
        }

        // Find best target: not already at war, not dogpiled, most tiles (most valuable)
        let mut candidates: Vec<_> = minor_nations
            .iter()
            .filter(|(mn_id, _, _, _)| {
                // Not already at war with this AI
                let at_war = game
                    .diplomacy
                    .get_relation(ai_id, *mn_id)
                    .map(|r| r.at_war)
                    .unwrap_or(false);
                // Not already targeted by another AI (anti-dogpile)
                let dogpiled =
                    already_targeted.contains(mn_id) || targeted_this_round.contains(mn_id);
                !at_war && !dogpiled
            })
            .collect();

        // Skip candidates with 0 tiles (already fully conquered)
        candidates.retain(|c| c.3 > 0);

        // Sort by tile count descending (most valuable first)
        candidates.sort_by(|a, b| b.3.cmp(&a.3));

        // Use pseudo-random seed to add some variety: pick from top 3 candidates
        if candidates.is_empty() {
            continue;
        }
        let seed = (game.turn.0 as usize).wrapping_add(ai_id.0 as usize);
        let pick_range = candidates.len().min(3);
        let target_index = seed % pick_range;
        let (target_id, target_capital, ref target_name, _) = *candidates[target_index];

        // Find a province actually owned by the target to attack
        let attack_province = game
            .provinces
            .iter()
            .find(|p| p.owner == target_id)
            .map(|p| p.id)
            .unwrap_or(target_capital);

        // Skip if no province is actually owned by the target
        if game.provinces.iter().all(|p| p.owner != target_id) {
            continue;
        }

        // Consult Lua for war evaluation — only when a relation has a non-zero score
        // (fresh consulates start at 0; only evaluate when there's real diplomatic history)
        #[cfg(feature = "lua")]
        if let Some(rel) = game
            .diplomacy
            .get_relation(ai_id, target_id)
            .filter(|r| r.score != 0)
        {
            let relations = rel.score;
            if let Some(false) =
                super::lua_bridge::lua_evaluate_war(game, personality, ai_id, target_id, relations)
            {
                if game.ai_debug {
                    eprintln!(
                        "[AI:war] Lua vetoed war on {} (relations={})",
                        target_name, relations
                    );
                }
                continue;
            }
        }

        let attacker_name = game
            .get_nation(ai_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        if game.ai_debug {
            eprintln!(
                "[AI:{}:war] Declaring war on {} (army={}, interval={}, standing={})",
                attacker_name, target_name, army_size, war_interval, standing
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

    // Phase 2: Great Power vs Great Power wars
    // Aggressive AIs will target weaker Great Powers when no minor targets remain
    // and they have military superiority.
    let remaining_minors: usize = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power() && !n.province_ids.is_empty())
        .count();

    if remaining_minors <= 2 && turn_number > 40 {
        for &ai_id in ai_nation_ids {
            let personality = get_personality(game, ai_id);

            // Aggressive, Balanced, and Economic AIs consider GP wars
            let gp_war_interval = match personality {
                AiPersonality::Aggressive => 30u32,
                AiPersonality::Economic => 50,
                AiPersonality::Balanced => 60,
                _ => continue,
            };

            if !turn_number.is_multiple_of(gp_war_interval) {
                continue;
            }

            let ai_army = game.get_nation(ai_id).map(|n| n.army.len()).unwrap_or(0);
            if ai_army < 4 {
                continue;
            }

            let ai_provinces = game
                .get_nation(ai_id)
                .map(|n| n.province_count())
                .unwrap_or(0);

            // Find weakest GP that is not allied, not already at war with us, not human
            let mut gp_targets: Vec<(NationId, usize, ProvinceId)> = game
                .nations
                .iter()
                .filter(|n| {
                    n.is_great_power()
                        && n.id != ai_id
                        && n.id != game.human_player_nation
                        && !game
                            .diplomacy
                            .get_relation(ai_id, n.id)
                            .is_some_and(|r| r.at_war)
                        && !game.diplomacy.has_treaty(
                            ai_id,
                            n.id,
                            crate::events::TreatyType::Alliance,
                        )
                })
                .map(|n| (n.id, n.province_count(), n.capital_province_id))
                .collect();

            // Target the GP with fewest provinces (weakest)
            gp_targets.sort_by_key(|&(_, p, _)| p);

            if let Some(&(target_id, target_provinces, target_capital)) = gp_targets.first() {
                // Only attack if we have more territory
                if ai_provinces > target_provinces + 2 {
                    // Find the weakest province of the target GP (fewest tiles)
                    // rather than always attacking the capital which is often
                    // the most heavily defended.
                    let attack_province = game
                        .provinces
                        .iter()
                        .filter(|p| p.owner == target_id)
                        .min_by_key(|p| p.tiles.len())
                        .map(|p| p.id)
                        .unwrap_or(target_capital);

                    let attacker_name = game
                        .get_nation(ai_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let target_name = game
                        .get_nation(target_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();

                    game.diplomacy.declare_war(ai_id, target_id);
                    game.pending_attacks.push((ai_id, attack_province));
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
        }
    }
}

/// Strategic military decisions for an AI nation.
///
/// - If at war and has >= 4 army units, queue an attack on the enemy's weakest province
/// - If not at war and has >= 6 army units and turn > 40, consider declaring war on a weak Minor Nation
/// - Upgrade units when tech allows (call unit_type.upgrade_to(), check if prereq tech is researched)
pub(crate) fn ai_military_strategy(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<String>,
) {
    // Phase 1: Upgrade units if possible
    ai_upgrade_units(game, nation_id);

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let army_size = nation.army.len();
    let nation_name = nation.name.clone();

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

    // If not at war and has >= 6 army units and turn > 40, consider proactive war
    if enemies.is_empty() && army_size >= 6 && game.turn.0 > 40 {
        // Find weakest Minor Nation (fewest total tiles, not at war with anyone)
        // Skip nations that have been fully conquered (0 provinces)
        let minor_nations: Vec<(NationId, ProvinceId, usize)> = game
            .nations
            .iter()
            .filter(|n| !n.is_great_power())
            .filter(|n| game.provinces.iter().any(|p| p.owner == n.id))
            .map(|n| {
                let total_tiles: usize = game
                    .provinces
                    .iter()
                    .filter(|p| p.owner == n.id)
                    .map(|p| p.tiles.len())
                    .sum();
                (n.id, n.capital_province_id, total_tiles)
            })
            .filter(|(mn_id, _, tiles)| {
                *tiles > 0
                    && !game
                        .diplomacy
                        .get_relation(nation_id, *mn_id)
                        .map(|r| r.at_war)
                        .unwrap_or(false)
            })
            .collect();

        // Sort by tile count ascending to find weakest
        let mut sorted = minor_nations;
        sorted.sort_by_key(|&(_, _, tiles)| tiles);

        if let Some(&(target_id, _, _)) = sorted.first() {
            // Find a province actually owned by the target
            let attack_province = match game.provinces.iter().find(|p| p.owner == target_id) {
                Some(p) => p.id,
                None => return, // Target has no provinces left
            };
            let target_name = game
                .get_nation(target_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            game.diplomacy.declare_war(nation_id, target_id);
            game.pending_attacks.push((nation_id, attack_province));
            actions.push(format!(
                "{} has declared war on {}!",
                nation_name, target_name
            ));
            let turn = game.turn;
            game.history.push((
                turn,
                format!("{} declared war on {}", nation_name, target_name),
            ));
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
    fn ai_declares_war_on_turn_25() {
        let mut game = test_game_with_ai_and_minor();
        // Set to turn 25 (divisible by 25, the Balanced war interval)
        game.turn = TurnNumber::new(25);
        // Give AI enough army units to meet the >= 4 threshold for war declaration
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..4 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        run_ai_turns(&mut game);

        // AI should have declared war on the minor nation
        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        assert!(rel.is_some(), "Relation between AI and minor should exist");
        assert!(
            rel.unwrap().at_war,
            "AI should be at war with the minor nation"
        );
        // Should have queued a pending attack on the minor's capital
        assert!(
            game.pending_attacks
                .iter()
                .any(|(attacker, target)| *attacker == NationId(2) && *target == ProvinceId(3)),
            "AI should queue an attack on the minor's capital"
        );
    }

    #[test]
    fn ai_does_not_declare_war_on_non_multiple_of_25() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(15);

        run_ai_turns(&mut game);

        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        // Either no relation exists, or it's not at war
        let at_war = rel.map(|r| r.at_war).unwrap_or(false);
        assert!(
            !at_war,
            "AI should not declare war on non-multiple-of-25 turns"
        );
        assert!(
            game.pending_attacks.is_empty(),
            "No attacks should be pending"
        );
    }

    #[test]
    fn ai_does_not_redeclare_war_on_existing_enemy() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(25);

        // Pre-set war
        game.diplomacy.declare_war(NationId(2), NationId(3));

        run_ai_turns(&mut game);

        // Should not have queued a duplicate pending attack
        let attack_count = game
            .pending_attacks
            .iter()
            .filter(|(a, _)| *a == NationId(2))
            .count();
        assert_eq!(
            attack_count, 0,
            "AI should not queue attack if already at war"
        );
    }

    // ── Personality affects war declaration ──────────────────

    #[test]
    fn aggressive_ai_declares_war_on_turn_25() {
        let mut game = test_game_with_ai_and_minor();
        // Set Aggressive personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Aggressive);
        // Give AI 5 army units (Aggressive threshold is 5)
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        // Turn 25: Aggressive declares (every 25 turns)
        game.turn = TurnNumber::new(25);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        assert!(
            rel.is_some() && rel.unwrap().at_war,
            "Aggressive AI should declare war on turn 25"
        );
    }

    #[test]
    fn diplomatic_ai_does_not_declare_war_on_turn_20() {
        let mut game = test_game_with_ai_and_minor();
        // Set Diplomatic personality
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.ai_personality = Some(AiPersonality::Diplomatic);
        // Give AI 5 army units (enough for Balanced, not enough for Diplomatic which needs 8)
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        game.turn = TurnNumber::new(20);

        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // Turn 20 is not a multiple of 40, so Diplomatic AI should not declare war
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            !at_war,
            "Diplomatic AI should not declare war on turn 20 (interval is 40)"
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

    // ── Regression: C5 — AI never declares war on Great Powers ──

    #[test]
    fn ai_does_not_declare_war_on_great_powers() {
        let mut game = test_game_with_ai();
        // Turn 20 is a war-consideration turn for Balanced AI
        game.turn = TurnNumber::new(20);

        // Give AI enough army units to meet the threshold (4 for Balanced)
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..6 {
            ai.army.push(ArmyUnit::new(
                UnitId(5000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        // No minor nations exist in this game — only two Great Powers
        let mut actions = Vec::new();
        ai_declare_wars(&mut game, &[NationId(2)], &mut actions);

        // AI should NOT declare war on the human Great Power
        let at_war = game
            .diplomacy
            .get_relation(NationId(2), NationId(1))
            .map(|r| r.at_war)
            .unwrap_or(false);
        assert!(
            !at_war,
            "AI should never declare war on a Great Power; only minor nations are valid targets"
        );
        assert!(
            game.pending_attacks.is_empty(),
            "No attacks should be pending against a Great Power"
        );
    }
}
