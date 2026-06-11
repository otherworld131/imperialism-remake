//! Game creation, scenario listing, and pre-game configuration.

use crate::ApiError;
use domain::ai::common::{AiPersonality, personality_for_nation_index};
use domain::game_state::{
    GameState, new_game_with_data_and_config_and_capital_override,
    new_observer_game_with_data_and_config,
};
use domain::hex::HexCoord;
use domain::map::{MapGenConfig, TerrainMix};
use domain::scenarios::{
    list_scenarios, new_scenario_game_with_data, new_scenario_game_with_data_and_capital_override,
};
use domain::types::*;
use infrastructure::data_loader::load_embedded_game_data;

/// Build a `MapGenConfig` from raw frontend values, clamped to safe ranges.
///
/// `terrain_json` is a JSON object with optional fields matching `TerrainMix`
/// (snake_case). Empty string or invalid JSON falls back to the default mix,
/// so older frontends keep working without changes.
pub fn build_map_config(
    map_width: i32,
    map_height: i32,
    num_great_powers: u32,
    num_minor_nations: u32,
    terrain_json: &str,
) -> MapGenConfig {
    MapGenConfig {
        width: map_width.clamp(30, 200),
        height: map_height.clamp(20, 150),
        num_great_powers: (num_great_powers as usize).clamp(1, 20),
        num_minor_nations: (num_minor_nations as usize).min(32),
        terrain: parse_terrain_mix(terrain_json),
    }
}

/// Parse a JSON `TerrainMix` patch from the frontend. Missing fields fall
/// back to the default mix so the caller can omit anything they don't want
/// to override. Returns the default mix on any parse error.
pub fn parse_terrain_mix(json: &str) -> TerrainMix {
    if json.trim().is_empty() {
        return TerrainMix::default();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return TerrainMix::default();
    };
    let mut mix = TerrainMix::default();
    let Some(obj) = value.as_object() else {
        return mix;
    };
    let f32_field = |key: &str, fallback: f32| -> f32 {
        obj.get(key)
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(fallback)
    };
    let i32_field = |key: &str, fallback: i32| -> i32 {
        obj.get(key)
            .and_then(|v| v.as_f64())
            .map(|v| v as i32)
            .unwrap_or(fallback)
    };
    mix.grassland = f32_field("grassland", mix.grassland).max(0.0);
    mix.forest = f32_field("forest", mix.forest).max(0.0);
    mix.hills = f32_field("hills", mix.hills).max(0.0);
    mix.mountain = f32_field("mountain", mix.mountain).max(0.0);
    mix.desert = f32_field("desert", mix.desert).max(0.0);
    mix.swamp = f32_field("swamp", mix.swamp).max(0.0);
    mix.tundra = f32_field("tundra", mix.tundra).max(0.0);
    mix.forest_cluster = i32_field("forest_cluster", mix.forest_cluster).clamp(0, 100);
    mix.hills_cluster = i32_field("hills_cluster", mix.hills_cluster).clamp(0, 100);
    mix.mountain_cluster = i32_field("mountain_cluster", mix.mountain_cluster).clamp(0, 100);
    mix.desert_cluster = i32_field("desert_cluster", mix.desert_cluster).clamp(0, 100);
    mix.swamp_cluster = i32_field("swamp_cluster", mix.swamp_cluster).clamp(0, 100);
    mix.pole_tundra_strength =
        f32_field("pole_tundra_strength", mix.pole_tundra_strength).clamp(0.0, 1.0);
    mix.sea_hard_margin = i32_field("sea_hard_margin", mix.sea_hard_margin).clamp(0, 10);
    // Falloff radius must always exceed the hard margin or there's no soft band.
    let falloff =
        i32_field("sea_falloff_radius", mix.sea_falloff_radius).clamp(mix.sea_hard_margin + 1, 30);
    mix.sea_falloff_radius = falloff;
    mix.land_amount = f32_field("land_amount", mix.land_amount).clamp(0.1, 4.0);
    mix.river_source_percent =
        i32_field("river_source_percent", mix.river_source_percent).clamp(0, 100);
    mix
}

/// Debug aid: returns the value of `game_config.debug_marker` from the
/// build-time-baked Lua data. Edit `scripts/config/game.lua`'s
/// `debug_marker = "..."` line, rebuild WASM, and call this from the JS
/// console to confirm the Lua → WASM pipeline is live without poking
/// gameplay numbers.
pub fn debug_marker() -> String {
    load_embedded_game_data().game_config.debug_marker
}

pub fn max_workers_supportable(grain: u32, fruit: u32, meat: u32) -> u32 {
    domain::economy::labor::max_workers_supportable(grain, fruit, meat)
}

/// Create a new game.
/// `flavor_key` seeds names/flags; pass an empty string to reuse `map_key`.
/// `terrain_json` is an optional JSON object overriding fields of `TerrainMix`
/// (snake_case keys). Empty string = use the default mix.
#[allow(clippy::too_many_arguments)]
pub fn new_game(
    map_key: &str,
    difficulty: u8,
    nation_index: usize,
    map_width: i32,
    map_height: i32,
    num_great_powers: u32,
    num_minor_nations: u32,
    flavor_key: &str,
    terrain_json: &str,
    capital_override: Option<HexCoord>,
) -> GameState {
    let diff = difficulty_from_u8(difficulty);
    let cfg = build_map_config(
        map_width,
        map_height,
        num_great_powers,
        num_minor_nations,
        terrain_json,
    );
    let mut game = new_game_with_data_and_config_and_capital_override(
        map_key,
        diff,
        nation_index,
        load_embedded_game_data(),
        cfg,
        capital_override,
    );
    crate::flavor::apply_flavor(&mut game, flavor_key);
    game
}

/// Create a new game from a historical scenario.
/// `flavor_key` seeds names/flags; pass an empty string to reuse `map_key`.
pub fn new_scenario_game(
    scenario_id: &str,
    difficulty: u8,
    nation_index: usize,
    flavor_key: &str,
    capital_override: Option<HexCoord>,
) -> Result<GameState, ApiError> {
    let diff = match difficulty {
        0 => Difficulty::Introductory,
        1 => Difficulty::Easy,
        2 => Difficulty::Normal,
        3 => Difficulty::Hard,
        _ => Difficulty::Normal,
    };
    match new_scenario_game_with_data_and_capital_override(
        scenario_id,
        diff,
        nation_index,
        load_embedded_game_data(),
        capital_override,
    ) {
        Ok(mut game) => {
            crate::flavor::apply_flavor(&mut game, flavor_key);
            Ok(game)
        }
        Err(e) => Err(ApiError::msg(e)),
    }
}

pub fn difficulty_from_u8(d: u8) -> Difficulty {
    match d {
        0 => Difficulty::Introductory,
        1 => Difficulty::Easy,
        2 => Difficulty::Normal,
        3 => Difficulty::Hard,
        4 => Difficulty::NighOnImpossible,
        _ => Difficulty::Normal,
    }
}

/// Create a new game in observer mode. All Great Powers are AI-controlled;
/// the human only observes. The nation at index 0 is the default viewpoint.
/// `flavor_key` seeds names/flags; pass an empty string to reuse `map_key`.
#[allow(clippy::too_many_arguments)]
pub fn new_observer_game(
    map_key: &str,
    difficulty: u8,
    map_width: i32,
    map_height: i32,
    num_great_powers: u32,
    num_minor_nations: u32,
    flavor_key: &str,
    terrain_json: &str,
) -> GameState {
    let cfg = build_map_config(
        map_width,
        map_height,
        num_great_powers,
        num_minor_nations,
        terrain_json,
    );
    let mut game = new_observer_game_with_data_and_config(
        map_key,
        difficulty_from_u8(difficulty),
        load_embedded_game_data(),
        cfg,
    );
    crate::flavor::apply_flavor(&mut game, flavor_key);
    game
}

/// Create a new observer-mode scenario game.
/// `flavor_key` seeds names/flags; pass an empty string to reuse the scenario id.
pub fn new_observer_scenario_game(
    scenario_id: &str,
    difficulty: u8,
    flavor_key: &str,
) -> Result<GameState, ApiError> {
    let diff = difficulty_from_u8(difficulty);
    match new_scenario_game_with_data(scenario_id, diff, 0, load_embedded_game_data()) {
        Ok(mut game) => {
            // Promote to observer mode: give seat 0 an AI personality + bonus.
            let human_id = game.human_player_nation;
            let gp_index = game
                .world
                .nations
                .iter()
                .filter(|n| n.is_great_power())
                .position(|n| n.id == human_id)
                .unwrap_or(0);
            let personality = personality_for_nation_index(gp_index);
            if let Some(nation) = game.get_nation_mut(human_id) {
                nation.diplomacy.ai_personality = Some(personality);
                match diff {
                    Difficulty::Hard => nation.economy.treasury += Money::dollars(1000),
                    Difficulty::NighOnImpossible => nation.economy.treasury += Money::dollars(5000),
                    _ => {}
                }
            }
            game.observer_mode = true;
            crate::flavor::apply_flavor(&mut game, flavor_key);
            Ok(game)
        }
        Err(e) => Err(ApiError::msg(e)),
    }
}

/// Switch which nation is the "viewpoint" (a.k.a. human player).
/// In normal mode this swaps the human and the picked nation's AI personality
/// and starting-cash bonus. In observer mode it just moves the viewpoint — every
/// GP is already AI-controlled.
pub fn set_human_player(game: &mut GameState, nation_index: usize) -> Result<(), ApiError> {
    // Identify GP nation ids by their index ordering in `nations`.
    let gp_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();
    if nation_index >= gp_ids.len() {
        return Err(ApiError::raw(r#"{"error":"nation_index out of range"}"#));
    }
    let new_human_id = gp_ids[nation_index];
    let old_human_id = game.human_player_nation;

    if new_human_id == old_human_id {
        // no-op
        return Ok(());
    }

    if game.observer_mode {
        // Just move the viewpoint. All personalities and bonuses stay intact.
        game.human_player_nation = new_human_id;
        return Ok(());
    }

    // Normal mode: swap personality + Hard/NOI bonus.
    let old_gp_idx = gp_ids
        .iter()
        .position(|&id| id == old_human_id)
        .unwrap_or(0);
    let old_personality: AiPersonality = personality_for_nation_index(old_gp_idx);
    let difficulty = game.difficulty;
    let bonus = match difficulty {
        Difficulty::Hard => Money::dollars(1000),
        Difficulty::NighOnImpossible => Money::dollars(5000),
        _ => Money::dollars(0),
    };

    if let Some(nation) = game.get_nation_mut(old_human_id) {
        nation.diplomacy.ai_personality = Some(old_personality);
        nation.economy.treasury += bonus;
    }
    if let Some(nation) = game.get_nation_mut(new_human_id) {
        nation.diplomacy.ai_personality = None;
        nation.economy.treasury -= bonus;
    }
    game.human_player_nation = new_human_id;

    Ok(())
}

/// Get list of available scenarios.
pub fn get_scenarios() -> serde_json::Value {
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

    serde_json::Value::Array(scenarios)
}
