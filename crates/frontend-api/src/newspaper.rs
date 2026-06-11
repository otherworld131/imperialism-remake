//! Newspaper archive queries.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals (error JSON strings included).

use crate::ApiError;
use domain::game_state::GameState;

/// Return the newspaper headline archive for all past turns.
pub fn get_newspaper_archive(game: &GameState) -> Result<serde_json::Value, ApiError> {
    get_newspaper_archive_since(game, 0)
}

/// Return newspaper archive entries after `after_turn`.
///
/// This lets the frontend refresh an already-loaded archive incrementally
/// instead of reserializing the full archive each time the newspaper opens.
pub fn get_newspaper_archive_since(
    game: &GameState,
    after_turn: u32,
) -> Result<serde_json::Value, ApiError> {
    let archive: Vec<serde_json::Value> = game
        .archive.newspaper_archive
        .iter()
        .filter(|(turn, _)| turn.0 > after_turn)
        .map(|(turn, headlines)| {
            let items: Vec<serde_json::Value> = headlines
                .iter()
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
                .collect();
            serde_json::json!({
                "turn": turn.0,
                "year": turn.year(),
                "quarter": turn.quarter(),
                "headlines": items,
            })
        })
        .collect();

    Ok(serde_json::Value::Array(archive))
}
