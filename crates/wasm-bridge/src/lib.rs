use wasm_bindgen::prelude::*;

use domain::game_state::{new_game, GameState};
use domain::scenarios::{list_scenarios, new_scenario_game};
use domain::turn::process_turn;
use domain::types::*;

/// Create a new game. Returns JSON string of the full game state.
#[wasm_bindgen]
pub fn wasm_new_game(map_key: &str, difficulty: u8, nation_index: usize) -> String {
    let diff = match difficulty {
        0 => Difficulty::Introductory,
        1 => Difficulty::Easy,
        2 => Difficulty::Normal,
        3 => Difficulty::Hard,
        4 => Difficulty::NighOnImpossible,
        _ => Difficulty::Normal,
    };
    let game = new_game(map_key, diff, nation_index);
    serde_json::to_string(&game).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Create a new game from a historical scenario.
#[wasm_bindgen]
pub fn wasm_new_scenario_game(scenario_id: &str, difficulty: u8, nation_index: usize) -> String {
    let diff = match difficulty {
        0 => Difficulty::Introductory,
        1 => Difficulty::Easy,
        2 => Difficulty::Normal,
        3 => Difficulty::Hard,
        _ => Difficulty::Normal,
    };
    match new_scenario_game(scenario_id, diff, nation_index) {
        Ok(game) => serde_json::to_string(&game).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e)),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    }
}

/// Process one turn. Accepts game state JSON, returns JSON with updated state + turn report.
#[wasm_bindgen]
pub fn wasm_process_turn(game_json: &str) -> String {
    let mut game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"deserialize: {}\"}}", e),
    };

    // Reconstruct tech tree (skipped in serialization)
    game.tech_tree = domain::tech::TechTree::new();

    let report = process_turn(&mut game);

    // Build response with game state + report summary
    let response = serde_json::json!({
        "game": serde_json::to_value(&game).unwrap_or_default(),
        "report": {
            "turn": format!("{}", report.turn),
            "year": report.year,
            "quarter": report.quarter,
            "headlines": report.newspaper_headlines,
            "resources": report.resource_production.iter()
                .filter(|(nid, _, _)| *nid == game.human_player_nation)
                .map(|(_, r, q)| serde_json::json!({"resource": format!("{:?}", r), "quantity": q}))
                .collect::<Vec<_>>(),
            "trade": report.trade_transactions.iter()
                .map(|t| serde_json::json!({
                    "resource": format!("{:?}", t.resource),
                    "quantity": t.quantity,
                    "cost": t.total_cost.as_dollars(),
                }))
                .collect::<Vec<_>>(),
            "battles": report.battles.iter()
                .map(|b| serde_json::json!({
                    "attacker_won": b.attacker_won,
                    "attacker_casualties": b.attacker_casualties,
                    "defender_casualties": b.defender_casualties,
                }))
                .collect::<Vec<_>>(),
            "scores": report.scores,
        }
    });

    response.to_string()
}

/// Get map data for rendering. Returns JSON array of tile objects.
#[wasm_bindgen]
pub fn wasm_get_map_data(game_json: &str) -> String {
    let game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    // Build province→nation lookup
    let mut province_nation: std::collections::HashMap<ProvinceId, (String, String)> =
        std::collections::HashMap::new();
    for nation in &game.nations {
        for pid in &nation.province_ids {
            province_nation.insert(*pid, (nation.name.clone(), format!("{:?}", nation.color)));
        }
    }

    let tiles: Vec<serde_json::Value> = game
        .hex_map
        .all_tiles()
        .map(|(coord, tile)| {
            let (owner_name, owner_color) = tile
                .province_id
                .and_then(|pid| province_nation.get(&pid))
                .map(|(n, c)| (n.as_str(), c.as_str()))
                .unwrap_or(("", ""));

            let province_name = tile
                .province_id
                .and_then(|pid| game.get_province(pid))
                .map(|p| p.name.as_str())
                .unwrap_or("");

            serde_json::json!({
                "q": coord.q,
                "r": coord.r,
                "terrain": format!("{:?}", tile.terrain()),
                "is_capital": tile.is_capital,
                "improvement_level": tile.improvement_level(),
                "owner": owner_name,
                "owner_color": owner_color,
                "province": province_name,
                "has_railroad": tile.infrastructure.has_railroad,
                "has_depot": tile.infrastructure.has_depot,
                "has_port": tile.infrastructure.has_port,
                "has_fort": tile.infrastructure.has_fort,
                "fort_level": tile.infrastructure.fort_level,
            })
        })
        .collect();

    serde_json::to_string(&tiles).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Get available technologies for the human player.
#[wasm_bindgen]
pub fn wasm_get_available_techs(game_json: &str) -> String {
    let mut game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };
    game.tech_tree = domain::tech::TechTree::new();

    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return "[]".to_string(),
    };

    let available = game
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

    serde_json::to_string(&techs).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Research a technology by name.
#[wasm_bindgen]
pub fn wasm_research_tech(game_json: &str, tech_name: &str) -> String {
    let mut game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };
    game.tech_tree = domain::tech::TechTree::new();

    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return format!("{{\"error\":\"player not found\"}}"),
    };

    let lower = tech_name.to_lowercase();
    let tech = game
        .tech_tree
        .available_techs(&nation.researched_techs, game.turn.year())
        .into_iter()
        .find(|t| t.name.to_lowercase().contains(&lower));

    match tech {
        Some(t) => {
            let cost = t.cost;
            let tech_id = t.id;
            let nation = game.get_nation_mut(game.human_player_nation).unwrap();
            if nation.treasury.checked_sub(cost).is_none() {
                return format!("{{\"error\":\"insufficient funds\"}}");
            }
            nation.treasury -= cost;
            nation.research_tech(tech_id);
            serde_json::to_string(&game).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
        }
        None => format!("{{\"error\":\"tech not found: {}\" }}", tech_name),
    }
}

/// Get list of available scenarios.
#[wasm_bindgen]
pub fn wasm_get_scenarios() -> String {
    let scenarios: Vec<serde_json::Value> = list_scenarios()
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "name": s.name,
                "year": s.year,
                "description": s.description,
                "great_powers": s.great_powers,
            })
        })
        .collect();

    serde_json::to_string(&scenarios).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}
