#![allow(unused_labels)]
//! Need-based spending system for AI treasury allocation.
//!
//! Replaces hard-capped independent spending functions with a centralized
//! scoring loop. Each iteration scores every spending category, picks the
//! highest-value action, and executes it. Repeats until treasury hits the
//! reserve floor or nothing scores above the minimum threshold.
//!
//! All weights are Lua-configurable per personality.

use std::collections::HashSet;

use crate::economy::civilians::{BuildTask, Civilian, CivilianType};
use crate::game_state::GameState;
use crate::hex::HexCoord;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::turn::connected_provinces;
use crate::types::*;

use super::common::{AiPersonality, PersonalityConfig, get_personality};
#[cfg(feature = "lua")]
use super::common::lua_or;

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpendingCategory {
    Military,
    Infrastructure,
    Consulate,
    Embassy,
    /// Hire an Engineer civilian (drives infrastructure throughput).
    HireEngineer,
    /// Hire an improver civilian (Farmer/Miner/Forester/etc.) for tile yield.
    HireImprover,
    /// Build a warship (card #112). Naval expansion now uses the same
    /// backlog-driven alternation as army expansion — no hard cap.
    Warship,
}

struct SpendingWeights {
    military_weight: f64,
    economy_weight: f64,
    diplomacy_weight: f64,
    reserve: Money,
    min_threshold: f64,
}

impl SpendingWeights {
    fn from_config(cfg: &PersonalityConfig) -> Self {
        Self {
            military_weight: cfg.spending_military_weight,
            economy_weight: cfg.spending_economy_weight,
            diplomacy_weight: cfg.spending_diplomacy_weight,
            reserve: cfg.spending_reserve,
            min_threshold: cfg.spending_min_threshold,
        }
    }
}

struct ScoredAction {
    category: SpendingCategory,
    score: f64,
    cost: Money,
}

// ── Main entry point ─────────────────────────────────────────────

/// Run the need-based spending loop for one AI nation.
///
/// Scores military, infrastructure, consulate, embassy, and civilian hiring
/// each iteration and executes the highest-value action. Stops when treasury
/// hits the reserve floor or nothing scores above threshold.
pub(crate) fn ai_scored_spending(
    game: &mut GameState,
    nation_id: NationId,
    actions: &mut Vec<super::AiAction>,
) {
    let personality = get_personality(game, nation_id);
    let weights = load_weights(game, personality);

    // Prune lost priority targets (annexed / destroyed) and top up with new
    // picks so we maintain the target count through the whole game. A minor
    // is "lost" if it no longer exists or has zero provinces.
    refresh_priority_targets(game, nation_id, personality);

    // Precompute province connectivity once (avoids per-province BFS each iteration)
    let mut connected = connected_provinces(game, nation_id);
    // Cache the depot plan — computing it is O(owned_hexes × Dijkstra) and
    // would dominate runtime if we re-ran it every spending-loop iteration.
    // It only changes after Infrastructure executes.
    //
    // Card #132: run the planner and persist any commitment changes so the
    // engineer sees a stable target turn over turn. `KeepCommitment` means
    // the existing commitment is still valid; `Fresh` means we cleared it
    // and (optionally) picked a new target.
    let first_outcome = super::economy::plan_next_depot(game, nation_id);
    apply_plan_outcome(game, nation_id, &first_outcome);
    let mut depot_plan: Option<super::economy::DepotPlan> = first_outcome.as_plan().cloned();
    // Priority mode: peacetime nations always develop economy first (diplomacy
    // close second); military only takes priority if at war or falling behind
    // on army size relative to other GPs.
    let military_priority = is_military_priority(game, nation_id);
    let current_turn = game.turn.0;
    // If an Infrastructure pick fails to start a build (e.g. the plan's next
    // hex is blocked by another civilian, unowned, or unaffordable at order
    // time), skip Infrastructure for the rest of this turn to avoid the
    // spending loop spinning on a no-op until it hits the iteration cap.
    let mut infra_blocked_this_turn = false;

    // Safety limit: max 20 spending actions per turn to prevent infinite loops
    for _iteration in 0..20 {
        let treasury = match game.get_nation(nation_id) {
            Some(n) => n.economy.treasury,
            None => return,
        };

        let available = match treasury.checked_sub(weights.reserve) {
            Some(a) if a > Money::ZERO => a,
            _ => break,
        };

        // Score all categories with backlog bonuses applied. The backlog
        // ("turns since last picked") drives natural alternation: any neglected
        // category climbs the priority ladder until it fires, regardless of how
        // dominant another category's base score is. Personality scales the
        // climb rate per category (Aggressive impatient about military,
        // Economic about infra, Diplomatic about diplomacy).
        let mut options: Vec<ScoredAction> = Vec::new();

        if let Some(mut opt) = score_military(game, nation_id, &weights) {
            opt.score += backlog_bonus(
                game,
                nation_id,
                SpendingCategory::Military,
                current_turn,
                military_priority,
            );
            if opt.cost <= available {
                options.push(opt);
            }
        }
        if let Some(mut opt) = score_infrastructure(
            game,
            nation_id,
            &weights,
            depot_plan.as_ref(),
            infra_blocked_this_turn,
        ) {
            opt.score += backlog_bonus(
                game,
                nation_id,
                SpendingCategory::Infrastructure,
                current_turn,
                military_priority,
            );
            if opt.cost <= available {
                options.push(opt);
            }
        }
        if let Some(mut opt) = score_consulate(game, nation_id, &weights) {
            opt.score += backlog_bonus(
                game,
                nation_id,
                SpendingCategory::Consulate,
                current_turn,
                military_priority,
            );
            if opt.cost <= available {
                options.push(opt);
            }
        }
        if let Some(mut opt) = score_embassy(game, nation_id, &weights) {
            opt.score += backlog_bonus(
                game,
                nation_id,
                SpendingCategory::Embassy,
                current_turn,
                military_priority,
            );
            if opt.cost <= available {
                options.push(opt);
            }
        }
        if let Some(mut opt) = score_hire_engineer(game, nation_id, &weights, depot_plan.as_ref()) {
            opt.score += backlog_bonus(
                game,
                nation_id,
                SpendingCategory::HireEngineer,
                current_turn,
                military_priority,
            );
            if opt.cost <= available {
                options.push(opt);
            }
        }
        if let Some(mut opt) = score_civilian(game, nation_id, &weights) {
            opt.score += backlog_bonus(
                game,
                nation_id,
                SpendingCategory::HireImprover,
                current_turn,
                military_priority,
            );
            if opt.cost <= available {
                options.push(opt);
            }
        }
        if let Some(mut opt) = score_warship(game, nation_id, &weights) {
            opt.score += backlog_bonus(
                game,
                nation_id,
                SpendingCategory::Warship,
                current_turn,
                military_priority,
            );
            if opt.cost <= available {
                options.push(opt);
            }
        }

        // Pick highest-scoring action above threshold
        options.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        match options.first() {
            Some(best) if best.score > weights.min_threshold => {
                let cat = best.category;
                if game.ai_debug {
                    let name = game
                        .get_nation(nation_id)
                        .map(|n| n.name.as_str())
                        .unwrap_or("?");
                    eprintln!(
                        "[AI:{}:spending] {:?} score={:.1} cost=${}",
                        name,
                        cat,
                        best.score,
                        best.cost.as_dollars()
                    );
                }
                // Snapshot engineer state before executing Infrastructure to
                // detect whether the execute actually started a build.
                let engineer_was_working = if cat == SpendingCategory::Infrastructure {
                    game.get_nation(nation_id).map(|n| {
                        n.military.civilians
                            .iter()
                            .any(|c| c.civilian_type == CivilianType::Engineer && c.working)
                    })
                } else {
                    None
                };
                let treasury_before = game.get_nation(nation_id).map(|n| n.economy.treasury);
                execute_with_plan(
                    game,
                    nation_id,
                    cat,
                    actions,
                    &connected,
                    depot_plan.as_ref(),
                );
                // Reset the backlog counter for the executed category.
                if let Some(nation) = game.get_nation_mut(nation_id) {
                    nation
                        .diplomacy.ai_priority_state
                        .last_invest_turn
                        .insert(cat, current_turn);
                }
                // Infrastructure mutates the rail network and potentially the
                // engineer's state, so recompute connectivity + plan cache.
                if cat == SpendingCategory::Infrastructure {
                    connected = connected_provinces(game, nation_id);
                    let outcome = super::economy::plan_next_depot(game, nation_id);
                    apply_plan_outcome(game, nation_id, &outcome);
                    depot_plan = outcome.as_plan().cloned();
                    // If the engineer did not transition from idle to working
                    // AND the treasury wasn't spent (engineer hire), the
                    // execute was a silent no-op: the next hex is blocked,
                    // unowned, or unaffordable. Skip infra for the rest of
                    // this turn to prevent the loop from spinning on it.
                    let engineer_now_working = game
                        .get_nation(nation_id)
                        .map(|n| {
                            n.military.civilians
                                .iter()
                                .any(|c| c.civilian_type == CivilianType::Engineer && c.working)
                        })
                        .unwrap_or(false);
                    let treasury_after = game.get_nation(nation_id).map(|n| n.economy.treasury);
                    let treasury_changed = treasury_before != treasury_after;
                    let engineer_transitioned =
                        engineer_was_working == Some(false) && engineer_now_working;
                    if !engineer_transitioned && !treasury_changed {
                        infra_blocked_this_turn = true;
                    }
                }
            }
            _ => break,
        }
    }
}

/// Persist the planner's commitment decision on `nation.diplomacy.ai_priority_state`.
///
/// Card #132: the planner runs pure (`&GameState`) and returns a
/// `PlanOutcome`; mutating the commitment belongs here in the spending
/// loop. `KeepCommitment` is a no-op (the field is already set); `Fresh`
/// either sets the new target or clears the field if nothing is worth
/// building this turn.
fn apply_plan_outcome(
    game: &mut GameState,
    nation_id: NationId,
    outcome: &super::economy::PlanOutcome,
) {
    let current_turn = game.turn.0;
    let Some(nation) = game.get_nation_mut(nation_id) else {
        return;
    };
    match outcome {
        super::economy::PlanOutcome::KeepCommitment(_) => {
            // Commitment already set; leave it.
        }
        super::economy::PlanOutcome::Fresh(Some(plan)) => {
            nation.diplomacy.ai_priority_state.committed_infra_target =
                Some(crate::nation::CommittedInfraTarget {
                    candidate: plan.candidate,
                    origin_capital: plan.origin_capital,
                    turn_committed: current_turn,
                });
        }
        super::economy::PlanOutcome::Fresh(None) => {
            nation.diplomacy.ai_priority_state.committed_infra_target = None;
        }
    }
}

// ── Scoring functions ────────────────────────────────────────────

/// True if the nation should treat military spending as the top priority.
/// Two conditions qualify: (a) the nation is currently at war with anyone,
/// or (b) its army is materially smaller than the strongest rival Great
/// Power's. Otherwise peacetime rules apply: economy > diplomacy > military.
/// Backlog-bonus for `category` for this nation: how many points to add to the
/// raw category score, based on how many turns it has been since the AI last
/// picked this category. Personality scales the per-turn climb rate. When
/// `military_priority` is set (at war or army-lagging), the military backlog
/// weight is doubled — keeps the alternation but tilts the scales toward
/// military investment. All weights live in Lua `game_config`.
fn backlog_bonus(
    game: &GameState,
    nation_id: NationId,
    category: SpendingCategory,
    current_turn: u32,
    military_priority: bool,
) -> f64 {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return 0.0,
    };
    let cfg = &game.game_data.game_config;
    let backlog_turns = match nation.diplomacy.ai_priority_state.last_invest_turn.get(&category) {
        Some(&last) => current_turn.saturating_sub(last),
        None => current_turn.min(cfg.backlog_initial_cap),
    };

    let personality = nation.diplomacy.ai_personality.unwrap_or(AiPersonality::Balanced);
    // Card #112: Warship shares the Military backlog weight — navy growth
    // is still a flavor of military investment; only the execute path is
    // different. Aliasing here lets the match reuse the existing Military
    // weight cells without duplicating the table.
    let category_for_weight = if category == SpendingCategory::Warship {
        SpendingCategory::Military
    } else {
        category
    };
    // Pull the right weight cell from the (personality × category) table.
    let mut weight = match (personality, category_for_weight) {
        (AiPersonality::Aggressive, SpendingCategory::Military) => {
            cfg.backlog_weight_aggressive_military
        }
        (AiPersonality::Aggressive, SpendingCategory::Infrastructure) => {
            cfg.backlog_weight_aggressive_infra
        }
        (AiPersonality::Aggressive, SpendingCategory::Consulate)
        | (AiPersonality::Aggressive, SpendingCategory::Embassy) => {
            cfg.backlog_weight_aggressive_diplomacy
        }
        (AiPersonality::Aggressive, SpendingCategory::HireEngineer) => {
            cfg.backlog_weight_aggressive_hire_engineer
        }
        (AiPersonality::Aggressive, SpendingCategory::HireImprover) => {
            cfg.backlog_weight_aggressive_hire_improver
        }
        (AiPersonality::Balanced, SpendingCategory::Military) => {
            cfg.backlog_weight_balanced_military
        }
        (AiPersonality::Balanced, SpendingCategory::Infrastructure) => {
            cfg.backlog_weight_balanced_infra
        }
        (AiPersonality::Balanced, SpendingCategory::Consulate)
        | (AiPersonality::Balanced, SpendingCategory::Embassy) => {
            cfg.backlog_weight_balanced_diplomacy
        }
        (AiPersonality::Balanced, SpendingCategory::HireEngineer) => {
            cfg.backlog_weight_balanced_hire_engineer
        }
        (AiPersonality::Balanced, SpendingCategory::HireImprover) => {
            cfg.backlog_weight_balanced_hire_improver
        }
        (AiPersonality::Economic, SpendingCategory::Military) => {
            cfg.backlog_weight_economic_military
        }
        (AiPersonality::Economic, SpendingCategory::Infrastructure) => {
            cfg.backlog_weight_economic_infra
        }
        (AiPersonality::Economic, SpendingCategory::Consulate)
        | (AiPersonality::Economic, SpendingCategory::Embassy) => {
            cfg.backlog_weight_economic_diplomacy
        }
        (AiPersonality::Economic, SpendingCategory::HireEngineer) => {
            cfg.backlog_weight_economic_hire_engineer
        }
        (AiPersonality::Economic, SpendingCategory::HireImprover) => {
            cfg.backlog_weight_economic_hire_improver
        }
        (AiPersonality::Diplomatic, SpendingCategory::Military) => {
            cfg.backlog_weight_diplomatic_military
        }
        (AiPersonality::Diplomatic, SpendingCategory::Infrastructure) => {
            cfg.backlog_weight_diplomatic_infra
        }
        (AiPersonality::Diplomatic, SpendingCategory::Consulate)
        | (AiPersonality::Diplomatic, SpendingCategory::Embassy) => {
            cfg.backlog_weight_diplomatic_diplomacy
        }
        (AiPersonality::Diplomatic, SpendingCategory::HireEngineer) => {
            cfg.backlog_weight_diplomatic_hire_engineer
        }
        (AiPersonality::Diplomatic, SpendingCategory::HireImprover) => {
            cfg.backlog_weight_diplomatic_hire_improver
        }
        // Warship is aliased to Military above — this arm is unreachable,
        // but the exhaustiveness check still wants it covered.
        (_, SpendingCategory::Warship) => cfg.backlog_weight_balanced_military,
    };

    // At war or military-lagging: double the military backlog weight to bias
    // the alternation toward catching up the army faster. Applies to navy
    // spend too — a mid-war navy build is as urgent as an army build.
    if military_priority
        && (category == SpendingCategory::Military || category == SpendingCategory::Warship)
    {
        weight *= 2;
    }

    backlog_turns as f64 * weight as f64
}

fn is_military_priority(game: &GameState, nation_id: NationId) -> bool {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return false,
    };
    // (a) At war with anyone
    for other in &game.world.nations {
        if other.id == nation_id {
            continue;
        }
        if game
            .world.diplomacy
            .get_relation(nation_id, other.id)
            .is_some_and(|r| r.at_war)
        {
            return true;
        }
    }
    // (b) Falling behind: my field army is < 60% of strongest rival GP's.
    // Garrison militia stay home and are excluded from the comparison.
    let my_army = nation.field_army_count() as f64;
    let strongest_rival = game
        .world.nations
        .iter()
        .filter(|n| n.id != nation_id && n.is_great_power())
        .map(|n| n.field_army_count() as f64)
        .fold(0.0f64, f64::max);
    my_army < strongest_rival * 0.6
}

fn score_military(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;
    // Military scoring uses *field army* (projectable) counts throughout —
    // garrison militia never leave their home and shouldn't inflate the
    // "do I need more units?" signal.
    let army_count = nation.field_army_count() as f64;
    let province_count = nation.province_count() as f64;

    // Threat: based on relative military strength vs other Great Powers
    let mut threat = 0.0f64;
    let mut strongest_rival_army = 0usize;
    for other in &game.world.nations {
        if other.id == nation_id || !other.is_great_power() {
            continue;
        }
        strongest_rival_army = strongest_rival_army.max(other.field_army_count());
        let at_war = game
            .world.diplomacy
            .get_relation(nation_id, other.id)
            .is_some_and(|r| r.at_war);
        if at_war {
            threat += 20.0 + other.field_army_count() as f64 * 2.0;
        } else {
            // Rivals with larger armies are a latent threat
            let their_army = other.field_army_count() as f64;
            if their_army > army_count {
                threat += (their_army - army_count) * 1.5;
            }
        }
    }

    // Desired army scales with territory and relative power
    let desired = (province_count * 1.5).max(strongest_rival_army as f64 * 0.6);
    let deficit = (desired - army_count).max(0.0) * 8.0;
    let territory = province_count * 2.0;
    let saturation = army_count * 3.0;

    // Penalty for building army when economy is weak — soldiers without workers
    // are unsustainable. Scale down military value when worker count is low.
    // Exception: nations with very few units still need basic defense.
    let workers = nation.economy.labor.total_workers() as f64;
    let economy_penalty = if army_count >= 8.0 && workers <= 1.0 {
        0.3 // large army with no economy — stop building
    } else if army_count >= 5.0 && workers <= 2.0 {
        0.6 // moderate army, weak economy — slow down
    } else {
        1.0
    };

    let raw = (threat + deficit + territory - saturation).max(0.0) * economy_penalty;
    let score = raw * weights.military_weight;

    // Cost of cheapest unit
    let cost = Money::dollars(500); // Regulars

    if score > 0.0 {
        Some(ScoredAction {
            category: SpendingCategory::Military,
            score,
            cost,
        })
    } else {
        None
    }
}

fn score_infrastructure(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
    plan: Option<&super::economy::DepotPlan>,
    infra_blocked: bool,
) -> Option<ScoredAction> {
    // Infrastructure already attempted this turn and failed to start a build
    // (e.g., next hex blocked by a civilian, unowned, or unaffordable).
    // Skip for the rest of the turn so the spending loop doesn't spin.
    if infra_blocked {
        return None;
    }
    let nation = game.get_nation(nation_id)?;
    // If the nation's engineer is already working, `execute_infrastructure`
    // will no-op — don't keep selecting this category and starve other
    // spending actions.
    let engineer_busy = nation
        .military.civilians
        .iter()
        .any(|c| c.civilian_type == CivilianType::Engineer && c.working);
    if engineer_busy {
        return None;
    }

    // Planner picks the best depot candidate. No plan → no infrastructure need.
    let plan = plan?;

    // Minimum cost the AI can afford right now to keep progressing: the
    // cheapest next hex on the path, or the depot cost if the path is empty.
    let cfg = &game.game_data.game_config;
    let next_cost = if let Some(next_coord) = plan.path.first() {
        game.world.hex_map
            .get_tile(*next_coord)
            .and_then(|t| crate::map::infrastructure::railroad_cost(t.terrain(), cfg))
            .unwrap_or_else(|| Money::dollars(cfg.railroad_cost_grassland))
    } else {
        Money::dollars(cfg.depot_cost)
    };
    nation.economy.treasury.checked_sub(next_cost)?;

    // Priority = normalised net_score, clamped to a positive floor so the
    // AI always invests in infrastructure when a candidate exists — even if
    // the cost/benefit doesn't "pay back" strictly within the horizon.
    // (Depots last the whole game; any coverage is a durable asset.)
    let normalised = (plan.net_score / 10.0).max(10.0);
    let score = normalised * weights.economy_weight;

    Some(ScoredAction {
        category: SpendingCategory::Infrastructure,
        score,
        cost: next_cost,
    })
}

fn score_consulate(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;
    let cfg = &game.game_data.game_config;

    // Priority-minor short-circuit: if any of our priority targets still lacks
    // a consulate with us, the score is huge — locking in early-game trade
    // partners is the most important diplomacy goal until it's done.
    let priority_unsecured = nation
        .diplomacy.ai_priority_state
        .priority_minor_targets
        .iter()
        .filter(|mn_id| {
            game.get_nation(**mn_id)
                .is_some_and(|n| !n.province_ids.is_empty())
        })
        .any(|mn_id| {
            game.world.diplomacy
                .get_relation(nation_id, *mn_id)
                .is_none_or(|r| !r.has_consulate)
        });
    if priority_unsecured {
        return Some(ScoredAction {
            category: SpendingCategory::Consulate,
            score: cfg.priority_minor_target_score * weights.diplomacy_weight,
            cost: Money::dollars(cfg.consulate_cost),
        });
    }

    // Count minor nations without consulates that have tradeable resources
    let mut available = 0u32;
    let mut trade_potential = 0u32;
    let mut existing_consulates = 0u32;

    for n in &game.world.nations {
        if n.is_great_power() || n.province_ids.is_empty() {
            continue;
        }
        if let Some(rel) = game.world.diplomacy.get_relation(nation_id, n.id)
            && rel.has_consulate
        {
            existing_consulates += 1;
            continue;
        }
        // Count tradeable resource tiles
        let potential: u32 = game
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
        if potential > 0 {
            available += 1;
            trade_potential += potential;
        }
    }

    if available == 0 {
        return None;
    }

    let target = cfg.ai_consulate_target;

    let raw = if existing_consulates < target {
        let deficit = target - existing_consulates;
        deficit as f64 * cfg.ai_consulate_priority_score + trade_potential as f64 / 10.0
    } else {
        available as f64 * cfg.ai_consulate_beyond_target_score + trade_potential as f64 / 10.0
            - (existing_consulates - target) as f64 * cfg.ai_consulate_beyond_target_decay
    };
    let score = raw.max(0.0) * weights.diplomacy_weight;

    Some(ScoredAction {
        category: SpendingCategory::Consulate,
        score,
        cost: Money::dollars(cfg.consulate_cost),
    })
}

fn score_embassy(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;
    let cfg = &game.game_data.game_config;

    // Priority-minor short-circuit: any priority target with consulate but
    // no embassy yet scores at 1000. Embassy is the full-trust upgrade that
    // secures the target; wanted fast for all personalities.
    let priority_needs_embassy = nation
        .diplomacy.ai_priority_state
        .priority_minor_targets
        .iter()
        .filter(|mn_id| {
            game.get_nation(**mn_id)
                .is_some_and(|n| !n.province_ids.is_empty())
        })
        .any(|mn_id| {
            game.world.diplomacy
                .get_relation(nation_id, *mn_id)
                .is_some_and(|r| r.has_consulate && !r.has_embassy)
        });
    if priority_needs_embassy {
        return Some(ScoredAction {
            category: SpendingCategory::Embassy,
            score: cfg.priority_minor_target_score * weights.diplomacy_weight,
            cost: Money::dollars(cfg.embassy_cost),
        });
    }

    // Count minor nations with consulate but no embassy
    let mut upgradeable = 0u32;
    for n in &game.world.nations {
        if n.is_great_power() || n.province_ids.is_empty() {
            continue;
        }
        if let Some(rel) = game.world.diplomacy.get_relation(nation_id, n.id)
            && rel.has_consulate
            && !rel.has_embassy
        {
            upgradeable += 1;
        }
    }

    if upgradeable == 0 {
        return None;
    }

    // Election proximity bonus
    let year = game.turn.year();
    let turns_to_election = (10 - (year % 10)) * 4; // rough quarters to next decade
    let election_bonus = if turns_to_election <= 16 { 20.0 } else { 3.0 };

    let raw = upgradeable as f64 * 4.0 + election_bonus;
    let score = raw * weights.diplomacy_weight;

    Some(ScoredAction {
        category: SpendingCategory::Embassy,
        score,
        cost: Money::dollars(cfg.embassy_cost),
    })
}

fn score_civilian(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;

    // Civilian construction requires an expert worker
    if game.game_data.game_config.civilian_costs_expert && nation.economy.labor.expert == 0 {
        return None;
    }

    // Count improvable tiles across owned provinces
    let mut improvable_tiles = 0u32;
    for &pid in &nation.province_ids {
        if let Some(province) = game.get_province(pid) {
            for &coord in &province.tiles {
                if let Some(tile) = game.world.hex_map.get_tile(coord) {
                    let max_level = tile
                        .resource_deposit()
                        .map(|r| r.max_improvement_level())
                        .unwrap_or(0);
                    if max_level > 0 && tile.improvement_level() < max_level {
                        improvable_tiles += 1;
                    }
                }
            }
        }
    }

    let civilian_count = nation
        .military.civilians
        .iter()
        .filter(|c| c.civilian_type != CivilianType::Engineer)
        .count();
    let idle_civilians = nation
        .military.civilians
        .iter()
        .filter(|c| {
            c.civilian_type != CivilianType::Engineer && !c.working && c.turns_remaining == 0
        })
        .count();

    // Continuous saturation formula (scales with empire size). Each existing
    // improver "covers" target_tiles_per_worker improvable tiles; each unmet
    // tile beyond that capacity adds coverage_per_unmet to the score.
    let cfg = &game.game_data.game_config;
    let target_ratio = cfg.civilian_target_tiles_per_worker as f64;
    let unmet = (improvable_tiles as f64 - civilian_count as f64 * target_ratio).max(0.0);
    let bootstrap = if civilian_count == 0 && improvable_tiles > 0 {
        cfg.civilian_hire_bootstrap
    } else {
        0.0
    };
    let coverage = unmet * cfg.civilian_coverage_per_unmet + bootstrap;
    let idle_penalty = idle_civilians as f64 * cfg.civilian_idle_penalty;

    let raw = (coverage - idle_penalty).max(0.0);
    let score = raw * weights.economy_weight;

    // Cost of cheapest improver civilian (Farmer: $100, from Lua).
    let cost = Money::dollars(cfg.farmer_cost);

    if score > 0.0 {
        Some(ScoredAction {
            category: SpendingCategory::HireImprover,
            score,
            cost,
        })
    } else {
        None
    }
}

/// Score hiring an additional Engineer civilian.
///
/// Engineers drive infrastructure throughput. Score reflects how much rail/depot
/// work is currently planned vs how many engineers we already have.
/// Coefficients live in Lua (`engineer_hire_*` config fields).
fn score_hire_engineer(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
    plan: Option<&super::economy::DepotPlan>,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;
    let cfg = &game.game_data.game_config;

    if cfg.civilian_costs_expert && nation.economy.labor.expert == 0 {
        return None;
    }

    // Cap engineer count to avoid unbounded hiring.
    let engineer_count = nation
        .military.civilians
        .iter()
        .filter(|c| c.civilian_type == CivilianType::Engineer)
        .count() as u32;
    if engineer_count >= cfg.engineer_hire_max {
        return None;
    }

    // Need a plan to justify hiring more engineers (no work → no need).
    let plan = plan?;
    let path_len = plan.path.len() as f64;

    let raw = cfg.engineer_hire_base as f64 + path_len * cfg.engineer_hire_path_coeff as f64;
    let raw = raw.min(cfg.engineer_hire_cap as f64);
    let score = raw * weights.economy_weight;

    let cost = Money::dollars(cfg.engineer_cost);
    nation.economy.treasury.checked_sub(cost)?;

    Some(ScoredAction {
        category: SpendingCategory::HireEngineer,
        score,
        cost,
    })
}

/// Card #112: score a Warship build. Naval expansion now flows through the
/// same scoring rotation as army expansion. Base score rises with:
///   - being at war (more urgent than peacetime)
///   - being outmatched at sea vs. any known enemy
///   - peacetime baseline (non-zero so the backlog-bonus eventually pushes
///     navy above idle alternatives)
///
/// Gates:
///   - AI must own at least one coastal province (can't build ships inland).
///   - Materials for one Frigate must be on hand (fabric + lumber + arms,
///     with steel→arms conversion allowed). Otherwise the score is zero so
///     the rotation picks a different category while materials accumulate.
fn score_warship(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;

    // Must have a coastal province — landlocked powers can't sail.
    let has_coast = nation
        .province_ids
        .iter()
        .any(|&pid| game.get_province(pid).is_some_and(|p| p.coastal));
    if !has_coast {
        return None;
    }

    // Materials gate: no point scoring this if we can't afford the build.
    if !super::naval::can_build_warship(game, nation_id) {
        return None;
    }

    let naval_cfg = &game.game_data.game_config;
    let naval_base = naval_cfg.spending_naval_base;
    let naval_war_bonus = naval_cfg.spending_naval_war_bonus;
    let naval_gap_coeff = naval_cfg.spending_naval_gap_coeff;

    // Peacetime baseline so backlog eventually fires a warship even in calm.
    let mut raw: f64 = naval_base;

    // At-war bonus, per enemy we're facing.
    let mut max_enemy_naval_fp: u32 = 0;
    let mut any_at_war = false;
    for other in &game.world.nations {
        if other.id == nation_id {
            continue;
        }
        let at_war = game
            .world.diplomacy
            .get_relation(nation_id, other.id)
            .is_some_and(|r| r.at_war);
        if at_war {
            any_at_war = true;
            max_enemy_naval_fp = max_enemy_naval_fp.max(other.total_naval_firepower());
        }
    }
    if any_at_war {
        raw += naval_war_bonus;
    }

    // Outmatched at sea: strong driver.
    let our_naval_fp = nation.total_naval_firepower();
    if max_enemy_naval_fp > our_naval_fp {
        raw += (max_enemy_naval_fp - our_naval_fp) as f64 * naval_gap_coeff;
    }

    // Scale by the Lua naval weight if set, otherwise use the military weight.
    let personality = get_personality(game, nation_id);
    let naval_weight = {
        #[cfg(feature = "lua")]
        {
            game.game_data
                .lua_engine
                .as_ref()
                .and_then(|e| super::lua_bridge::lua_get_config(e, personality))
                .and_then(|c| c.spending_naval_weight)
                .unwrap_or(weights.military_weight)
        }
        #[cfg(not(feature = "lua"))]
        {
            let _ = personality;
            weights.military_weight
        }
    };
    let score = raw * naval_weight;

    // Warships cost materials, not treasury. Use zero so the treasury
    // gate in the scoring loop doesn't incorrectly block the build.
    // The real affordability gate is can_build_warship above.
    let cost = Money::dollars(0);

    if score > 0.0 {
        Some(ScoredAction {
            category: SpendingCategory::Warship,
            score,
            cost,
        })
    } else {
        None
    }
}

// ── Execution functions ──────────────────────────────────────────

fn execute_with_plan(
    game: &mut GameState,
    nation_id: NationId,
    category: SpendingCategory,
    actions: &mut Vec<super::AiAction>,
    _connected: &HashSet<ProvinceId>,
    plan: Option<&super::economy::DepotPlan>,
) {
    match category {
        SpendingCategory::Military => execute_military(game, nation_id, actions),
        SpendingCategory::Infrastructure => execute_infrastructure(game, nation_id, plan),
        SpendingCategory::Consulate => execute_consulate(game, nation_id),
        SpendingCategory::Embassy => execute_embassy(game, nation_id),
        SpendingCategory::HireEngineer => execute_hire_engineer(game, nation_id),
        SpendingCategory::HireImprover => execute_hire_improver(game, nation_id),
        SpendingCategory::Warship => execute_warship(game, nation_id, actions),
    }
}

fn execute_hire_engineer(game: &mut GameState, nation_id: NationId) {
    let cfg = game.game_data.game_config.clone();
    let cost = Money::dollars(cfg.engineer_cost);
    {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };
        if cfg.civilian_costs_expert && nation.economy.labor.expert == 0 {
            return;
        }
        if nation.economy.treasury.checked_sub(cost).is_none() {
            return;
        }
    }
    // Allocate ID before taking mutable borrow of nation.
    let civ_id = game.alloc_unit_id();
    if let Some(nation) = game.get_nation_mut(nation_id) {
        nation.economy.treasury -= cost;
        if cfg.civilian_costs_expert {
            nation.economy.labor.expert -= 1;
        }
        nation.military.civilians.push(Civilian::new(civ_id, CivilianType::Engineer, nation_id));
    }
    game.transient.pending_ai_cash_spending.push((
        nation_id,
        crate::economy::ledger::CashSink::AiCivilianBuild,
        cost,
        None,
    ));
}

fn execute_military(game: &mut GameState, nation_id: NationId, actions: &mut Vec<super::AiAction>) {
    let turn_number = game.turn.0;
    let (capital, nation_name, unit_type, cost, can_afford) = {
        let nation = match game.get_nation_mut(nation_id) {
            Some(n) => n,
            None => return,
        };

        let capital = nation.capital_province_id;
        let nation_name = nation.name.clone();
        let variety_seed = (turn_number as usize).wrapping_mul(nation_id.0 as usize + 7);
        // Field army count — ignore garrison militia so early-game recruit
        // choices aren't skewed by the always-present home garrison.
        let army_count = nation.field_army_count();

        // Pick unit type based on army composition and variety
        let unit_type = if army_count < 3 {
            // Early game: mostly regulars
            let options = [
                ArmyUnitType::Regulars,
                ArmyUnitType::Regulars,
                ArmyUnitType::Grenadiers,
            ];
            options[variety_seed % options.len()]
        } else if army_count < 8 {
            // Mid game: mix
            let options = [
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
                ArmyUnitType::Grenadiers,
            ];
            options[variety_seed % options.len()]
        } else {
            // Late game: artillery focus
            let options = [
                ArmyUnitType::LightArtillery,
                ArmyUnitType::Grenadiers,
                ArmyUnitType::LightArtillery,
            ];
            options[variety_seed % options.len()]
        };

        let cost = match unit_type {
            ArmyUnitType::LightArtillery => Money::dollars(2000),
            ArmyUnitType::Grenadiers => Money::dollars(1000),
            _ => Money::dollars(500),
        };

        let can_afford = nation.economy.treasury >= cost;
        (capital, nation_name, unit_type, cost, can_afford)
    };
    if can_afford {
        let unit_id = game.alloc_unit_id();
        if let Some(nation) = game.get_nation_mut(nation_id) {
            nation.economy.treasury -= cost;
            let unit = ArmyUnit::new(unit_id, unit_type, nation_id, capital);
            nation.military.army.push(unit);
            actions.push(super::AiAction {
                text: format!("{} has been expanding its military forces", nation_name),
                reason: "Spending system selected military category for expansion".to_string(),
                is_non_action: false,
                nation_id,
            });
        }
        game.transient.pending_ai_cash_spending.push((
            nation_id,
            crate::economy::ledger::CashSink::AiArmyBuild,
            cost,
            None,
        ));
    }
}

/// Card #112: Warship executor — delegates to the shared single-ship
/// builder in the naval module. The scored-spending loop resets this
/// category's backlog counter whenever it fires, so successive Warship
/// picks naturally interleave with Military / Infrastructure / etc.
fn execute_warship(game: &mut GameState, nation_id: NationId, actions: &mut Vec<super::AiAction>) {
    let nation_name = game
        .get_nation(nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    if super::naval::build_one_warship(game, nation_id) {
        actions.push(super::AiAction {
            text: format!("{} has commissioned a new warship", nation_name),
            reason: "Spending system selected naval expansion".to_string(),
            is_non_action: false,
            nation_id,
        });
    }
}

/// AI infrastructure step: drive (or hire) an Engineer civilian along the
/// plan returned by `plan_next_depot`.
///
/// - No Engineer → hire one and return (next turn will deploy).
/// - Engineer working → let the turn processor tick it; do nothing.
/// - Engineer stranded on an unowned hex (province lost) → recall to capital.
/// - Engineer idle:
///     * Let the planner pick the best (candidate, path). If no plan, return.
///     * If path is empty, the candidate is already reachable — build the depot.
///     * Otherwise lay one rail hex along the path, adjacent to the engineer's
///       current position (or the first hex on the path if undeployed).
fn execute_infrastructure(
    game: &mut GameState,
    nation_id: NationId,
    plan: Option<&super::economy::DepotPlan>,
) {
    let cfg = game.game_data.game_config.clone();

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    let capital_pid = nation.capital_province_id;
    let capital_tile = match game.get_province(capital_pid) {
        Some(p) => p.capital_tile,
        None => return,
    };

    // Find (or hire) an Engineer.
    let engineer_idx = nation
        .military.civilians
        .iter()
        .position(|c| c.civilian_type == CivilianType::Engineer);

    let engineer_idx = match engineer_idx {
        Some(idx) => idx,
        None => {
            let civilian_costs_expert = cfg.civilian_costs_expert;
            let cost = CivilianType::Engineer.creation_cost(&cfg);
            {
                let nation = match game.get_nation(nation_id) {
                    Some(n) => n,
                    None => return,
                };
                if civilian_costs_expert && nation.economy.labor.expert == 0 {
                    return;
                }
                if nation.economy.treasury.checked_sub(cost).is_none() {
                    return;
                }
            }
            // Allocate ID before the mutable borrow.
            let civ_id = game.alloc_unit_id();
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.economy.treasury -= cost;
                if civilian_costs_expert {
                    nation.economy.labor.expert -= 1;
                }
                nation.military.civilians.push(Civilian::new(civ_id, CivilianType::Engineer, nation_id));
            }
            game.transient.pending_ai_cash_spending.push((
                nation_id,
                crate::economy::ledger::CashSink::AiCivilianBuild,
                cost,
                None,
            ));
            return;
        }
    };

    // Province-loss resilience: if the engineer is sitting on a hex the nation
    // no longer owns, clear its position (will redeploy this turn).
    {
        let owned_hexes: HashSet<HexCoord> = game
            .world.provinces
            .iter()
            .filter(|p| p.owner == nation_id)
            .flat_map(|p| p.tiles.iter().copied())
            .collect();
        let stranded = {
            let nation = match game.get_nation(nation_id) {
                Some(n) => n,
                None => return,
            };
            let civ = &nation.military.civilians[engineer_idx];
            civ.position.is_some_and(|p| !owned_hexes.contains(&p))
        };
        if stranded {
            let (civ_id, old_pos) = {
                let civ = &game.get_nation(nation_id).unwrap().military.civilians[engineer_idx];
                (civ.id, civ.position)
            };
            if let Some(pos) = old_pos
                && let Some(tile) = game.world.hex_map.get_tile_mut(pos)
                && tile.assigned_civilian == Some(civ_id)
            {
                tile.assigned_civilian = None;
            }
            if let Some(nation) = game.get_nation_mut(nation_id) {
                let civ = &mut nation.military.civilians[engineer_idx];
                civ.position = None;
                civ.working = false;
                civ.turns_remaining = 0;
                civ.build_task = None;
            }
        }
    }

    // If the engineer is still working, let the turn processor finish it.
    if let Some(nation) = game.get_nation(nation_id)
        && nation.military.civilians[engineer_idx].working
    {
        return;
    }

    // Use the cached plan from the spending loop. If `None`, the planner
    // decided there was nothing worth building.
    let plan = match plan {
        Some(p) => p,
        None => return,
    };

    // Path empty → candidate is already reachable, just build the depot there.
    if plan.path.is_empty() {
        start_engineer_task(
            game,
            nation_id,
            engineer_idx,
            plan.candidate,
            BuildTask::Depot,
            &cfg,
        );
        return;
    }

    // Where is the engineer? Default to capital if undeployed.
    let engineer_pos = game
        .get_nation(nation_id)
        .and_then(|n| n.military.civilians[engineer_idx].position)
        .unwrap_or(capital_tile);

    // Pick the next unbuilt hex, preferring one adjacent to the engineer.
    let next_hex = plan
        .path
        .iter()
        .find(|c| engineer_pos.neighbors().contains(c))
        .copied()
        .or_else(|| plan.path.first().copied());
    let next_hex = match next_hex {
        Some(c) => c,
        None => return,
    };

    // If the next-hex choice already has a rail, advance one further along path.
    let already_has_rail = game
        .world.hex_map
        .get_tile(next_hex)
        .is_some_and(|t| t.infrastructure.has_railroad);
    let build_coord = if already_has_rail {
        plan.path
            .iter()
            .skip_while(|c| **c != next_hex)
            .nth(1)
            .copied()
            .unwrap_or(next_hex)
    } else {
        next_hex
    };

    // Safety guard: if build_coord already has rail/depot (e.g. built by a
    // second engineer this same turn), the path is effectively exhausted —
    // place the depot rather than spinning on already-built tiles.
    let build_coord_already_built = game
        .world.hex_map
        .get_tile(build_coord)
        .is_some_and(|t| t.infrastructure.has_railroad || t.infrastructure.has_depot);

    if build_coord_already_built {
        start_engineer_task(
            game,
            nation_id,
            engineer_idx,
            plan.candidate,
            BuildTask::Depot,
            &cfg,
        );
    } else {
        start_engineer_task(
            game,
            nation_id,
            engineer_idx,
            build_coord,
            BuildTask::Railroad,
            &cfg,
        );
    }
}

/// Deploy the engineer to `coord` and begin `task`. Clears any prior tile
/// assignment and sets `assigned_civilian` on the target tile.
fn start_engineer_task(
    game: &mut GameState,
    nation_id: NationId,
    engineer_idx: usize,
    coord: HexCoord,
    task: BuildTask,
    cfg: &crate::data::GameConfig,
) {
    // Clear old tile's assigned_civilian (if any).
    let (civ_id, old_pos) = match game.get_nation(nation_id) {
        Some(n) => (
            n.military.civilians[engineer_idx].id,
            n.military.civilians[engineer_idx].position,
        ),
        None => return,
    };
    if let Some(old) = old_pos
        && let Some(tile) = game.world.hex_map.get_tile_mut(old)
        && tile.assigned_civilian == Some(civ_id)
    {
        tile.assigned_civilian = None;
    }
    // Target hex must be owned and empty of another civilian.
    let owned = game
        .world.provinces
        .iter()
        .filter(|p| p.owner == nation_id)
        .any(|p| p.tiles.contains(&coord));
    if !owned {
        return;
    }
    let target_ok = game
        .world.hex_map
        .get_tile(coord)
        .is_some_and(|t| t.assigned_civilian.is_none() || t.assigned_civilian == Some(civ_id));
    if !target_ok {
        return;
    }
    // Only start a task the nation can afford at completion time. Treasury is
    // debited when the build finishes, so we guard at order time.
    let cost = match task {
        BuildTask::Railroad => {
            let terrain = match game.world.hex_map.get_tile(coord) {
                Some(t) => t.terrain(),
                None => return,
            };
            match crate::map::infrastructure::railroad_cost(terrain, cfg) {
                Some(m) => m,
                None => return, // e.g. sea — should not happen given earlier checks
            }
        }
        BuildTask::Depot => Money::dollars(cfg.depot_cost),
        BuildTask::Port => Money::dollars(cfg.port_cost),
    };
    let treasury = game
        .get_nation(nation_id)
        .map(|n| n.economy.treasury)
        .unwrap_or(Money::ZERO);
    if treasury.checked_sub(cost).is_none() {
        return;
    }
    if let Some(tile) = game.world.hex_map.get_tile_mut(coord) {
        tile.assigned_civilian = Some(civ_id);
    }
    if let Some(nation) = game.get_nation_mut(nation_id) {
        let civ = &mut nation.military.civilians[engineer_idx];
        civ.deploy(coord);
        civ.start_build(task, cfg);
    }
}

fn execute_consulate(game: &mut GameState, nation_id: NationId) {
    let cost = Money::dollars(game.game_data.game_config.consulate_cost);

    // Prefer unsecured priority targets first. Only fall back to best-trade
    // match among non-priority minors once all priority targets are secured.
    let priority_pick: Option<NationId> = game.get_nation(nation_id).and_then(|n| {
        n.diplomacy.ai_priority_state
            .priority_minor_targets
            .iter()
            .find(|mn_id| {
                game.get_nation(**mn_id)
                    .is_some_and(|m| !m.province_ids.is_empty() && !m.diplomacy.is_in_anarchy)
                    && game
                        .world.diplomacy
                        .get_relation(nation_id, **mn_id)
                        .is_none_or(|r| !r.has_consulate)
            })
            .copied()
    });

    let best_mn = priority_pick.or_else(|| {
        let mut best: Option<NationId> = None;
        let mut best_potential = 0u32;
        for n in &game.world.nations {
            if n.is_great_power() || n.province_ids.is_empty() || n.diplomacy.is_in_anarchy {
                continue;
            }
            if game
                .world.diplomacy
                .get_relation(nation_id, n.id)
                .is_some_and(|r| r.has_consulate)
            {
                continue;
            }
            let potential: u32 = game
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
            if potential > best_potential {
                best_potential = potential;
                best = Some(n.id);
            }
        }
        best
    });

    if let Some(mn_id) = best_mn
        && game.world.diplomacy.build_consulate(nation_id, mn_id).is_ok()
    {
        if let Some(nation) = game.get_nation_mut(nation_id) {
            nation.economy.treasury -= cost;
        }
        game.transient.pending_ai_cash_spending.push((
            nation_id,
            crate::economy::ledger::CashSink::AiDiplomacyConsulate,
            cost,
            Some(mn_id),
        ));
    }
}

fn execute_embassy(game: &mut GameState, nation_id: NationId) {
    let cost = Money::dollars(game.game_data.game_config.embassy_cost);

    // Prefer priority targets with consulate-but-no-embassy; fall back to
    // best-relation non-priority match.
    let priority_pick: Option<NationId> = game.get_nation(nation_id).and_then(|n| {
        n.diplomacy.ai_priority_state
            .priority_minor_targets
            .iter()
            .find(|mn_id| {
                game.get_nation(**mn_id)
                    .is_some_and(|m| !m.province_ids.is_empty() && !m.diplomacy.is_in_anarchy)
                    && game
                        .world.diplomacy
                        .get_relation(nation_id, **mn_id)
                        .is_some_and(|r| r.has_consulate && !r.has_embassy)
            })
            .copied()
    });

    let best_mn = priority_pick.or_else(|| {
        let mut best: Option<NationId> = None;
        let mut best_relation = i32::MIN;
        for n in &game.world.nations {
            if n.is_great_power() || n.province_ids.is_empty() || n.diplomacy.is_in_anarchy {
                continue;
            }
            if let Some(rel) = game.world.diplomacy.get_relation(nation_id, n.id)
                && rel.has_consulate
                && !rel.has_embassy
                && rel.score > best_relation
            {
                best_relation = rel.score;
                best = Some(n.id);
            }
        }
        best
    });

    if let Some(mn_id) = best_mn
        && game.world.diplomacy.build_embassy(nation_id, mn_id).is_ok()
    {
        if let Some(nation) = game.get_nation_mut(nation_id) {
            nation.economy.treasury -= cost;
        }
        game.transient.pending_ai_cash_spending.push((
            nation_id,
            crate::economy::ledger::CashSink::AiDiplomacyEmbassy,
            cost,
            Some(mn_id),
        ));
    }
}

fn execute_hire_improver(game: &mut GameState, nation_id: NationId) {
    let civilian_costs_expert = game.game_data.game_config.civilian_costs_expert;
    let cfg = game.game_data.game_config.clone();

    // Read-only phase: determine if we can and should hire.
    let (civ_type, cost) = {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };
        if civilian_costs_expert && nation.economy.labor.expert == 0 {
            return;
        }
        let improver_count = nation
            .military.civilians
            .iter()
            .filter(|c| c.civilian_type != CivilianType::Engineer)
            .count();
        let civ_type = if improver_count < 2 {
            CivilianType::Farmer
        } else {
            let has_forester = nation
                .military.civilians
                .iter()
                .any(|c| c.civilian_type == CivilianType::Forester);
            if has_forester {
                CivilianType::Miner
            } else {
                CivilianType::Forester
            }
        };
        let already_idle_unplaced = nation
            .military.civilians
            .iter()
            .any(|c| c.civilian_type == civ_type && c.position.is_none());
        if already_idle_unplaced {
            return;
        }
        let cost = civ_type.creation_cost(&cfg);
        if nation.economy.treasury.checked_sub(cost).is_none() {
            return;
        }
        (civ_type, cost)
    };
    // Allocate ID before the mutable borrow.
    let civ_id = game.alloc_unit_id();
    if let Some(nation) = game.get_nation_mut(nation_id) {
        nation.economy.treasury -= cost;
        if civilian_costs_expert {
            nation.economy.labor.expert -= 1;
        }
        nation.military.civilians.push(Civilian::new(civ_id, civ_type, nation_id));
    }
    game.transient.pending_ai_cash_spending.push((
        nation_id,
        crate::economy::ledger::CashSink::AiCivilianBuild,
        cost,
        None,
    ));
}

/// Drop annexed/destroyed priority targets and pick replacements so the
/// nation always has up to its personality-count of live targets. Called
/// once per turn at the top of the spending loop.
fn refresh_priority_targets(game: &mut GameState, nation_id: NationId, personality: AiPersonality) {
    let cfg = game.game_data.game_config.clone();
    let target_count = priority_target_count(&cfg, personality);

    let mut kept: Vec<NationId> = match game.get_nation(nation_id) {
        Some(n) => n
            .diplomacy.ai_priority_state
            .priority_minor_targets
            .iter()
            .filter(|mn_id| {
                game.get_nation(**mn_id)
                    .is_some_and(|m| !m.province_ids.is_empty())
            })
            .copied()
            .collect(),
        None => return,
    };

    if kept.len() < target_count {
        let fresh = pick_priority_minor_targets(game, nation_id, target_count - kept.len(), &kept);
        kept.extend(fresh);
    }

    if let Some(nation) = game.get_nation_mut(nation_id) {
        nation.diplomacy.ai_priority_state.priority_minor_targets = kept;
    }
}

// ── Priority-minor target selection ──────────────────────────────

/// Count of priority minor-nation targets for a personality, from Lua config.
pub fn priority_target_count(cfg: &crate::data::GameConfig, personality: AiPersonality) -> usize {
    let n = match personality {
        AiPersonality::Aggressive => cfg.priority_minor_targets_aggressive,
        AiPersonality::Balanced => cfg.priority_minor_targets_balanced,
        AiPersonality::Economic => cfg.priority_minor_targets_economic,
        AiPersonality::Diplomatic => cfg.priority_minor_targets_diplomatic,
    };
    n as usize
}

/// Pick up to `n` minor-nation targets whose visible exports best complement
/// the Great Power's resource demand. Used at game init to seed
/// `priority_minor_targets`, and again to replace a target that was lost.
///
/// Excludes minors already in `exclude` (used to avoid re-picking the same
/// target when filling in a replacement after one is annexed/destroyed).
pub fn pick_priority_minor_targets(
    game: &GameState,
    gp_id: NationId,
    n: usize,
    exclude: &[NationId],
) -> Vec<NationId> {
    let nation = match game.get_nation(gp_id) {
        Some(n) => n,
        None => return Vec::new(),
    };
    let demand = super::economy::compute_resource_demand(nation, game, &game.game_data.game_config);

    let mut scored: Vec<(NationId, f64)> = Vec::new();
    for minor in &game.world.nations {
        if minor.is_great_power() || minor.province_ids.is_empty() {
            continue;
        }
        if exclude.contains(&minor.id) {
            continue;
        }
        let score: f64 = game
            .world.provinces
            .iter()
            .filter(|p| p.owner == minor.id)
            .flat_map(|p| &p.tiles)
            .filter_map(|c| game.world.hex_map.get_tile(*c).and_then(|t| t.calculate_yield()))
            .filter(|y| y.resource.is_tradeable())
            .map(|y| {
                let w = demand.get(&y.resource).copied().unwrap_or(0.0);
                y.quantity as f64 * w
            })
            .sum();
        if score > 0.0 {
            scored.push((minor.id, score));
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(n).map(|(id, _)| id).collect()
}

// ── Config loading ───────────────────────────────────────────────

#[allow(unused_mut, unused_variables)] // mut + game used only with cfg(feature = "lua")
fn load_weights(game: &GameState, personality: AiPersonality) -> SpendingWeights {
    let defaults = PersonalityConfig::for_personality(personality);
    let mut w = SpendingWeights::from_config(&defaults);

    #[cfg(feature = "lua")]
    if let Some(engine) = &game.game_data.lua_engine
        && let Some(cfg) = super::lua_bridge::lua_get_config(engine, personality)
    {
        w.military_weight = lua_or(cfg.spending_military_weight, defaults.spending_military_weight);
        w.economy_weight = lua_or(cfg.spending_economy_weight, defaults.spending_economy_weight);
        w.diplomacy_weight =
            lua_or(cfg.spending_diplomacy_weight, defaults.spending_diplomacy_weight);
        w.reserve = lua_or(cfg.treasury_reserve.map(Money::dollars), defaults.spending_reserve);
        w.min_threshold = lua_or(cfg.min_score_threshold, defaults.spending_min_threshold);
    }

    // Suppress unused variable warning when lua feature is off
    let _ = game;
    w
}
