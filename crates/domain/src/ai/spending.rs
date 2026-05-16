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

use super::common::{AiPersonality, PersonalityConfig, get_personality, lua_or};

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
    #[allow(dead_code)] // used in #[cfg(test)] score_consulate / score_embassy
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
    let mut depot_plans: Vec<super::economy::DepotPlan> =
        refresh_infra_commitments(game, nation_id, weights.reserve);
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
            &depot_plans,
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
        if let Some(mut opt) = score_hire_engineer(game, nation_id, &weights, &depot_plans) {
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
        // Card #217 follow-up: when the engineer has finished a rail run and
        // the only remaining infra step is the depot ($2000), bleeding $100
        // into civilian hires keeps treasury permanently below the depot
        // threshold. Suppress HireImprover while we're saving up so cash
        // accumulates instead of leaking to improvers that can't yet harvest
        // anything (the depot being unbuilt is exactly why their tiles aren't
        // collectable). Only kicks in when an idle engineer is on hand —
        // otherwise the depot can't be built anyway and we shouldn't starve
        // other categories.
        let saving_for_depot = depot_plans.iter().any(|p| p.path.is_empty()) && {
            let nation = game.get_nation(nation_id);
            let any_idle_engineer = nation.is_some_and(|n| {
                n.military.civilians.iter().any(|c| {
                    c.civilian_type == CivilianType::Engineer
                        && !c.working
                        && c.turns_remaining == 0
                })
            });
            let depot_total =
                weights.reserve + Money::dollars(game.game_data.game_config.depot_cost);
            let cant_afford_yet = nation
                .map(|n| n.economy.treasury < depot_total)
                .unwrap_or(false);
            any_idle_engineer && cant_afford_yet
        };
        if !saving_for_depot && let Some(mut opt) = score_civilian(game, nation_id, &weights) {
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
                // detect whether the execute actually started a *new* build.
                // Count working engineers (not just any-working) so multi-
                // engineer parallelism is detected correctly: a second
                // engineer transitioning to working still grows the count.
                let engineer_working_count_before = if cat == SpendingCategory::Infrastructure {
                    game.get_nation(nation_id).map(|n| {
                        n.military
                            .civilians
                            .iter()
                            .filter(|c| c.civilian_type == CivilianType::Engineer && c.working)
                            .count()
                    })
                } else {
                    None
                };
                let treasury_before = game.get_nation(nation_id).map(|n| n.economy.treasury);
                execute_with_plan(game, nation_id, cat, actions, &connected, &depot_plans);
                let treasury_after = game.get_nation(nation_id).map(|n| n.economy.treasury);
                let treasury_changed = treasury_before != treasury_after;
                // For Military, only count as invested when a unit was actually
                // purchased (treasury changed). Skipped executions (no arms/oil/
                // labor) must not reset the backlog counter.
                if (cat != SpendingCategory::Military || treasury_changed)
                    && let Some(nation) = game.get_nation_mut(nation_id)
                {
                    nation
                        .diplomacy
                        .ai_priority_state
                        .last_invest_turn
                        .insert(cat, current_turn);
                }
                // Infrastructure mutates the rail network and potentially the
                // engineer's state, so recompute connectivity + plan cache.
                if cat == SpendingCategory::Infrastructure {
                    connected = connected_provinces(game, nation_id);
                    depot_plans = refresh_infra_commitments(game, nation_id, weights.reserve);
                    // If the engineer did not transition from idle to working
                    // AND the treasury wasn't spent (engineer hire), the
                    // execute was a silent no-op: the next hex is blocked,
                    // unowned, or unaffordable. Skip infra for the rest of
                    // this turn to prevent the loop from spinning on it.
                    let engineer_working_count_after = game
                        .get_nation(nation_id)
                        .map(|n| {
                            n.military
                                .civilians
                                .iter()
                                .filter(|c| c.civilian_type == CivilianType::Engineer && c.working)
                                .count()
                        })
                        .unwrap_or(0);
                    let engineer_transitioned = engineer_working_count_before
                        .is_some_and(|before| engineer_working_count_after > before);
                    if !engineer_transitioned && !treasury_changed {
                        if game.ai_debug {
                            let name = game
                                .get_nation(nation_id)
                                .map(|n| n.name.as_str())
                                .unwrap_or("?");
                            eprintln!(
                                "[AI:{}:infra] blocked_this_turn engineer_working_before={:?} engineer_working_after={} treasury_changed={} plan_present={}",
                                name,
                                engineer_working_count_before,
                                engineer_working_count_after,
                                treasury_changed,
                                !depot_plans.is_empty()
                            );
                        }
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
#[allow(dead_code)]
fn apply_plan_outcome(
    game: &mut GameState,
    nation_id: NationId,
    outcome: &super::economy::PlanOutcome,
) {
    let current_turn = game.turn.0;
    let ai_debug = game.ai_debug;
    let Some(nation) = game.get_nation_mut(nation_id) else {
        return;
    };
    let nation_name = if ai_debug {
        nation.name.clone()
    } else {
        String::new()
    };
    match outcome {
        super::economy::PlanOutcome::KeepCommitment(_) => {
            // Commitment already set; leave it.
            if ai_debug {
                eprintln!("[AI:{}:infra-plan] commitment unchanged", nation_name);
            }
        }
        super::economy::PlanOutcome::Fresh(Some(plan)) => {
            nation.diplomacy.ai_priority_state.committed_infra_target =
                Some(crate::nation::CommittedInfraTarget {
                    candidate: plan.candidate,
                    origin_capital: plan.origin_capital,
                    turn_committed: current_turn,
                });
            if ai_debug {
                eprintln!(
                    "[AI:{}:infra-plan] commitment set candidate=({}, {}) origin=({}, {}) turn={}",
                    nation_name,
                    plan.candidate.q,
                    plan.candidate.r,
                    plan.origin_capital.q,
                    plan.origin_capital.r,
                    current_turn
                );
            }
        }
        super::economy::PlanOutcome::Fresh(None) => {
            nation.diplomacy.ai_priority_state.committed_infra_target = None;
            if ai_debug {
                eprintln!("[AI:{}:infra-plan] commitment cleared", nation_name);
            }
        }
    }
}

fn desired_infra_commitments(game: &GameState, nation_id: NationId, reserve: Money) -> usize {
    let Some(nation) = game.get_nation(nation_id) else {
        return 0;
    };
    let engineer_count = nation
        .military
        .civilians
        .iter()
        .filter(|c| c.civilian_type == CivilianType::Engineer)
        .count()
        .max(1);
    let treasury_headroom = nation
        .economy
        .treasury
        .checked_sub(reserve)
        .unwrap_or(Money::ZERO)
        .as_dollars()
        .max(0);
    let affordable = ((treasury_headroom / 10_000).max(1)) as usize;
    engineer_count.min(affordable)
}

fn refresh_infra_commitments(
    game: &mut GameState,
    nation_id: NationId,
    reserve: Money,
) -> Vec<super::economy::DepotPlan> {
    let desired = desired_infra_commitments(game, nation_id, reserve);
    if desired == 0 {
        return Vec::new();
    }

    let commitments: Vec<crate::nation::CommittedInfraTarget> = game
        .get_nation(nation_id)
        .map(|nation| {
            nation
                .diplomacy
                .ai_priority_state
                .committed_infra_target
                .iter()
                .cloned()
                .chain(
                    nation
                        .diplomacy
                        .ai_priority_state
                        .additional_committed_infra_targets
                        .iter()
                        .cloned(),
                )
                .collect()
        })
        .unwrap_or_default();

    let mut plans: Vec<super::economy::DepotPlan> = Vec::new();
    let mut kept_commitments: Vec<crate::nation::CommittedInfraTarget> = Vec::new();
    let mut excluded: HashSet<HexCoord> = HashSet::new();

    for commitment in commitments.into_iter().take(desired) {
        let outcome = super::economy::plan_next_depot_excluding(
            game,
            nation_id,
            Some(&commitment),
            &excluded,
        );
        if let Some(plan) = outcome.as_plan() {
            excluded.insert(plan.candidate);
            kept_commitments.push(crate::nation::CommittedInfraTarget {
                candidate: plan.candidate,
                origin_capital: plan.origin_capital,
                turn_committed: commitment.turn_committed,
            });
            plans.push(plan.clone());
        }
    }

    while plans.len() < desired {
        let outcome = super::economy::plan_next_depot_excluding(game, nation_id, None, &excluded);
        let Some(plan) = outcome.as_plan().cloned() else {
            break;
        };
        excluded.insert(plan.candidate);
        kept_commitments.push(crate::nation::CommittedInfraTarget {
            candidate: plan.candidate,
            origin_capital: plan.origin_capital,
            turn_committed: game.turn.0,
        });
        plans.push(plan);
    }

    if let Some(nation) = game.get_nation_mut(nation_id) {
        nation.diplomacy.ai_priority_state.committed_infra_target =
            kept_commitments.first().cloned();
        nation
            .diplomacy
            .ai_priority_state
            .additional_committed_infra_targets = kept_commitments.into_iter().skip(1).collect();
    }

    plans
}

fn remove_infra_commitment(game: &mut GameState, nation_id: NationId, candidate: HexCoord) {
    let Some(nation) = game.get_nation_mut(nation_id) else {
        return;
    };
    if nation
        .diplomacy
        .ai_priority_state
        .committed_infra_target
        .as_ref()
        .is_some_and(|t| t.candidate == candidate)
    {
        let replacement = nation
            .diplomacy
            .ai_priority_state
            .additional_committed_infra_targets
            .first()
            .cloned();
        nation.diplomacy.ai_priority_state.committed_infra_target = replacement;
        if !nation
            .diplomacy
            .ai_priority_state
            .additional_committed_infra_targets
            .is_empty()
        {
            nation
                .diplomacy
                .ai_priority_state
                .additional_committed_infra_targets
                .remove(0);
        }
        return;
    }
    nation
        .diplomacy
        .ai_priority_state
        .additional_committed_infra_targets
        .retain(|t| t.candidate != candidate);
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
    // A category that has never been invested in starts with backlog 0 on
    // the very first scoring pass — otherwise on turn 1 every category
    // gets a free `current_turn` bonus, which on a fresh game biases the AI
    // hard toward Military (high weight × small province count = army spam
    // before any threat exists). The cap still applies for genuinely
    // long-neglected categories deeper in the game.
    let backlog_turns = match nation
        .diplomacy
        .ai_priority_state
        .last_invest_turn
        .get(&category)
    {
        Some(&last) => current_turn.saturating_sub(last),
        None => current_turn.saturating_sub(1).min(cfg.backlog_initial_cap),
    };

    let personality = nation
        .diplomacy
        .ai_personality
        .unwrap_or(AiPersonality::Balanced);
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
        (AiPersonality::Diplomatic, SpendingCategory::HireEngineer) => {
            cfg.backlog_weight_diplomatic_hire_engineer
        }
        (AiPersonality::Diplomatic, SpendingCategory::HireImprover) => {
            cfg.backlog_weight_diplomatic_hire_improver
        }
        // Warship is aliased to Military above — this arm is unreachable,
        // but the exhaustiveness check still wants it covered.
        (_, SpendingCategory::Warship) => cfg.backlog_weight_balanced_military,
        // Consulate/Embassy are no longer scored in the main loop (handled by
        // ai_diplomatic_mop_up instead), so these arms are unreachable.
        (_, SpendingCategory::Consulate) | (_, SpendingCategory::Embassy) => 0,
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
            .world
            .diplomacy
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
        .world
        .nations
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

    // Workforce-capacity gate: don't recruit new army units when the
    // industrial chain is already understaffed — soldiers need workers
    // behind them to keep producing arms, food, clothing.
    if !nation.chain_labor_gate_passes(&game.game_data.game_config) {
        return None;
    }

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
            .world
            .diplomacy
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

    // Threat-gated desired field army. The territorial floor only kicks in
    // when *some rival is actually projecting power that exceeds our own*
    // — provinces are already garrisoned by militia, so the field army is
    // for matching/exceeding rival projection, not for static defense.
    // Without this gate the original formula produced a turn-1 deficit
    // that pushed every AI to spend on armies before any threat existed.
    let rival_pressure = strongest_rival_army as f64 * 0.6;
    let desired = if rival_pressure > army_count {
        (province_count * 1.5).max(rival_pressure)
    } else {
        0.0
    };
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

    // F-004: cost estimate must reflect what `execute_military` will
    // actually spend, not the old $500 Regulars baseline. Use the cheapest
    // currently-unlocked variant in the line-infantry chain (Regulars →
    // RifleInfantry → Infantry) — same role the recruiter most often picks.
    let researched = &nation.researched_techs;
    let cost = ArmyUnitType::Regulars
        .latest_unlocked_in_chain(|tech_name| {
            game.game_data
                .tech_tree
                .all_techs()
                .iter()
                .any(|t| t.name == tech_name && researched.contains(&t.id))
        })
        .stats()
        .cost;

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
    plans: &[super::economy::DepotPlan],
    infra_blocked: bool,
) -> Option<ScoredAction> {
    // Infrastructure already attempted this turn and failed to start a build
    // (e.g., next hex blocked by a civilian, unowned, or unaffordable).
    // Skip for the rest of the turn so the spending loop doesn't spin.
    if infra_blocked {
        return None;
    }
    let nation = game.get_nation(nation_id)?;
    // We can only progress infrastructure if there's at least one idle
    // engineer. With multiple engineers, the others may be busy mid-build
    // — that's fine, the executor will pick an idle one. Returning None
    // only when *no* engineer is idle prevents infinite infra picks while
    // still letting parallelism work when the AI has staff to spare.
    let any_idle_engineer =
        nation.military.civilians.iter().any(|c| {
            c.civilian_type == CivilianType::Engineer && !c.working && c.turns_remaining == 0
        });
    if !any_idle_engineer {
        return None;
    }

    let cfg = &game.game_data.game_config;

    // No depot plan: check whether there's a stranded coastal province that
    // can only be reached by sea. If so, score a port build at that cost.
    if plans.is_empty() {
        super::economy::find_stranded_port_target(game, nation_id)?;
        let port_cost = Money::dollars(cfg.port_cost);
        nation.economy.treasury.checked_sub(port_cost)?;
        let score = 10.0_f64 * weights.economy_weight;
        return Some(ScoredAction {
            category: SpendingCategory::Infrastructure,
            score,
            cost: port_cost,
        });
    }

    // Planner picks the best depot candidate. No plan → no infrastructure need.
    let plan = plans.first()?;

    // Minimum cost the AI can afford right now to keep progressing: the
    // cheapest next hex on the path, or the depot cost if the path is empty.
    let next_cost = if let Some(next_coord) = plan.path.first() {
        game.world
            .hex_map
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
    // Card #217: early-game bias — connecting an L0 tile produces yield
    // immediately, while improving a disconnected tile produces nothing
    // until rail catches up. For the first N turns, lean toward rail.
    let early_bias = if game.turn.0 < cfg.infra_early_game_bias_turns {
        cfg.infra_early_game_bias
    } else {
        1.0
    };
    let score = normalised * weights.economy_weight * early_bias;

    Some(ScoredAction {
        category: SpendingCategory::Infrastructure,
        score,
        cost: next_cost,
    })
}

#[cfg(test)]
fn score_consulate(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;
    let cfg = &game.game_data.game_config;

    // Per-personality hard cap on consulates pursued under normal financial
    // conditions. Reuses `priority_minor_targets_*` (Aggressive 3 / Balanced 4
    // / Economic 4 / Diplomatic 5) so the cap and the early-game priority
    // pick share one knob. The cap is *lifted* once the AI has accumulated
    // serious cash (treasury >= `labor_wealthy_treasury_threshold`), at
    // which point the existing soft-decay branch below resumes governing
    // growth — wealthy AIs can chase more trade partners.
    let personality = nation
        .diplomacy
        .ai_personality
        .unwrap_or(AiPersonality::Balanced);
    let cap = priority_target_count(cfg, personality) as u32;
    let wealthy = nation.economy.treasury.as_dollars() >= cfg.labor_wealthy_treasury_threshold;

    // Pre-count current consulates so the cap can short-circuit before the
    // priority-target boost kicks in — otherwise the priority short-circuit
    // would happily push past the cap chasing late-added priority targets.
    let existing_consulates_pre: u32 = game
        .world
        .nations
        .iter()
        .filter(|n| !n.is_great_power() && !n.province_ids.is_empty())
        .filter(|n| {
            game.world
                .diplomacy
                .get_relation(nation_id, n.id)
                .is_some_and(|r| r.has_consulate)
        })
        .count() as u32;
    if !wealthy && existing_consulates_pre >= cap {
        return None;
    }

    // Priority-minor short-circuit: if any of our priority targets still lacks
    // a consulate with us, the score is huge — locking in early-game trade
    // partners is the most important diplomacy goal until it's done.
    let priority_unsecured = nation
        .diplomacy
        .ai_priority_state
        .priority_minor_targets
        .iter()
        .filter(|mn_id| {
            game.get_nation(**mn_id)
                .is_some_and(|n| !n.province_ids.is_empty())
        })
        .any(|mn_id| {
            game.world
                .diplomacy
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
            .world
            .provinces
            .iter()
            .filter(|p| p.owner == n.id)
            .flat_map(|p| &p.tiles)
            .filter_map(|coord| {
                game.world
                    .hex_map
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

#[cfg(test)]
fn score_embassy(
    game: &GameState,
    nation_id: NationId,
    weights: &SpendingWeights,
) -> Option<ScoredAction> {
    let cfg = &game.game_data.game_config;

    // Card #210: an embassy is only justified once the relationship has
    // warmed up to `ai_embassy_min_relation`. Consulates are cheap and
    // already grant a relationship bonus, so the AI should *let consulates
    // do their job* before spending on the costly upgrade. This rule
    // applies uniformly — priority-minor targets do NOT bypass it. They
    // simply build a consulate first and wait for the score to climb.
    let min_relation = cfg.ai_embassy_min_relation;
    let mut upgradeable = 0u32;
    let mut priority_upgradeable = 0u32;
    let priority_targets = &game
        .get_nation(nation_id)?
        .diplomacy
        .ai_priority_state
        .priority_minor_targets;
    for n in &game.world.nations {
        if n.is_great_power() || n.province_ids.is_empty() {
            continue;
        }
        if let Some(rel) = game.world.diplomacy.get_relation(nation_id, n.id)
            && rel.has_consulate
            && !rel.has_embassy
            && rel.score >= min_relation
        {
            upgradeable += 1;
            if priority_targets.contains(&n.id) {
                priority_upgradeable += 1;
            }
        }
    }

    if upgradeable == 0 {
        return None;
    }

    // Priority targets that have warmed up still get the headline score —
    // the gate is on warmth, not on prioritisation.
    if priority_upgradeable > 0 {
        return Some(ScoredAction {
            category: SpendingCategory::Embassy,
            score: cfg.priority_minor_target_score * weights.diplomacy_weight,
            cost: Money::dollars(cfg.embassy_cost),
        });
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

    // Workforce-capacity gate: don't bleed workers into civilian units when
    // the industrial chain is already understaffed.
    if !nation.chain_labor_gate_passes(&game.game_data.game_config) {
        return None;
    }

    // Card #217 follow-up: per-tile weighted demand. An improvable tile only
    // produces yield once the nation can collect from it, so a tile in a
    // connected depot's collection radius pulls a stronger "we need a worker"
    // signal than a tile that's still disconnected. The buckets mirror the
    // ones the improver-deployment uses (`ai_deploy_civilians`):
    //   - collectable        in the connected harvest set today
    //   - rail_adjacent      adjacent to *our* existing rail/depot — easy
    //                        to extend a depot to within a few turns
    //   - unconnected        far from rail; speculative
    //   - undiscovered hex   un-prospected deposit-eligible hex (Prospector pull)
    let cfg = &game.game_data.game_config;

    // Precompute the connectivity sets for this nation.
    let owned_provinces: Vec<&crate::map::Province> = game
        .world
        .provinces
        .iter()
        .filter(|p| p.owner == nation_id)
        .collect();
    let connected = connected_provinces(game, nation_id);
    let collectable: HashSet<HexCoord> = crate::map::infrastructure::collectable_hexes(
        &game.world.hex_map,
        &owned_provinces,
        &connected,
    );
    let owned_hexes: HashSet<HexCoord> = owned_provinces
        .iter()
        .flat_map(|p| p.tiles.iter().copied())
        .collect();
    let rail_adjacent: HashSet<HexCoord> = {
        let mut s = HashSet::new();
        for &coord in &owned_hexes {
            let Some(tile) = game.world.hex_map.get_tile(coord) else {
                continue;
            };
            if tile.infrastructure.has_railroad || tile.infrastructure.has_depot {
                for n in coord.neighbors().iter().copied() {
                    s.insert(n);
                }
            }
        }
        s
    };

    let mut weighted_demand: f64 = 0.0;
    let mut visible_improvable_count: u32 = 0;
    let mut undiscovered_count: u32 = 0;
    for &pid in &nation.province_ids {
        if let Some(province) = game.get_province(pid) {
            for &coord in &province.tiles {
                let Some(tile) = game.world.hex_map.get_tile(coord) else {
                    continue;
                };
                // A tile is "undiscovered" only when it could plausibly hide
                // a deposit AND nothing is visible on it.
                if tile.terrain().can_have_deposits()
                    && !tile.is_prospected()
                    && !tile.has_visible_resource()
                {
                    undiscovered_count += 1;
                    weighted_demand += cfg.civilian_coverage_undiscovered;
                    continue;
                }
                if !tile.has_visible_resource() {
                    continue;
                }
                let max_level = game.game_data.tech_tree.effective_max_improvement_level(
                    tile.terrain(),
                    tile.resource_deposit(),
                    &nation.researched_techs,
                );
                if max_level == 0 || tile.improvement_level() >= max_level {
                    continue;
                }
                visible_improvable_count += 1;
                let mult = if collectable.contains(&coord) {
                    cfg.civilian_coverage_collectable
                } else if rail_adjacent.contains(&coord) {
                    cfg.civilian_coverage_rail_adjacent
                } else {
                    cfg.civilian_coverage_unconnected
                };
                weighted_demand += mult;
            }
        }
    }
    let any_demand = visible_improvable_count + undiscovered_count > 0;

    let civilian_count = nation
        .military
        .civilians
        .iter()
        .filter(|c| c.civilian_type != CivilianType::Engineer)
        .count();
    let idle_civilians = nation
        .military
        .civilians
        .iter()
        .filter(|c| {
            c.civilian_type != CivilianType::Engineer && !c.working && c.turns_remaining == 0
        })
        .count();

    // Saturation: each existing improver "covers" target_tiles_per_worker
    // weighted demand units. The unmet weighted demand is the score's
    // "we need more workers" signal.
    let target_ratio = cfg.civilian_target_tiles_per_worker as f64;
    let capacity = civilian_count as f64 * target_ratio;
    let unmet = (weighted_demand - capacity).max(0.0);
    let bootstrap = if civilian_count == 0 && any_demand {
        cfg.civilian_hire_bootstrap
    } else {
        0.0
    };
    let coverage = unmet + bootstrap;
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
    plans: &[super::economy::DepotPlan],
) -> Option<ScoredAction> {
    let nation = game.get_nation(nation_id)?;
    let cfg = &game.game_data.game_config;

    if cfg.civilian_costs_expert && nation.economy.labor.expert == 0 {
        return None;
    }
    if !nation.chain_labor_gate_passes(cfg) {
        return None;
    }

    // Cap engineer count to avoid unbounded hiring.
    let engineer_count = nation
        .military
        .civilians
        .iter()
        .filter(|c| c.civilian_type == CivilianType::Engineer)
        .count() as u32;
    if engineer_count >= cfg.engineer_hire_max {
        return None;
    }

    // Need a plan to justify hiring more engineers (no work → no need).
    let path_len = plans.iter().map(|p| p.path.len()).sum::<usize>() as f64;
    if path_len == 0.0 {
        return None;
    }

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
            .world
            .diplomacy
            .get_relation(nation_id, other.id)
            .is_some_and(|r| r.at_war);
        if at_war {
            any_at_war = true;
            max_enemy_naval_fp =
                max_enemy_naval_fp.max(other.total_naval_firepower(&game.game_data));
        }
    }
    if any_at_war {
        raw += naval_war_bonus;
    }

    // Outmatched at sea: strong driver.
    let our_naval_fp = nation.total_naval_firepower(&game.game_data);
    if max_enemy_naval_fp > our_naval_fp {
        raw += (max_enemy_naval_fp - our_naval_fp) as f64 * naval_gap_coeff;
    }

    // Scale by the Lua naval weight if set, otherwise use the military weight.
    let personality = get_personality(game, nation_id);
    let naval_weight = super::lua_bridge::get_personality_config(game, personality)
        .and_then(|c| c.spending_naval_weight)
        .unwrap_or(weights.military_weight);
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
    plans: &[super::economy::DepotPlan],
) {
    match category {
        SpendingCategory::Military => execute_military(game, nation_id, actions),
        SpendingCategory::Infrastructure => execute_infrastructure(game, nation_id, plans),
        // Consulate/Embassy are handled by ai_diplomatic_mop_up, not the scored loop.
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
        if !nation.chain_labor_gate_passes(&cfg) {
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
        nation
            .military
            .civilians
            .push(Civilian::new(civ_id, CivilianType::Engineer, nation_id));
    }
    game.transient.pending_ai_cash_spending.push((
        nation_id,
        crate::economy::ledger::CashSink::AiCivilianBuild,
        cost,
        None,
    ));
}

/// Pick the unit type from `options` whose current share in `current_types` is
/// furthest below its target share (derived from the frequency of each type in
/// `options`). Falls back to `fallback_seed % len` for an empty army.
fn pick_unit_for_balance(
    options: &[ArmyUnitType],
    current_types: &[ArmyUnitType],
    fallback_seed: usize,
) -> ArmyUnitType {
    if current_types.is_empty() {
        return options[fallback_seed % options.len()];
    }
    let n = options.len() as f64;
    let mut unique: Vec<ArmyUnitType> = Vec::new();
    for &t in options {
        if !unique.contains(&t) {
            unique.push(t);
        }
    }
    let tracked = unique
        .iter()
        .map(|&t| current_types.iter().filter(|&&u| u == t).count())
        .sum::<usize>()
        .max(1) as f64;
    unique
        .into_iter()
        .max_by(|&a, &b| {
            let ta = options.iter().filter(|&&t| t == a).count() as f64 / n;
            let tb = options.iter().filter(|&&t| t == b).count() as f64 / n;
            let ca = current_types.iter().filter(|&&u| u == a).count() as f64 / tracked;
            let cb = current_types.iter().filter(|&&u| u == b).count() as f64 / tracked;
            (ta - ca)
                .partial_cmp(&(tb - cb))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(options[fallback_seed % options.len()])
}

fn execute_military(game: &mut GameState, nation_id: NationId, actions: &mut Vec<super::AiAction>) {
    let turn_number = game.turn.0;
    let personality = get_personality(game, nation_id);
    let (capital, nation_name, unit_type, cost, arms_to_produce) = {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };

        let capital = nation.capital_province_id;
        let nation_name = nation.name.clone();
        let variety_seed = (turn_number as usize).wrapping_mul(nation_id.0 as usize + 7);
        // Field army count — ignore garrison militia so early-game recruit
        // choices aren't skewed by the always-present home garrison.
        let army_count = nation.field_army_count();
        // Snapshot unit types for balance calculation.
        let army_types: Vec<ArmyUnitType> =
            nation.military.army.iter().map(|u| u.unit_type).collect();

        // Pick role based on army composition, personality, and balance.
        // The role is then upgraded to the latest variant the nation has
        // unlocked, so once RifleInfantry / FieldArtillery / Guards land
        // the AI starts recruiting them in place of their Era I parents.
        let role = if army_count < 3 {
            let options: &[ArmyUnitType] = match personality {
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
                AiPersonality::Economic | AiPersonality::Balanced => &[
                    ArmyUnitType::Regulars,
                    ArmyUnitType::Grenadiers,
                    ArmyUnitType::Regulars,
                ],
            };
            pick_unit_for_balance(options, &army_types, variety_seed)
        } else if army_count < 8 {
            let options: &[ArmyUnitType] = match personality {
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
                AiPersonality::Economic | AiPersonality::Balanced => &[
                    ArmyUnitType::Grenadiers,
                    ArmyUnitType::LightArtillery,
                    ArmyUnitType::Grenadiers,
                ],
            };
            pick_unit_for_balance(options, &army_types, variety_seed)
        } else {
            let options: &[ArmyUnitType] = match personality {
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
            pick_unit_for_balance(options, &army_types, variety_seed)
        };

        let researched = &nation.researched_techs;
        let unit_type = role.latest_unlocked_in_chain(|tech_name| {
            game.game_data
                .tech_tree
                .all_techs()
                .iter()
                .any(|t| t.name == tech_name && researched.contains(&t.id))
        });

        let cost = unit_type.stats().cost;
        let arms_need = unit_type.stats().arms_required;
        let arms_have = nation.goods_amount(GoodsType::Arms);
        let steel_have = nation.material_amount(MaterialType::Steel);
        // Produce arms from steel if we're short, mirroring build_one_warship.
        // Hold back the AI's expansion-reserve steel so unit recruitment never
        // starves a planned mill/factory upgrade.
        let (_, steel_reserve) = super::economy::reserve_for_expansion(
            game,
            nation_id,
            super::economy::expansions_per_turn_target(game, personality),
            super::economy::expansion_reserve_buildings_factor(game, personality),
        );
        let (_, _, m_steel_reserve, _) =
            super::naval::merchant_navy_material_reserve(game, nation_id);
        let (_, freight_steel_reserve) =
            super::economy::freight_expansion_material_reserve(game, nation_id);
        let usable_steel = steel_have
            .saturating_sub(steel_reserve)
            .saturating_sub(m_steel_reserve)
            .saturating_sub(freight_steel_reserve);
        let needs_arms_production = arms_have < arms_need && usable_steel > 0;
        let arms_to_produce = if needs_arms_production {
            (arms_need - arms_have).min(usable_steel)
        } else {
            0
        };

        (capital, nation_name, unit_type, cost, arms_to_produce)
    };
    // Only convert when all non-arms requirements are already met; prevents
    // consuming steel when treasury, labor, horse, or oil would block anyway.
    let non_arms_ok = game
        .get_nation(nation_id)
        .map(|n| {
            let stats = unit_type.stats();
            let labor_ok = match stats.recruit_tier {
                crate::economy::labor::WorkerType::Untrained => n.economy.labor.untrained >= 1,
                crate::economy::labor::WorkerType::Trained => n.economy.labor.trained >= 1,
                crate::economy::labor::WorkerType::Expert => n.economy.labor.expert >= 1,
            };
            n.economy.treasury >= stats.cost
                && labor_ok
                && (!stats.requires_horse || n.resource_amount(ResourceType::Horses) >= 1)
                && (stats.fuel_required == 0
                    || n.resource_amount(ResourceType::Oil) >= stats.fuel_required)
        })
        .unwrap_or(false);
    if arms_to_produce > 0 && non_arms_ok {
        if let Some(nation) = game.get_nation_mut(nation_id) {
            nation.consume_material(MaterialType::Steel, arms_to_produce);
            nation.add_goods(GoodsType::Arms, arms_to_produce);
        }
        game.transient.pending_ai_material_outflows.push((
            nation_id,
            MaterialType::Steel,
            crate::economy::ledger::ResourceOut::FactoryConsumed,
            arms_to_produce,
        ));
        game.transient.pending_ai_goods_inflows.push((
            nation_id,
            GoodsType::Arms,
            crate::economy::ledger::ResourceIn::FactoryOutput,
            arms_to_produce,
        ));
    }
    let can_afford = game
        .get_nation(nation_id)
        .map(|n| n.can_recruit_unit(unit_type))
        .unwrap_or(false);
    if can_afford {
        let unit_id = game.alloc_unit_id();
        if let Some(nation) = game.get_nation_mut(nation_id) {
            nation.deduct_recruit_resources(unit_type);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineerTaskStart {
    Started,
    AssignedConflict,
    NotOwned,
    Unaffordable,
    InvalidTarget,
}

/// AI infrastructure step: drive (or hire) an Engineer civilian along one of
/// the currently committed depot plans.
fn execute_infrastructure(
    game: &mut GameState,
    nation_id: NationId,
    plans: &[super::economy::DepotPlan],
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
    let nation_name = if game.ai_debug {
        nation.name.clone()
    } else {
        String::new()
    };

    // Pick an *idle* engineer so multiple engineers can build in parallel
    // (one rail/depot per engineer per turn). Falls back to the hire path
    // only when the nation has zero engineers; if the nation has engineers
    // but all are busy, score_infrastructure already returned None and we
    // wouldn't be here.
    let engineer_idx = nation.military.civilians.iter().position(|c| {
        c.civilian_type == CivilianType::Engineer && !c.working && c.turns_remaining == 0
    });

    let any_engineer_present = nation
        .military
        .civilians
        .iter()
        .any(|c| c.civilian_type == CivilianType::Engineer);

    let engineer_idx = match engineer_idx {
        Some(idx) => idx,
        None if any_engineer_present => {
            // Have engineer(s) but none idle — nothing for us to start now.
            // (score_infrastructure should have prevented this; defensive.)
            if game.ai_debug {
                eprintln!("[AI:{}:infra] no idle engineer available", nation_name);
            }
            return;
        }
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
                if !nation.chain_labor_gate_passes(&cfg) {
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
                nation.military.civilians.push(Civilian::new(
                    civ_id,
                    CivilianType::Engineer,
                    nation_id,
                ));
            }
            game.transient.pending_ai_cash_spending.push((
                nation_id,
                crate::economy::ledger::CashSink::AiCivilianBuild,
                cost,
                None,
            ));
            if game.ai_debug {
                eprintln!("[AI:{}:infra] hired engineer", nation_name);
            }
            return;
        }
    };

    // Province-loss resilience: if the engineer is sitting on a hex the nation
    // no longer owns, clear its position (will redeploy this turn).
    {
        let owned_hexes: HashSet<HexCoord> = game
            .world
            .provinces
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
            if game.ai_debug {
                eprintln!("[AI:{}:infra] recalled stranded engineer", nation_name);
            }
        }
    }

    // If the engineer is still working, let the turn processor finish it.
    if let Some(nation) = game.get_nation(nation_id)
        && nation.military.civilians[engineer_idx].working
    {
        return;
    }

    // Use the cached plans from the spending loop. If none exist, check for
    // stranded coastal provinces that are tech-blocked from rail.
    if plans.is_empty() {
        if let Some(port_coord) = super::economy::find_stranded_port_target(game, nation_id) {
            if game.ai_debug {
                eprintln!(
                    "[AI:{}:infra] no depot plan, trying stranded port at ({}, {})",
                    nation_name, port_coord.q, port_coord.r
                );
            }
            let _ = start_engineer_task(
                game,
                nation_id,
                engineer_idx,
                port_coord,
                BuildTask::Port,
                &cfg,
            );
        } else if game.ai_debug {
            eprintln!(
                "[AI:{}:infra] no depot plan and no stranded port target",
                nation_name
            );
        }
        return;
    }

    // Where is the engineer? Default to capital if undeployed.
    let engineer_pos = game
        .get_nation(nation_id)
        .and_then(|n| n.military.civilians[engineer_idx].position)
        .unwrap_or(capital_tile);

    for plan in plans {
        // Card #421: before committing to depot+rail, check whether building a
        // port instead would connect the target province more cheaply / faster.
        if let Some(port_coord) = find_port_alternative(game, nation_id, plan, &cfg) {
            remove_infra_commitment(game, nation_id, plan.candidate);
            if game.ai_debug {
                eprintln!(
                    "[AI:{}:infra] using port alternative at ({}, {}) instead of depot candidate=({}, {}) path_len={}",
                    nation_name,
                    port_coord.q,
                    port_coord.r,
                    plan.candidate.q,
                    plan.candidate.r,
                    plan.path.len()
                );
            }
            if start_engineer_task(
                game,
                nation_id,
                engineer_idx,
                port_coord,
                BuildTask::Port,
                &cfg,
            ) == EngineerTaskStart::Started
            {
                return;
            }
            continue;
        }

        if plan.path.is_empty() {
            if game.ai_debug {
                eprintln!(
                    "[AI:{}:infra] candidate already reached, starting depot at ({}, {})",
                    nation_name, plan.candidate.q, plan.candidate.r
                );
            }
            if start_engineer_task(
                game,
                nation_id,
                engineer_idx,
                plan.candidate,
                BuildTask::Depot,
                &cfg,
            ) == EngineerTaskStart::Started
            {
                return;
            }
            continue;
        }

        let unbuilt_unassigned = |c: HexCoord| -> bool {
            game.world.hex_map.get_tile(c).is_some_and(|t| {
                !t.infrastructure.has_railroad
                    && !t.infrastructure.has_depot
                    && t.assigned_civilian.is_none()
            })
        };
        let build_coord = plan
            .path
            .iter()
            .copied()
            .find(|c| engineer_pos.neighbors().contains(c) && unbuilt_unassigned(*c))
            .or_else(|| plan.path.iter().copied().find(|c| unbuilt_unassigned(*c)));

        let attempt = match build_coord {
            Some(coord) => {
                if game.ai_debug {
                    eprintln!(
                        "[AI:{}:infra] starting rail on ({}, {}) engineer_pos=({}, {}) candidate=({}, {}) path_len={}",
                        nation_name,
                        coord.q,
                        coord.r,
                        engineer_pos.q,
                        engineer_pos.r,
                        plan.candidate.q,
                        plan.candidate.r,
                        plan.path.len()
                    );
                }
                start_engineer_task(
                    game,
                    nation_id,
                    engineer_idx,
                    coord,
                    BuildTask::Railroad,
                    &cfg,
                )
            }
            None => {
                if game.ai_debug {
                    eprintln!(
                        "[AI:{}:infra] no open rail hex on path, falling back to depot at ({}, {})",
                        nation_name, plan.candidate.q, plan.candidate.r
                    );
                }
                start_engineer_task(
                    game,
                    nation_id,
                    engineer_idx,
                    plan.candidate,
                    BuildTask::Depot,
                    &cfg,
                )
            }
        };
        if attempt == EngineerTaskStart::Started {
            return;
        }
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
) -> EngineerTaskStart {
    let nation_name = if game.ai_debug {
        game.get_nation(nation_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "?".to_string())
    } else {
        String::new()
    };
    // Clear old tile's assigned_civilian (if any).
    let (civ_id, old_pos) = match game.get_nation(nation_id) {
        Some(n) => (
            n.military.civilians[engineer_idx].id,
            n.military.civilians[engineer_idx].position,
        ),
        None => return EngineerTaskStart::InvalidTarget,
    };
    if let Some(old) = old_pos
        && let Some(tile) = game.world.hex_map.get_tile_mut(old)
        && tile.assigned_civilian == Some(civ_id)
    {
        tile.assigned_civilian = None;
    }
    // Target hex must be owned and empty of another civilian.
    let owned = game
        .world
        .provinces
        .iter()
        .filter(|p| p.owner == nation_id)
        .any(|p| p.tiles.contains(&coord));
    if !owned {
        if game.ai_debug {
            eprintln!(
                "[AI:{}:infra] start_task {:?} at ({}, {}) failed: target not owned",
                nation_name, task, coord.q, coord.r
            );
        }
        return EngineerTaskStart::NotOwned;
    }
    let current_assignee = game
        .world
        .hex_map
        .get_tile(coord)
        .and_then(|t| t.assigned_civilian);
    let target_ok = game
        .world
        .hex_map
        .get_tile(coord)
        .is_some_and(|t| t.assigned_civilian.is_none() || t.assigned_civilian == Some(civ_id));
    if !target_ok {
        if game.ai_debug {
            eprintln!(
                "[AI:{}:infra] start_task {:?} at ({}, {}) failed: assigned_civilian={:?} self={}",
                nation_name, task, coord.q, coord.r, current_assignee, civ_id.0
            );
        }
        return EngineerTaskStart::AssignedConflict;
    }
    // Only start a task the nation can afford at completion time. Treasury is
    // debited when the build finishes, so we guard at order time.
    let cost = match task {
        BuildTask::Railroad => {
            let terrain = match game.world.hex_map.get_tile(coord) {
                Some(t) => t.terrain(),
                None => return EngineerTaskStart::InvalidTarget,
            };
            match crate::map::infrastructure::railroad_cost(terrain, cfg) {
                Some(m) => m,
                None => return EngineerTaskStart::InvalidTarget, // e.g. sea
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
        if game.ai_debug {
            eprintln!(
                "[AI:{}:infra] start_task {:?} at ({}, {}) failed: treasury=${} cost=${}",
                nation_name,
                task,
                coord.q,
                coord.r,
                treasury.as_dollars(),
                cost.as_dollars()
            );
        }
        return EngineerTaskStart::Unaffordable;
    }
    if let Some(tile) = game.world.hex_map.get_tile_mut(coord) {
        tile.assigned_civilian = Some(civ_id);
    }
    if let Some(nation) = game.get_nation_mut(nation_id) {
        let civ = &mut nation.military.civilians[engineer_idx];
        civ.deploy(coord);
        civ.start_build(task, cfg);
    }
    if game.ai_debug {
        eprintln!(
            "[AI:{}:infra] start_task {:?} at ({}, {}) success civ_id={} cost=${}",
            nation_name,
            task,
            coord.q,
            coord.r,
            civ_id.0,
            cost.as_dollars()
        );
    }
    EngineerTaskStart::Started
}

/// Card #421: decide whether a port is a better build target than the
/// depot+rail plan returned by `plan_next_depot`.
///
/// A port wins when:
/// 1. The nation has a sea hub (a coastal country-capital — implicit port —
///    or any owned tile with a literal port).
/// 2. The depot plan's candidate province has a coastal tile (real ocean,
///    not a lake) owned by the nation.
/// 3. The depot+rail elapsed cost beats the port option:
///    - $: `depot_cost + path_cost > port_cost`
///    - turns: `depot_turns + path_len > port_turns`
///
/// Returns the coastal tile the engineer should head to. The engineer's own
/// idle/working state is checked by the caller.
fn find_port_alternative(
    game: &GameState,
    nation_id: NationId,
    plan: &super::economy::DepotPlan,
    cfg: &crate::data::GameConfig,
) -> Option<HexCoord> {
    let nation = game.get_nation(nation_id)?;

    // (1) Nation must have a sea hub already (built port or coastal capital).
    let owned_hexes: Vec<HexCoord> = game
        .world
        .provinces
        .iter()
        .filter(|p| p.owner == nation_id)
        .flat_map(|p| p.tiles.iter().copied())
        .collect();
    let has_sea_hub = owned_hexes.iter().any(|c| {
        crate::map::infrastructure::has_effective_port(
            &game.world.hex_map,
            &game.world.sea_zones,
            *c,
        )
    });
    if !has_sea_hub {
        return None;
    }

    // (2) Find the candidate province that the depot plan is heading to.
    // Use the candidate tile's province_id.
    let candidate_pid = game
        .world
        .hex_map
        .get_tile(plan.candidate)
        .and_then(|t| t.province_id)?;
    if candidate_pid == nation.capital_province_id {
        return None;
    }
    let province = game.get_province(candidate_pid)?;
    if province.owner != nation_id {
        return None;
    }

    // Skip if the province already has a port — depot+rail is the right call.
    let already_has_port = province.tiles.iter().any(|c| {
        game.world
            .hex_map
            .get_tile(*c)
            .is_some_and(|t| t.infrastructure.has_port)
    });
    if already_has_port {
        return None;
    }

    // (3) Find an owned coastal land tile (sea-adjacent, real ocean, no
    // building yet, no civilian assigned). Prefer the province centroid if
    // it qualifies, else any qualifying tile in the province.
    let mut centroid_first = std::iter::once(province.capital_tile).chain(
        province
            .tiles
            .iter()
            .copied()
            .filter(|c| *c != province.capital_tile),
    );

    let coastal_tile = centroid_first.find(|c| {
        let Some(tile) = game.world.hex_map.get_tile(*c) else {
            return false;
        };
        if !tile.terrain().is_land() || tile.infrastructure.has_port {
            return false;
        }
        if tile.assigned_civilian.is_some() {
            return false;
        }
        // Must be adjacent to a real ocean tile (not a lake).
        c.neighbors().iter().any(|n| {
            let Some(nt) = game.world.hex_map.get_tile(*n) else {
                return false;
            };
            if nt.terrain().is_land() {
                return false;
            }
            !game
                .world
                .sea_zones
                .iter()
                .any(|z| z.is_lake && z.hexes.contains(n))
        })
    })?;

    // (4) Compare depot+rail cost/turns vs. port cost/turns.
    let depot_cost = cfg.depot_cost;
    let port_cost = cfg.port_cost;
    let path_cost_dollars = plan
        .path
        .iter()
        .filter_map(|c| game.world.hex_map.get_tile(*c))
        .filter(|t| !t.infrastructure.has_railroad && !t.infrastructure.has_depot)
        .filter_map(|t| crate::map::infrastructure::railroad_cost(t.terrain(), cfg))
        .map(|m| m.as_dollars())
        .sum::<i64>();
    let depot_total_cost = depot_cost + path_cost_dollars;

    let depot_turns: u32 =
        cfg.build_turns_depot as u32 + plan.path.len() as u32 * cfg.build_turns_railroad as u32;
    let port_turns: u32 = cfg.build_turns_port as u32;

    if port_cost < depot_total_cost && port_turns < depot_turns {
        Some(coastal_tile)
    } else {
        None
    }
}

fn execute_consulate(game: &mut GameState, nation_id: NationId) {
    let cost = Money::dollars(game.game_data.game_config.consulate_cost);

    // Prefer unsecured priority targets first. Only fall back to best-trade
    // match among non-priority minors once all priority targets are secured.
    let priority_pick: Option<NationId> = game.get_nation(nation_id).and_then(|n| {
        n.diplomacy
            .ai_priority_state
            .priority_minor_targets
            .iter()
            .find(|mn_id| {
                game.get_nation(**mn_id)
                    .is_some_and(|m| !m.province_ids.is_empty() && !m.diplomacy.is_in_anarchy)
                    && game
                        .world
                        .diplomacy
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
                .world
                .diplomacy
                .get_relation(nation_id, n.id)
                .is_some_and(|r| r.has_consulate)
            {
                continue;
            }
            let potential: u32 = game
                .world
                .provinces
                .iter()
                .filter(|p| p.owner == n.id)
                .flat_map(|p| &p.tiles)
                .filter_map(|coord| {
                    game.world
                        .hex_map
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
        && game
            .world
            .diplomacy
            .build_consulate(nation_id, mn_id)
            .is_ok()
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
    let min_relation = game.game_data.game_config.ai_embassy_min_relation;

    // Card #210: relation gate applies uniformly. Priority targets are
    // preferred among warmed-up candidates, but they do NOT bypass the
    // gate — a priority MN with a cold relation should keep building rapport
    // through the consulate before the AI burns money on the upgrade.
    let priority_targets: std::collections::HashSet<NationId> = game
        .get_nation(nation_id)
        .map(|n| {
            n.diplomacy
                .ai_priority_state
                .priority_minor_targets
                .iter()
                .copied()
                .collect()
        })
        .unwrap_or_default();

    let mut best: Option<NationId> = None;
    let mut best_priority = false;
    let mut best_relation = i32::MIN;
    for n in &game.world.nations {
        if n.is_great_power() || n.province_ids.is_empty() || n.diplomacy.is_in_anarchy {
            continue;
        }
        let Some(rel) = game.world.diplomacy.get_relation(nation_id, n.id) else {
            continue;
        };
        if !rel.has_consulate || rel.has_embassy || rel.score < min_relation {
            continue;
        }
        let is_priority = priority_targets.contains(&n.id);
        // Priority status takes precedence; among same-priority entries,
        // pick the warmest relation.
        let better = match (is_priority, best_priority) {
            (true, false) => true,
            (false, true) => false,
            _ => rel.score > best_relation,
        };
        if better {
            best = Some(n.id);
            best_priority = is_priority;
            best_relation = rel.score;
        }
    }

    if let Some(mn_id) = best
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
        if !nation.chain_labor_gate_passes(&cfg) {
            return;
        }

        let civ_type = match select_civilian_to_hire(game, nation, &cfg) {
            Some(t) => t,
            None => return,
        };

        let already_idle_unplaced = nation
            .military
            .civilians
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
        nation
            .military
            .civilians
            .push(Civilian::new(civ_id, civ_type, nation_id));
    }
    game.transient.pending_ai_cash_spending.push((
        nation_id,
        crate::economy::ledger::CashSink::AiCivilianBuild,
        cost,
        None,
    ));
}

/// Pick which civilian type the AI should hire next.
///
/// Two-stage decision per the manual + card #86 fix:
///
/// 1. **Prospector first**: if the nation has un-prospected deposit-eligible
///    hexes and not yet enough Prospectors (one per `ai_prospector_per_hexes`
///    hexes), hire a Prospector — minerals are unknown until revealed.
/// 2. **Saturation picker for improvers**: among tech-unlocked improver types
///    (Farmer/Rancher/Forester/Miner/Driller), score each by
///    `unimproved_tiles[T] / (workers[T] + 1)`. Pick the highest. Tie-break
///    by raw `unimproved_tiles[T]`.
///
/// Skips a type if it has zero improvable tiles, so Rancher/Driller/Forester
/// don't displace useful hires when there's nothing for them to do.
fn select_civilian_to_hire(
    game: &GameState,
    nation: &crate::nation::Nation,
    cfg: &crate::data::GameConfig,
) -> Option<CivilianType> {
    use std::collections::HashMap;

    // ── Collect per-type counts in one pass ──────────────────────
    let mut improvable_by_type: HashMap<CivilianType, u32> = HashMap::new();
    let mut undiscovered_hexes: u32 = 0;
    for &pid in &nation.province_ids {
        if let Some(province) = game.get_province(pid) {
            for &coord in &province.tiles {
                let Some(tile) = game.world.hex_map.get_tile(coord) else {
                    continue;
                };
                let terrain = tile.terrain();

                // Undiscovered = deposit-capable terrain, not yet prospected,
                // AND nothing visible on the tile. Hills + visible Wool are
                // deposit-capable but already useful to a Rancher; treat them
                // as Rancher demand below, not as Prospector demand.
                if terrain.can_have_deposits()
                    && !tile.is_prospected()
                    && !tile.has_visible_resource()
                {
                    undiscovered_hexes += 1;
                    continue;
                }
                if !tile.has_visible_resource() {
                    continue;
                }
                let resource = tile.resource_deposit();
                let max_level = game.game_data.tech_tree.effective_max_improvement_level(
                    terrain,
                    resource,
                    &nation.researched_techs,
                );
                if max_level == 0 || tile.improvement_level() >= max_level {
                    continue;
                }
                for civ in [
                    CivilianType::Farmer,
                    CivilianType::Rancher,
                    CivilianType::Forester,
                    CivilianType::Miner,
                    CivilianType::Driller,
                ] {
                    if civ.can_improve(terrain, resource) {
                        *improvable_by_type.entry(civ).or_insert(0) += 1;
                        break;
                    }
                }
            }
        }
    }

    let mut workers_by_type: HashMap<CivilianType, u32> = HashMap::new();
    for civ in &nation.military.civilians {
        *workers_by_type.entry(civ.civilian_type).or_insert(0) += 1;
    }

    // ── Saturation-driven pick across all civilian types ─────────
    //
    // For each type T:
    //   demand[T]     = number of tiles T can usefully be assigned to
    //   saturation[T] = demand[T] / (workers[T] + 1)
    //
    // The improver types (Farmer/Rancher/Forester/Miner/Driller) use raw
    // unimproved-tile counts. Prospector is normalised by
    // `ai_prospector_per_hexes` so a vast unexplored map doesn't crowd out
    // bread-and-butter improvers — one Prospector can cover many hexes over
    // multiple turns. Tie-break by raw demand so a swing of one tile doesn't
    // pin the pick.
    let per_hexes = cfg.ai_prospector_per_hexes.max(1);
    let prospector_demand = if cfg.ai_prospector_per_hexes == 0 {
        0
    } else {
        undiscovered_hexes / per_hexes
    };

    // Each existing worker is treated as covering `target_tiles_per_worker`
    // tiles' worth of work. Once `workers >= ceil(demand / coverage)`, the
    // type is saturated and we don't hire another. This is what stops the
    // AI from buying a Farmer for every grain plain on a 50-tile empire.
    let coverage = cfg.civilian_target_tiles_per_worker.max(1);
    let mut best: Option<(CivilianType, f64, u32)> = None;
    let consider = |civ: CivilianType, demand: u32, best: &mut Option<(CivilianType, f64, u32)>| {
        if demand == 0 {
            return;
        }
        let workers = workers_by_type.get(&civ).copied().unwrap_or(0);
        let workers_needed = demand.div_ceil(coverage);
        if workers >= workers_needed {
            return;
        }
        let saturation = demand as f64 / (workers as f64 + 1.0);
        let beats = match *best {
            None => true,
            Some((_, s, t)) => saturation > s || (saturation == s && demand > t),
        };
        if beats {
            *best = Some((civ, saturation, demand));
        }
    };

    for civ in [
        CivilianType::Farmer,
        CivilianType::Rancher,
        CivilianType::Forester,
        CivilianType::Miner,
        CivilianType::Driller,
    ] {
        if !civ.is_unlocked(&nation.researched_techs, &game.game_data, cfg) {
            continue;
        }
        let demand = improvable_by_type.get(&civ).copied().unwrap_or(0);
        consider(civ, demand, &mut best);
    }
    consider(CivilianType::Prospector, prospector_demand, &mut best);

    // Bootstrap: if we matched nothing yet (e.g. no improvable tiles known
    // but plenty of un-prospected hexes), still hire a Prospector — it's
    // the only way to surface new mineral demand.
    if best.is_none() && undiscovered_hexes > 0 && cfg.ai_prospector_per_hexes > 0 {
        return Some(CivilianType::Prospector);
    }

    best.map(|(t, _, _)| t)
}

/// Drop annexed/destroyed priority targets and pick replacements so the
/// nation always has up to its personality-count of live targets. Called
/// once per turn at the top of the spending loop.
fn refresh_priority_targets(game: &mut GameState, nation_id: NationId, personality: AiPersonality) {
    let cfg = game.game_data.game_config.clone();
    let target_count = priority_target_count(&cfg, personality);

    let mut kept: Vec<NationId> = match game.get_nation(nation_id) {
        Some(n) => n
            .diplomacy
            .ai_priority_state
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
            .world
            .provinces
            .iter()
            .filter(|p| p.owner == minor.id)
            .flat_map(|p| &p.tiles)
            .filter_map(|c| {
                game.world
                    .hex_map
                    .get_tile(*c)
                    .and_then(|t| t.calculate_yield())
            })
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

    if let Some(cfg) = super::lua_bridge::get_personality_config(game, personality) {
        w.military_weight = lua_or(
            cfg.spending_military_weight,
            defaults.spending_military_weight,
        );
        w.economy_weight = lua_or(
            cfg.spending_economy_weight,
            defaults.spending_economy_weight,
        );
        w.diplomacy_weight = lua_or(
            cfg.spending_diplomacy_weight,
            defaults.spending_diplomacy_weight,
        );
        w.reserve = lua_or(
            cfg.treasury_reserve.map(Money::dollars),
            defaults.spending_reserve,
        );
        w.min_threshold = lua_or(cfg.min_score_threshold, defaults.spending_min_threshold);
    }

    w
}

/// Greedily build all affordable consulates and embassies each turn.
///
/// Consulates and embassies only cost money — they have no strategic
/// opportunity cost like recruiting a unit or building infrastructure. So they
/// should not compete in the scored spending auction. Instead, after the main
/// loop runs, this pass builds every consulate and embassy the AI can afford
/// above its treasury reserve, in priority order, until money runs out.
pub(crate) fn ai_diplomatic_mop_up(game: &mut GameState, nation_id: NationId) {
    // Build consulates until we can't afford the next one or there are none left.
    // No reserve deduction — consulates/embassies are pure money purchases and
    // should not be blocked by the strategic spending reserve.
    loop {
        let consulate_cost = Money::dollars(game.game_data.game_config.consulate_cost);
        let can_afford = game
            .get_nation(nation_id)
            .is_some_and(|n| n.economy.treasury >= consulate_cost);
        if !can_afford {
            break;
        }
        // Below the wealth threshold, cap consulates at the personality limit so
        // cash-strapped early-game nations don't blow their treasury on diplomacy.
        // Above it, build consulates freely with any affordable minor.
        let has_target = {
            let cfg = &game.game_data.game_config;
            let nation = match game.get_nation(nation_id) {
                Some(n) => n,
                None => return,
            };
            let wealthy =
                nation.economy.treasury.as_dollars() >= cfg.labor_wealthy_treasury_threshold;
            let cap = priority_target_count(
                cfg,
                nation
                    .diplomacy
                    .ai_personality
                    .unwrap_or(AiPersonality::Balanced),
            ) as u32;
            let existing: u32 = game
                .world
                .nations
                .iter()
                .filter(|n| !n.is_great_power() && !n.province_ids.is_empty())
                .filter(|n| {
                    game.world
                        .diplomacy
                        .get_relation(nation_id, n.id)
                        .is_some_and(|r| r.has_consulate)
                })
                .count() as u32;
            if !wealthy && existing >= cap {
                false
            } else {
                game.world.nations.iter().any(|n| {
                    !n.is_great_power()
                        && !n.province_ids.is_empty()
                        && !n.diplomacy.is_in_anarchy
                        && game
                            .world
                            .diplomacy
                            .get_relation(nation_id, n.id)
                            .is_none_or(|r| !r.has_consulate)
                })
            }
        };
        if !has_target {
            break;
        }
        let treasury_before = game.get_nation(nation_id).map(|n| n.economy.treasury);
        execute_consulate(game, nation_id);
        let treasury_after = game.get_nation(nation_id).map(|n| n.economy.treasury);
        if treasury_before == treasury_after {
            // execute_consulate found nothing to build
            break;
        }
    }

    // Build embassies until we can't afford the next one or there are none left.
    loop {
        let embassy_cost = Money::dollars(game.game_data.game_config.embassy_cost);
        let min_relation = game.game_data.game_config.ai_embassy_min_relation;
        let can_afford = game
            .get_nation(nation_id)
            .is_some_and(|n| n.economy.treasury >= embassy_cost);
        if !can_afford {
            break;
        }
        let has_target = game.world.nations.iter().any(|n| {
            !n.is_great_power()
                && !n.province_ids.is_empty()
                && !n.diplomacy.is_in_anarchy
                && game
                    .world
                    .diplomacy
                    .get_relation(nation_id, n.id)
                    .is_some_and(|r| r.has_consulate && !r.has_embassy && r.score >= min_relation)
        });
        if !has_target {
            break;
        }
        let treasury_before = game.get_nation(nation_id).map(|n| n.economy.treasury);
        execute_embassy(game, nation_id);
        let treasury_after = game.get_nation(nation_id).map(|n| n.economy.treasury);
        if treasury_before == treasury_after {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::{test_game_with_ai, test_game_with_ai_and_minor};
    use crate::economy::civilians::{Civilian, CivilianType, next_civilian_id};
    use crate::map::tile::Tile;
    use crate::types::{NationId, ProvinceId, ResourceType, TerrainType};

    /// Card #217 follow-up: when many improvable tiles sit inside a
    /// connected depot's collection radius, HireImprover should score higher
    /// than when the same tile count is disconnected — improving a
    /// disconnected tile produces no yield until rail catches up.
    #[test]
    fn hire_improver_score_lower_for_disconnected_tiles_than_collectable_ones() {
        fn run_with_tiles_in_collectable(in_collectable: bool) -> f64 {
            let mut game = test_game_with_ai();
            // To get 6 collectable test tiles, mark the AI capital at (3,3)
            // as country_capital and use its 6 neighbours as test tiles.
            // For the disconnected branch, place 6 tiles far from any
            // collector. We use 6 tiles in both branches to keep counts
            // identical so the only differentiator is the connectivity bucket.
            // Ensure the AI capital tile exists in hex_map (the helper
            // doesn't populate it) and mark it as country_capital so it acts
            // as a collector.
            let cap = crate::hex::HexCoord::new(3, 3);
            let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
            cap_tile.is_country_capital = true;
            game.world.hex_map.set_tile(cap, cap_tile);
            let coords: Vec<crate::hex::HexCoord> = if in_collectable {
                crate::hex::HexCoord::new(3, 3).neighbors().to_vec()
            } else {
                (0..6)
                    .map(|i| crate::hex::HexCoord::new(20 + i, 20 + i))
                    .collect()
            };
            let prov = game
                .world
                .provinces
                .iter_mut()
                .find(|p| p.id == ProvinceId(2))
                .expect("AI province");
            for &c in &coords {
                if !prov.tiles.contains(&c) {
                    prov.tiles.push(c);
                }
            }
            for &c in &coords {
                let mut tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
                tile.set_resource(ResourceType::Grain);
                game.world.hex_map.set_tile(c, tile);
            }

            let ai = game.get_nation_mut(NationId(2)).unwrap();
            ai.economy.treasury = Money::dollars(10_000);
            ai.economy.labor.expert = 1;
            ai.military.civilians.clear();
            // One *non-idle* civilian so idle_penalty stays at 0 and the
            // bucket weight differences come through cleanly.
            let mut working_civ =
                Civilian::new(next_civilian_id(), CivilianType::Farmer, NationId(2));
            working_civ.working = true;
            working_civ.turns_remaining = 2;
            ai.military.civilians.push(working_civ);
            // Seed Drill so Grain tiles are improvable.
            ai.researched_techs.push(crate::events::TechId(2));

            let weights = load_weights(&game, super::super::common::AiPersonality::Diplomatic);
            score_civilian(&game, NationId(2), &weights)
                .map(|a| a.score)
                .unwrap_or(0.0)
        }

        let score_collectable = run_with_tiles_in_collectable(true);
        let score_disconnected = run_with_tiles_in_collectable(false);

        assert!(
            score_collectable > score_disconnected,
            "score for collectable improvable tiles must exceed the score for the same tiles disconnected (collectable={}, disconnected={})",
            score_collectable,
            score_disconnected,
        );
    }

    /// Card #210: AI must not score an embassy when the only candidate MN
    /// has a consulate but a cold relationship — consulate alone gives a
    /// relationship bonus, the embassy is the costly upgrade and should wait.
    #[test]
    fn score_embassy_skips_cold_consulate_relationship() {
        let mut game = test_game_with_ai_and_minor();
        let ai = NationId(2);
        let mn = NationId(3);

        // Consulate established, but score never warmed up.
        game.world.diplomacy.build_consulate(ai, mn).unwrap();
        let rel = game.world.diplomacy.get_relation_mut(ai, mn).unwrap();
        rel.score = 0; // cold

        let weights = load_weights(&game, super::super::common::AiPersonality::Balanced);
        let scored = score_embassy(&game, ai, &weights);
        assert!(
            scored.is_none(),
            "embassy should not be considered until relationship reaches ai_embassy_min_relation",
        );
    }

    /// Once the relationship clears the threshold, the embassy becomes a
    /// scored option again — the gate is conditional, not permanent.
    #[test]
    fn score_embassy_allowed_when_relationship_warm() {
        let mut game = test_game_with_ai_and_minor();
        let ai = NationId(2);
        let mn = NationId(3);

        game.world.diplomacy.build_consulate(ai, mn).unwrap();
        let threshold = game.game_data.game_config.ai_embassy_min_relation;
        let rel = game.world.diplomacy.get_relation_mut(ai, mn).unwrap();
        rel.score = threshold;

        let weights = load_weights(&game, super::super::common::AiPersonality::Balanced);
        let scored = score_embassy(&game, ai, &weights);
        assert!(
            scored.is_some(),
            "embassy should be scored when relationship >= ai_embassy_min_relation",
        );
    }

    // ── Card #217 follow-up: multi-engineer + depot saving ──

    /// Build a minimal game state with an AI nation, a country-capital tile,
    /// a committed depot target, and N engineers (some idle, some busy).
    /// Returns (game, ai_id, candidate_coord).
    fn game_with_engineers(idle_count: usize, busy_count: usize) -> (GameState, NationId) {
        let mut game = test_game_with_ai();
        let cap = crate::hex::HexCoord::new(3, 3);
        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        cap_tile.set_resource(ResourceType::Grain);
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        game.world.hex_map.set_tile(cap, cap_tile);

        // A path of owned grassland tiles ending at a candidate 3 hexes
        // away from the capital so plan_next_depot returns a real plan
        // (the capital's 1-hex radius is already covered, so the candidate
        // must be outside it).
        let path_coords: Vec<crate::hex::HexCoord> = vec![
            crate::hex::HexCoord::new(4, 3),
            crate::hex::HexCoord::new(5, 3),
            crate::hex::HexCoord::new(6, 3),
        ];
        let candidate = *path_coords.last().unwrap();
        for &c in &path_coords {
            let mut t = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
            t.set_resource(ResourceType::Grain);
            game.world.hex_map.set_tile(c, t);
        }
        let prov = game
            .world
            .provinces
            .iter_mut()
            .find(|p| p.id == ProvinceId(2))
            .expect("AI province");
        for &c in &path_coords {
            if !prov.tiles.contains(&c) {
                prov.tiles.push(c);
            }
        }
        let _ = candidate;

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.treasury = Money::dollars(10_000);
        ai.economy.labor.expert = 4;
        ai.military.civilians.clear();
        for _ in 0..busy_count {
            let mut e = Civilian::new(next_civilian_id(), CivilianType::Engineer, NationId(2));
            e.working = true;
            e.turns_remaining = 2;
            ai.military.civilians.push(e);
        }
        for _ in 0..idle_count {
            let e = Civilian::new(next_civilian_id(), CivilianType::Engineer, NationId(2));
            ai.military.civilians.push(e);
        }
        (game, NationId(2))
    }

    /// Card #217 follow-up: with two engineers, one mid-build and one idle,
    /// score_infrastructure must still fire — the idle engineer is
    /// available to start a parallel build.
    #[test]
    fn score_infrastructure_fires_when_at_least_one_engineer_idle() {
        let (game, ai_id) = game_with_engineers(1, 1);
        let weights = load_weights(&game, super::super::common::AiPersonality::Balanced);
        let plan_outcome = super::super::economy::plan_next_depot(&game, ai_id);
        let plans: Vec<_> = plan_outcome.as_plan().cloned().into_iter().collect();

        let scored = score_infrastructure(&game, ai_id, &weights, &plans, false);
        assert!(
            scored.is_some(),
            "score_infrastructure should fire while at least one engineer is idle, got None",
        );
    }

    /// Conversely, when every engineer is busy, score_infrastructure must
    /// return None — there's nobody to assign to a new task.
    #[test]
    fn score_infrastructure_none_when_no_engineer_idle() {
        let (game, ai_id) = game_with_engineers(0, 2);
        let weights = load_weights(&game, super::super::common::AiPersonality::Balanced);
        let plan_outcome = super::super::economy::plan_next_depot(&game, ai_id);
        let plans: Vec<_> = plan_outcome.as_plan().cloned().into_iter().collect();

        let scored = score_infrastructure(&game, ai_id, &weights, &plans, false);
        assert!(
            scored.is_none(),
            "score_infrastructure must return None when no engineer is idle, got Some",
        );
    }

    /// Card #217 follow-up: when the engineer is parked at a built rail line
    /// and the only remaining infra step is the depot, but the AI can't yet
    /// afford the depot+reserve, the spending loop must NOT bleed cash into
    /// civilian hires — it must save up.
    #[test]
    fn ai_scored_spending_skips_improver_while_saving_for_depot() {
        // Build a state with: a country-capital + connected rail leading to a
        // candidate (so plan.path is empty), an idle engineer, an improvable
        // tile that would otherwise pull HireImprover, and a treasury just
        // above the reserve but below `reserve + depot_cost`.
        let mut game = test_game_with_ai();
        let cap = crate::hex::HexCoord::new(3, 3);
        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        cap_tile.set_resource(ResourceType::Grain);
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        game.world.hex_map.set_tile(cap, cap_tile);

        // Rail-connected candidate at (4,3): the path is empty (rail already
        // there), so the next infra step is to plant the depot at (4,3).
        let candidate = crate::hex::HexCoord::new(4, 3);
        let mut cand_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        cand_tile.set_resource(ResourceType::Grain);
        cand_tile.infrastructure.has_railroad = true;
        game.world.hex_map.set_tile(candidate, cand_tile);

        // Improvable visible tile far from rail to drive HireImprover demand
        // (uses the unconnected weight, so contribution is small but present).
        let far = crate::hex::HexCoord::new(8, 8);
        let mut far_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        far_tile.set_resource(ResourceType::Grain);
        game.world.hex_map.set_tile(far, far_tile);

        let prov = game
            .world
            .provinces
            .iter_mut()
            .find(|p| p.id == ProvinceId(2))
            .expect("AI province");
        for &c in &[candidate, far] {
            if !prov.tiles.contains(&c) {
                prov.tiles.push(c);
            }
        }

        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.economy.labor.expert = 4;
        ai.military.civilians.clear();
        // Idle engineer so the depot CAN be built — and should be.
        ai.military.civilians.push(Civilian::new(
            next_civilian_id(),
            CivilianType::Engineer,
            NationId(2),
        ));
        // Existing improver so HireImprover scoring path runs cleanly.
        ai.military.civilians.push(Civilian::new(
            next_civilian_id(),
            CivilianType::Farmer,
            NationId(2),
        ));
        ai.researched_techs.push(crate::events::TechId(2));
        // Diplomatic reserve = $1000; depot_cost = $2000. Treasury at $1500
        // is above reserve (loop fires) but below reserve + depot_cost
        // ($3000) → saving_for_depot kicks in.
        ai.economy.treasury = Money::dollars(1500);
        ai.diplomacy.ai_personality = Some(super::super::common::AiPersonality::Diplomatic);

        let civs_before = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .civilians
            .iter()
            .filter(|c| c.civilian_type != CivilianType::Engineer)
            .count();
        let mut actions = Vec::new();
        ai_scored_spending(&mut game, NationId(2), &mut actions);
        let civs_after = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .civilians
            .iter()
            .filter(|c| c.civilian_type != CivilianType::Engineer)
            .count();
        assert_eq!(
            civs_after, civs_before,
            "AI must not hire improvers while saving for the depot (civs_before={}, civs_after={})",
            civs_before, civs_after,
        );
    }

    /// Card #210 follow-up: a priority-minor target with a cold relation
    /// must NOT score an embassy. The gate applies uniformly — priority
    /// status only changes which warmed-up MN is preferred, never whether
    /// the upgrade is permitted at all. (Without this, on turn 1 the AI
    /// would build a consulate and immediately upgrade it to an embassy
    /// with rel.score still at 0.)
    #[test]
    fn score_embassy_priority_target_still_blocked_when_cold() {
        let mut game = test_game_with_ai_and_minor();
        let ai = NationId(2);
        let mn = NationId(3);

        game.world.diplomacy.build_consulate(ai, mn).unwrap();
        let rel = game.world.diplomacy.get_relation_mut(ai, mn).unwrap();
        rel.score = 0; // cold despite priority status

        let nation = game.get_nation_mut(ai).unwrap();
        nation
            .diplomacy
            .ai_priority_state
            .priority_minor_targets
            .push(mn);

        let weights = load_weights(&game, super::super::common::AiPersonality::Balanced);
        let scored = score_embassy(&game, ai, &weights);
        assert!(
            scored.is_none(),
            "priority-minor targets must still respect the relation gate; turn-1 embassy spam is the bug we're killing",
        );
    }

    /// Once a priority-minor target's relationship clears the gate, it
    /// gets the headline `priority_minor_target_score` rather than the
    /// regular per-MN score — priority still matters when warm.
    #[test]
    fn score_embassy_priority_target_gets_headline_score_when_warm() {
        let mut game = test_game_with_ai_and_minor();
        let ai = NationId(2);
        let mn = NationId(3);

        game.world.diplomacy.build_consulate(ai, mn).unwrap();
        let threshold = game.game_data.game_config.ai_embassy_min_relation;
        let rel = game.world.diplomacy.get_relation_mut(ai, mn).unwrap();
        rel.score = threshold;

        let nation = game.get_nation_mut(ai).unwrap();
        nation
            .diplomacy
            .ai_priority_state
            .priority_minor_targets
            .push(mn);

        let weights = load_weights(&game, super::super::common::AiPersonality::Balanced);
        let scored = score_embassy(&game, ai, &weights).expect("embassy should score");

        let expected =
            game.game_data.game_config.priority_minor_target_score * weights.diplomacy_weight;
        assert!(
            (scored.score - expected).abs() < 1e-6,
            "warm priority target should get headline score {expected}, got {}",
            scored.score,
        );
    }

    /// Card #210 follow-up: a category never yet invested in must score
    /// zero backlog on turn 1, not `current_turn.min(cap)`. The earlier
    /// behaviour gave Military a free 30-point bonus on turn 1 (Balanced
    /// weight 30 × 1 turn) which biased every fresh AI toward army spam.
    #[test]
    fn backlog_bonus_zero_on_turn_1_for_uninvested_category() {
        let game = test_game_with_ai();
        let bonus = backlog_bonus(&game, NationId(2), SpendingCategory::Military, 1, false);
        assert_eq!(
            bonus, 0.0,
            "fresh AI must not get backlog credit on turn 1 for never-invested category",
        );
    }

    /// And the backlog still accrues normally once a few turns have passed
    /// without investment — the cap takes over only at long horizons.
    #[test]
    fn backlog_bonus_accrues_after_turn_1() {
        let game = test_game_with_ai();
        let bonus = backlog_bonus(&game, NationId(2), SpendingCategory::Military, 5, false);
        // 4 turns of accrual × Balanced military weight (30) = 120
        assert_eq!(bonus, 120.0);
    }

    // ── Consulate cap (per-personality, wealth-overridable) ──

    /// Build on top of `test_game_with_ai_and_minor`: extend the world with
    /// `extra_minors` additional minor nations, each with a tradeable Grain
    /// tile so they're visible to `score_consulate`. Returns the IDs of all
    /// minor nations (the original + the extras).
    fn game_with_extra_minors(extra_minors: usize) -> (GameState, NationId, Vec<NationId>) {
        use crate::ai::common::test_helpers::test_game_with_ai_and_minor;
        use crate::hex::HexCoord;
        use crate::map::Province;
        use crate::map::tile::Tile;
        use crate::nation::{Nation, NationColor};
        use crate::types::NationType;

        let mut game = test_game_with_ai_and_minor();
        let ai_id = NationId(2);
        let mut minor_ids = vec![NationId(3)]; // pre-existing minor

        for i in 0..extra_minors {
            let mn_id = NationId(100 + i as u32);
            let prov_id = ProvinceId(100 + i as u32);
            let coord = HexCoord::new(8 + i as i32 % 5, 8 + i as i32 / 5);
            let mut tile = Tile::with_province(TerrainType::Grassland, prov_id);
            tile.set_resource(ResourceType::Grain);
            game.world.hex_map.set_tile(coord, tile);
            let mut prov =
                Province::new(prov_id, format!("Minor {i}"), mn_id, coord, vec![coord], 3);
            prov.tiles.push(coord);
            game.world.provinces.push(prov);
            let mut mn = Nation::new(
                mn_id,
                format!("Minor {i}"),
                NationColor::Gray,
                NationType::MinorNation,
                prov_id,
            );
            mn.add_province(prov_id);
            game.world.nations.push(mn);
            minor_ids.push(mn_id);
        }
        (game, ai_id, minor_ids)
    }

    /// Card follow-up: under normal financial conditions, the AI must not
    /// score additional consulates once it already holds the
    /// per-personality cap (Balanced = 4).
    #[test]
    fn score_consulate_capped_at_personality_target_when_not_wealthy() {
        let (mut game, ai_id, minors) = game_with_extra_minors(7);
        // Treasury well below the wealthy threshold so the cap holds.
        game.get_nation_mut(ai_id).unwrap().economy.treasury = Money::dollars(5_000);
        // Pre-build 4 consulates (== Balanced cap). Use minors that are NOT
        // priority targets so the priority short-circuit can't fire.
        for &mn in minors.iter().take(4) {
            game.world.diplomacy.build_consulate(ai_id, mn).unwrap();
        }

        let weights = load_weights(&game, AiPersonality::Balanced);
        let scored = score_consulate(&game, ai_id, &weights);
        assert!(
            scored.is_none(),
            "non-wealthy Balanced AI must not score a 5th consulate (cap = 4)",
        );
    }

    /// Card follow-up: when the AI is wealthy (treasury >=
    /// `labor_wealthy_treasury_threshold`), the cap is lifted and the
    /// soft-decay branch resumes governing growth. The AI may still score
    /// additional consulates above the personality cap.
    #[test]
    fn score_consulate_cap_lifted_when_wealthy() {
        let (mut game, ai_id, minors) = game_with_extra_minors(7);
        let threshold = game.game_data.game_config.labor_wealthy_treasury_threshold;
        game.get_nation_mut(ai_id).unwrap().economy.treasury = Money::dollars(threshold + 1_000);
        for &mn in minors.iter().take(4) {
            game.world.diplomacy.build_consulate(ai_id, mn).unwrap();
        }

        let weights = load_weights(&game, AiPersonality::Balanced);
        let scored = score_consulate(&game, ai_id, &weights);
        assert!(
            scored.is_some(),
            "wealthy AI must still score additional consulates beyond the personality cap",
        );
    }

    /// Card follow-up: the cap applies *before* the priority short-circuit,
    /// so an AI at the cap will not chase a still-unsecured priority target
    /// past its limit (under normal financial conditions).
    #[test]
    fn score_consulate_cap_blocks_even_priority_short_circuit() {
        let (mut game, ai_id, minors) = game_with_extra_minors(7);
        game.get_nation_mut(ai_id).unwrap().economy.treasury = Money::dollars(5_000);
        // 4 consulates already (cap reached) with non-priority MNs.
        for &mn in minors.iter().take(4) {
            game.world.diplomacy.build_consulate(ai_id, mn).unwrap();
        }
        // Add a 5th MN as a priority target with no consulate yet.
        let stretch_target = minors[4];
        game.get_nation_mut(ai_id)
            .unwrap()
            .diplomacy
            .ai_priority_state
            .priority_minor_targets
            .push(stretch_target);

        let weights = load_weights(&game, AiPersonality::Balanced);
        let scored = score_consulate(&game, ai_id, &weights);
        assert!(
            scored.is_none(),
            "cap must apply ahead of the priority short-circuit so the AI doesn't chase priority targets past the limit when poor",
        );
    }
}
