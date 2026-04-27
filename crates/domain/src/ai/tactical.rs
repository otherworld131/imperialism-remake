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
pub fn ai_tactical_decisions(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<super::AiAction>,
) {
    let personality = get_personality(game, nation_id);

    if game.ai_debug {
        let nation_name = game
            .get_nation(nation_id)
            .map(|n| n.name.as_str())
            .unwrap_or("?");
        let army_size = game
            .get_nation(nation_id)
            .map(|n| n.field_army_count())
            .unwrap_or(0);
        eprintln!(
            "[AI:{}:tactical] army={}, personality={}",
            nation_name, army_size, personality
        );
    }

    // Phase 1: Build forts on border provinces
    ai_build_forts(game, nation_id, personality, actions);

    // Phase 2: Distribute the field army.
    // - If the capital is threatened (enemy army nearby, adjacent hostile
    //   province, or pending landing), concentrate units at the capital.
    // - Otherwise push surplus units from the capital / deep interior out
    //   to undefended border provinces (and forward staging near pending
    //   attack targets).
    ai_distribute_field_army(game, nation_id, personality);

    // Phase 3: Propose peace after prolonged losing war
    ai_propose_peace(game, nation_id, personality, actions);
}

/// Threat level against the nation's capital, driving field-army distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapitalThreat {
    /// Capital is not currently endangered.
    Safe,
    /// Enemy presence is near the capital (adjacent province or army within 4 hexes).
    Nearby,
    /// Enemy is at the gates (army within 2 hexes or a pending landing
    /// targeting the capital or an adjacent province).
    Imminent,
}

/// Assess the threat level to a nation's capital.
///
/// Uses hex distance from the capital tile to the closest enemy army unit, and
/// checks for pending naval landings on or near the capital province.
pub(crate) fn capital_threat_level(game: &GameState, nation_id: NationId) -> CapitalThreat {
    let Some(nation) = game.get_nation(nation_id) else {
        return CapitalThreat::Safe;
    };
    let capital_pid = nation.capital_province_id;
    let Some(capital_prov) = game.get_province(capital_pid) else {
        return CapitalThreat::Safe;
    };
    let capital_tile = capital_prov.capital_tile;

    // Enemies at war with us.
    let enemies: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.id != nation_id)
        .filter(|n| {
            game.world.diplomacy
                .get_relation(nation_id, n.id)
                .map(|r| r.at_war)
                .unwrap_or(false)
        })
        .map(|n| n.id)
        .collect();
    if enemies.is_empty() {
        return CapitalThreat::Safe;
    }

    // Pending landing on capital or adjacent province → Imminent.
    let capital_and_adj: Vec<ProvinceId> = std::iter::once(capital_pid)
        .chain(game.world.provinces.iter().filter_map(|p| {
            if p.owner == nation_id
                && p.id != capital_pid
                && crate::map::provinces_are_adjacent(&game.world.hex_map, capital_prov, p)
            {
                Some(p.id)
            } else {
                None
            }
        }))
        .collect();
    for (_, target_pid, _) in &game.transient.pending_landings {
        if capital_and_adj.contains(target_pid)
            && game
                .get_province(*target_pid)
                .is_some_and(|p| enemies.contains(&p.owner) || p.owner == nation_id)
        {
            return CapitalThreat::Imminent;
        }
    }

    // Minimum distance from capital tile to any enemy army unit's province
    // capital tile (rough proxy — enemy units sit in their own provinces).
    let mut min_enemy_dist: i32 = i32::MAX;
    for enemy_id in &enemies {
        let Some(enemy) = game.get_nation(*enemy_id) else {
            continue;
        };
        for unit in &enemy.military.army {
            if let Some(prov) = game.get_province(unit.position) {
                let d = capital_tile.distance(prov.capital_tile);
                if d < min_enemy_dist {
                    min_enemy_dist = d;
                }
            }
        }
    }

    // Adjacent enemy-owned province → at least Nearby.
    let has_adj_enemy_province = game.world.provinces.iter().any(|p| {
        enemies.contains(&p.owner)
            && crate::map::provinces_are_adjacent(&game.world.hex_map, capital_prov, p)
    });

    if min_enemy_dist <= 2 {
        CapitalThreat::Imminent
    } else if min_enemy_dist <= 4 || has_adj_enemy_province {
        CapitalThreat::Nearby
    } else {
        CapitalThreat::Safe
    }
}

/// Distribute the nation's field army each turn.
///
/// Two phases, both capped by `max_redeploys_per_turn`:
///   1. **Concentrate** — if the capital is threatened and has fewer than the
///      threatened-reserve units, pull from safer interior / non-adjacent
///      border provinces toward the capital.
///   2. **Spread** — push surplus units (capital beyond its reserve, or any
///      over-stocked interior province) to undefended border provinces.
fn ai_distribute_field_army(game: &mut GameState, nation_id: NationId, personality: AiPersonality) {
    // ── Load Lua tunables (feature-gated) ───────────────────────────
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
    #[cfg(not(feature = "lua"))]
    let _ = personality;

    #[cfg(feature = "lua")]
    let reserve_normal: usize = lua_cfg
        .as_ref()
        .and_then(|c| c.capital_reserve_normal)
        .unwrap_or(2);
    #[cfg(not(feature = "lua"))]
    let reserve_normal: usize = 2;

    #[cfg(feature = "lua")]
    let reserve_threatened: usize = lua_cfg
        .as_ref()
        .and_then(|c| c.capital_reserve_threatened)
        .unwrap_or(6);
    #[cfg(not(feature = "lua"))]
    let reserve_threatened: usize = 6;

    // Phase 2: `max_redeploys_per_turn` is no longer enforced here. Distribution
    // moves as many units as needed in a single turn; per-turn movement will
    // be constrained by transport capacity in a future ticket. The Lua field
    // is left in `LuaAiConfig` for that future work.
    #[cfg(feature = "lua")]
    let _ = lua_cfg.as_ref().and_then(|c| c.max_redeploys_per_turn);

    // ── Snapshot the nation ────────────────────────────────────────
    let Some(nation) = game.get_nation(nation_id) else {
        return;
    };
    let capital_pid = nation.capital_province_id;
    let owned_provinces: Vec<ProvinceId> = nation.province_ids.clone();

    // Enemies we are at war with.
    let enemies: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.id != nation_id)
        .filter(|n| {
            game.world.diplomacy
                .get_relation(nation_id, n.id)
                .map(|r| r.at_war)
                .unwrap_or(false)
        })
        .map(|n| n.id)
        .collect();
    if enemies.is_empty() {
        return;
    }

    let threat = capital_threat_level(game, nation_id);

    let enemy_province_ids: Vec<ProvinceId> = game
        .world.provinces
        .iter()
        .filter(|p| enemies.contains(&p.owner))
        .map(|p| p.id)
        .collect();

    // Count our MOVABLE field-army units stationed in each province.
    // Militia/GarrisonArtillery (movement = 0) cannot redeploy — counting
    // them inflates both capital surplus and the deficit-satisfied tally.
    let unit_counts: std::collections::HashMap<ProvinceId, usize> = {
        let mut m: std::collections::HashMap<ProvinceId, usize> = std::collections::HashMap::new();
        if let Some(n) = game.get_nation(nation_id) {
            for u in &n.military.army {
                if u.unit_type.can_move() {
                    *m.entry(u.position).or_insert(0) += 1;
                }
            }
        }
        m
    };

    // Classify each owned province.
    let mut border_provinces: Vec<ProvinceId> = Vec::new();
    let mut interior_provinces: Vec<ProvinceId> = Vec::new();
    for &pid in &owned_provinces {
        if pid == capital_pid {
            continue;
        }
        let Some(prov) = game.get_province(pid) else {
            continue;
        };
        let borders_enemy = prov.tiles.iter().any(|&tile_coord| {
            tile_coord.neighbors().iter().any(|neighbor| {
                game.world.hex_map
                    .get_tile(*neighbor)
                    .and_then(|t| t.province_id)
                    .is_some_and(|npid| enemy_province_ids.contains(&npid))
            })
        });
        if borders_enemy {
            border_provinces.push(pid);
        } else {
            interior_provinces.push(pid);
        }
    }

    let capital_reserve_target = match threat {
        CapitalThreat::Safe => reserve_normal,
        CapitalThreat::Nearby | CapitalThreat::Imminent => reserve_threatened,
    };

    // ── Phase A: concentrate toward capital when threatened ─────────
    if matches!(threat, CapitalThreat::Nearby | CapitalThreat::Imminent) {
        let capital_have = *unit_counts.get(&capital_pid).unwrap_or(&0);
        if capital_have < capital_reserve_target {
            let deficit = capital_reserve_target - capital_have;
            // Pull from interior first, then from border provinces that are
            // NOT adjacent to the attacker-of-capital (cheapest to vacate).
            let pull_sources: Vec<ProvinceId> = interior_provinces
                .iter()
                .chain(border_provinces.iter())
                .copied()
                .collect();

            // Precompute the set of already-pending unit ids ONCE and update
            // it incrementally as we push moves. Rebuilding per iteration
            // made the whole distribution O(N²) in the number of moves.
            let mut already_pending: std::collections::HashSet<crate::map::UnitId> =
                game.transient.pending_moves.iter().map(|(_, uid, _)| *uid).collect();

            let mut pulled = 0;
            for src_pid in pull_sources {
                if pulled >= deficit {
                    break;
                }
                let Some(nation) = game.get_nation(nation_id) else {
                    return;
                };
                let candidate_unit_ids: Vec<crate::map::UnitId> = nation
                    .military.army
                    .iter()
                    .filter(|u| {
                        u.unit_type.can_move()
                            && u.position == src_pid
                            && !already_pending.contains(&u.id)
                    })
                    .map(|u| u.id)
                    .collect();
                for uid in candidate_unit_ids {
                    if pulled >= deficit {
                        break;
                    }
                    game.transient.pending_moves.push((nation_id, uid, capital_pid));
                    already_pending.insert(uid);
                    pulled += 1;
                }
            }
        }
    }

    // ── Phase B: spread to undefended border / forward staging ──────
    // Under a `Safe` threat, the capital only needs `reserve_normal` units;
    // everything else is surplus and should push to the front. When the
    // threat is non-Safe, we still distribute *genuine* surplus beyond the
    // threatened reserve.
    // Count only movable units at the capital when computing surplus —
    // static garrison (militia) can't be "spread forward" anyway.
    let capital_have_now: usize = {
        let nation = game.get_nation(nation_id).expect("nation present");
        nation
            .military.army
            .iter()
            .filter(|u| u.unit_type.can_move() && u.position == capital_pid)
            .count()
    };
    let capital_surplus = capital_have_now.saturating_sub(capital_reserve_target);

    // Destinations: empty border first, then low-garrisoned border,
    // then provinces adjacent to a pending attack target (forward staging).
    let mut dest_priority: Vec<ProvinceId> = Vec::new();
    let mut border_sorted: Vec<(ProvinceId, usize)> = border_provinces
        .iter()
        .map(|&pid| (pid, *unit_counts.get(&pid).unwrap_or(&0)))
        .collect();
    border_sorted.sort_by_key(|&(_, c)| c);
    for (pid, _) in border_sorted {
        dest_priority.push(pid);
    }
    // Forward staging: provinces we own that are adjacent to a province
    // currently in pending_attacks targeted by this nation.
    let pending_attack_targets: Vec<ProvinceId> = game
        .transient.pending_attacks
        .iter()
        .filter(|(a, _)| *a == nation_id)
        .map(|(_, p)| *p)
        .collect();
    if !pending_attack_targets.is_empty() {
        for &pid in &owned_provinces {
            if pid == capital_pid || dest_priority.contains(&pid) {
                continue;
            }
            let Some(prov) = game.get_province(pid) else {
                continue;
            };
            let is_staging = pending_attack_targets.iter().any(|&tpid| {
                game.get_province(tpid)
                    .is_some_and(|tp| crate::map::provinces_are_adjacent(&game.world.hex_map, prov, tp))
            });
            if is_staging {
                dest_priority.push(pid);
            }
        }
    }

    if dest_priority.is_empty() {
        return;
    }

    // Sources for spreading: capital surplus first, then any interior
    // province with >1 stationed unit (leaving at least 1 if it's not
    // empty — we won't fully vacate interior small stacks here).
    let mut spread_sources: Vec<(ProvinceId, usize)> = Vec::new();
    if capital_surplus > 0 {
        spread_sources.push((capital_pid, capital_surplus));
    }
    for &pid in &interior_provinces {
        let have = *unit_counts.get(&pid).unwrap_or(&0);
        if have > 1 {
            spread_sources.push((pid, have - 1));
        }
    }

    // Round-robin destinations so we balance across the front. No per-turn
    // cap — surplus redistributes in one pass. We also bail if every chosen
    // destination equals the current source (otherwise the while loop would
    // spin forever when the only staging province IS the source).
    let mut already_pending: std::collections::HashSet<crate::map::UnitId> =
        game.transient.pending_moves.iter().map(|(_, uid, _)| *uid).collect();
    let mut dest_idx = 0usize;
    for (src_pid, src_surplus) in spread_sources {
        let mut remaining = src_surplus;
        let non_self_dests = dest_priority.iter().any(|&d| d != src_pid);
        if !non_self_dests {
            continue;
        }
        while remaining > 0 {
            let dest_pid = dest_priority[dest_idx % dest_priority.len()];
            dest_idx += 1;
            if dest_pid == src_pid {
                continue;
            }
            let Some(nation) = game.get_nation(nation_id) else {
                return;
            };
            let candidate = nation
                .military.army
                .iter()
                .find(|u| {
                    u.unit_type.can_move()
                        && u.position == src_pid
                        && !already_pending.contains(&u.id)
                })
                .map(|u| u.id);
            let Some(uid) = candidate else {
                break; // nothing left to move from this source
            };
            game.transient.pending_moves.push((nation_id, uid, dest_pid));
            already_pending.insert(uid);
            remaining -= 1;
        }
    }
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
    actions: &mut Vec<super::AiAction>,
) {
    use crate::map::infrastructure::build_fort;

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Need treasury > $5,000 to build a fort (level 1 costs $5,000)
    if nation.economy.treasury <= Money::dollars(5000) {
        return;
    }

    // Find enemies we are at war with.
    // Anarchic enemies are excluded: you cannot sue for peace with a country
    // whose government has collapsed (card #81).
    let enemies: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.id != nation_id)
        .filter(|n| !n.diplomacy.is_in_anarchy)
        .filter(|n| {
            game.world.diplomacy
                .get_relation(nation_id, n.id)
                .map(|r| r.at_war)
                .unwrap_or(false)
        })
        .map(|n| n.id)
        .collect();

    if enemies.is_empty() {
        return;
    }

    let Some(nation) = game.get_nation(nation_id) else {
        return;
    };
    let capital_province_id = nation.capital_province_id;
    let nation_name = nation.name.clone();
    let owned_provinces: Vec<ProvinceId> = nation.province_ids.clone();

    // Collect enemy-owned tiles for adjacency check
    let enemy_province_ids: Vec<ProvinceId> = game
        .world.provinces
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
                    game.world.hex_map
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
        .world.hex_map
        .get_tile(fort_coord)
        .map(|t| t.infrastructure.fort_level)
        .unwrap_or(0);
    if current_level >= 3 {
        return;
    }

    // Build the fort
    let new_level = current_level + 1;
    let cost = match crate::map::infrastructure::fort_cost(new_level, &game.game_data.game_config) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Can we afford it?
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    if nation.economy.treasury.checked_sub(cost).is_none() {
        return;
    }

    if build_fort(&mut game.world.hex_map, fort_coord, &game.game_data.game_config).is_ok() {
        let treasury_after = {
            let Some(nation) = game.get_nation_mut(nation_id) else {
                return;
            };
            nation.economy.treasury -= cost;
            nation.economy.treasury.as_dollars()
        };
        game.transient.pending_ai_cash_spending.push((
            nation_id,
            crate::economy::ledger::CashSink::AiSpendingOther,
            cost,
            None,
        ));
        actions.push(super::AiAction {
            text: format!("{} has fortified its borders", nation_name),
            reason: format!(
                "Built level {} fort ({} enemies at border, treasury ${})",
                new_level,
                enemies.len(),
                treasury_after,
            ),
            is_non_action: false,
            nation_id,
        });
    }
}

#[cfg(test)]
fn ai_move_units_to_threatened(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);
    ai_distribute_field_army(game, nation_id, personality);
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
    actions: &mut Vec<super::AiAction>,
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

    // Find enemies we are at war with.
    // Anarchic enemies are excluded: you cannot sue for peace with a country
    // whose government has collapsed (card #81).
    let enemies: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.id != nation_id)
        .filter(|n| !n.diplomacy.is_in_anarchy)
        .filter(|n| {
            game.world.diplomacy
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
        let has_pending_attack = game.transient.pending_attacks.iter().any(|(attacker, prov_id)| {
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
        let war_start = super::assessment::find_war_start_turn(game, nation_id, enemy_id);
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
            // Non-action: AI stays at war with this enemy despite evaluating
            actions.push(super::AiAction {
                text: format!(
                    "{} did not propose peace with {}",
                    nation_name, enemy_name
                ),
                reason: format!(
                    "war_duration={}, win_likelihood={:.2}, captured={}, lost={}, won_enough={}, lost_enough={} — conditions for peace not met",
                    war_duration,
                    assessment.win_likelihood,
                    worthiness.provinces_captured,
                    worthiness.provinces_lost,
                    worthiness.won_enough,
                    worthiness.lost_enough,
                ),
                is_non_action: true,
                nation_id,
            });
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
            .is_some_and(|n| n.diplomacy.ai_personality.is_some());
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
                game.world.diplomacy.queue_peace(nation_id, enemy_id);
                let reason = if worthiness.lost_enough {
                    " (heavy losses)"
                } else if worthiness.won_enough {
                    " (objectives achieved)"
                } else {
                    ""
                };
                actions.push(super::AiAction {
                    text: format!(
                        "{} has sued for peace with {}{}",
                        nation_name, enemy_name, reason
                    ),
                    reason: format!(
                        "captured={}, lost={}, won_enough={}, lost_enough={}",
                        worthiness.provinces_captured,
                        worthiness.provinces_lost,
                        worthiness.won_enough,
                        worthiness.lost_enough,
                    ),
                    is_non_action: false,
                    nation_id,
                });
                let turn = game.turn;
                game.archive.history.push((
                    turn,
                    crate::events::HistoryEvent::PeaceMade {
                        a: nation_id,
                        b: enemy_id,
                    },
                ));
            } else if game.ai_debug {
                eprintln!(
                    "[AI:{}:peace] {} rejected peace proposal",
                    nation_name, enemy_name,
                );
            }
        } else if target_is_human {
            // AI-to-human: create a pending proposal for the UI
            let _ = game.world.diplomacy.propose_peace(nation_id, enemy_id, game.turn);
            let reason = if worthiness.lost_enough {
                " (heavy losses)"
            } else if worthiness.won_enough {
                " (objectives achieved)"
            } else {
                ""
            };
            actions.push(super::AiAction {
                text: format!(
                    "{} proposes peace with {}{}",
                    nation_name, enemy_name, reason
                ),
                reason: format!(
                    "captured={}, lost={}, won_enough={}, lost_enough={}",
                    worthiness.provinces_captured,
                    worthiness.provinces_lost,
                    worthiness.won_enough,
                    worthiness.lost_enough,
                ),
                is_non_action: false,
                nation_id,
            });
        } else {
            // AI-to-minor-nation: auto-accept (minor nations are passive)
            game.world.diplomacy.queue_peace(nation_id, enemy_id);
            let reason = if worthiness.lost_enough {
                " (heavy losses)"
            } else if worthiness.won_enough {
                " (objectives achieved)"
            } else {
                ""
            };
            actions.push(super::AiAction {
                text: format!(
                    "{} has sued for peace with {}{}",
                    nation_name, enemy_name, reason
                ),
                reason: format!(
                    "captured={}, lost={}, won_enough={}, lost_enough={}",
                    worthiness.provinces_captured,
                    worthiness.provinces_lost,
                    worthiness.won_enough,
                    worthiness.lost_enough,
                ),
                is_non_action: false,
                nation_id,
            });
            let turn = game.turn;
            game.archive.history.push((
                turn,
                crate::events::HistoryEvent::PeaceMade {
                    a: nation_id,
                    b: enemy_id,
                },
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
        let tile = game.world.hex_map.get_tile(ai_capital_tile).unwrap();
        assert!(
            tile.infrastructure.has_fort,
            "AI should build a fort on border province capital tile"
        );
        assert_eq!(tile.infrastructure.fort_level, 1, "Fort should be level 1");

        // Treasury should be reduced by $5,000
        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.economy.treasury,
            Money::dollars(15000),
            "Treasury should be reduced by $5,000 for fort"
        );

        assert!(
            actions.iter().any(|a| a.text.contains("fortified")),
            "Should report fort building"
        );
    }

    #[test]
    fn ai_does_not_build_fort_when_poor() {
        let mut game = test_game_with_adjacent_provinces();
        game.get_nation_mut(NationId(2)).unwrap().economy.treasury = Money::dollars(3000);

        let mut actions = Vec::new();
        ai_build_forts(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        let ai_capital_tile = HexCoord::new(0, 0);
        let tile = game.world.hex_map.get_tile(ai_capital_tile).unwrap();
        assert!(
            !tile.infrastructure.has_fort,
            "AI should not build fort when too poor"
        );
    }

    #[test]
    fn ai_does_not_build_fort_when_not_at_war() {
        let mut game = test_game_with_adjacent_provinces();
        // Make peace
        game.world.diplomacy.make_peace(NationId(2), NationId(3));

        let mut actions = Vec::new();
        ai_build_forts(
            &mut game,
            NationId(2),
            AiPersonality::Balanced,
            &mut actions,
        );

        let ai_capital_tile = HexCoord::new(0, 0);
        let tile = game.world.hex_map.get_tile(ai_capital_tile).unwrap();
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
        ai.military.army.push(ArmyUnit::new(
            UnitId(9000),
            ArmyUnitType::Regulars,
            NationId(2),
            ProvinceId(2), // stationed in AI province
        ));

        // Add another province for the AI that is NOT a border province
        let safe_tile = HexCoord::new(0, 5);
        game.world.hex_map.set_tile(
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
        game.world.provinces.push(safe_province);
        game.get_nation_mut(NationId(2))
            .unwrap()
            .add_province(ProvinceId(4));

        // Move the Regulars unit to the safe province so it's available to
        // redeploy. We match by UnitId because `seed_militia_from_garrison_count`
        // also pushed static militia into `army` — those cannot satisfy a
        // redeploy deficit (F-007).
        {
            let ai = game.get_nation_mut(NationId(2)).unwrap();
            for u in ai.military.army.iter_mut() {
                if u.id == UnitId(9000) {
                    u.position = ProvinceId(4);
                }
            }
        }

        ai_move_units_to_threatened(&mut game, NationId(2));

        // Should have a pending move to the border province (ProvinceId(2))
        assert!(
            game.transient.pending_moves
                .iter()
                .any(|(nation, _, dest)| *nation == NationId(2) && *dest == ProvinceId(2)),
            "AI should queue a move to the threatened border province"
        );
    }

    #[test]
    fn ai_distributes_multiple_units_to_undefended_borders() {
        // Seed a game where the AI holds its capital plus three interior
        // provinces and three undefended border provinces. Pile 8 units at
        // the capital and verify that `ai_distribute_field_army` pushes
        // several of them out in a single turn.
        use crate::map::{HexMap, Province};
        use crate::nation::{Nation, NationColor};

        let mut hex_map = HexMap::new(30, 30);
        let tile = |x: i32, y: i32| HexCoord::new(x, y);
        // Capital (prov 1) deep in the interior.
        let capital_tile = tile(10, 10);
        hex_map.set_tile(
            capital_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        // Border provinces (prov 2, 3, 4) each touching an enemy province.
        let borders = [(tile(15, 10), 2u32), (tile(15, 12), 3), (tile(15, 14), 4)];
        for (coord, pid) in borders.iter() {
            hex_map.set_tile(
                *coord,
                crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(*pid)),
            );
        }
        // Enemy provinces adjacent to each border.
        let enemies = [
            (tile(16, 10), 10u32),
            (tile(16, 12), 11),
            (tile(16, 14), 12),
        ];
        for (coord, pid) in enemies.iter() {
            hex_map.set_tile(
                *coord,
                crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(*pid)),
            );
        }

        let mut provinces: Vec<Province> = vec![Province::new(
            ProvinceId(1),
            "Capital".into(),
            NationId(2),
            capital_tile,
            vec![capital_tile],
            4,
        )];
        for (coord, pid) in borders.iter() {
            provinces.push(Province::new(
                ProvinceId(*pid),
                format!("Border{}", pid),
                NationId(2),
                *coord,
                vec![*coord],
                4,
            ));
        }
        // One single enemy nation owns all three enemy provinces.
        for (coord, pid) in enemies.iter() {
            provinces.push(Province::new(
                ProvinceId(*pid),
                format!("Enemy{}", pid),
                NationId(3),
                *coord,
                vec![*coord],
                3,
            ));
        }

        let mut ai = Nation::new(
            NationId(2),
            "AINation".into(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.add_province(ProvinceId(2));
        ai.add_province(ProvinceId(3));
        ai.add_province(ProvinceId(4));
        // 8 units piled at the capital.
        for i in 0..8 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(7000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(1),
            ));
        }
        let enemy = Nation::new(
            NationId(3),
            "Enemy".into(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(10),
        );
        let human = Nation::new(
            NationId(1),
            "Human".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(99),
        );

        let mut diplomacy = crate::diplomacy::DiplomacyState::new();
        diplomacy.declare_war(NationId(2), NationId(3));

        let mut game = crate::test_game_state! {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map: hex_map,
            provinces: provinces,
            nations: vec![human, ai, enemy],
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

        ai_distribute_field_army(&mut game, NationId(2), AiPersonality::Balanced);

        // Capital is Safe (enemy is ~5 hexes away, no adjacent enemy-owned
        // province touches the capital), so Phase B spreads the surplus.
        // Expect moves to each of the three undefended border provinces.
        let destinations: std::collections::HashSet<ProvinceId> = game
            .transient.pending_moves
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(2))
            .map(|(_, _, dest)| *dest)
            .collect();
        assert!(
            destinations.contains(&ProvinceId(2))
                && destinations.contains(&ProvinceId(3))
                && destinations.contains(&ProvinceId(4)),
            "should queue moves to all three undefended border provinces; got {:?}",
            destinations
        );
        assert!(
            game.transient.pending_moves.len() >= 3,
            "should queue at least 3 redeployments in a single turn; got {}",
            game.transient.pending_moves.len()
        );
    }

    #[test]
    fn ai_concentrates_units_at_threatened_capital() {
        // Capital sits next to an enemy province (threat = Nearby). AI has
        // 6 units split across interior / border, capital is empty. Expect
        // several units to get pulled back to the capital.
        use crate::map::{HexMap, Province};
        use crate::nation::{Nation, NationColor};

        let mut hex_map = HexMap::new(30, 30);
        let capital_tile = HexCoord::new(10, 10);
        let enemy_tile = HexCoord::new(11, 10); // adjacent to capital
        let interior_tiles = [HexCoord::new(8, 10), HexCoord::new(8, 12)];
        let border_tile = HexCoord::new(10, 8);

        hex_map.set_tile(
            capital_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            enemy_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(10)),
        );
        for (i, t) in interior_tiles.iter().enumerate() {
            hex_map.set_tile(
                *t,
                crate::map::tile::Tile::with_province(
                    TerrainType::Grassland,
                    ProvinceId(2 + i as u32),
                ),
            );
        }
        hex_map.set_tile(
            border_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(4)),
        );

        let mut provinces = vec![
            Province::new(
                ProvinceId(1),
                "Capital".into(),
                NationId(2),
                capital_tile,
                vec![capital_tile],
                4,
            ),
            Province::new(
                ProvinceId(2),
                "InteriorA".into(),
                NationId(2),
                interior_tiles[0],
                vec![interior_tiles[0]],
                4,
            ),
            Province::new(
                ProvinceId(3),
                "InteriorB".into(),
                NationId(2),
                interior_tiles[1],
                vec![interior_tiles[1]],
                4,
            ),
            Province::new(
                ProvinceId(4),
                "QuietBorder".into(),
                NationId(2),
                border_tile,
                vec![border_tile],
                4,
            ),
            Province::new(
                ProvinceId(10),
                "Enemy".into(),
                NationId(3),
                enemy_tile,
                vec![enemy_tile],
                3,
            ),
        ];
        // No opposing enemy next to the QuietBorder, so it's an interior
        // province from our classifier's POV. Good for this test.
        let _ = &mut provinces;

        let mut ai = Nation::new(
            NationId(2),
            "AINation".into(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        ai.add_province(ProvinceId(2));
        ai.add_province(ProvinceId(3));
        ai.add_province(ProvinceId(4));
        // 6 units spread across the three non-capital provinces.
        for i in 0..2 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(8000 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }
        for i in 0..2 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(8100 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(3),
            ));
        }
        for i in 0..2 {
            ai.military.army.push(ArmyUnit::new(
                UnitId(8200 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(4),
            ));
        }

        let enemy = Nation::new(
            NationId(3),
            "Enemy".into(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(10),
        );
        let human = Nation::new(
            NationId(1),
            "Human".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(99),
        );

        let mut diplomacy = crate::diplomacy::DiplomacyState::new();
        diplomacy.declare_war(NationId(2), NationId(3));

        let mut game = crate::test_game_state! {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map: hex_map,
            provinces: provinces,
            nations: vec![human, ai, enemy],
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

        assert_eq!(
            capital_threat_level(&game, NationId(2)),
            CapitalThreat::Nearby,
            "enemy adjacent to capital province must trigger Nearby threat"
        );

        ai_distribute_field_army(&mut game, NationId(2), AiPersonality::Balanced);

        let moves_to_capital: usize = game
            .transient.pending_moves
            .iter()
            .filter(|(nid, _, dest)| *nid == NationId(2) && *dest == ProvinceId(1))
            .count();
        assert!(
            moves_to_capital >= 3,
            "capital under Nearby threat should pull >=3 units back (balanced reserve_threatened=6), got {}",
            moves_to_capital
        );
    }

    #[test]
    fn capital_threat_is_safe_when_enemies_are_far() {
        // Capital far from any enemy army, no adjacent enemy provinces, no
        // pending landings → Safe.
        use crate::map::{HexMap, Province};
        use crate::nation::{Nation, NationColor};

        let mut hex_map = HexMap::new(40, 40);
        let capital_tile = HexCoord::new(5, 5);
        let border_tile = HexCoord::new(20, 20);
        let enemy_tile = HexCoord::new(21, 20);
        hex_map.set_tile(
            capital_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            border_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        hex_map.set_tile(
            enemy_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );

        let provinces = vec![
            Province::new(
                ProvinceId(1),
                "Capital".into(),
                NationId(2),
                capital_tile,
                vec![capital_tile],
                4,
            ),
            Province::new(
                ProvinceId(2),
                "FarBorder".into(),
                NationId(2),
                border_tile,
                vec![border_tile],
                4,
            ),
            Province::new(
                ProvinceId(3),
                "Enemy".into(),
                NationId(3),
                enemy_tile,
                vec![enemy_tile],
                3,
            ),
        ];
        let mut ai = Nation::new(
            NationId(2),
            "AI".into(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        ai.add_province(ProvinceId(2));
        let enemy = Nation::new(
            NationId(3),
            "Enemy".into(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(3),
        );
        let human = Nation::new(
            NationId(1),
            "Human".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(99),
        );

        let mut diplomacy = crate::diplomacy::DiplomacyState::new();
        diplomacy.declare_war(NationId(2), NationId(3));

        let game = crate::test_game_state! {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".into(),
            hex_map: hex_map,
            provinces: provinces,
            nations: vec![human, ai, enemy],
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
        assert_eq!(
            capital_threat_level(&game, NationId(2)),
            CapitalThreat::Safe,
            "capital at distance 15 from only enemy should be Safe"
        );
    }

    #[test]
    fn capital_threat_is_imminent_with_pending_landing_on_capital() {
        // A pending naval landing on the capital province promotes threat
        // to Imminent even if no enemy army is physically nearby.
        use crate::map::{HexMap, Province};
        use crate::nation::{Nation, NationColor};

        let mut hex_map = HexMap::new(40, 40);
        let capital_tile = HexCoord::new(5, 5);
        let enemy_tile = HexCoord::new(30, 30);
        hex_map.set_tile(
            capital_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            enemy_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );

        let provinces = vec![
            Province::new(
                ProvinceId(1),
                "Capital".into(),
                NationId(2),
                capital_tile,
                vec![capital_tile],
                4,
            ),
            Province::new(
                ProvinceId(3),
                "Enemy".into(),
                NationId(3),
                enemy_tile,
                vec![enemy_tile],
                3,
            ),
        ];
        let ai = Nation::new(
            NationId(2),
            "AI".into(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        let enemy = Nation::new(
            NationId(3),
            "Enemy".into(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(3),
        );
        let human = Nation::new(
            NationId(1),
            "Human".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(99),
        );

        let mut diplomacy = crate::diplomacy::DiplomacyState::new();
        diplomacy.declare_war(NationId(2), NationId(3));

        let game = crate::test_game_state! {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".into(),
            hex_map: hex_map,
            provinces: provinces,
            nations: vec![human, ai, enemy],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: crate::data::GameData::default(),
            diplomacy: diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: vec![(NationId(3), ProvinceId(1), TurnNumber::new(2))],
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
        assert_eq!(
            capital_threat_level(&game, NationId(2)),
            CapitalThreat::Imminent,
            "pending landing on the capital must raise threat to Imminent"
        );
    }

    #[test]
    fn ai_does_not_move_units_when_not_at_war() {
        let mut game = test_game_with_adjacent_provinces();
        game.world.diplomacy.make_peace(NationId(2), NationId(3));

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.military.army.push(ArmyUnit::new(
            UnitId(9001),
            ArmyUnitType::Regulars,
            NationId(2),
            ProvinceId(2),
        ));

        ai_move_units_to_threatened(&mut game, NationId(2));

        assert!(
            game.transient.pending_moves.is_empty(),
            "No moves should be queued when not at war"
        );
    }

    #[test]
    fn ai_proposes_peace_after_heavy_losses() {
        use crate::events::HistoryEvent;
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(25);

        // Record war declaration and province losses in history
        game.archive.history.push((
            TurnNumber::new(1),
            HistoryEvent::WarDeclared {
                attacker: NationId(2),
                defender: NationId(3),
                protectee: None,
            },
        ));
        // AI lost 2 provinces (meeting Balanced lost_enough_losses threshold)
        game.archive.history.push((
            TurnNumber::new(10),
            HistoryEvent::ProvinceConquered {
                conqueror: NationId(3),
                loser: NationId(2),
                province: ProvinceId(2),
            },
        ));
        game.archive.history.push((
            TurnNumber::new(15),
            HistoryEvent::ProvinceConquered {
                conqueror: NationId(3),
                loser: NationId(2),
                province: ProvinceId(2),
            },
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
            .world.diplomacy
            .get_relation(NationId(2), NationId(3))
            .map(|r| r.at_war)
            .unwrap_or(true);
        assert!(
            !at_war,
            "AI should propose peace after heavy losses (lost_enough triggered)"
        );
        assert!(
            actions.iter().any(|a| a.text.contains("sued for peace")),
            "Should report peace proposal"
        );
    }

    #[test]
    fn diplomatic_ai_proposes_peace_earlier() {
        use crate::events::HistoryEvent;
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(15);

        game.archive.history.push((
            TurnNumber::new(1),
            HistoryEvent::WarDeclared {
                attacker: NationId(2),
                defender: NationId(3),
                protectee: None,
            },
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
            .world.diplomacy
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
        use crate::events::HistoryEvent;
        let mut game = test_game_with_adjacent_provinces();
        game.turn = TurnNumber::new(25);

        game.archive.history.push((
            TurnNumber::new(1),
            HistoryEvent::WarDeclared {
                attacker: NationId(2),
                defender: NationId(3),
                protectee: None,
            },
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
            .world.diplomacy
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
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        // AI has 1 province (ProvinceId(2)) but lost 3 provinces in history
        use crate::events::HistoryEvent;
        game.archive.history.push((
            TurnNumber::new(5),
            HistoryEvent::WarDeclared {
                attacker: NationId(2),
                defender: NationId(1),
                protectee: None,
            },
        ));
        for turn in [10u32, 12, 14] {
            game.archive.history.push((
                TurnNumber::new(turn),
                HistoryEvent::ProvinceConquered {
                    conqueror: NationId(1),
                    loser: NationId(2),
                    province: ProvinceId(2),
                },
            ));
        }

        // Put AI at war with human
        game.world.diplomacy.declare_war(NationId(2), NationId(1));

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
                .any(|a| a.text.contains("proposes peace") && a.text.contains("heavy losses")),
            "AI should propose peace when heavily losing; actions: {:?}",
            actions
        );

        // War is still active (human hasn't accepted yet)
        assert!(
            game.world.diplomacy.is_at_war(NationId(2), NationId(1)),
            "War should still be active until human accepts"
        );

        // But a pending peace proposal should exist
        assert!(
            game.world.diplomacy.pending_proposals.iter().any(|p| {
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
        ai.diplomacy.ai_personality = Some(AiPersonality::Diplomatic);
        // Diplomatic AI has low lost_enough_losses threshold (1)
        // Simulate losing 1 province
        use crate::events::HistoryEvent;
        game.archive.history.push((
            TurnNumber::new(5),
            HistoryEvent::WarDeclared {
                attacker: NationId(2),
                defender: NationId(1),
                protectee: None,
            },
        ));
        game.archive.history.push((
            TurnNumber::new(10),
            HistoryEvent::ProvinceConquered {
                conqueror: NationId(1),
                loser: NationId(2),
                province: ProvinceId(2),
            },
        ));

        // Put at war
        game.world.diplomacy.declare_war(NationId(2), NationId(1));

        let mut actions = Vec::new();
        ai_propose_peace(
            &mut game,
            NationId(2),
            AiPersonality::Diplomatic,
            &mut actions,
        );

        // Diplomatic AI should propose peace (lost_enough: 1 loss >= threshold of 1)
        assert!(
            actions.iter().any(|a| a.text.contains("proposes peace")),
            "Diplomatic AI should propose peace after 1 province loss; actions: {:?}",
            actions
        );
        // Should be a pending proposal to human
        assert!(
            game.world.diplomacy.pending_proposals.iter().any(|p| {
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
        ai.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        // AI has not lost any provinces — no conquest history against it

        // Put at war
        game.world.diplomacy.declare_war(NationId(2), NationId(1));
        game.turn = TurnNumber::new(50); // past any war duration threshold
        use crate::events::HistoryEvent;
        game.archive.history.push((
            TurnNumber::new(1),
            HistoryEvent::WarDeclared {
                attacker: NationId(2),
                defender: NationId(1),
                protectee: None,
            },
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
            !actions.iter().any(|a| !a.is_non_action),
            "AI should not sue for peace when not losing (non-actions allowed); actions: {:?}",
            actions
        );
    }
}
