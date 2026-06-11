//! End-turn processing: advance the game one or more turns and summarize
//! the per-turn reports for the frontend.

use crate::battles::{serialize_battle, serialize_naval_battle};
use domain::game_state::GameState;

/// Process N turns in a row. Clamped to 1..=50.
/// Returns JSON `{reports, stopped_early}` where `reports` is an array of
/// per-turn report summaries in chronological order. The wasm wrapper
/// reattaches the serialized game under a `"game"` key.
pub fn process_turns(game: &mut GameState, count: u32) -> serde_json::Value {
    let n = count.clamp(1, 50);
    let mut reports: Vec<serde_json::Value> = Vec::with_capacity(n as usize);
    let mut stopped_early = false;

    for _ in 0..n {
        if game.is_game_over() {
            stopped_early = true;
            break;
        }
        let report = domain::turn::process_turn(game);
        let entry = serde_json::json!({
            "turn": format!("{}", report.turn),
            "year": report.year,
            "quarter": report.quarter,
            "headlines": report.newspaper_headlines.iter()
                .map(|h| {
                    let mut obj = serde_json::json!({"text": &h.text, "category": format!("{:?}", h.category)});
                    if let Some(ref reason) = h.reason {
                        obj["reason"] = serde_json::json!(reason);
                    }
                    if h.is_non_action {
                        obj["is_non_action"] = serde_json::json!(true);
                    }
                    if !h.nation_ids.is_empty() {
                        obj["nation_ids"] = serde_json::json!(h.nation_ids.iter().map(|id| id.0).collect::<Vec<_>>());
                    }
                    obj
                })
                .collect::<Vec<_>>(),
            "battles": report.battles.iter()
                .map(|b| serialize_battle(b, game))
                .collect::<Vec<_>>(),
            "naval_battles": report.naval_battles.iter()
                .map(|nb| serialize_naval_battle(nb, game))
                .collect::<Vec<_>>(),
            "scores": report.scores.iter().map(|(id, name, score)| serde_json::json!({"nation_id": id.0, "name": name, "score": score})).collect::<Vec<_>>(),
        });
        reports.push(entry);
    }

    serde_json::json!({
        "reports": reports,
        "stopped_early": stopped_early,
    })
}

/// Process one turn. Returns JSON `{report: {...}}` summarizing the turn.
/// The wasm wrapper splices the serialized game in under a top-level
/// `"game"` key alongside `"report"`.
pub fn process_turn(game: &mut GameState) -> serde_json::Value {
    let report = domain::turn::process_turn(game);

    // Build response with the report summary
    serde_json::json!({
        "report": {
            "turn": format!("{}", report.turn),
            "year": report.year,
            "quarter": report.quarter,
            "headlines": report.newspaper_headlines.iter()
                .map(|h| {
                    let mut obj = serde_json::json!({"text": &h.text, "category": format!("{:?}", h.category)});
                    if let Some(ref reason) = h.reason {
                        obj["reason"] = serde_json::json!(reason);
                    }
                    if h.is_non_action {
                        obj["is_non_action"] = serde_json::json!(true);
                    }
                    if !h.nation_ids.is_empty() {
                        obj["nation_ids"] = serde_json::json!(h.nation_ids.iter().map(|id| id.0).collect::<Vec<_>>());
                    }
                    obj
                })
                .collect::<Vec<_>>(),
            "resources": report.resource_production.iter()
                .filter(|(nid, _, _)| *nid == game.human_player_nation)
                .map(|(_, r, q)| serde_json::json!({"resource": format!("{:?}", r), "quantity": q}))
                .collect::<Vec<_>>(),
            "trade": report.trade_transactions.iter()
                .map(|t| serde_json::json!({
                    "resource": t.commodity.to_string(),
                    "quantity": t.quantity,
                    "cost": t.total_cost.as_dollars(),
                }))
                .collect::<Vec<_>>(),
            "battles": report.battles.iter()
                .map(|b| serialize_battle(b, game))
                .collect::<Vec<_>>(),
            "naval_battles": report.naval_battles.iter()
                .map(|nb| serialize_naval_battle(nb, game))
                .collect::<Vec<_>>(),
            "scores": report.scores.iter().map(|(id, name, score)| serde_json::json!({"nation_id": id.0, "name": name, "score": score})).collect::<Vec<_>>(),
        }
    })
}
