//! Newspaper headline generation phase.
//!
//! Produces the per-turn newspaper from the turn report's accumulated AI
//! actions, incorporations, trade activity, and flavor entries. This is the
//! first phase extracted from `turn::processor` as part of the C-1 split.
//! Future PRs will move additional phase modules (economy, military,
//! diplomacy, civilian) alongside this one.

use crate::events::{Headline, HeadlineCategory};
use crate::game_state::GameState;
use crate::turn::processor::TurnReport;

pub(super) fn generate_newspaper(game: &GameState, report: &mut TurnReport) {
    let year = game.turn.year();
    let quarter = game.turn.quarter();

    report.newspaper_headlines.push(Headline::new(
        format!("The Imperial Times - {year} Q{quarter}"),
        HeadlineCategory::Default,
    ));

    // AI actions (tech research, military buildup, war declarations).
    // Non-actions ("considered but declined") flow through with is_non_action=true
    // so the UI can filter them behind a debug toggle.
    for action in &report.ai_actions {
        let category = if action.text.contains("declared war on")
            || action.text.contains("did not declare war")
            || action.text.contains("held back from war")
        {
            HeadlineCategory::War
        } else {
            HeadlineCategory::Default
        };
        let headline = if action.is_non_action {
            Headline::non_action(action.text.clone(), category, action.reason.clone())
        } else {
            Headline::with_reason(action.text.clone(), category, action.reason.clone())
        };
        report
            .newspaper_headlines
            .push(headline.for_nation(action.nation_id));
    }

    // Voluntary incorporations — major headline
    for (minor_id, gp_id) in &report.incorporations {
        let minor_name = game
            .get_nation(*minor_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let gp_name = game
            .get_nation(*gp_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        report.newspaper_headlines.push(
            Headline::new(
                format!(
                    "BREAKING: {} has voluntarily joined the {} empire!",
                    minor_name, gp_name
                ),
                HeadlineCategory::Politics,
            )
            .for_nations(&[*gp_id, *minor_id]),
        );
    }

    // Unit upgrades — brief mention
    if !report.unit_upgrades.is_empty() {
        let upgrade_count = report.unit_upgrades.len();
        report.newspaper_headlines.push(Headline::new(
            format!(
                "Military modernization: {} unit{} upgraded across the nations",
                upgrade_count,
                if upgrade_count == 1 { "" } else { "s" }
            ),
            HeadlineCategory::Military,
        ));
    }

    // Trade activity headline for the human player
    if !report.trade_transactions.is_empty()
        && let Some(human_nation) = game.get_nation(game.human_player_nation)
    {
        let human_traded = report
            .trade_transactions
            .iter()
            .any(|txn| txn.buyer == game.human_player_nation);
        if human_traded {
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "Trade flourishes between {} and its partners",
                        human_nation.name
                    ),
                    HeadlineCategory::Trade,
                )
                .for_nation(game.human_player_nation),
            );
        }
    }

    if let Some(human_nation) = game.get_nation(game.human_player_nation) {
        report.newspaper_headlines.push(
            Headline::new(
                format!("The {} empire grows stronger", human_nation.name),
                HeadlineCategory::Default,
            )
            .for_nation(game.human_player_nation),
        );
    }

    if game.turn.is_decade_election() {
        report.newspaper_headlines.push(Headline::new(
            "Council of Governors to convene!".to_string(),
            HeadlineCategory::Politics,
        ));
    }

    // Human player anarchy game-over notice
    if game
        .get_nation(game.human_player_nation)
        .is_some_and(|n| n.is_in_anarchy)
    {
        report.newspaper_headlines.push(
            Headline::new(
                "Your nation has fallen into anarchy! All governance has ceased.".to_string(),
                HeadlineCategory::Crisis,
            )
            .for_nation(game.human_player_nation),
        );
    }

    // Period-appropriate flavor headlines that rotate based on turn number
    let flavor_headlines = [
        "Railroad expansion continues across the continent",
        "Industrial production reaches new heights",
        "Diplomatic tensions simmer between the Great Powers",
        "Colonial ambitions drive the Great Powers forward",
        "New trade routes open promising opportunities",
        "The age of progress marches ever onward",
        "Rumors of unrest in the frontier provinces",
        "Great exhibitions showcase industrial might",
    ];
    let flavor_index = (game.turn.0 as usize) % flavor_headlines.len();
    report.newspaper_headlines.push(Headline::new(
        flavor_headlines[flavor_index].to_string(),
        HeadlineCategory::Default,
    ));
}
