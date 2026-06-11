//! Guard helpers shared by frontend API entry points.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals (error JSON strings included).

use crate::parse::parse_treaty_type;
use domain::events::TreatyType;
use domain::game_state::GameState;
use domain::types::*;

/// Returns an error JSON string if the target nation is in anarchy — no
/// diplomatic interaction (proposals, grants, declarations, peace, treaties)
/// is permitted with a country whose government has collapsed (card #81).
pub fn reject_if_target_in_anarchy(game: &GameState, target: NationId) -> Option<String> {
    if game
        .get_nation(target)
        .is_some_and(|n| n.diplomacy.is_in_anarchy)
    {
        Some("{\"error\":\"target nation is in anarchy\"}".to_string())
    } else {
        None
    }
}

pub fn reject_if_great_power_target_for_consulate(
    game: &GameState,
    target: NationId,
) -> Option<String> {
    if game.get_nation(target).is_some_and(|n| n.is_great_power()) {
        Some("{\"error\":\"Consulates are for Minor Nations only.\"}".to_string())
    } else {
        None
    }
}

/// Check if a nation has researched a tech by its display name.
pub fn nation_has_tech(
    nation: &domain::nation::Nation,
    tech_name: &str,
    game_data: &domain::data::GameData,
) -> bool {
    game_data
        .tech_tree
        .all_techs()
        .iter()
        .any(|t| t.name == tech_name && nation.researched_techs.contains(&t.id))
}

pub fn pending_break_treaties(game: &GameState, from: NationId, to: NationId) -> Vec<TreatyType> {
    game.transient
        .pending_diplomacy_actions
        .iter()
        .filter_map(|action| match action {
            domain::game_state::PendingDiplomacyAction::BreakTreaty {
                from: a,
                to: b,
                treaty_type,
            } if *a == from && *b == to => Some(*treaty_type),
            _ => None,
        })
        .collect()
}

pub fn pending_grant_amount_dollars(game: &GameState, from: NationId, to: NationId) -> Option<i64> {
    game.transient
        .pending_diplomacy_actions
        .iter()
        .find_map(|action| match action {
            domain::game_state::PendingDiplomacyAction::SendGrant {
                from: a,
                to: b,
                amount,
            } if *a == from && *b == to => Some(amount.as_dollars()),
            _ => None,
        })
}

pub fn dismiss_pending_direct_diplomacy_action(
    game: &mut GameState,
    from: NationId,
    to: NationId,
    action_key: &str,
) -> bool {
    let original_len = game.transient.pending_diplomacy_actions.len();
    game.transient.pending_diplomacy_actions.retain(|action| {
        let matches = match action {
            domain::game_state::PendingDiplomacyAction::BuildConsulate { player, target } => {
                action_key == "consulate" && *player == from && *target == to
            }
            domain::game_state::PendingDiplomacyAction::BuildEmbassy { player, target } => {
                action_key == "embassy" && *player == from && *target == to
            }
            domain::game_state::PendingDiplomacyAction::DeclareWar { from: a, to: b } => {
                action_key == "war" && *a == from && *b == to
            }
            domain::game_state::PendingDiplomacyAction::SendGrant {
                from: a,
                to: b,
                amount,
            } => {
                if *a != from || *b != to {
                    false
                } else {
                    action_key
                        .strip_prefix("grant:")
                        .and_then(|value| value.parse::<i64>().ok())
                        .is_some_and(|queued_amount| amount.as_dollars() == queued_amount)
                }
            }
            domain::game_state::PendingDiplomacyAction::BreakTreaty {
                from: a,
                to: b,
                treaty_type,
            } => {
                if *a != from || *b != to {
                    false
                } else {
                    action_key
                        .strip_prefix("break_treaty:")
                        .and_then(parse_treaty_type)
                        .is_some_and(|queued_type| queued_type == *treaty_type)
                }
            }
        };
        !matches
    });
    game.transient.pending_diplomacy_actions.len() != original_len
}
