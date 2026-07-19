//! Diplomacy screen queries and commands, plus the proposal modal.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals (error JSON strings included).

use crate::ApiError;
use crate::guards::{
    dismiss_pending_direct_diplomacy_action, pending_break_treaties, pending_grant_amount_dollars,
    reject_if_great_power_target_for_consulate, reject_if_target_in_anarchy,
};
use crate::parse::parse_treaty_type;
use domain::diplomacy::relations::{BREAK_TREATY_RELATIONS_LOSS, BREAK_TREATY_STANDING_LOSS};
use domain::events::TreatyType;
use domain::game_state::GameState;
use domain::types::*;

/// Query diplomacy screen data for a nation.
pub fn get_diplomacy_screen_data(
    game: &GameState,
    nation_id: u32,
) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    let player_standing = game
        .world
        .diplomacy
        .standing
        .get(&nid)
        .copied()
        .unwrap_or(100);
    let treasury = nation.economy.treasury.as_dollars();
    let player_is_gp = nation.nation_type == NationType::GreatPower;
    let player_already_at_war = game.world.diplomacy.is_at_war_with_anyone(nid);
    let player_in_anarchy = nation.diplomacy.is_in_anarchy;

    let relations: Vec<serde_json::Value> = game
        .world
        .nations
        .iter()
        .filter(|n| n.id != nid)
        .map(|n| {
            let rel = game.world.diplomacy.get_relation(nid, n.id);
            let score = rel.map(|r| r.score).unwrap_or(0);
            let raw_at_war = rel.map(|r| r.at_war).unwrap_or(false);
            let has_consulate = rel.map(|r| r.has_consulate).unwrap_or(false);
            let has_embassy = rel.map(|r| r.has_embassy).unwrap_or(false);
            let treaties: Vec<String> = rel
                .map(|r| {
                    r.active_treaties
                        .iter()
                        .map(|t| format!("{:?}", t))
                        .collect()
                })
                .unwrap_or_default();
            let has_nap = rel
                .map(|r| r.has_treaty(TreatyType::NonAggressionPact))
                .unwrap_or(false);
            let has_alliance = rel
                .map(|r| r.has_treaty(TreatyType::Alliance))
                .unwrap_or(false);

            // Anarchy takes precedence in the status label; otherwise either
            // side being anarchic forces "At War" presentation for the boolean
            // `at_war` flag the UI reads. `raw_at_war` remains authoritative
            // for every action-gating decision so button availability stays
            // aligned with what the backend commands will accept.
            let target_in_anarchy = n.diplomacy.is_in_anarchy;
            let display_at_war = raw_at_war || target_in_anarchy || player_in_anarchy;

            let status = if target_in_anarchy {
                "Anarchy"
            } else if display_at_war {
                "At War"
            } else if has_alliance {
                "Alliance"
            } else if has_nap {
                "NAP"
            } else {
                "Neutral"
            };

            let target_is_gp = n.nation_type == NationType::GreatPower;

            // Outgoing pending proposals (for badge display)
            let has_pending_nap = game.world.diplomacy.pending_proposals.iter().any(|p| {
                p.proposal_type == TreatyType::NonAggressionPact && p.from == nid && p.to == n.id
            });
            let has_pending_alliance =
                game.world.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == TreatyType::Alliance && p.from == nid && p.to == n.id
                });
            let has_pending_peace = game.world.diplomacy.pending_proposals.iter().any(|p| {
                p.proposal_type == TreatyType::PeaceTreaty && p.from == nid && p.to == n.id
            });

            // Incoming proposals of the same type still gate new proposals in
            // the opposite direction. Outgoing proposals are replaceable.
            let incoming_pending_nap = game.world.diplomacy.pending_proposals.iter().any(|p| {
                p.proposal_type == TreatyType::NonAggressionPact && p.from == n.id && p.to == nid
            });
            let incoming_pending_alliance =
                game.world.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == TreatyType::Alliance && p.from == n.id && p.to == nid
                });
            let incoming_pending_peace = game.world.diplomacy.pending_proposals.iter().any(|p| {
                p.proposal_type == TreatyType::PeaceTreaty && p.from == n.id && p.to == nid
            });

            // Pre-compute available actions. No diplomatic interaction is
            // possible with a nation in anarchy (card #81); every action is
            // gated on `!target_in_anarchy`. Gating uses `raw_at_war` (the
            // actual backend relation) rather than the anarchy-inflated
            // `display_at_war` so button availability never contradicts
            // what the command handlers will accept.
            let available_treasury = treasury - game.pending_diplomacy_reserved_dollars(nid);
            let queued_consulate = game.has_pending_consulate(nid, n.id);
            let queued_embassy = game.has_pending_embassy(nid, n.id);
            let queued_war = game.has_pending_war(nid, n.id);
            let queued_grant_amount_dollars = pending_grant_amount_dollars(game, nid, n.id);
            let queued_break_treaties = pending_break_treaties(game, nid, n.id);
            let pending_break_treaty_labels: Vec<String> = queued_break_treaties
                .iter()
                .map(|t| format!("{:?}", t))
                .collect();
            let breakable_treaties: Vec<String> = treaties
                .iter()
                .filter(|t| !pending_break_treaty_labels.contains(*t))
                .cloned()
                .collect();
            let can_build_consulate = !target_is_gp
                && !target_in_anarchy
                && !has_consulate
                && !queued_consulate
                && available_treasury >= 500;
            let can_build_embassy = !target_in_anarchy
                && has_consulate
                && !has_embassy
                && !queued_embassy
                && available_treasury >= 5000;
            let can_propose_nap = !target_in_anarchy
                && has_embassy
                && !raw_at_war
                && !has_nap
                && !has_alliance
                && !incoming_pending_nap
                && player_standing >= 30;
            let can_propose_alliance = !target_in_anarchy
                && has_embassy
                && !raw_at_war
                && !has_alliance
                && !incoming_pending_alliance
                && player_standing >= 30
                && player_is_gp
                && target_is_gp;
            let can_declare_war = !target_in_anarchy
                && !raw_at_war
                && !queued_war
                && game.can_project_war_against(nid, n.id);
            let can_send_grant = !target_in_anarchy
                && !raw_at_war
                && queued_grant_amount_dollars.is_none()
                && available_treasury >= 500;
            let can_break_treaty = !breakable_treaties.is_empty();
            let can_propose_peace = !target_in_anarchy && raw_at_war && !incoming_pending_peace;

            serde_json::json!({
                "nation_id": n.id.0,
                "nation_name": n.name,
                "nation_color": format!("{:?}", n.color),
                "nation_type": format!("{:?}", n.nation_type),
                "score": score,
                "at_war": display_at_war,
                "status": status,
                "treaties": treaties,
                "has_consulate": has_consulate,
                "has_embassy": has_embassy,
                "has_pending_consulate": queued_consulate,
                "has_pending_embassy": queued_embassy,
                "has_pending_war": queued_war,
                "pending_grant_amount_dollars": queued_grant_amount_dollars,
                "pending_break_treaties": pending_break_treaty_labels,
                "has_pending_nap": has_pending_nap,
                "has_pending_alliance": has_pending_alliance,
                "has_pending_peace": has_pending_peace,
                "is_in_anarchy": target_in_anarchy,
                "actions": {
                    "can_build_consulate": can_build_consulate,
                    "consulate_cost": 500,
                    "can_build_embassy": can_build_embassy,
                    "embassy_cost": 5000,
                    "can_propose_nap": can_propose_nap,
                    "can_propose_alliance": can_propose_alliance,
                    "can_declare_war": can_declare_war,
                    "can_send_grant": can_send_grant,
                    "can_break_treaty": can_break_treaty,
                    "breakable_treaties": breakable_treaties,
                    "can_propose_peace": can_propose_peace,
                },
            })
        })
        .collect();

    Ok(serde_json::json!({
        "player_standing": player_standing,
        "treasury": treasury,
        "player_already_at_war": player_already_at_war,
        "relations": relations,
    }))
}

/// Build a consulate with a target nation ($500).
pub fn build_consulate(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if game.get_nation(target).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }
    if let Some(err) = reject_if_great_power_target_for_consulate(game, target) {
        return Err(ApiError::raw(err));
    }
    if let Some(err) = reject_if_target_in_anarchy(game, target) {
        return Err(ApiError::raw(err));
    }

    // Validate actor nation exists
    if game.get_nation(nid).is_none() {
        return Err(ApiError::raw("{\"error\":\"nation not found\"}"));
    }

    // Validate treasury before committing
    let consulate_cost = Money::dollars(500);
    if game
        .get_nation(nid)
        .map(|n| n.economy.treasury.as_dollars() < consulate_cost.as_dollars())
        .unwrap_or(false)
    {
        return Err(ApiError::raw("{\"error\":\"not enough treasury\"}"));
    }

    if let Err(e) = game.queue_direct_diplomacy_action(
        domain::game_state::PendingDiplomacyAction::BuildConsulate {
            player: nid,
            target,
        },
    ) {
        return Err(ApiError::msg(e));
    }

    Ok(())
}

/// Build an embassy with a target nation ($5,000).
pub fn build_embassy(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if game.get_nation(target).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }
    if let Some(err) = reject_if_target_in_anarchy(game, target) {
        return Err(ApiError::raw(err));
    }

    // Validate actor nation exists
    if game.get_nation(nid).is_none() {
        return Err(ApiError::raw("{\"error\":\"nation not found\"}"));
    }

    // Validate treasury before committing
    let embassy_cost = Money::dollars(5000);
    if game
        .get_nation(nid)
        .map(|n| n.economy.treasury.as_dollars() < embassy_cost.as_dollars())
        .unwrap_or(false)
    {
        return Err(ApiError::raw("{\"error\":\"not enough treasury\"}"));
    }

    if let Err(e) = game.queue_direct_diplomacy_action(
        domain::game_state::PendingDiplomacyAction::BuildEmbassy {
            player: nid,
            target,
        },
    ) {
        return Err(ApiError::msg(e));
    }

    Ok(())
}

/// Propose a Non-Aggression Pact.
pub fn propose_nap(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if game.get_nation(nid).is_none() {
        return Err(ApiError::raw("{\"error\":\"nation not found\"}"));
    }
    if game.get_nation(target).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }
    if let Some(err) = reject_if_target_in_anarchy(game, target) {
        return Err(ApiError::raw(err));
    }

    let turn = game.turn;
    match game
        .world
        .diplomacy
        .propose_treaty(nid, target, TreatyType::NonAggressionPact, turn)
    {
        Ok(()) => {}
        Err(e) => return Err(ApiError::msg(e)),
    }

    Ok(())
}

/// Propose an Alliance.
pub fn propose_alliance(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if game.get_nation(nid).is_none() {
        return Err(ApiError::raw("{\"error\":\"nation not found\"}"));
    }
    if game.get_nation(target).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }
    if let Some(err) = reject_if_target_in_anarchy(game, target) {
        return Err(ApiError::raw(err));
    }

    let turn = game.turn;
    match game
        .world
        .diplomacy
        .propose_treaty(nid, target, TreatyType::Alliance, turn)
    {
        Ok(()) => {}
        Err(e) => return Err(ApiError::msg(e)),
    }

    Ok(())
}

/// Declare war on a target nation.
pub fn declare_war(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if game.get_nation(nid).is_none() {
        return Err(ApiError::raw("{\"error\":\"nation not found\"}"));
    }
    if game.get_nation(target).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }
    if let Some(err) = reject_if_target_in_anarchy(game, target) {
        return Err(ApiError::raw(err));
    }

    if let Err(e) =
        game.queue_direct_diplomacy_action(domain::game_state::PendingDiplomacyAction::DeclareWar {
            from: nid,
            to: target,
        })
    {
        return Err(ApiError::msg(e));
    }
    Ok(())
}

/// Send a monetary grant to a nation to improve relations.
pub fn send_grant(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
    amount: i64,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if amount <= 0 {
        return Err(ApiError::raw(
            "{\"error\":\"grant amount must be positive\"}",
        ));
    }

    let money = Money::dollars(amount);
    if let Err(e) =
        game.queue_direct_diplomacy_action(domain::game_state::PendingDiplomacyAction::SendGrant {
            from: nid,
            to: target,
            amount: money,
        })
    {
        return Err(ApiError::msg(e));
    }
    Ok(())
}

/// Break a treaty with a target nation.
pub fn break_treaty(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
    treaty_type: &str,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if game.get_nation(target).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }

    let tt = match parse_treaty_type(treaty_type) {
        Some(t) => t,
        None => return Err(ApiError::raw("{\"error\":\"unknown treaty type\"}")),
    };

    if let Err(e) = game.queue_direct_diplomacy_action(
        domain::game_state::PendingDiplomacyAction::BreakTreaty {
            from: nid,
            to: target,
            treaty_type: tt,
        },
    ) {
        return Err(ApiError::msg(e));
    }
    Ok(())
}

/// Propose peace to a nation currently at war.
pub fn propose_peace(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if game.get_nation(target).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }
    if let Some(err) = reject_if_target_in_anarchy(game, target) {
        return Err(ApiError::raw(err));
    }

    let turn = game.turn;

    match game.world.diplomacy.propose_peace(nid, target, turn) {
        Ok(()) => {}
        Err(e) => return Err(ApiError::msg(e)),
    }

    Ok(())
}

/// Dismiss any outgoing treaty proposal (NAP / Alliance / Peace) from the
/// requesting nation to the target nation.
pub fn dismiss_outgoing_proposal(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
) -> Result<(), ApiError> {
    let from = NationId(nation_id);
    let to = NationId(target_nation_id);

    if from == to {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if game.get_nation(from).is_none() {
        return Err(ApiError::raw("{\"error\":\"nation not found\"}"));
    }
    if game.get_nation(to).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }

    if !game
        .world
        .diplomacy
        .dismiss_outgoing_treaty_proposal(from, to)
    {
        return Err(ApiError::raw(
            "{\"error\":\"no outgoing proposal to dismiss\"}",
        ));
    }

    Ok(())
}

/// Dismiss a specific pending diplomacy action or outgoing treaty proposal.
pub fn dismiss_pending_action(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
    action_key: &str,
) -> Result<(), ApiError> {
    let from = NationId(nation_id);
    let to = NationId(target_nation_id);

    if from == to {
        return Err(ApiError::raw("{\"error\":\"cannot target self\"}"));
    }
    if game.get_nation(from).is_none() {
        return Err(ApiError::raw("{\"error\":\"nation not found\"}"));
    }
    if game.get_nation(to).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }

    let dismissed = match action_key {
        "nap" => game
            .world
            .diplomacy
            .dismiss_outgoing_treaty_proposal(from, to),
        "alliance" => game
            .world
            .diplomacy
            .dismiss_outgoing_treaty_proposal(from, to),
        "peace" => game
            .world
            .diplomacy
            .dismiss_outgoing_treaty_proposal(from, to),
        _ => dismiss_pending_direct_diplomacy_action(game, from, to, action_key),
    };

    if !dismissed {
        return Err(ApiError::raw(
            "{\"error\":\"no pending diplomacy action to dismiss\"}",
        ));
    }

    Ok(())
}

/// One-line factual consequence of accepting a proposal (decision context,
/// card #514). Standing/relations numbers are pulled from the domain's
/// [`BREAK_TREATY_STANDING_LOSS`] / [`BREAK_TREATY_RELATIONS_LOSS`]
/// constants (`break_treaty` in `crates/domain/src/diplomacy/relations.rs`),
/// so this text can't drift from the actual penalty. Alliances auto-join
/// wars, a separate peace breaks the peacemaker's alliances, and a snubbed
/// join-empire minor drops 20.
fn accept_hint(proposal_type: TreatyType, from_name: &str, attacker_name: Option<&str>) -> String {
    match proposal_type {
        TreatyType::NonAggressionPact => format!(
            "Mutual promise not to attack; breaking it later costs {BREAK_TREATY_STANDING_LOSS} standing and {BREAK_TREATY_RELATIONS_LOSS} relations."
        ),
        TreatyType::Alliance => format!(
            "Allies automatically join each other's wars; breaking it or making a separate peace costs {BREAK_TREATY_STANDING_LOSS} standing."
        ),
        TreatyType::PeaceTreaty => format!(
            "Ends your war with {from_name}; allies still fighting them will break their alliance with you."
        ),
        TreatyType::RequestToJoinEmpire => format!(
            "{from_name}'s provinces join your empire; rejecting drops their relations by 20."
        ),
        TreatyType::WarDeclaration => {
            "The war is already in effect — this notice only acknowledges it.".into()
        }
        TreatyType::PactDefenseRequest => format!(
            "Declares war on {} and brings {from_name} into your empire; rejecting passes the plea to other powers.",
            attacker_name.unwrap_or("an aggressor")
        ),
    }
}

/// Get pending diplomatic proposals for a nation, each with the decision
/// context the player needs: current relation with the proposer (score,
/// status, treaties, embassies) and what accepting means (card #514).
pub fn get_pending_proposals(
    game: &GameState,
    nation_id: u32,
) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);

    let proposals: Vec<serde_json::Value> = game
        .world
        .diplomacy
        .pending_proposals
        .iter()
        .enumerate()
        .filter(|(_, p)| p.to == nid)
        .map(|(idx, p)| {
            let from_name = game
                .get_nation(p.from)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let from_color = game
                .get_nation(p.from)
                .map(|n| format!("{:?}", n.color))
                .unwrap_or_default();

            // Relation context (same source as the diplomacy screen).
            let rel = game.world.diplomacy.get_relation(nid, p.from);
            let relation_score = rel.map(|r| r.score).unwrap_or(0);
            let at_war = rel.map(|r| r.at_war).unwrap_or(false);
            let has_consulate = rel.map(|r| r.has_consulate).unwrap_or(false);
            let has_embassy = rel.map(|r| r.has_embassy).unwrap_or(false);
            let treaties: Vec<String> = rel
                .map(|r| {
                    r.active_treaties
                        .iter()
                        .map(|t| format!("{:?}", t))
                        .collect()
                })
                .unwrap_or_default();
            let proposer_in_anarchy = game
                .get_nation(p.from)
                .map(|n| n.diplomacy.is_in_anarchy)
                .unwrap_or(false);
            let relation_status = if proposer_in_anarchy {
                "Anarchy"
            } else if at_war {
                "At War"
            } else if rel.is_some_and(|r| r.has_treaty(TreatyType::Alliance)) {
                "Alliance"
            } else if rel.is_some_and(|r| r.has_treaty(TreatyType::NonAggressionPact)) {
                "NAP"
            } else {
                "Neutral"
            };
            let attacker_name = p
                .attacker
                .and_then(|a| game.get_nation(a))
                .map(|n| n.name.as_str());

            let display_text = match p.proposal_type {
                TreatyType::NonAggressionPact => {
                    format!("{} proposes a Non-Aggression Pact", from_name)
                }
                TreatyType::Alliance => format!("{} proposes an Alliance", from_name),
                TreatyType::PeaceTreaty => format!("{} proposes Peace", from_name),
                TreatyType::RequestToJoinEmpire => {
                    format!("{} requests to join your empire", from_name)
                }
                TreatyType::WarDeclaration => {
                    format!("{} declares war", from_name)
                }
                TreatyType::PactDefenseRequest => {
                    format!(
                        "{} requests your protection against {}",
                        from_name,
                        attacker_name.unwrap_or("an aggressor")
                    )
                }
            };
            let turns_until_expiry = 4_i32 - (game.turn.0 as i32 - p.turn_proposed.0 as i32);
            serde_json::json!({
                "index": idx,
                "from_nation_id": p.from.0,
                "from_nation_name": from_name,
                "from_nation_color": from_color,
                "proposal_type": format!("{:?}", p.proposal_type),
                "display_text": display_text,
                "turn_proposed": p.turn_proposed.0,
                "turns_until_expiry": turns_until_expiry.max(0),
                "relation_score": relation_score,
                "relation_status": relation_status,
                "at_war": at_war,
                "treaties": treaties,
                "has_consulate": has_consulate,
                "has_embassy": has_embassy,
                "accept_hint": accept_hint(p.proposal_type, from_name, attacker_name),
            })
        })
        .collect();

    Ok(serde_json::json!({ "proposals": proposals }))
}

/// Accept a diplomatic proposal by index.
pub fn accept_proposal(
    game: &mut GameState,
    nation_id: u32,
    proposal_index: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let idx = proposal_index as usize;

    if idx >= game.world.diplomacy.pending_proposals.len() {
        return Err(ApiError::raw("{\"error\":\"proposal index out of range\"}"));
    }

    let proposal = game.world.diplomacy.pending_proposals[idx].clone();
    if proposal.to != nid {
        return Err(ApiError::raw(
            "{\"error\":\"proposal not addressed to you\"}",
        ));
    }

    // Execute the treaty action — propagate errors
    match proposal.proposal_type {
        TreatyType::NonAggressionPact => {
            if let Err(e) = game
                .world
                .diplomacy
                .propose_pact(proposal.from, proposal.to)
            {
                return Err(ApiError::msg(e));
            }
        }
        TreatyType::Alliance => {
            if let Err(e) = game
                .world
                .diplomacy
                .propose_alliance(proposal.from, proposal.to)
            {
                return Err(ApiError::msg(e));
            }
        }
        TreatyType::PeaceTreaty => {
            game.world.diplomacy.queue_peace(proposal.from, proposal.to);
        }
        TreatyType::PactDefenseRequest => {
            if let Some(attacker_id) = proposal.attacker {
                let mut report = domain::turn::TurnReport::empty();
                domain::turn::accept_pact_defense(
                    game,
                    nid,
                    attacker_id,
                    proposal.from,
                    &mut report,
                );
            } else {
                return Err(ApiError::raw("{\"error\":\"missing attacker context\"}"));
            }
        }
        TreatyType::RequestToJoinEmpire => {
            let mut report = domain::turn::TurnReport::empty();
            domain::turn::accept_request_to_join_empire(game, nid, proposal.from, &mut report);
        }
        TreatyType::WarDeclaration => {
            // War-declaration modal is notification-only. The war is already
            // in effect; accepting just dismisses the alert.
        }
    }

    // Remove the proposal
    game.world.diplomacy.pending_proposals.remove(idx);

    Ok(())
}

/// Reject a diplomatic proposal by index.
pub fn reject_proposal(
    game: &mut GameState,
    nation_id: u32,
    proposal_index: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let idx = proposal_index as usize;

    if idx >= game.world.diplomacy.pending_proposals.len() {
        return Err(ApiError::raw("{\"error\":\"proposal index out of range\"}"));
    }

    if game.world.diplomacy.pending_proposals[idx].to != nid {
        return Err(ApiError::raw(
            "{\"error\":\"proposal not addressed to you\"}",
        ));
    }

    let proposal = game.world.diplomacy.pending_proposals.remove(idx);

    // For PactDefenseRequest: continue the cascade with remaining candidates
    if proposal.proposal_type == TreatyType::PactDefenseRequest
        && let Some(attacker_id) = proposal.attacker
    {
        let remaining = proposal.cascade_remaining.unwrap_or_default();
        let mut report = domain::turn::TurnReport::empty();
        domain::turn::continue_pact_defense_cascade(
            game,
            attacker_id,
            proposal.from,
            &remaining,
            &mut report,
        );
    }

    // For RequestToJoinEmpire: the snubbed minor's relationship with the
    // rejecting Great Power drops sharply.
    if proposal.proposal_type == TreatyType::RequestToJoinEmpire {
        domain::turn::reject_request_to_join_empire(game, nid, proposal.from);
    }

    // WarDeclaration rejection has no extra effect — the war is already live.

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The NAP hint's penalty numbers must come from the domain's
    /// `break_treaty` constants — if someone changes
    /// `BREAK_TREATY_STANDING_LOSS` / `BREAK_TREATY_RELATIONS_LOSS` without
    /// updating this text, that drift should fail a test, not surface as a
    /// stale number in the UI.
    #[test]
    fn accept_hint_non_aggression_pact_uses_domain_penalty_constants() {
        let hint = accept_hint(TreatyType::NonAggressionPact, "Gallia", None);
        assert!(hint.contains(&format!("{BREAK_TREATY_STANDING_LOSS} standing")));
        assert!(hint.contains(&format!("{BREAK_TREATY_RELATIONS_LOSS} relations")));
    }

    #[test]
    fn accept_hint_alliance_uses_domain_standing_constant() {
        let hint = accept_hint(TreatyType::Alliance, "Gallia", None);
        assert!(hint.contains(&format!("{BREAK_TREATY_STANDING_LOSS} standing")));
    }

    /// `accept_hint` and `get_pending_proposals`'s `display_text` should
    /// agree on how an unknown attacker is described (card review: they used
    /// to disagree — "the aggressor" vs "an aggressor").
    #[test]
    fn accept_hint_pact_defense_request_default_attacker_matches_display_text() {
        let hint = accept_hint(TreatyType::PactDefenseRequest, "Gallia", None);
        assert!(hint.contains("an aggressor"));
    }
}
