#![deny(warnings, clippy::all)]
//! Native frontend API — the single source of truth for every view model
//! and player command shared by the web (wasm-bridge) and native (Bevy)
//! frontends.
//!
//! Queries take `&GameState` and return `serde_json::Value` view models;
//! commands take `&mut GameState` and mutate pending/queued state (nothing
//! resolves before end turn). All bodies were moved verbatim from
//! `crates/wasm-bridge/src/lib.rs`; the golden contract test in that crate
//! pins the JSON byte-compatibility of the move.

pub mod battles;
pub mod diplomacy;
pub mod flavor;
pub mod industry;
pub mod ledger;
pub mod map;
pub mod newspaper;
pub mod session;
pub mod setup;
pub mod tech;
pub mod trade;
pub mod transport;
pub mod turn;
pub mod turn_session;
pub mod units;

pub mod guards;
pub mod parse;

use domain::game_state::GameState;
use domain_snapshot::game_state::GameState as SnapshotGameState;
use infrastructure::data_loader::load_embedded_game_data;

pub use session::Session;

/// A frontend-facing error carrying the *verbatim* error JSON the legacy
/// wasm exports produced (e.g. `{"error":"nation not found"}`). The wasm
/// wrappers return `.0` unchanged, which keeps the React error contract
/// byte-identical; native callers use [`ApiError::message`] for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError(pub String);

impl ApiError {
    /// Legacy `format!("{{\"error\":\"{}\"}}", e)` style (no JSON escaping —
    /// preserved on purpose for byte-compatibility).
    pub fn msg(m: impl std::fmt::Display) -> Self {
        ApiError(format!("{{\"error\":\"{}\"}}", m))
    }

    /// Legacy `serde_json::json!({"error": e}).to_string()` style (escaped).
    pub fn json(m: impl std::fmt::Display) -> Self {
        ApiError(serde_json::json!({ "error": m.to_string() }).to_string())
    }

    /// A verbatim pre-built error JSON string.
    pub fn raw(s: impl Into<String>) -> Self {
        ApiError(s.into())
    }

    /// Best-effort human-readable message for native UIs.
    pub fn message(&self) -> String {
        serde_json::from_str::<serde_json::Value>(&self.0)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or_else(|| self.0.clone())
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ApiError {}

// ── Game (de)serialization shared by wasm wrappers and Session ──────────

pub fn game_to_json(game: &GameState) -> String {
    let snap: SnapshotGameState = game.into();
    serde_json::to_string(&snap).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

pub fn game_to_value(game: &GameState) -> serde_json::Value {
    let snap: SnapshotGameState = game.into();
    serde_json::to_value(&snap).unwrap_or(serde_json::Value::Null)
}

pub fn game_from_json(json: &str) -> Result<GameState, ApiError> {
    let snap: SnapshotGameState =
        serde_json::from_str(json).map_err(|e| ApiError::json(format!("deserialize: {e}")))?;
    let mut game: GameState = snap.into();
    game.game_data = load_embedded_game_data();
    Ok(game)
}
