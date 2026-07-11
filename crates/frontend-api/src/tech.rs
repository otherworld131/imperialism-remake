//! Technology screen queries and commands.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals (error JSON strings included).

use crate::ApiError;
use domain::game_state::GameState;

/// Get available technologies for the human player.
pub fn get_available_techs(game: &GameState) -> Result<serde_json::Value, ApiError> {
    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return Ok(serde_json::Value::Array(Vec::new())),
    };

    let available = game
        .game_data
        .tech_tree
        .available_techs(&nation.researched_techs, game.turn.year());

    let techs: Vec<serde_json::Value> = available
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id.0,
                "name": t.name,
                "cost": t.cost.as_dollars(),
            })
        })
        .collect();

    Ok(serde_json::Value::Array(techs))
}

/// Research a technology immediately (deducts cost and applies in one call).
/// This is a direct / scripting path — it bypasses the queued end-of-turn model
/// used by the Tech screen. Prefer `queue_tech_research` for human-player UI.
pub fn research_tech(game: &mut GameState, tech_name: &str) -> Result<(), ApiError> {
    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"player not found\"}")),
    };

    let lower = tech_name.to_lowercase();
    let tech = game
        .game_data
        .tech_tree
        .available_techs(&nation.researched_techs, game.turn.year())
        .into_iter()
        .find(|t| t.name.to_lowercase() == lower);

    match tech {
        Some(t) => {
            let cost = t.cost;
            let tech_id = t.id;
            let current_year = game.turn.year();
            let nation = match game.get_nation_mut(game.human_player_nation) {
                Some(n) => n,
                None => return Err(ApiError::raw("{\"error\":\"player nation not found\"}")),
            };
            if nation.economy.treasury.checked_sub(cost).is_none() {
                return Err(ApiError::raw("{\"error\":\"insufficient funds\"}"));
            }
            nation.economy.treasury -= cost;
            nation.research_tech_in_year(tech_id, current_year);
            Ok(())
        }
        None => Err(ApiError::raw(format!(
            "{{\"error\":\"tech not found: {}\" }}",
            tech_name
        ))),
    }
}

fn tech_description(effects: &[domain::tech::tree::TechEffect]) -> String {
    use domain::tech::tree::TechEffect;
    let mut parts: Vec<String> = Vec::new();
    for effect in effects {
        let s = match effect {
            TechEffect::EnableInfrastructure(name) => format!("Enables {} construction", name),
            TechEffect::EnableTerrainImprovement { terrain, max_level } => {
                format!("Improves {} to level {}", terrain, max_level)
            }
            TechEffect::UnlockBuilding(b) => format!("Unlocks {}", b),
            TechEffect::UnlockShip(name) => format!("Unlocks {} ships", name),
            TechEffect::UnlockUnit(u) => format!("Unlocks {} units", u),
            TechEffect::UpgradeUnit { from, to } => format!("Upgrades {} to {}", from, to),
            TechEffect::EnableCivilian(c) => format!("Enables {} workers", c),
            TechEffect::LuaScript(_) => continue,
        };
        parts.push(s);
    }
    parts.join("; ")
}

/// Get technology screen data for the human player.
///
/// Returns a JSON object with:
/// - `available`: techs the player can queue for research (year window + prereqs met)
/// - `researched`: techs already acquired, each with the year it was researched
/// - `pending`: the tech currently queued for end-of-turn, or null
/// - `treasury`: current treasury in dollars
pub fn get_tech_screen_data(game: &GameState) -> Result<serde_json::Value, ApiError> {
    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"player not found\"}")),
    };

    let year = game.turn.year();
    let treasury = nation.economy.treasury.as_dollars();

    let available: Vec<serde_json::Value> = game
        .game_data
        .tech_tree
        .available_techs(&nation.researched_techs, year)
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id.0,
                "name": t.name,
                "cost": t.cost.as_dollars(),
                "earliest_year": t.earliest_year,
                "latest_year": t.latest_year,
                "description": tech_description(&t.effects),
            })
        })
        .collect();

    let researched: Vec<serde_json::Value> = nation
        .researched_techs
        .iter()
        .enumerate()
        .map(|(i, tid)| {
            let year_researched = nation.researched_tech_years.get(i).copied().unwrap_or(0);
            let (name, description) = game
                .game_data
                .tech_tree
                .get(*tid)
                .map(|t| (t.name.as_str(), tech_description(&t.effects)))
                .unwrap_or(("Unknown", String::new()));
            serde_json::json!({
                "id": tid.0,
                "name": name,
                "year": year_researched,
                "description": description,
            })
        })
        .collect();

    let pending = nation.pending_tech_research.and_then(|tid| {
        game.game_data.tech_tree.get(tid).map(|t| {
            serde_json::json!({
                "id": tid.0,
                "name": t.name,
                "cost": t.cost.as_dollars(),
                "description": tech_description(&t.effects),
            })
        })
    });

    // Full timeline: every tech in the tree, ordered by availability year,
    // so the screen can show adopted / available / future in one view.
    let mut all: Vec<&domain::tech::tree::Technology> =
        game.game_data.tech_tree.all_techs().iter().collect();
    all.sort_by(|a, b| {
        a.earliest_year
            .cmp(&b.earliest_year)
            .then_with(|| a.name.cmp(&b.name))
    });
    let timeline: Vec<serde_json::Value> = all
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id.0,
                "name": t.name,
                "cost": t.cost.as_dollars(),
                "earliest_year": t.earliest_year,
                "latest_year": t.latest_year,
                "description": tech_description(&t.effects),
            })
        })
        .collect();

    let result = serde_json::json!({
        "available": available,
        "researched": researched,
        "pending": pending,
        "treasury": treasury,
        "timeline": timeline,
    });
    Ok(result)
}

/// Queue a technology for research at the next end-of-turn.
/// The cost is NOT deducted immediately — it is validated and deducted by the turn processor.
pub fn queue_tech_research(game: &mut GameState, tech_name: &str) -> Result<(), ApiError> {
    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"player not found\"}")),
    };

    let lower = tech_name.to_lowercase();
    let available = game
        .game_data
        .tech_tree
        .available_techs(&nation.researched_techs, game.turn.year());
    let tech = available
        .into_iter()
        .find(|t| t.name.to_lowercase() == lower);

    match tech {
        Some(t) => {
            let tech_id = t.id;
            if let Some(n) = game.get_nation_mut(game.human_player_nation) {
                n.pending_tech_research = Some(tech_id);
            }
            Ok(())
        }
        None => Err(ApiError::raw(format!(
            "{{\"error\":\"tech not available: {}\" }}",
            tech_name
        ))),
    }
}

/// Cancel any pending tech research queued for end-of-turn.
pub fn cancel_tech_research(game: &mut GameState) -> Result<(), ApiError> {
    if let Some(n) = game.get_nation_mut(game.human_player_nation) {
        n.pending_tech_research = None;
    }
    Ok(())
}
