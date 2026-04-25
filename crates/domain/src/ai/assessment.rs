//! Coalition-aware power assessment and treaty evaluation for AI diplomacy.
//!
//! Provides functions to evaluate relative strength between warring coalitions,
//! assess whether continuing a war is worthwhile, and evaluate incoming treaty
//! proposals (peace, NAP, alliance).
#![allow(unused_labels)] // labeled blocks used only with cfg(feature = "lua")

use crate::ai::common::AiPersonality;
use crate::game_state::GameState;
use crate::types::*;

#[cfg(feature = "lua")]
use crate::ai::lua_bridge::LuaAiConfig;

// ── Structs ───────────────────────────────────────────────────────

/// Result of evaluating relative coalition strength in an active or hypothetical war.
#[derive(Debug, Clone)]
pub struct WarAssessment {
    /// Our coalition's total military firepower (land + naval * weight).
    pub our_military: f64,
    /// Enemy coalition's total military firepower.
    pub enemy_military: f64,
    /// Our coalition's province count.
    pub our_provinces: usize,
    /// Enemy coalition's province count.
    pub enemy_provinces: usize,
    /// Our coalition's economic strength.
    pub our_economic: f64,
    /// Enemy coalition's economic strength.
    pub enemy_economic: f64,
    /// War momentum: positive = gaining ground, negative = losing ground.
    pub momentum: f64,
    /// Overall power ratio (our / enemy). >1.0 means we're stronger.
    pub power_ratio: f64,
    /// Estimated likelihood of winning: 0.0 to 1.0.
    pub win_likelihood: f64,
}

/// Result of evaluating whether continuing a war is worthwhile.
#[derive(Debug, Clone)]
pub struct WarWorthiness {
    /// Provinces captured from the enemy during this war.
    pub provinces_captured: usize,
    /// Provinces lost to the enemy during this war.
    pub provinces_lost: usize,
    /// Distinct resource types gained in captured provinces.
    pub resources_gained: usize,
    /// Proxy for ongoing war cost per turn.
    pub ongoing_cost: f64,
    /// Diminishing returns: 0.0 = no more gains possible, 1.0 = lots of potential.
    pub marginal_value: f64,
    /// Whether we've captured enough to propose peace from strength.
    pub won_enough: bool,
    /// Whether we've lost enough to sue for peace.
    pub lost_enough: bool,
}

// ── Default assessment weights ────────────────────────────────────

struct AssessmentWeights {
    mil_weight: f64,
    prov_weight: f64,
    econ_weight: f64,
    momentum_weight: f64,
    naval_weight: f64,
    sigmoid_steepness: f64,
}

impl Default for AssessmentWeights {
    fn default() -> Self {
        Self {
            mil_weight: 0.5,
            prov_weight: 0.3,
            econ_weight: 0.2,
            momentum_weight: 0.15,
            naval_weight: 0.3,
            sigmoid_steepness: 3.0,
        }
    }
}

#[cfg(feature = "lua")]
fn weights_from_lua(cfg: Option<&LuaAiConfig>) -> AssessmentWeights {
    let d = AssessmentWeights::default();
    match cfg {
        Some(c) => AssessmentWeights {
            mil_weight: c.coalition_mil_weight.unwrap_or(d.mil_weight),
            prov_weight: c.coalition_prov_weight.unwrap_or(d.prov_weight),
            econ_weight: c.coalition_econ_weight.unwrap_or(d.econ_weight),
            momentum_weight: c.coalition_momentum_weight.unwrap_or(d.momentum_weight),
            naval_weight: c.coalition_naval_weight.unwrap_or(d.naval_weight),
            sigmoid_steepness: c.coalition_sigmoid_steepness.unwrap_or(d.sigmoid_steepness),
        },
        None => d,
    }
}

// ── Helper functions ──────────────────────────────────────────────

/// Compute a nation's military score (land firepower + naval firepower scaled).
pub fn nation_military_score(game: &GameState, nation_id: NationId, naval_weight: f64) -> f64 {
    game.get_nation(nation_id)
        .map(|n| n.total_military_firepower() + n.total_naval_firepower() as f64 * naval_weight)
        .unwrap_or(0.0)
}

/// Compute a nation's economic score (treasury + buildings + workers).
pub fn nation_economic_score(game: &GameState, nation_id: NationId) -> f64 {
    game.get_nation(nation_id)
        .map(|n| {
            n.economy.treasury.as_dollars() as f64 / 10_000.0
                + n.economy.buildings.len() as f64 * 0.1
                + n.economy.labor.total_workers() as f64 * 0.05
        })
        .unwrap_or(0.0)
}

/// Collect the coalition for `nation_id` in a war against `enemy_id`.
/// Returns nation_id plus all allies that are also at war with enemy_id.
pub fn collect_war_coalition(
    game: &GameState,
    nation_id: NationId,
    enemy_id: NationId,
) -> Vec<NationId> {
    let mut coalition = vec![nation_id];
    for ally_id in game.diplomacy.get_allies(nation_id) {
        if game.diplomacy.is_at_war(ally_id, enemy_id) {
            coalition.push(ally_id);
        }
    }
    coalition
}

/// Collect the hypothetical coalition for `attacker` if war were declared on `target`.
/// Includes attacker's allies, minus any that are also allied with target.
pub fn collect_hypothetical_coalition(
    game: &GameState,
    attacker: NationId,
    target: NationId,
) -> Vec<NationId> {
    let target_allies = game.diplomacy.get_allies(target);
    let mut coalition = vec![attacker];
    for ally_id in game.diplomacy.get_allies(attacker) {
        if !target_allies.contains(&ally_id) {
            coalition.push(ally_id);
        }
    }
    coalition
}

/// Collect the hypothetical defender coalition for a target being attacked.
/// Includes the target's alliance partners AND, for minor nation targets,
/// all nations with Non-Aggression Pacts (any pact holder may choose to
/// intervene, so the AI treats them all as potential threats).
pub fn collect_target_hypothetical_coalition(
    game: &GameState,
    attacker: NationId,
    target: NationId,
) -> Vec<NationId> {
    let mut defenders = vec![target];
    // Add Alliance partners
    for ally_id in game.diplomacy.get_allies(target) {
        if ally_id != attacker && !defenders.contains(&ally_id) {
            defenders.push(ally_id);
        }
    }
    // Add NAP pact-defense holders (only for minor nations — pact defense
    // only triggers when a minor nation with a NAP is attacked)
    let is_minor = game.get_nation(target).is_some_and(|n| !n.is_great_power());
    if is_minor {
        for pact_id in game.diplomacy.get_pact_holders(target) {
            if pact_id != attacker && !defenders.contains(&pact_id) {
                defenders.push(pact_id);
            }
        }
    }
    defenders
}

fn sigmoid(x: f64, steepness: f64) -> f64 {
    1.0 / (1.0 + (-x * steepness).exp())
}

/// Compute momentum by scanning history for province changes in the last `window` turns.
fn compute_momentum(
    game: &GameState,
    nation_id: NationId,
    enemy_id: NationId,
    window: u32,
) -> (f64, usize, usize) {
    use crate::events::HistoryEvent;
    let min_turn = game.turn.0.saturating_sub(window);
    let mut captured = 0usize;
    let mut lost = 0usize;
    for (turn_entry, event) in &game.history {
        if turn_entry.0 < min_turn {
            continue;
        }
        if let HistoryEvent::ProvinceConquered {
            conqueror, loser, ..
        } = event
        {
            if *conqueror == nation_id && *loser == enemy_id {
                captured += 1;
            } else if *conqueror == enemy_id && *loser == nation_id {
                lost += 1;
            }
        }
    }
    let momentum = captured as f64 - lost as f64;
    (momentum, captured, lost)
}

/// Find the turn when the *current* war started between two nations by scanning history.
/// Uses the most recent war declaration (not the first ever) so that province counts
/// are scoped to the current conflict when nations have fought multiple wars.
/// Pact-defense intervention (`X declared war on Y to protect Z`) only matches the
/// (X, Y) pair — never the (Y, Z) pair — because the typed event keeps protectee
/// distinct from the war participants.
pub fn find_war_start_turn(game: &GameState, a: NationId, b: NationId) -> Option<u32> {
    use crate::events::HistoryEvent;
    game.history
        .iter()
        .filter(|(_, ev)| match ev {
            HistoryEvent::WarDeclared {
                attacker, defender, ..
            } => (*attacker == a && *defender == b) || (*attacker == b && *defender == a),
            HistoryEvent::JoinedWar { joiner, target } => {
                (*joiner == a && *target == b) || (*joiner == b && *target == a)
            }
            _ => false,
        })
        .map(|(turn, _)| turn.0)
        .max()
}

// ── Core assessment functions ─────────────────────────────────────

/// Evaluate relative coalition strength for an active war.
pub fn evaluate_coalition_strength(
    game: &GameState,
    nation_id: NationId,
    enemy_id: NationId,
    #[cfg(feature = "lua")] lua_cfg: Option<&LuaAiConfig>,
) -> WarAssessment {
    #[cfg(feature = "lua")]
    let w = weights_from_lua(lua_cfg);
    #[cfg(not(feature = "lua"))]
    let w = AssessmentWeights::default();

    let our_side = collect_war_coalition(game, nation_id, enemy_id);
    let enemy_side = collect_war_coalition(game, enemy_id, nation_id);

    let our_military: f64 = our_side
        .iter()
        .map(|&id| nation_military_score(game, id, w.naval_weight))
        .sum();
    let enemy_military: f64 = enemy_side
        .iter()
        .map(|&id| nation_military_score(game, id, w.naval_weight))
        .sum();

    let our_provinces: usize = our_side
        .iter()
        .filter_map(|&id| game.get_nation(id))
        .map(|n| n.province_ids.len())
        .sum();
    let enemy_provinces: usize = enemy_side
        .iter()
        .filter_map(|&id| game.get_nation(id))
        .map(|n| n.province_ids.len())
        .sum();

    let our_economic: f64 = our_side
        .iter()
        .map(|&id| nation_economic_score(game, id))
        .sum();
    let enemy_economic: f64 = enemy_side
        .iter()
        .map(|&id| nation_economic_score(game, id))
        .sum();

    let (momentum, _, _) = compute_momentum(game, nation_id, enemy_id, 5);

    let our_total = our_military * w.mil_weight
        + our_provinces as f64 * w.prov_weight
        + our_economic * w.econ_weight
        + momentum * w.momentum_weight;
    let enemy_total = enemy_military * w.mil_weight
        + enemy_provinces as f64 * w.prov_weight
        + enemy_economic * w.econ_weight;

    let power_ratio = our_total / (enemy_total + 0.01);
    let win_likelihood = sigmoid(power_ratio - 1.0, w.sigmoid_steepness);

    WarAssessment {
        our_military,
        enemy_military,
        our_provinces,
        enemy_provinces,
        our_economic,
        enemy_economic,
        momentum,
        power_ratio,
        win_likelihood,
    }
}

/// Evaluate relative coalition strength for a *hypothetical* war (pre-declaration).
pub fn evaluate_hypothetical_war(
    game: &GameState,
    attacker: NationId,
    target: NationId,
    #[cfg(feature = "lua")] lua_cfg: Option<&LuaAiConfig>,
) -> WarAssessment {
    #[cfg(feature = "lua")]
    let w = weights_from_lua(lua_cfg);
    #[cfg(not(feature = "lua"))]
    let w = AssessmentWeights::default();

    let our_side = collect_hypothetical_coalition(game, attacker, target);
    // Use target coalition that includes pact-defense partners for minor nations
    let enemy_side = collect_target_hypothetical_coalition(game, attacker, target);

    let our_military: f64 = our_side
        .iter()
        .map(|&id| nation_military_score(game, id, w.naval_weight))
        .sum();
    let enemy_military: f64 = enemy_side
        .iter()
        .map(|&id| nation_military_score(game, id, w.naval_weight))
        .sum();

    let our_provinces: usize = our_side
        .iter()
        .filter_map(|&id| game.get_nation(id))
        .map(|n| n.province_ids.len())
        .sum();
    let enemy_provinces: usize = enemy_side
        .iter()
        .filter_map(|&id| game.get_nation(id))
        .map(|n| n.province_ids.len())
        .sum();

    let our_economic: f64 = our_side
        .iter()
        .map(|&id| nation_economic_score(game, id))
        .sum();
    let enemy_economic: f64 = enemy_side
        .iter()
        .map(|&id| nation_economic_score(game, id))
        .sum();

    let our_total = our_military * w.mil_weight
        + our_provinces as f64 * w.prov_weight
        + our_economic * w.econ_weight;
    let enemy_total = enemy_military * w.mil_weight
        + enemy_provinces as f64 * w.prov_weight
        + enemy_economic * w.econ_weight;

    let power_ratio = our_total / (enemy_total + 0.01);
    let win_likelihood = sigmoid(power_ratio - 1.0, w.sigmoid_steepness);

    WarAssessment {
        our_military,
        enemy_military,
        our_provinces,
        enemy_provinces,
        our_economic,
        enemy_economic,
        momentum: 0.0, // no momentum for hypothetical wars
        power_ratio,
        win_likelihood,
    }
}

/// Evaluate whether an AI protector should intervene to defend a minor nation
/// against an attacker. Uses relationship with the minor, military capability
/// vs the attacker, and personality bias.
pub fn evaluate_pact_defense(
    game: &GameState,
    protector_id: NationId,
    attacker_id: NationId,
    _minor_id: NationId,
    personality: AiPersonality,
    #[cfg(feature = "lua")] lua_cfg: Option<&LuaAiConfig>,
) -> bool {
    // Standing gate
    let standing = game.diplomacy.get_standing(protector_id);
    if standing < 30 {
        return false;
    }

    // Already at war with attacker — already fighting, no need to re-declare
    if game.diplomacy.is_at_war(protector_id, attacker_id) {
        return false;
    }

    // Relationship factor: how much does the protector care about the minor?
    let rel_score = game
        .diplomacy
        .get_relation(protector_id, _minor_id)
        .map(|r| r.score)
        .unwrap_or(0);
    let relationship_factor = (rel_score as f64 / 100.0).clamp(0.0, 1.0) * 0.4;

    // Military factor: can the protector beat the attacker?
    #[cfg(feature = "lua")]
    let assessment = evaluate_hypothetical_war(game, protector_id, attacker_id, lua_cfg);
    #[cfg(not(feature = "lua"))]
    let assessment = evaluate_hypothetical_war(game, protector_id, attacker_id);
    let military_factor = assessment.win_likelihood * 0.4;

    // Personality bias
    let personality_bias = match personality {
        AiPersonality::Aggressive => 0.2,
        AiPersonality::Diplomatic => 0.1,
        AiPersonality::Balanced => 0.0,
        AiPersonality::Economic => -0.15,
    };

    let combined = relationship_factor + military_factor + personality_bias;

    // Personality-dependent threshold
    let threshold = match personality {
        AiPersonality::Aggressive => 0.2,
        AiPersonality::Diplomatic => 0.3,
        AiPersonality::Balanced => 0.35,
        AiPersonality::Economic => 0.5,
    };

    combined >= threshold
}

/// Evaluate whether continuing a war is worthwhile.
pub fn evaluate_war_worthiness(
    game: &GameState,
    nation_id: NationId,
    enemy_id: NationId,
    personality: AiPersonality,
    win_likelihood: f64,
    #[cfg(feature = "lua")] lua_cfg: Option<&LuaAiConfig>,
) -> WarWorthiness {
    use crate::events::HistoryEvent;
    // Find war start turn
    let war_start = find_war_start_turn(game, nation_id, enemy_id).unwrap_or(0);

    // Count captures and losses since war start
    let mut provinces_captured = 0usize;
    let mut provinces_lost = 0usize;
    for (turn_entry, event) in &game.history {
        if turn_entry.0 < war_start {
            continue;
        }
        if let HistoryEvent::ProvinceConquered {
            conqueror, loser, ..
        } = event
        {
            if *conqueror == nation_id && *loser == enemy_id {
                provinces_captured += 1;
            } else if *conqueror == enemy_id && *loser == nation_id {
                provinces_lost += 1;
            }
        }
    }

    // Count unique resources in captured provinces (approximate via enemy's remaining tiles)
    let resources_gained = 0usize; // TODO: track captured province resources

    // Ongoing cost proxy — militia are free per manual; only field army
    // and warships generate upkeep pressure.
    let ongoing_cost = game
        .get_nation(nation_id)
        .map(|n| n.field_army_count() as f64 * 500.0 + n.warships.len() as f64 * 300.0)
        .unwrap_or(0.0);

    // Marginal value: how much is left to gain?
    let enemy_current_provinces = game
        .get_nation(enemy_id)
        .map(|n| n.province_ids.len())
        .unwrap_or(0);
    let enemy_at_war_start = enemy_current_provinces + provinces_captured;

    // Read thresholds from Lua or use personality defaults
    let won_enough_captures: usize = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.won_enough_captures) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 1,
            AiPersonality::Aggressive => 4,
            _ => 2,
        }
    };

    let lost_enough_losses: usize = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.lost_enough_losses) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 1,
            AiPersonality::Aggressive => 3,
            _ => 2,
        }
    };

    let lost_enough_likelihood: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.lost_enough_likelihood) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 0.40,
            AiPersonality::Aggressive => 0.20,
            AiPersonality::Economic => 0.35,
            _ => 0.30,
        }
    };

    let won_enough_marginal: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.won_enough_marginal) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 0.40,
            AiPersonality::Aggressive => 0.20,
            AiPersonality::Economic => 0.35,
            _ => 0.30,
        }
    };

    let marginal_value = if enemy_at_war_start > 0 {
        let remaining_fraction = enemy_current_provinces as f64 / enemy_at_war_start as f64;
        let capture_saturation = if won_enough_captures > 0 {
            1.0 - (provinces_captured as f64 / (won_enough_captures as f64 * 2.0)).min(1.0)
        } else {
            0.0
        };
        remaining_fraction * capture_saturation
    } else {
        0.0
    };

    let won_enough = provinces_captured >= won_enough_captures
        && win_likelihood >= 0.55
        && marginal_value < won_enough_marginal;

    let lost_enough =
        provinces_lost >= lost_enough_losses || win_likelihood < lost_enough_likelihood;

    WarWorthiness {
        provinces_captured,
        provinces_lost,
        resources_gained,
        ongoing_cost,
        marginal_value,
        won_enough,
        lost_enough,
    }
}

// ── Treaty proposal evaluation ────────────────────────────────────

/// Evaluate whether an AI nation should accept a peace proposal.
pub fn evaluate_peace_proposal(
    game: &GameState,
    from: NationId,
    to: NationId,
    personality: AiPersonality,
    #[cfg(feature = "lua")] lua_cfg: Option<&LuaAiConfig>,
) -> bool {
    // Lua hook: let scripts override the decision
    #[cfg(feature = "lua")]
    {
        let relationship = game
            .diplomacy
            .get_relation(from, to)
            .map(|r| r.score)
            .unwrap_or(0);
        // Compute a quick power ratio for the Lua hook
        let our_mil = nation_military_score(game, to, 0.3);
        let their_mil = nation_military_score(game, from, 0.3);
        let power_ratio = our_mil / (their_mil + 0.01);
        if let Some(result) = super::lua_bridge::lua_evaluate_treaty_response(
            game,
            personality,
            to,
            from,
            "PeaceTreaty",
            relationship,
            power_ratio,
        ) {
            return result;
        }
    }

    // The receiver (to) evaluates whether to accept peace from (from)
    let assessment = evaluate_coalition_strength(
        game,
        to,
        from,
        #[cfg(feature = "lua")]
        lua_cfg,
    );
    let worthiness = evaluate_war_worthiness(
        game,
        to,
        from,
        personality,
        assessment.win_likelihood,
        #[cfg(feature = "lua")]
        lua_cfg,
    );

    let accept_threshold: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.peace_accept_threshold) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 0.50,
            AiPersonality::Aggressive => 0.35,
            AiPersonality::Economic => 0.45,
            _ => 0.45,
        }
    };

    let reject_threshold: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.peace_reject_threshold) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 0.65,
            AiPersonality::Aggressive => 0.80,
            AiPersonality::Economic => 0.70,
            _ => 0.70,
        }
    };

    let stalemate_duration: u32 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg.as_ref().and_then(|c| c.peace_stalemate_duration) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 12,
            AiPersonality::Aggressive => 25,
            AiPersonality::Economic => 15,
            _ => 15,
        }
    };

    // Reject if clearly winning and want to press advantage
    if assessment.win_likelihood > reject_threshold
        && worthiness.provinces_lost > 0
        && assessment.win_likelihood > 0.55
    {
        return false;
    }

    // Accept if not winning enough to refuse
    if assessment.win_likelihood < accept_threshold {
        return true;
    }

    // Accept if we've already gotten what we wanted
    if worthiness.won_enough {
        return true;
    }

    // Accept if lost enough
    if worthiness.lost_enough {
        return true;
    }

    // Accept stalemate
    let war_start = find_war_start_turn(game, to, from);
    if let Some(start) = war_start {
        let duration = game.turn.0.saturating_sub(start);
        if duration > stalemate_duration && worthiness.provinces_captured == 0 {
            return true;
        }
    }

    // Default: reject — we're doing well enough to keep fighting
    false
}

/// Evaluate whether an AI nation should accept a NAP proposal.
pub fn evaluate_nap_proposal(
    game: &GameState,
    from: NationId,
    to: NationId,
    personality: AiPersonality,
    #[cfg(feature = "lua")] _lua_cfg: Option<&LuaAiConfig>,
) -> bool {
    // Lua hook: let scripts override the decision
    #[cfg(feature = "lua")]
    {
        let relationship = game
            .diplomacy
            .get_relation(from, to)
            .map(|r| r.score)
            .unwrap_or(0);
        let our_mil = nation_military_score(game, to, 0.3);
        let their_mil = nation_military_score(game, from, 0.3);
        let power_ratio = our_mil / (their_mil + 0.01);
        if let Some(result) = super::lua_bridge::lua_evaluate_treaty_response(
            game,
            personality,
            to,
            from,
            "NonAggressionPact",
            relationship,
            power_ratio,
        ) {
            return result;
        }
    }

    let nap_threshold: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = _lua_cfg.as_ref().and_then(|c| c.nap_accept_threshold) {
            break 'val v;
        }
        0.3
    };

    let mut score = 0.0f64;

    // Relationship factor
    if let Some(rel) = game.diplomacy.get_relation(from, to) {
        score += rel.score as f64 / 100.0 * 0.3;
        // Trust from diplomatic infrastructure
        if rel.has_embassy {
            score += 0.2;
        } else if rel.has_consulate {
            score += 0.1;
        }
    }

    // Common enemy bonus
    let nations: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();
    let has_common_enemy = nations.iter().any(|&third| {
        third != from
            && third != to
            && game.diplomacy.is_at_war(from, third)
            && game.diplomacy.is_at_war(to, third)
    });
    if has_common_enemy {
        score += 0.2;
    }

    // Threat penalty — if proposer is much stronger
    let our_military = nation_military_score(game, to, 0.3);
    let their_military = nation_military_score(game, from, 0.3);
    if their_military > our_military * 2.0 {
        score -= 0.3;
    }

    // Personality bias
    let bias: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = _lua_cfg.as_ref().and_then(|c| c.treaty_personality_bias) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 0.3,
            AiPersonality::Aggressive => -0.2,
            _ => 0.1,
        }
    };
    score += bias;

    score >= nap_threshold
}

/// Evaluate whether an AI nation should accept an alliance proposal.
pub fn evaluate_alliance_proposal(
    game: &GameState,
    from: NationId,
    to: NationId,
    personality: AiPersonality,
    #[cfg(feature = "lua")] _lua_cfg: Option<&LuaAiConfig>,
) -> bool {
    // Lua hook: let scripts override the decision
    #[cfg(feature = "lua")]
    {
        let relationship = game
            .diplomacy
            .get_relation(from, to)
            .map(|r| r.score)
            .unwrap_or(0);
        let our_mil = nation_military_score(game, to, 0.3);
        let their_mil = nation_military_score(game, from, 0.3);
        let power_ratio = our_mil / (their_mil + 0.01);
        if let Some(result) = super::lua_bridge::lua_evaluate_treaty_response(
            game,
            personality,
            to,
            from,
            "Alliance",
            relationship,
            power_ratio,
        ) {
            return result;
        }
    }

    let alliance_threshold: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = _lua_cfg.as_ref().and_then(|c| c.alliance_accept_threshold) {
            break 'val v;
        }
        0.5
    };

    let mut score = 0.0f64;

    // Relationship factor (higher weight for alliances)
    if let Some(rel) = game.diplomacy.get_relation(from, to) {
        score += rel.score as f64 / 100.0 * 0.4;
    }

    // Common enemy bonus
    let nations: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();
    let has_common_enemy = nations.iter().any(|&third| {
        third != from
            && third != to
            && game.diplomacy.is_at_war(from, third)
            && game.diplomacy.is_at_war(to, third)
    });
    let has_common_threat = !has_common_enemy
        && nations.iter().any(|&third| {
            third != from
                && third != to
                && (game.diplomacy.is_at_war(from, third) || game.diplomacy.is_at_war(to, third))
        });
    if has_common_enemy {
        score += 0.3;
    } else if has_common_threat {
        score += 0.15;
    }

    // Power complementarity: bonus if they're militarily strong and we're economically strong
    // or vice versa
    let our_mil = nation_military_score(game, to, 0.3);
    let our_econ = nation_economic_score(game, to);
    let their_mil = nation_military_score(game, from, 0.3);
    let their_econ = nation_economic_score(game, from);
    let mil_ratio = if our_mil > 0.01 {
        their_mil / our_mil
    } else {
        2.0
    };
    let econ_ratio = if their_econ > 0.01 {
        our_econ / their_econ
    } else {
        2.0
    };
    if (mil_ratio > 1.5 && econ_ratio > 1.5) || (mil_ratio < 0.67 && econ_ratio < 0.67) {
        score += 0.2;
    }

    // Rival penalty — don't ally with top competitors
    let rival_penalty: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = _lua_cfg.as_ref().and_then(|c| c.alliance_rival_penalty) {
            break 'val v;
        }
        0.4
    };
    // Simple rivalry check: if they have more provinces than us, they're a rival
    let our_provinces = game
        .get_nation(to)
        .map(|n| n.province_ids.len())
        .unwrap_or(0);
    let their_provinces = game
        .get_nation(from)
        .map(|n| n.province_ids.len())
        .unwrap_or(0);
    if their_provinces > our_provinces + 2 {
        score -= rival_penalty;
    }

    // Overcommitment penalty
    let overcommit_penalty: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = _lua_cfg
            .as_ref()
            .and_then(|c| c.alliance_overcommit_penalty)
        {
            break 'val v;
        }
        0.2
    };
    let existing_alliances = game.diplomacy.get_allies(to).len();
    if existing_alliances > 1 {
        score -= overcommit_penalty * (existing_alliances - 1) as f64;
    }

    // Personality bias
    let bias: f64 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = _lua_cfg.as_ref().and_then(|c| c.treaty_personality_bias) {
            break 'val v;
        }
        match personality {
            AiPersonality::Diplomatic => 0.4,
            AiPersonality::Aggressive => -0.15,
            AiPersonality::Economic => 0.0,
            _ => 0.1,
        }
    };
    score += bias;

    score >= alliance_threshold
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::test_game_with_adjacent_provinces;

    #[test]
    fn coalition_strength_basic() {
        let game = test_game_with_adjacent_provinces();
        // NationId(2) and NationId(3) are at war in this test helper
        let assessment = evaluate_coalition_strength(
            &game,
            NationId(2),
            NationId(3),
            #[cfg(feature = "lua")]
            None,
        );
        // Both should have non-zero values
        assert!(assessment.our_provinces > 0);
        assert!(assessment.enemy_provinces > 0);
        // win_likelihood should be between 0 and 1
        assert!(assessment.win_likelihood >= 0.0 && assessment.win_likelihood <= 1.0);
    }

    #[test]
    fn hypothetical_war_basic() {
        let mut game = test_game_with_adjacent_provinces();
        // Make peace so we can test hypothetical
        game.diplomacy.make_peace(NationId(2), NationId(3));

        let assessment = evaluate_hypothetical_war(
            &game,
            NationId(2),
            NationId(3),
            #[cfg(feature = "lua")]
            None,
        );
        assert!(assessment.win_likelihood >= 0.0 && assessment.win_likelihood <= 1.0);
        assert_eq!(assessment.momentum, 0.0); // hypothetical has no momentum
    }

    #[test]
    fn war_worthiness_no_history() {
        let game = test_game_with_adjacent_provinces();
        let worthiness = evaluate_war_worthiness(
            &game,
            NationId(2),
            NationId(3),
            AiPersonality::Balanced,
            0.5,
            #[cfg(feature = "lua")]
            None,
        );
        assert_eq!(worthiness.provinces_captured, 0);
        assert_eq!(worthiness.provinces_lost, 0);
        assert!(!worthiness.won_enough);
        // With win_likelihood 0.5 and 0 losses, lost_enough depends on threshold
        // Balanced lost_enough_likelihood = 0.30, so 0.5 > 0.30 → not lost enough from likelihood alone
        // lost_enough_losses = 2 and provinces_lost = 0 → not lost enough
        assert!(!worthiness.lost_enough);
    }

    #[test]
    fn lost_enough_when_low_likelihood() {
        let game = test_game_with_adjacent_provinces();
        let worthiness = evaluate_war_worthiness(
            &game,
            NationId(2),
            NationId(3),
            AiPersonality::Balanced,
            0.2, // well below 0.30 threshold
            #[cfg(feature = "lua")]
            None,
        );
        assert!(worthiness.lost_enough);
    }

    #[test]
    fn sigmoid_boundaries() {
        // sigmoid(0, 3) should be 0.5 (equal power)
        let equal = sigmoid(0.0, 3.0);
        assert!((equal - 0.5).abs() < 0.01);

        // sigmoid(large positive, 3) should approach 1.0
        let strong = sigmoid(2.0, 3.0);
        assert!(strong > 0.95);

        // sigmoid(large negative, 3) should approach 0.0
        let weak = sigmoid(-2.0, 3.0);
        assert!(weak < 0.05);
    }

    #[test]
    fn find_war_start_returns_most_recent_war() {
        use crate::events::HistoryEvent;
        let mut game = test_game_with_adjacent_provinces();
        // Simulate war→peace→war: two war declarations between same nations
        game.history.push((
            TurnNumber(5),
            HistoryEvent::WarDeclared {
                attacker: NationId(2),
                defender: NationId(3),
                protectee: None,
            },
        ));
        game.history.push((
            TurnNumber(15),
            HistoryEvent::WarDeclared {
                attacker: NationId(2),
                defender: NationId(3),
                protectee: None,
            },
        ));
        // Should return the most recent (turn 15), not the first (turn 5)
        let start = find_war_start_turn(&game, NationId(2), NationId(3));
        assert_eq!(start, Some(15));
    }

    #[test]
    fn find_war_start_matches_alliance_join() {
        use crate::events::HistoryEvent;
        let mut game = test_game_with_adjacent_provinces();
        game.history.push((
            TurnNumber(10),
            HistoryEvent::JoinedWar {
                joiner: NationId(2),
                target: NationId(3),
            },
        ));
        let start = find_war_start_turn(&game, NationId(2), NationId(3));
        assert_eq!(start, Some(10));
    }

    #[test]
    fn find_war_start_ignores_pact_defense_false_positive() {
        use crate::events::HistoryEvent;
        let mut game = test_game_with_adjacent_provinces();
        // "A declared war on B to protect C" should NOT match pair (B, C):
        // attacker=1, defender=2, protectee=3
        game.history.push((
            TurnNumber(8),
            HistoryEvent::WarDeclared {
                attacker: NationId(1),
                defender: NationId(2),
                protectee: Some(NationId(3)),
            },
        ));
        // No direct war between B (2) and C (3)
        let start = find_war_start_turn(&game, NationId(2), NationId(3));
        assert_eq!(
            start, None,
            "pact defense entry should not match (defender, protectee) pair"
        );

        // But it SHOULD match the (attacker, defender) pair
        let start_ab = find_war_start_turn(&game, NationId(1), NationId(2));
        assert_eq!(start_ab, Some(8));
    }
}
