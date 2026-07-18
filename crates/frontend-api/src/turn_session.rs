//! Interactive end-turn session driver (card #494).
//!
//! `begin_turn` runs the first half of the turn pipeline (player pending
//! diplomacy, AI decisions, diplomacy resolution, pre-trade economy) and
//! pauses with the frozen trade-offer pool stored on the [`Session`]. The
//! GUI then shows the diplomatic session (proposals answered through the
//! existing `diplomacy::accept_proposal` / `reject_proposal` commands, which
//! take effect on the paused state) and the trade session (`accept_trade`),
//! and finally calls `finish_turn` to resolve the rest of the turn.

use crate::{ApiError, Session};
use domain::types::{Money, NationId};

/// Run the first half of the turn and pause. War declarations addressed to
/// the player are auto-acknowledged here (the war is already in effect) and
/// surfaced as notification events instead of actionable proposals.
///
/// Errors if a session is already pending.
pub fn begin_turn(session: &mut Session) -> Result<(), ApiError> {
    if session.pending_turn().is_some() {
        return Err(ApiError::raw(
            r#"{"error":"a turn session is already pending"}"#,
        ));
    }
    let observer = session.observer_mode();
    let human = session.human_nation();
    let mut turn_session = domain::turn::begin_turn(session.game_mut());
    turn_session.interactive = !observer;

    // Auto-acknowledge war declarations against the player and surface them
    // as session notifications (mirrors the old post-turn auto-ack flow).
    if !observer {
        let war_texts = acknowledge_war_declarations(session, human)?;
        for text in war_texts {
            turn_session.diplo_events.insert(
                0,
                domain::turn::DiploSessionEvent {
                    text,
                    category: domain::events::HeadlineCategory::War,
                    nation_ids: vec![NationId(human)],
                },
            );
        }
    }

    *session.pending_turn_mut() = Some(turn_session);
    Ok(())
}

/// Accept war-declaration proposals addressed to `nation`, returning the
/// display texts of the acknowledged declarations.
fn acknowledge_war_declarations(
    session: &mut Session,
    nation: u32,
) -> Result<Vec<String>, ApiError> {
    let nid = NationId(nation);
    let mut texts = Vec::new();
    loop {
        let decl = session
            .game()
            .world
            .diplomacy
            .pending_proposals
            .iter()
            .enumerate()
            .find(|(_, p)| {
                p.to == nid && p.proposal_type == domain::events::TreatyType::WarDeclaration
            })
            .map(|(idx, p)| (idx, p.from));
        let Some((idx, from)) = decl else {
            break;
        };
        let from_name = session
            .game()
            .get_nation(from)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".into());
        crate::diplomacy::accept_proposal(session.game_mut(), nation, idx as u32)?;
        texts.push(format!("{from_name} has declared war on you!"));
    }
    Ok(texts)
}

/// The session view for the GUI: diplomatic events + pending proposals for
/// the diplomatic session, wishlist offers + budget/cargo state for the
/// trade session.
pub fn session_view(session: &Session) -> Result<serde_json::Value, ApiError> {
    let Some(turn_session) = session.pending_turn() else {
        return Err(ApiError::raw(r#"{"error":"no pending turn session"}"#));
    };
    let game = session.game();
    let human = session.human_nation();

    let diplo_events: Vec<serde_json::Value> = turn_session
        .diplo_events
        .iter()
        .map(|e| {
            serde_json::json!({
                "text": e.text,
                "category": format!("{:?}", e.category),
                "nation_ids": e.nation_ids.iter().map(|n| n.0).collect::<Vec<_>>(),
            })
        })
        .collect();

    let proposals = crate::diplomacy::get_pending_proposals(game, human)?;

    let offers: Vec<serde_json::Value> = turn_session
        .offers_for_player(game)
        .iter()
        .map(|o| {
            let seller_name = game
                .get_nation(o.seller)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            serde_json::json!({
                "seller_id": o.seller.0,
                "seller_name": seller_name,
                "resource": format!("{:?}", o.resource),
                "remaining": o.remaining,
                "price": o.price_per_unit.as_dollars(),
                "relation_score": o.relation_score,
            })
        })
        .collect();

    let treasury = game
        .get_nation(NationId(human))
        .map(|n| n.economy.treasury)
        .unwrap_or(Money::ZERO);

    let accepted: Vec<serde_json::Value> = turn_session
        .accepted
        .iter()
        .map(|a| {
            let seller_name = game
                .get_nation(a.seller)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            serde_json::json!({
                "seller_id": a.seller.0,
                "seller_name": seller_name,
                "resource": format!("{:?}", a.resource),
                "quantity": a.quantity,
                "price": a.price_per_unit.as_dollars(),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "observer": session.observer_mode(),
        "diplo_events": diplo_events,
        "proposals": proposals["proposals"],
        "offers": offers,
        "accepted": accepted,
        "treasury": treasury.as_dollars(),
        "money_committed": turn_session.money_committed().as_dollars(),
        "cargo_capacity": turn_session.player_cargo_capacity,
        "cargo_committed": turn_session.cargo_committed(),
    }))
}

/// Accept a trade offer during the session. Returns the quantity actually
/// accepted (clamped to remaining offer, cargo, and treasury).
pub fn accept_trade(
    session: &mut Session,
    seller_id: u32,
    resource_name: &str,
    quantity: u32,
) -> Result<u32, ApiError> {
    if session.observer_mode() {
        return Err(ApiError::raw(r#"{"error":"observer games cannot trade"}"#));
    }
    let resource = match crate::parse::parse_resource_type(resource_name) {
        Some(r) => r,
        None => return Err(ApiError::raw(r#"{"error":"invalid resource"}"#)),
    };
    // Split borrow: validate against an immutable game snapshot reference
    // while mutating the session state.
    let mut pending = session.pending_turn_mut().take();
    let result = match pending.as_mut() {
        Some(turn_session) => turn_session
            .accept_trade(session.game(), NationId(seller_id), resource, quantity)
            .map_err(|e| ApiError::json(format!("{{\"error\":\"{e}\"}}"))),
        None => Err(ApiError::raw(r#"{"error":"no pending turn session"}"#)),
    };
    *session.pending_turn_mut() = pending;
    result
}

/// Run the second half of the turn using the paused session's decisions.
/// Returns the same report JSON shape as `turn::process_turn`.
pub fn finish_turn(session: &mut Session) -> Result<serde_json::Value, ApiError> {
    let Some(turn_session) = session.pending_turn_mut().take() else {
        return Err(ApiError::raw(r#"{"error":"no pending turn session"}"#));
    };
    let report = domain::turn::finish_turn(session.game_mut(), turn_session);
    Ok(crate::turn::report_json(&report, session.game()))
}
