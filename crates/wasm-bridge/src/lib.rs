use wasm_bindgen::prelude::*;

mod flavor_bridge;
mod session;

use domain::ai::common::{AiPersonality, personality_for_nation_index};
use domain::economy::buildings::BuildingType;
use domain::economy::civilians::{CivilianType, parse_civilian_type};
use domain::economy::production::{
    ProductionChain, calculate_factory_production, calculate_mill_production,
};
use domain::economy::trade::{Commodity, base_price, commodity_price};
use domain::economy::transport::TransportSystem;
use domain::events::TreatyType;
use domain::game_state::{
    GameState, new_game_with_data_and_config, new_observer_game_with_data_and_config,
};
#[cfg(test)]
use domain::game_state::{new_game, new_observer_game};
use domain::hex::HexCoord;
use domain::map::MapGenConfig;
use domain::military::combat::BattleResult;
use domain::military::naval::NavalBattleResult;
use domain::military::ships::{Ship, ShipCategory, ShipType};
use domain::military::units::{ArmyUnit, ArmyUnitType};
use domain::scenarios::{list_scenarios, new_scenario_game_with_data};
use domain::turn::process_turn;
use domain::types::*;
use domain_snapshot::game_state::GameState as SnapshotGameState;
use infrastructure::data_loader::load_embedded_game_data;

// ── Snapshot helpers ─────────────────────────────────────────────────────

fn game_to_json(game: &GameState) -> String {
    let snap: SnapshotGameState = game.into();
    serde_json::to_string(&snap).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

fn game_to_value(game: &GameState) -> serde_json::Value {
    let snap: SnapshotGameState = game.into();
    serde_json::to_value(&snap).unwrap_or(serde_json::Value::Null)
}

fn game_from_json(json: &str) -> Result<GameState, String> {
    let snap: SnapshotGameState = serde_json::from_str(json)
        .map_err(|e| format!("deserialize: {e}"))?;
    let mut game: GameState = snap.into();
    game.game_data = load_embedded_game_data();
    Ok(game)
}

/// Build a `MapGenConfig` from raw frontend values, clamped to safe ranges.
fn build_map_config(
    map_width: i32,
    map_height: i32,
    num_great_powers: u32,
    num_minor_nations: u32,
) -> MapGenConfig {
    MapGenConfig {
        width: map_width.clamp(30, 200),
        height: map_height.clamp(20, 150),
        num_great_powers: (num_great_powers as usize).clamp(1, 20),
        num_minor_nations: (num_minor_nations as usize).min(32),
    }
}

/// Create a new game. Returns JSON string of the full game state.
/// `flavor_key` seeds names/flags; pass an empty string to reuse `map_key`.
#[wasm_bindgen]
pub fn wasm_new_game(
    map_key: &str,
    difficulty: u8,
    nation_index: usize,
    map_width: i32,
    map_height: i32,
    num_great_powers: u32,
    num_minor_nations: u32,
    flavor_key: &str,
) -> String {
    let diff = difficulty_from_u8(difficulty);
    let cfg = build_map_config(map_width, map_height, num_great_powers, num_minor_nations);
    let mut game = new_game_with_data_and_config(
        map_key,
        diff,
        nation_index,
        load_embedded_game_data(),
        cfg,
    );
    flavor_bridge::apply_flavor(&mut game, flavor_key);
    game_to_json(&game)
}

/// Create a new game from a historical scenario.
/// `flavor_key` seeds names/flags; pass an empty string to reuse `map_key`.
#[wasm_bindgen]
pub fn wasm_new_scenario_game(
    scenario_id: &str,
    difficulty: u8,
    nation_index: usize,
    flavor_key: &str,
) -> String {
    let diff = match difficulty {
        0 => Difficulty::Introductory,
        1 => Difficulty::Easy,
        2 => Difficulty::Normal,
        3 => Difficulty::Hard,
        _ => Difficulty::Normal,
    };
    match new_scenario_game_with_data(
        scenario_id,
        diff,
        nation_index,
        load_embedded_game_data(),
    ) {
        Ok(mut game) => {
            flavor_bridge::apply_flavor(&mut game, flavor_key);
            game_to_json(&game)
        }
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    }
}

fn difficulty_from_u8(d: u8) -> Difficulty {
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
#[wasm_bindgen]
pub fn wasm_new_observer_game(
    map_key: &str,
    difficulty: u8,
    map_width: i32,
    map_height: i32,
    num_great_powers: u32,
    num_minor_nations: u32,
    flavor_key: &str,
) -> String {
    let cfg = build_map_config(map_width, map_height, num_great_powers, num_minor_nations);
    let mut game = new_observer_game_with_data_and_config(
        map_key,
        difficulty_from_u8(difficulty),
        load_embedded_game_data(),
        cfg,
    );
    flavor_bridge::apply_flavor(&mut game, flavor_key);
    game_to_json(&game)
}

/// Create a new observer-mode scenario game.
/// `flavor_key` seeds names/flags; pass an empty string to reuse the scenario id.
#[wasm_bindgen]
pub fn wasm_new_observer_scenario_game(
    scenario_id: &str,
    difficulty: u8,
    flavor_key: &str,
) -> String {
    let diff = difficulty_from_u8(difficulty);
    match new_scenario_game_with_data(scenario_id, diff, 0, load_embedded_game_data()) {
        Ok(mut game) => {
            // Promote to observer mode: give seat 0 an AI personality + bonus.
            let human_id = game.human_player_nation;
            let gp_index = game
                .world.nations
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
            flavor_bridge::apply_flavor(&mut game, flavor_key);
            game_to_json(&game)
        }
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    }
}

/// Re-roll the flavor (names, flags, government titles) on an existing game
/// state, leaving everything else untouched. Used by GameSetup's
/// "Re-roll Names" button.
#[wasm_bindgen]
pub fn wasm_apply_flavor(game_json: &str, flavor_key: &str) -> String {
    let mut game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };
    flavor_bridge::clear_flavor(&mut game);
    flavor_bridge::apply_flavor(&mut game, flavor_key);
    game_to_json(&game)
}

/// Switch which nation is the "viewpoint" (a.k.a. human player).
/// In normal mode this swaps the human and the picked nation's AI personality
/// and starting-cash bonus. In observer mode it just moves the viewpoint — every
/// GP is already AI-controlled.
#[wasm_bindgen]
pub fn wasm_set_human_player(game_json: &str, nation_index: usize) -> String {
    let mut game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"deserialize: {}\"}}", e),
    };

    // Identify GP nation ids by their index ordering in `nations`.
    let gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();
    if nation_index >= gp_ids.len() {
        return format!("{{\"error\":\"nation_index out of range\"}}");
    }
    let new_human_id = gp_ids[nation_index];
    let old_human_id = game.human_player_nation;

    if new_human_id == old_human_id {
        // no-op
        return game_to_json(&game);
    }

    if game.observer_mode {
        // Just move the viewpoint. All personalities and bonuses stay intact.
        game.human_player_nation = new_human_id;
        return game_to_json(&game);
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

    game_to_json(&game)
}

/// Process N turns in a row. Clamped to 1..=50.
/// Returns JSON `{game, reports, stopped_early}` where `reports` is an array of
/// per-turn report summaries in chronological order.
#[wasm_bindgen]
pub fn wasm_process_turns(game_json: &str, count: u32) -> String {
    let mut game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return serde_json::json!({"error": e}).to_string(),
    };

    let n = count.clamp(1, 50);
    let mut reports: Vec<serde_json::Value> = Vec::with_capacity(n as usize);
    let mut stopped_early = false;

    for _ in 0..n {
        if game.is_game_over() {
            stopped_early = true;
            break;
        }
        let report = process_turn(&mut game);
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
                .map(|b| serialize_battle(b, &game))
                .collect::<Vec<_>>(),
            "naval_battles": report.naval_battles.iter()
                .map(|nb| serialize_naval_battle(nb, &game))
                .collect::<Vec<_>>(),
            "scores": report.scores.iter().map(|(id, name, score)| serde_json::json!({"nation_id": id.0, "name": name, "score": score})).collect::<Vec<_>>(),
        });
        reports.push(entry);
    }

    let response = serde_json::json!({
        "game": game_to_value(&game),
        "reports": reports,
        "stopped_early": stopped_early,
    });
    response.to_string()
}

/// Process one turn. Accepts game state JSON, returns JSON with updated state + turn report.
#[wasm_bindgen]
pub fn wasm_process_turn(game_json: &str) -> String {
    let mut game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return serde_json::json!({"error": e}).to_string(),
    };

    let report = process_turn(&mut game);

    // Build response with game state + report summary
    let response = serde_json::json!({
        "game": game_to_value(&game),
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
                    "resource": format!("{:?}", t.resource),
                    "quantity": t.quantity,
                    "cost": t.total_cost.as_dollars(),
                }))
                .collect::<Vec<_>>(),
            "battles": report.battles.iter()
                .map(|b| serialize_battle(b, &game))
                .collect::<Vec<_>>(),
            "naval_battles": report.naval_battles.iter()
                .map(|nb| serialize_naval_battle(nb, &game))
                .collect::<Vec<_>>(),
            "scores": report.scores.iter().map(|(id, name, score)| serde_json::json!({"nation_id": id.0, "name": name, "score": score})).collect::<Vec<_>>(),
        }
    });

    response.to_string()
}

/// Compute the set of hexes visible to the human player under fog-of-war.
///
/// A hex is visible if (a) it belongs to one of the player's provinces,
/// (b) it belongs to a province that shares any hex edge with the player's
/// territory, or (c) it's a non-province hex (sea, empty) directly adjacent
/// to the player. Returns an empty set when `disable_fog` is true (callers
/// should special-case that).
fn compute_visible_hexes(
    game: &GameState,
    disable_fog: bool,
) -> std::collections::HashSet<domain::hex::HexCoord> {
    if disable_fog {
        return std::collections::HashSet::new();
    }
    let human_nation_id = game.human_player_nation;
    let mut visible: std::collections::HashSet<domain::hex::HexCoord> =
        std::collections::HashSet::new();

    let mut border_ring: std::collections::HashSet<domain::hex::HexCoord> =
        std::collections::HashSet::new();
    for province in &game.world.provinces {
        if province.owner == human_nation_id {
            for &coord in &province.tiles {
                visible.insert(coord);
                for nb in coord.neighbors() {
                    border_ring.insert(nb);
                }
            }
        }
    }

    for province in &game.world.provinces {
        if province.owner == human_nation_id {
            continue;
        }
        if province.tiles.iter().any(|t| border_ring.contains(t)) {
            for &coord in &province.tiles {
                visible.insert(coord);
            }
        }
    }

    for coord in &border_ring {
        visible.insert(*coord);
    }

    visible
}

/// Get map data for rendering. Returns JSON array of tile objects.
/// `disable_fog` — when true, all tiles are visible and enemy data is not filtered.
#[wasm_bindgen]
pub fn wasm_get_map_data(game_json: &str, disable_fog: bool) -> String {
    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let human_nation_id = game.human_player_nation;

    // Build province→nation lookup using Province.owner (the ground truth)
    // and identify country capitals
    let nation_lookup: std::collections::HashMap<NationId, (&str, String)> = game
        .world.nations
        .iter()
        .map(|n| (n.id, (n.name.as_str(), format!("{:?}", n.color))))
        .collect();
    let nation_type_lookup: std::collections::HashMap<NationId, NationType> =
        game.world.nations.iter().map(|n| (n.id, n.nation_type)).collect();
    let nation_anarchy_lookup: std::collections::HashMap<NationId, bool> = game
        .world.nations
        .iter()
        .map(|n| (n.id, n.diplomacy.is_in_anarchy))
        .collect();
    let mut province_nation: std::collections::HashMap<ProvinceId, (String, String, NationId)> =
        std::collections::HashMap::new();
    for prov in &game.world.provinces {
        if let Some((name, color)) = nation_lookup.get(&prov.owner) {
            province_nation.insert(prov.id, (name.to_string(), color.clone(), prov.owner));
        }
    }
    // Build province → incorporated_from lookup
    let province_incorporated: std::collections::HashMap<ProvinceId, Option<NationId>> = game
        .world.provinces
        .iter()
        .map(|p| (p.id, p.incorporated_from))
        .collect();

    let mut country_capital_provinces: std::collections::HashSet<ProvinceId> =
        std::collections::HashSet::new();
    for nation in &game.world.nations {
        country_capital_provinces.insert(nation.capital_province_id);
    }

    // Build province → (total army FP, unit count) lookup
    let mut province_army: std::collections::HashMap<ProvinceId, (f64, u32)> =
        std::collections::HashMap::new();
    // Per-province composition breakdown keyed by unit-type name. Used by the
    // hex tooltip to show "Guards × 2, Regulars × 3" at capitals.
    let mut province_army_composition: std::collections::HashMap<
        ProvinceId,
        std::collections::BTreeMap<String, u32>,
    > = std::collections::HashMap::new();
    for nation in &game.world.nations {
        for unit in &nation.military.army {
            let e = province_army.entry(unit.position).or_insert((0.0, 0));
            e.0 += unit.effective_firepower();
            e.1 += 1;
            let bucket = province_army_composition.entry(unit.position).or_default();
            *bucket.entry(format!("{:?}", unit.unit_type)).or_insert(0) += 1;
        }
    }

    // Build nation → (naval FP, warship count) lookup
    let nation_naval: std::collections::HashMap<NationId, (u32, usize)> = game
        .world.nations
        .iter()
        .map(|n| (n.id, (n.total_naval_firepower(), n.warship_count())))
        .collect();

    // Build hex coord → civilian lookup for ALL nations
    let mut civilian_on_tile: std::collections::HashMap<domain::hex::HexCoord, serde_json::Value> =
        std::collections::HashMap::new();
    for nation in &game.world.nations {
        let (nation_name, nation_color) = nation_lookup
            .get(&nation.id)
            .map(|(name, color)| (*name, color.as_str()))
            .unwrap_or(("", ""));
        let is_human = nation.id == human_nation_id;
        for civ in &nation.military.civilians {
            if let Some(pos) = civ.position {
                // If tile already has a civilian, only overwrite if this is the human player
                if civilian_on_tile.contains_key(&pos) && !is_human {
                    continue;
                }
                civilian_on_tile.insert(
                    pos,
                    serde_json::json!({
                        "id": civ.id.0,
                        "type": format!("{}", civ.civilian_type),
                        "working": civ.working,
                        "turns_remaining": civ.turns_remaining,
                        "build_task": civ.build_task.map(|t| format!("{}", t)),
                        "owner": nation_name,
                        "owner_color": nation_color,
                        "is_human": is_human,
                    }),
                );
            }
        }
    }

    let visible_hexes = compute_visible_hexes(&game, disable_fog);

    let map_width = game.world.hex_map.width();
    let map_height = game.world.hex_map.height();

    let tiles: Vec<serde_json::Value> = game
        .world.hex_map
        .all_tiles()
        .map(|(coord, tile)| {
            let is_visible = disable_fog || visible_hexes.contains(&coord);

            let (owner_name, owner_color, owner_nation_id) = tile
                .province_id
                .and_then(|pid| province_nation.get(&pid))
                .map(|(n, c, nid)| (n.as_str(), c.as_str(), nid.0))
                .unwrap_or(("", "", 0));

            let province_name = tile
                .province_id
                .and_then(|pid| game.get_province(pid))
                .map(|p| p.name.as_str())
                .unwrap_or("");

            // Minor nation / incorporated status
            let owner_nid = NationId(owner_nation_id);
            let is_minor = owner_nation_id != 0
                && nation_type_lookup
                    .get(&owner_nid)
                    .copied()
                    .unwrap_or(NationType::GreatPower)
                    == NationType::MinorNation;
            let incorporated_from_id = tile
                .province_id
                .and_then(|pid| province_incorporated.get(&pid).copied().flatten());
            let is_incorporated_minor = incorporated_from_id.is_some();

            // Visual group: for incorporated provinces, use the minor nation's name;
            // otherwise use the owner name. Controls border grouping.
            let visual_group: Option<&str> = if let Some(inc_nid) = incorporated_from_id {
                nation_lookup.get(&inc_nid).map(|(name, _)| *name)
            } else {
                None
            };

            // For independent minor nations, override display color to Beige
            let display_color = if is_minor && !is_incorporated_minor && !owner_color.is_empty() {
                "Beige"
            } else {
                owner_color
            };

            // A tile is a country capital if it's marked as capital AND is in
            // the nation's capital province
            let is_country_capital = tile.is_capital
                && tile
                    .province_id
                    .is_some_and(|pid| country_capital_provinces.contains(&pid));

            // Strength data — only on capital tiles, filtered by fog of war
            let (army_fp, army_count) = if tile.is_capital && is_visible {
                tile.province_id
                    .and_then(|pid| province_army.get(&pid))
                    .copied()
                    .unwrap_or((0.0, 0))
            } else {
                (0.0, 0)
            };

            let army_composition: Option<&std::collections::BTreeMap<String, u32>> =
                if tile.is_capital && is_visible {
                    tile.province_id
                        .and_then(|pid| province_army_composition.get(&pid))
                } else {
                    None
                };

            let (naval_fp, naval_count) = if is_country_capital && is_visible {
                nation_naval
                    .get(&NationId(owner_nation_id))
                    .copied()
                    .unwrap_or((0, 0))
            } else {
                (0, 0)
            };

            // Civilian data — hidden on fogged tiles
            let civ_data = if is_visible {
                civilian_on_tile.get(&coord)
            } else {
                None
            };

            serde_json::json!({
                "q": coord.q,
                "r": coord.r,
                "terrain": format!("{:?}", tile.terrain()),
                "resource": tile.resource_deposit().map(|r| format!("{:?}", r)),
                "resource_hidden": tile.resource_deposit().is_some() && !tile.has_visible_resource(),
                "is_capital": tile.is_capital,
                "is_country_capital": is_country_capital,
                "improvement_level": tile.improvement_level(),
                "max_improvement_level": tile.resource_deposit().map(|r| r.max_improvement_level()).unwrap_or(0),
                "owner": owner_name,
                "owner_color": display_color,
                "province": province_name,
                "province_id": tile.province_id.map(|pid| pid.0),
                "has_railroad": tile.infrastructure.has_railroad,
                "has_depot": tile.infrastructure.has_depot,
                "has_port": tile.infrastructure.has_port,
                "has_fort": tile.infrastructure.has_fort,
                "fort_level": tile.infrastructure.fort_level,
                "map_width": map_width,
                "map_height": map_height,
                "nation_id": owner_nation_id,
                "army_firepower": army_fp,
                "army_unit_count": army_count,
                "army_composition": army_composition,
                "naval_firepower": naval_fp,
                "naval_ship_count": naval_count,
                "civilian_on_tile": civ_data,
                "is_minor": is_minor,
                "is_incorporated_minor": is_incorporated_minor,
                "incorporated_nation_id": incorporated_from_id.map(|n| n.0),
                "is_anarchic": nation_anarchy_lookup.get(&owner_nid).copied().unwrap_or(false),
                "visual_group": visual_group,
                "visible": is_visible,
            })
        })
        .collect();

    serde_json::to_string(&tiles).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Get navy markers for map rendering. One aggregate marker per
/// (nation, fleet|beachhead-target). Returns a JSON array.
///
/// Fog of war: markers belonging to other nations are only returned when their
/// anchor hex is visible to the human player (same visibility rule as the
/// map-data call). With `disable_fog = true`, all markers are returned.
#[wasm_bindgen]
pub fn wasm_get_navy_markers(game_json: &str, disable_fog: bool) -> String {
    use domain::military::navy_placement::{beachhead_anchor, beachhead_coast_tile, fleet_anchor};

    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let human_nation_id = game.human_player_nation;
    let visible_hexes = compute_visible_hexes(&game, disable_fog);

    let province_name_by_id: std::collections::HashMap<ProvinceId, &str> = game
        .world.provinces
        .iter()
        .map(|p| (p.id, p.name.as_str()))
        .collect();

    let mut markers: Vec<serde_json::Value> = Vec::new();

    for nation in &game.world.nations {
        if nation.military.warships.is_empty() {
            continue;
        }

        let owner_name = nation.name.as_str();
        let owner_color = format!("{:?}", nation.color);

        // Group warships into (a) non-beachhead fleet, (b) per-target beachhead groups.
        let mut fleet_group: Vec<&Ship> = Vec::new();
        let mut beachhead_groups: std::collections::BTreeMap<u32, Vec<&Ship>> =
            std::collections::BTreeMap::new();
        for ship in &nation.military.warships {
            if ship.ship_type.category() != ShipCategory::Warship {
                continue;
            }
            match ship.operation {
                Some(domain::military::naval::NavalOperation::Beachhead(pid)) => {
                    beachhead_groups.entry(pid.0).or_default().push(ship);
                }
                _ => fleet_group.push(ship),
            }
        }

        // ── Fleet marker ─────────────────────────────────────────
        if !fleet_group.is_empty()
            && let Some(anchor) = fleet_anchor(nation, &game.world.hex_map, &game.world.provinces)
        {
            let is_human = nation.id == human_nation_id;
            let is_visible = disable_fog || is_human || visible_hexes.contains(&anchor);
            if is_visible
                && let Some(marker) = build_marker(
                    anchor,
                    nation.id,
                    owner_name,
                    &owner_color,
                    "fleet",
                    None,
                    None,
                    &fleet_group,
                )
            {
                markers.push(marker);
            }
        }

        // ── Beachhead markers ────────────────────────────────────
        for (pid_raw, ships) in beachhead_groups {
            let pid = ProvinceId(pid_raw);
            let target = match game.get_province(pid) {
                Some(p) => p,
                None => continue,
            };
            let anchor = match beachhead_anchor(&game.world.hex_map, target) {
                Some(a) => a,
                None => continue,
            };
            let is_human = nation.id == human_nation_id;
            let is_visible = disable_fog || is_human || visible_hexes.contains(&anchor);
            if !is_visible {
                continue;
            }
            let coast_tile = beachhead_coast_tile(&game.world.hex_map, target);
            let target_province_name = province_name_by_id
                .get(&pid)
                .copied()
                .unwrap_or("")
                .to_string();
            if let Some(marker) = build_marker(
                anchor,
                nation.id,
                owner_name,
                &owner_color,
                "beachhead",
                Some(target_province_name),
                coast_tile,
                &ships,
            ) {
                markers.push(marker);
            }
        }
    }

    serde_json::to_string(&markers).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

fn build_marker(
    anchor: domain::hex::HexCoord,
    nation_id: NationId,
    owner_name: &str,
    owner_color: &str,
    kind: &str,
    target_province: Option<String>,
    target_hex: Option<domain::hex::HexCoord>,
    ships: &[&Ship],
) -> Option<serde_json::Value> {
    if ships.is_empty() {
        return None;
    }

    let mut by_type: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    let mut by_operation: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();
    let mut total_fp: u32 = 0;
    let mut total_hull: u32 = 0;

    for ship in ships {
        let type_key = format!("{:?}", ship.ship_type);
        *by_type.entry(type_key).or_insert(0) += 1;
        let op_key = format_operation(ship.operation);
        *by_operation.entry(op_key).or_insert(0) += 1;
        total_fp += ship.ship_type.stats().firepower;
        total_hull += ship.hull_remaining;
    }

    let mut json = serde_json::json!({
        "q": anchor.q,
        "r": anchor.r,
        "nation_id": nation_id.0,
        "owner_name": owner_name,
        "owner_color": owner_color,
        "kind": kind,
        "ship_count": ships.len(),
        "total_fp": total_fp,
        "total_hull": total_hull,
        "by_type": by_type,
        "by_operation": by_operation,
        // Always true at emission — invisible markers are filtered upstream.
        // The field is kept in the contract so the frontend never has to
        // re-derive visibility.
        "visible": true,
    });
    if let Some(name) = target_province {
        json["target_province"] = serde_json::Value::String(name);
    }
    if let Some(hex) = target_hex {
        json["target_hex"] = serde_json::json!({ "q": hex.q, "r": hex.r });
    }
    Some(json)
}

fn format_operation(op: Option<domain::military::naval::NavalOperation>) -> String {
    use domain::military::naval::NavalOperation;
    match op {
        None => "Idle".to_string(),
        Some(NavalOperation::Patrol) => "Patrol".to_string(),
        Some(NavalOperation::Escort) => "Escort".to_string(),
        Some(NavalOperation::Blockade(n)) => format!("Blockade(n{})", n.0),
        Some(NavalOperation::Beachhead(p)) => format!("Beachhead(p{})", p.0),
        Some(NavalOperation::Reconnaissance(n)) => format!("Recon(n{})", n.0),
    }
}

/// Get available technologies for the human player.
#[wasm_bindgen]
pub fn wasm_get_available_techs(game_json: &str) -> String {
    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return "[]".to_string(),
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

    serde_json::to_string(&techs).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Research a technology by name.
#[wasm_bindgen]
pub fn wasm_research_tech(game_json: &str, tech_name: &str) -> String {
    let mut game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return "{\"error\":\"player not found\"}".to_string(),
    };

    let lower = tech_name.to_lowercase();
    let tech = game
        .game_data
        .tech_tree
        .available_techs(&nation.researched_techs, game.turn.year())
        .into_iter()
        .find(|t| t.name.to_lowercase().contains(&lower));

    match tech {
        Some(t) => {
            let cost = t.cost;
            let tech_id = t.id;
            let nation = match game.get_nation_mut(game.human_player_nation) {
                Some(n) => n,
                None => return "{\"error\":\"player nation not found\"}".to_string(),
            };
            if nation.economy.treasury.checked_sub(cost).is_none() {
                return "{\"error\":\"insufficient funds\"}".to_string();
            }
            nation.economy.treasury -= cost;
            nation.research_tech(tech_id);
            game_to_json(&game)
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

/// Get diplomacy overlay data for a specific nation's perspective.
/// Returns JSON with relations from the selected nation to all others.
#[wasm_bindgen]
pub fn wasm_get_diplomacy_overlay(game_json: &str, nation_id: u32) -> String {
    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let selected_nid = NationId(nation_id);
    let selected_name = game
        .get_nation(selected_nid)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let selected_in_anarchy = game
        .get_nation(selected_nid)
        .is_some_and(|n| n.diplomacy.is_in_anarchy);

    let relations: Vec<serde_json::Value> = game
        .world.nations
        .iter()
        .filter(|n| n.id != selected_nid)
        .map(|n| {
            let rel = game.world.diplomacy.get_relation(selected_nid, n.id);
            // Card #31: a nation in anarchy is displayed as at war with
            // everyone regardless of the underlying relation record. This
            // must match the diplomacy-screen override so the two surfaces
            // agree. Either side being anarchic forces "At War".
            let target_in_anarchy = n.diplomacy.is_in_anarchy;
            let raw_at_war = rel.map(|r| r.at_war).unwrap_or(false);
            let at_war = raw_at_war || target_in_anarchy || selected_in_anarchy;
            let (status, score) = match rel {
                Some(r) => {
                    let s = if at_war {
                        "At War"
                    } else if r.has_treaty(domain::events::TreatyType::Alliance) {
                        "Alliance"
                    } else if r.has_treaty(domain::events::TreatyType::NonAggressionPact) {
                        "NAP"
                    } else {
                        "Neutral"
                    };
                    (s, r.score)
                }
                None => (if at_war { "At War" } else { "Neutral" }, 0),
            };
            let treaties: Vec<String> = rel
                .map(|r| {
                    r.active_treaties
                        .iter()
                        .map(|t| format!("{:?}", t))
                        .collect()
                })
                .unwrap_or_default();
            let has_consulate = rel.map(|r| r.has_consulate).unwrap_or(false);
            let has_embassy = rel.map(|r| r.has_embassy).unwrap_or(false);

            serde_json::json!({
                "nation_name": n.name,
                "nation_id": n.id.0,
                "nation_color": format!("{:?}", n.color),
                "score": score,
                "at_war": at_war,
                "status": status,
                "treaties": treaties,
                "has_consulate": has_consulate,
                "has_embassy": has_embassy,
            })
        })
        .collect();

    serde_json::json!({
        "selected_nation": selected_name,
        "selected_nation_id": nation_id,
        "relations": relations,
    })
    .to_string()
}

/// Get military overlay data for all nations (army + naval strength summaries).
#[wasm_bindgen]
pub fn wasm_get_military_overlay(game_json: &str) -> String {
    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let entries: Vec<serde_json::Value> = game
        .world.nations
        .iter()
        .map(|n| {
            serde_json::json!({
                "nation_name": n.name,
                "nation_id": n.id.0,
                "nation_color": format!("{:?}", n.color),
                "total_army_fp": n.total_military_firepower(),
                "total_naval_fp": n.total_naval_firepower(),
                "army_unit_count": n.military.army.len(),
                "warship_count": n.warship_count(),
            })
        })
        .collect();

    serde_json::to_string(&entries).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

// ── Helpers ──────────────────────────────────────────────────────────

fn parse_army_unit_type(name: &str) -> Option<ArmyUnitType> {
    match name {
        "Militia" => Some(ArmyUnitType::Militia),
        "GarrisonArtillery" => Some(ArmyUnitType::GarrisonArtillery),
        "Regulars" => Some(ArmyUnitType::Regulars),
        "Grenadiers" => Some(ArmyUnitType::Grenadiers),
        "RifleInfantry" => Some(ArmyUnitType::RifleInfantry),
        "Guards" => Some(ArmyUnitType::Guards),
        "Sharpshooters" => Some(ArmyUnitType::Sharpshooters),
        "ModernInfantry" => Some(ArmyUnitType::ModernInfantry),
        "MachineGunners" => Some(ArmyUnitType::MachineGunners),
        "Rangers" => Some(ArmyUnitType::Rangers),
        "Cuirassiers" => Some(ArmyUnitType::Cuirassiers),
        "Scouts" => Some(ArmyUnitType::Scouts),
        "CarbineCavalry" => Some(ArmyUnitType::CarbineCavalry),
        "Armour" => Some(ArmyUnitType::Armour),
        "Mechanised" => Some(ArmyUnitType::Mechanised),
        "LightArtillery" => Some(ArmyUnitType::LightArtillery),
        "StandardArtillery" => Some(ArmyUnitType::StandardArtillery),
        "FieldArtillery" => Some(ArmyUnitType::FieldArtillery),
        "SiegeArtillery" => Some(ArmyUnitType::SiegeArtillery),
        "RailroadGun" => Some(ArmyUnitType::RailroadGun),
        "MobileArtillery" => Some(ArmyUnitType::MobileArtillery),
        "Sapper" => Some(ArmyUnitType::Sapper),
        "General" => Some(ArmyUnitType::General),
        _ => None,
    }
}

fn parse_ship_type(name: &str) -> Option<ShipType> {
    match name {
        "Trader" => Some(ShipType::Trader),
        "Indiaman" => Some(ShipType::Indiaman),
        "Clipper" => Some(ShipType::Clipper),
        "Paddlewheeler" => Some(ShipType::Paddlewheeler),
        "Freighter" => Some(ShipType::Freighter),
        "Frigate" => Some(ShipType::Frigate),
        "ShipOfTheLine" => Some(ShipType::ShipOfTheLine),
        "Raider" => Some(ShipType::Raider),
        "Ironclad" => Some(ShipType::Ironclad),
        "AdvancedIronclad" => Some(ShipType::AdvancedIronclad),
        "ArmouredCruiser" => Some(ShipType::ArmouredCruiser),
        "Dreadnought" => Some(ShipType::Dreadnought),
        "Battlecruiser" => Some(ShipType::Battlecruiser),
        _ => None,
    }
}

fn deserialize_game(game_json: &str) -> Result<GameState, String> {
    game_from_json(game_json)
        .map_err(|e| serde_json::json!({"error": e}).to_string())
}

fn serialize_game(game: &GameState) -> String {
    game_to_json(game)
}

/// Returns an error JSON string if the target nation is in anarchy — no
/// diplomatic interaction (proposals, grants, declarations, peace, treaties)
/// is permitted with a country whose government has collapsed (card #81).
fn reject_if_target_in_anarchy(game: &GameState, target: NationId) -> Option<String> {
    if game.get_nation(target).is_some_and(|n| n.diplomacy.is_in_anarchy) {
        Some("{\"error\":\"target nation is in anarchy\"}".to_string())
    } else {
        None
    }
}

/// Check if a nation has researched a tech by its display name.
fn nation_has_tech(
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

// ── Query: Units in Province ─────────────────────────────────────────

/// Get all army units in a province. Returns JSON with unit details.
#[wasm_bindgen]
pub fn wasm_get_units_in_province(game_json: &str, province_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let pid = ProvinceId(province_id);
    let province = match game.get_province(pid) {
        Some(p) => p,
        None => return "{\"error\":\"province not found\"}".to_string(),
    };

    let province_name = province.name.clone();
    let garrison_count = province.garrison_count;

    let mut units: Vec<serde_json::Value> = Vec::new();
    for nation in &game.world.nations {
        for unit in &nation.military.army {
            if unit.position == pid {
                let stats = unit.unit_type.stats();
                units.push(serde_json::json!({
                    "id": unit.id.0,
                    "unit_type": format!("{:?}", unit.unit_type),
                    "category": format!("{:?}", stats.category),
                    "owner_id": nation.id.0,
                    "owner_name": nation.name,
                    "health": unit.health,
                    "medals": unit.medals,
                    "firepower": stats.firepower,
                    "effective_firepower": unit.effective_firepower(),
                    "movement": stats.movement,
                    "movement_remaining": unit.movement_remaining,
                }));
            }
        }
    }

    serde_json::json!({
        "army_units": units,
        "garrison_count": garrison_count,
        "province_name": province_name,
    })
    .to_string()
}

// ── Query: Civilians ─────────────────────────────────────────────────

/// Get all civilians for a nation. Returns deployed/undeployed groups.
#[wasm_bindgen]
pub fn wasm_get_civilians(game_json: &str, nation_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    let mut deployed: Vec<serde_json::Value> = Vec::new();
    let mut undeployed: Vec<serde_json::Value> = Vec::new();

    for civ in &nation.military.civilians {
        match civ.position {
            Some(pos) => {
                let tile = game.world.hex_map.get_tile(pos);
                let terrain_str = tile
                    .map(|t| format!("{:?}", t.terrain()))
                    .unwrap_or_default();
                // F-005: Only expose resource if visible (not hidden behind prospecting)
                let resource_str = tile
                    .filter(|t| t.has_visible_resource())
                    .and_then(|t| t.resource_deposit())
                    .map(|r| format!("{:?}", r));
                deployed.push(serde_json::json!({
                    "id": civ.id.0,
                    "type": format!("{}", civ.civilian_type),
                    "position": {"q": pos.q, "r": pos.r},
                    "working": civ.working,
                    "turns_remaining": civ.turns_remaining,
                    "build_task": civ.build_task.map(|t| format!("{}", t)),
                    "tile_terrain": terrain_str,
                    "tile_resource": resource_str,
                }));
            }
            None => {
                undeployed.push(serde_json::json!({
                    "id": civ.id.0,
                    "type": format!("{}", civ.civilian_type),
                    "position": null,
                    "working": false,
                    "turns_remaining": 0,
                }));
            }
        }
    }

    serde_json::json!({
        "deployed": deployed,
        "undeployed": undeployed,
    })
    .to_string()
}

// ── Query: Ships ─────────────────────────────────────────────────────

/// Get all ships for a nation.
#[wasm_bindgen]
pub fn wasm_get_ships(game_json: &str, nation_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    let merchants: Vec<serde_json::Value> = nation
        .military.merchant_fleet
        .iter()
        .map(|s| {
            let stats = s.ship_type.stats();
            serde_json::json!({
                "id": s.id.0,
                "type": format!("{:?}", s.ship_type),
                "hull": s.hull_remaining,
                "hull_max": stats.hull,
                "cargo": stats.cargo,
                "sea_zone": s.sea_zone.map(|z| z.0),
            })
        })
        .collect();

    let warships: Vec<serde_json::Value> = nation
        .military.warships
        .iter()
        .map(|s| {
            let stats = s.ship_type.stats();
            serde_json::json!({
                "id": s.id.0,
                "type": format!("{:?}", s.ship_type),
                "hull": s.hull_remaining,
                "hull_max": stats.hull,
                "firepower": stats.firepower,
                "sea_zone": s.sea_zone.map(|z| z.0),
            })
        })
        .collect();

    serde_json::json!({
        "merchants": merchants,
        "warships": warships,
        "total_cargo": nation.total_cargo_capacity(),
        "total_naval_fp": nation.total_naval_firepower(),
    })
    .to_string()
}

// ── Query: Valid Move Targets ────────────────────────────────────────

/// Get valid move destinations for an army unit.
#[wasm_bindgen]
pub fn wasm_get_valid_move_targets(game_json: &str, nation_id: u32, unit_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let nid = NationId(nation_id);
    let uid = domain::map::UnitId(unit_id);

    // Find the unit
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };
    let unit = match nation.military.army.iter().find(|u| u.id == uid) {
        Some(u) => u,
        None => return "{\"error\":\"unit not found\"}".to_string(),
    };
    if !unit.unit_type.can_move() {
        return serde_json::json!({"friendly": [], "hostile": []}).to_string();
    }

    let mut friendly: Vec<serde_json::Value> = Vec::new();
    let mut hostile: Vec<serde_json::Value> = Vec::new();

    for prov in &game.world.provinces {
        if prov.id == unit.position {
            continue; // Skip current province
        }
        if prov.owner == nid {
            // Own province
            friendly.push(serde_json::json!({
                "province_id": prov.id.0,
                "name": prov.name,
            }));
        } else {
            // F-011: Allow attacking provinces at war OR owned by anarchic nations
            let at_war = game.world.diplomacy.is_at_war(nid, prov.owner);
            let target_anarchic = game.get_nation(prov.owner).is_some_and(|n| n.diplomacy.is_in_anarchy);
            if at_war || target_anarchic {
                // Adjacency check: nation must own a province adjacent to
                // the target, or have an active landing site (matching backend logic).
                let nation_adjacent = nation.province_ids.iter().any(|&our_pid| {
                    game.get_province(our_pid).is_some_and(|our_prov| {
                        domain::map::provinces_are_adjacent(&game.world.hex_map, our_prov, prov)
                    })
                });
                let has_landing = game.transient.pending_landings.iter().any(|(lid, pid, established)| {
                    *lid == nid && *pid == prov.id && *established < game.turn
                });
                if !nation_adjacent && !has_landing {
                    continue;
                }

                let owner_name = game
                    .get_nation(prov.owner)
                    .map(|n| n.name.as_str())
                    .unwrap_or("Unknown");
                hostile.push(serde_json::json!({
                    "province_id": prov.id.0,
                    "name": prov.name,
                    "owner": owner_name,
                }));
            }
        }
    }

    serde_json::json!({
        "friendly": friendly,
        "hostile": hostile,
    })
    .to_string()
}

// ── Query: Buildable Units ───────────────────────────────────────────

/// Get all buildable unit types for a nation (army, civilian, ship).
#[wasm_bindgen]
pub fn wasm_get_buildable_units(game_json: &str, nation_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    let arms_available = nation.material_amount(MaterialType::Arms);
    let treasury = nation.economy.treasury;

    // Army units
    let all_army_types = [
        ArmyUnitType::Regulars,
        ArmyUnitType::Grenadiers,
        ArmyUnitType::RifleInfantry,
        ArmyUnitType::Guards,
        ArmyUnitType::Sharpshooters,
        ArmyUnitType::ModernInfantry,
        ArmyUnitType::MachineGunners,
        ArmyUnitType::Rangers,
        ArmyUnitType::Cuirassiers,
        ArmyUnitType::Scouts,
        ArmyUnitType::CarbineCavalry,
        ArmyUnitType::Armour,
        ArmyUnitType::Mechanised,
        ArmyUnitType::LightArtillery,
        ArmyUnitType::StandardArtillery,
        ArmyUnitType::FieldArtillery,
        ArmyUnitType::SiegeArtillery,
        ArmyUnitType::RailroadGun,
        ArmyUnitType::MobileArtillery,
        ArmyUnitType::Sapper,
    ];

    let army: Vec<serde_json::Value> = all_army_types
        .iter()
        .filter(|t| t.can_build())
        .map(|t| {
            let stats = t.stats();
            let tech_met = match t.required_tech() {
                Some(tech) => nation_has_tech(nation, tech, &game.game_data),
                None => true,
            };
            let can_afford = treasury >= stats.cost && arms_available >= stats.arms_required;
            let reason = if !tech_met {
                Some(format!("Requires {}", t.required_tech().unwrap_or("?")))
            } else if treasury < stats.cost {
                Some("Insufficient funds".to_string())
            } else if arms_available < stats.arms_required {
                Some("Not enough arms".to_string())
            } else {
                None
            };

            serde_json::json!({
                "type": format!("{:?}", t),
                "category": format!("{:?}", stats.category),
                "cost": stats.cost.as_dollars(),
                "arms_required": stats.arms_required,
                "firepower": stats.firepower,
                "movement": stats.movement,
                "can_afford": can_afford,
                "tech_met": tech_met,
                "reason": reason,
                "requires_horse": stats.requires_horse,
            })
        })
        .collect();

    // Civilians
    let all_civilian_types = [
        CivilianType::Farmer,
        CivilianType::Rancher,
        CivilianType::Forester,
        CivilianType::Engineer,
        CivilianType::Miner,
        CivilianType::Driller,
        CivilianType::Prospector,
    ];

    let cfg = &game.game_data.game_config;
    let civilians: Vec<serde_json::Value> = all_civilian_types
        .iter()
        .map(|ct| {
            let cost = ct.creation_cost(cfg);
            let can_afford = treasury >= cost;
            let reason = if !can_afford {
                Some("Insufficient funds".to_string())
            } else {
                None
            };
            serde_json::json!({
                "type": format!("{}", ct),
                "cost": cost.as_dollars(),
                "can_afford": can_afford,
                "tech_met": true,
                "reason": reason,
            })
        })
        .collect();

    // Ships
    let all_ship_types = [
        ShipType::Trader,
        ShipType::Indiaman,
        ShipType::Clipper,
        ShipType::Paddlewheeler,
        ShipType::Freighter,
        ShipType::Frigate,
        ShipType::ShipOfTheLine,
        ShipType::Raider,
        ShipType::Ironclad,
        ShipType::AdvancedIronclad,
        ShipType::ArmouredCruiser,
        ShipType::Dreadnought,
        ShipType::Battlecruiser,
    ];

    let ships: Vec<serde_json::Value> = all_ship_types
        .iter()
        .map(|st| {
            let stats = st.stats();
            let tech_met = match &stats.prerequisite_tech {
                Some(tech) => nation_has_tech(nation, tech, &game.game_data),
                None => true,
            };

            let mut resources_needed = serde_json::Map::new();
            if stats.fabric_cost > 0 {
                resources_needed.insert("Fabric".into(), stats.fabric_cost.into());
            }
            if stats.lumber_cost > 0 {
                resources_needed.insert("Lumber".into(), stats.lumber_cost.into());
            }
            if stats.arms_cost > 0 {
                resources_needed.insert("Arms".into(), stats.arms_cost.into());
            }
            if stats.steel_cost > 0 {
                resources_needed.insert("Steel".into(), stats.steel_cost.into());
            }
            if stats.coal_cost > 0 {
                resources_needed.insert("Coal".into(), stats.coal_cost.into());
            }

            let has_fabric = nation.material_amount(MaterialType::Fabric) >= stats.fabric_cost;
            let has_lumber = nation.material_amount(MaterialType::Lumber) >= stats.lumber_cost;
            let has_arms = nation.material_amount(MaterialType::Arms) >= stats.arms_cost;
            let has_steel = nation.material_amount(MaterialType::Steel) >= stats.steel_cost;
            let has_coal = nation.resource_amount(ResourceType::Coal) >= stats.coal_cost;
            let can_afford = has_fabric && has_lumber && has_arms && has_steel && has_coal;

            let reason = if !tech_met {
                Some(format!(
                    "Requires {}",
                    stats.prerequisite_tech.as_deref().unwrap_or("?")
                ))
            } else if !can_afford {
                Some("Insufficient resources".to_string())
            } else {
                None
            };

            serde_json::json!({
                "type": format!("{:?}", st),
                "category": format!("{:?}", st.category()),
                "resources_needed": serde_json::Value::Object(resources_needed),
                "can_afford": can_afford,
                "tech_met": tech_met,
                "reason": reason,
                "firepower": stats.firepower,
                "hull": stats.hull,
                "cargo": stats.cargo,
            })
        })
        .collect();

    serde_json::json!({
        "army": army,
        "civilians": civilians,
        "ships": ships,
        "treasury": treasury.as_dollars(),
        "arms": arms_available,
    })
    .to_string()
}

// ── Command: Queue Unit Move ─────────────────────────────────────────

/// Queue a unit move for turn resolution.
#[wasm_bindgen]
pub fn wasm_queue_unit_move(
    game_json: &str,
    nation_id: u32,
    unit_id: u32,
    dest_province_id: u32,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let nid = NationId(nation_id);
    if nid != game.human_player_nation {
        return "{\"error\":\"cannot queue moves for another nation\"}".to_string();
    }
    let uid = domain::map::UnitId(unit_id);
    let dest = ProvinceId(dest_province_id);

    // Validate unit exists and belongs to nation
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };
    let unit = match nation.military.army.iter().find(|u| u.id == uid) {
        Some(u) => u,
        None => return "{\"error\":\"unit not found\"}".to_string(),
    };
    if !unit.unit_type.can_move() {
        return "{\"error\":\"this unit cannot move\"}".to_string();
    }

    // Validate destination province exists
    let dest_prov = match game.get_province(dest) {
        Some(p) => p,
        None => return "{\"error\":\"province not found\"}".to_string(),
    };

    // F-003+F-011: Validate target legality — own province, at-war, or anarchic target
    let target_is_own = dest_prov.owner == nid;
    let target_at_war = game.world.diplomacy.is_at_war(nid, dest_prov.owner);
    let target_anarchic = game
        .get_nation(dest_prov.owner)
        .is_some_and(|n| n.diplomacy.is_in_anarchy);
    if !target_is_own && !target_at_war && !target_anarchic {
        return "{\"error\":\"cannot move to that province\"}".to_string();
    }

    // F-003: Replace existing pending move for this unit (prevent duplicates)
    game.transient.pending_moves.retain(|(_, id, _)| *id != uid);
    game.transient.pending_moves.push((nid, uid, dest));
    serialize_game(&game)
}

// ── Command: Cancel Unit Move ────────────────────────────────────────

/// Cancel a pending unit move.
#[wasm_bindgen]
pub fn wasm_cancel_unit_move(game_json: &str, unit_id: u32) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let uid = domain::map::UnitId(unit_id);
    let player = game.human_player_nation;
    game.transient.pending_moves
        .retain(|(nid, id, _)| !(*nid == player && *id == uid));
    serialize_game(&game)
}

// ── Command: Disband Unit ────────────────────────────────────────────

/// Dismiss (disband) one of the player's army units.
///
/// Validates the unit belongs to the human player nation and is not a Garrison
/// (militia / garrison artillery). Removes the unit and any pending move for it.
#[wasm_bindgen]
pub fn wasm_disband_unit(game_json: &str, unit_id: u32) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    if game.observer_mode {
        return "{\"error\":\"disband not allowed in observer mode\"}".to_string();
    }
    let uid = domain::map::UnitId(unit_id);
    let player_nation = game.human_player_nation;
    match domain::military::units::disband_unit(&mut game, player_nation, uid) {
        Ok(()) => serialize_game(&game),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    }
}

// ── Command: Deploy Civilian ─────────────────────────────────────────

/// Deploy a civilian to a hex tile to start improving it.
#[wasm_bindgen]
pub fn wasm_deploy_civilian(game_json: &str, civilian_id: u32, hex_q: i32, hex_r: i32) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let cid = domain::map::UnitId(civilian_id);
    let coord = HexCoord::new(hex_q, hex_r);
    let human_nid = game.human_player_nation;

    // Validate tile exists and is owned by the player
    let tile = match game.world.hex_map.get_tile(coord) {
        Some(t) => t,
        None => return "{\"error\":\"tile not found\"}".to_string(),
    };
    let tile_province = match tile.province_id {
        Some(pid) => pid,
        None => return "{\"error\":\"tile has no province\"}".to_string(),
    };
    let prov = match game.get_province(tile_province) {
        Some(p) => p,
        None => return "{\"error\":\"province not found\"}".to_string(),
    };
    if prov.owner != human_nid {
        return "{\"error\":\"tile not owned by player\"}".to_string();
    }

    // F-006: Check tile doesn't already have an assigned civilian
    if tile.assigned_civilian.is_some() {
        return "{\"error\":\"tile already has a civilian assigned\"}".to_string();
    }

    let terrain = tile.terrain();
    // F-017: Only use visible resources for can_improve check.
    // Prospectors work on terrain (not resources), so they're unaffected.
    // Other civilians need visible resources — hidden deposits are not valid targets.
    let resource = if tile.has_visible_resource() {
        tile.resource_deposit()
    } else {
        None
    };
    let improvement_level = tile.improvement_level();

    // Find the civilian in the player's nation
    let nation = match game.get_nation_mut(human_nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };
    let civ = match nation.military.civilians.iter_mut().find(|c| c.id == cid) {
        Some(c) => c,
        None => return "{\"error\":\"civilian not found\"}".to_string(),
    };
    if civ.position.is_some() {
        return "{\"error\":\"civilian already deployed\"}".to_string();
    }
    if !civ.civilian_type.can_improve(terrain, resource) {
        return "{\"error\":\"civilian cannot improve this tile\"}".to_string();
    }

    civ.deploy(coord);
    // Engineers are deployed without an auto-start; the player issues a build
    // order via wasm_engineer_build once the engineer is on the right hex.
    if civ.civilian_type != domain::economy::CivilianType::Engineer {
        let turns = if improvement_level == 0 { 3 } else { 5 };
        civ.start_work(turns);
    }

    // F-006: Set assigned_civilian on the tile
    if let Some(tile_mut) = game.world.hex_map.get_tile_mut(coord) {
        tile_mut.assigned_civilian = Some(cid);
    }

    serialize_game(&game)
}

// ── Command: Recall Civilian ─────────────────────────────────────────

/// Recall a deployed civilian back to the capital.
#[wasm_bindgen]
pub fn wasm_recall_civilian(game_json: &str, civilian_id: u32) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let cid = domain::map::UnitId(civilian_id);
    let human_nid = game.human_player_nation;

    // Extract old position before mutating, to avoid borrow conflicts
    let old_pos = {
        let nation = match game.get_nation(human_nid) {
            Some(n) => n,
            None => return "{\"error\":\"nation not found\"}".to_string(),
        };
        let civ = match nation.military.civilians.iter().find(|c| c.id == cid) {
            Some(c) => c,
            None => return "{\"error\":\"civilian not found\"}".to_string(),
        };
        civ.position
    };

    // F-006: Clear assigned_civilian on the old tile
    if let Some(pos) = old_pos
        && let Some(tile_mut) = game.world.hex_map.get_tile_mut(pos)
    {
        tile_mut.assigned_civilian = None;
    }

    // Now mutate the civilian
    let nation = match game.get_nation_mut(human_nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };
    let civ = match nation.military.civilians.iter_mut().find(|c| c.id == cid) {
        Some(c) => c,
        None => return "{\"error\":\"civilian not found\"}".to_string(),
    };
    civ.position = None;
    civ.working = false;
    civ.turns_remaining = 0;
    civ.build_task = None;

    serialize_game(&game)
}

// ── Command: Engineer Build (railroad / depot / port) ────────────────

/// Order a deployed Engineer civilian to start a build task on its current hex.
/// The engineer must already be deployed (via `wasm_deploy_civilian`) on an
/// owned hex. `build_kind` is one of "railroad", "depot", "port".
#[wasm_bindgen]
pub fn wasm_engineer_build(game_json: &str, civilian_id: u32, build_kind: &str) -> String {
    use domain::economy::BuildTask;

    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let cid = domain::map::UnitId(civilian_id);
    let human_nid = game.human_player_nation;

    let task = match build_kind.to_lowercase().as_str() {
        "railroad" | "rail" => BuildTask::Railroad,
        "depot" => BuildTask::Depot,
        "port" => BuildTask::Port,
        other => return format!("{{\"error\":\"unknown build kind: {}\"}}", other),
    };

    // Look up the engineer, its position, and the target tile's state.
    let (position, working) = {
        let nation = match game.get_nation(human_nid) {
            Some(n) => n,
            None => return "{\"error\":\"nation not found\"}".to_string(),
        };
        let civ = match nation.military.civilians.iter().find(|c| c.id == cid) {
            Some(c) => c,
            None => return "{\"error\":\"civilian not found\"}".to_string(),
        };
        if civ.civilian_type != domain::economy::CivilianType::Engineer {
            return "{\"error\":\"civilian is not an engineer\"}".to_string();
        }
        (civ.position, civ.working)
    };

    let pos = match position {
        Some(p) => p,
        None => return "{\"error\":\"engineer is not deployed\"}".to_string(),
    };
    if working {
        return "{\"error\":\"engineer is already working\"}".to_string();
    }

    // Validate tile ownership + prerequisites (depot needs railroad or capital,
    // port needs coastal tile). Railroad only needs ownership + land.
    let tile = match game.world.hex_map.get_tile(pos) {
        Some(t) => t,
        None => return "{\"error\":\"tile not found\"}".to_string(),
    };
    let owns_tile = tile
        .province_id
        .and_then(|pid| game.get_province(pid))
        .is_some_and(|p| p.owner == human_nid);
    if !owns_tile {
        return "{\"error\":\"tile not owned by player\"}".to_string();
    }
    match task {
        BuildTask::Railroad => {
            if !tile.terrain().is_land() {
                return "{\"error\":\"cannot build railroad on sea\"}".to_string();
            }
            if tile.infrastructure.has_railroad {
                return "{\"error\":\"railroad already exists\"}".to_string();
            }
            // Tech pre-flight: some terrains require a researched tech.
            let cfg_for_tech = &game.game_data.game_config;
            let researched = &game.get_nation(human_nid).unwrap().researched_techs;
            if !domain::map::infrastructure::rail_terrain_enabled(
                tile.terrain(),
                researched,
                &game.game_data,
                cfg_for_tech,
            ) {
                let tech = domain::map::infrastructure::railroad_required_tech(
                    tile.terrain(),
                    cfg_for_tech,
                )
                .unwrap_or("?");
                return format!(
                    "{{\"error\":\"railroad on {:?} requires tech: {}\"}}",
                    tile.terrain(),
                    tech
                );
            }
        }
        BuildTask::Depot => {
            if !tile.terrain().is_land() {
                return "{\"error\":\"cannot build depot on sea\"}".to_string();
            }
            if tile.infrastructure.has_depot {
                return "{\"error\":\"depot already exists\"}".to_string();
            }
            if !tile.infrastructure.has_railroad {
                return "{\"error\":\"depot requires a railroad on the tile\"}".to_string();
            }
        }
        BuildTask::Port => {
            if !tile.terrain().is_land() {
                return "{\"error\":\"cannot build port on sea\"}".to_string();
            }
            if tile.infrastructure.has_port {
                return "{\"error\":\"port already exists\"}".to_string();
            }
            let is_coastal = pos.neighbors().iter().any(|n| {
                game.world.hex_map
                    .get_tile(*n)
                    .is_some_and(|t| !t.terrain().is_land())
            });
            if !is_coastal {
                return "{\"error\":\"port requires a coastal tile\"}".to_string();
            }
        }
    }

    // Affordability gate — treasury is debited on completion, so reject orders
    // the nation cannot pay for up front (matches CLI/AI contract).
    let cfg = game.game_data.game_config.clone();
    let task_cost = match task {
        BuildTask::Railroad => {
            match domain::map::infrastructure::railroad_cost(tile.terrain(), &cfg) {
                Some(c) => c,
                None => return "{\"error\":\"cannot build railroad on this terrain\"}".to_string(),
            }
        }
        BuildTask::Depot => Money::dollars(cfg.depot_cost),
        BuildTask::Port => Money::dollars(cfg.port_cost),
    };
    let nation_treasury = game
        .get_nation(human_nid)
        .map(|n| n.economy.treasury)
        .unwrap_or(Money::ZERO);
    if nation_treasury.checked_sub(task_cost).is_none() {
        return "{\"error\":\"insufficient funds\"}".to_string();
    }

    // Issue the build order.
    let nation = match game.get_nation_mut(human_nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };
    if let Some(civ) = nation.military.civilians.iter_mut().find(|c| c.id == cid) {
        civ.start_build(task, &cfg);
    }
    serialize_game(&game)
}

// ── Command: Recruit Army Unit ───────────────────────────────────────

/// Recruit a new army unit at the capital province.
#[wasm_bindgen]
pub fn wasm_recruit_army_unit(game_json: &str, nation_id: u32, unit_type_str: &str) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let nid = NationId(nation_id);
    let unit_type = match parse_army_unit_type(unit_type_str) {
        Some(t) => t,
        None => return format!("{{\"error\":\"unknown unit type: {}\"}}", unit_type_str),
    };

    if !unit_type.can_build() {
        return "{\"error\":\"this unit type cannot be built\"}".to_string();
    }

    let stats = unit_type.stats();

    // Check tech prerequisite
    {
        let nation = match game.get_nation(nid) {
            Some(n) => n,
            None => return "{\"error\":\"nation not found\"}".to_string(),
        };
        if let Some(tech) = unit_type.required_tech()
            && !nation_has_tech(nation, tech, &game.game_data)
        {
            return format!("{{\"error\":\"requires tech: {}\"}}", tech);
        }
        if nation.economy.treasury < stats.cost {
            return "{\"error\":\"insufficient funds\"}".to_string();
        }
        if nation.material_amount(MaterialType::Arms) < stats.arms_required {
            return "{\"error\":\"not enough arms\"}".to_string();
        }
    }

    // Deduct costs and create unit
    let capital = {
        let nation = match game.get_nation_mut(nid) {
            Some(n) => n,
            None => return "{\"error\":\"nation not found\"}".to_string(),
        };
        nation.economy.treasury -= stats.cost;
        nation.consume_material(MaterialType::Arms, stats.arms_required);
        nation.capital_province_id
    };
    let uid = game.alloc_unit_id();
    let new_unit = ArmyUnit::new(uid, unit_type, nid, capital);
    if let Some(nation) = game.get_nation_mut(nid) {
        nation.military.army.push(new_unit);
    }

    serialize_game(&game)
}

// ── Command: Hire Civilian ───────────────────────────────────────────

/// Hire a new civilian unit.
#[wasm_bindgen]
pub fn wasm_hire_civilian(game_json: &str, nation_id: u32, civilian_type_str: &str) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let nid = NationId(nation_id);
    let civ_type = match parse_civilian_type(civilian_type_str) {
        Some(t) => t,
        None => {
            return format!(
                "{{\"error\":\"unknown civilian type: {}\"}}",
                civilian_type_str
            );
        }
    };

    let cost = civ_type.creation_cost(&game.game_data.game_config);

    {
        let nation = match game.get_nation_mut(nid) {
            Some(n) => n,
            None => return "{\"error\":\"nation not found\"}".to_string(),
        };
        if nation.economy.treasury < cost {
            return "{\"error\":\"insufficient funds\"}".to_string();
        }
        nation.economy.treasury -= cost;
    }
    let cid = game.alloc_unit_id();
    if let Some(nation) = game.get_nation_mut(nid) {
        let new_civ = domain::economy::civilians::Civilian::new(cid, civ_type, nid);
        nation.military.civilians.push(new_civ);
    }

    serialize_game(&game)
}

// ── Command: Build Ship ──────────────────────────────────────────────

/// Build a new ship.
#[wasm_bindgen]
pub fn wasm_build_ship(game_json: &str, nation_id: u32, ship_type_str: &str) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };

    let nid = NationId(nation_id);
    let ship_type = match parse_ship_type(ship_type_str) {
        Some(t) => t,
        None => return format!("{{\"error\":\"unknown ship type: {}\"}}", ship_type_str),
    };

    let stats = ship_type.stats();

    // Check tech prerequisite
    {
        let nation = match game.get_nation(nid) {
            Some(n) => n,
            None => return "{\"error\":\"nation not found\"}".to_string(),
        };
        if let Some(ref tech) = stats.prerequisite_tech
            && !nation_has_tech(nation, tech, &game.game_data)
        {
            return format!("{{\"error\":\"requires tech: {}\"}}", tech);
        }
        // Check all resources
        if nation.material_amount(MaterialType::Fabric) < stats.fabric_cost {
            return "{\"error\":\"not enough fabric\"}".to_string();
        }
        if nation.material_amount(MaterialType::Lumber) < stats.lumber_cost {
            return "{\"error\":\"not enough lumber\"}".to_string();
        }
        if nation.material_amount(MaterialType::Arms) < stats.arms_cost {
            return "{\"error\":\"not enough arms\"}".to_string();
        }
        if nation.material_amount(MaterialType::Steel) < stats.steel_cost {
            return "{\"error\":\"not enough steel\"}".to_string();
        }
        if nation.resource_amount(ResourceType::Coal) < stats.coal_cost {
            return "{\"error\":\"not enough coal\"}".to_string();
        }
    }

    // Deduct resources and create ship
    {
        let nation = match game.get_nation_mut(nid) {
            Some(n) => n,
            None => return "{\"error\":\"nation not found\"}".to_string(),
        };
        nation.consume_material(MaterialType::Fabric, stats.fabric_cost);
        nation.consume_material(MaterialType::Lumber, stats.lumber_cost);
        nation.consume_material(MaterialType::Arms, stats.arms_cost);
        nation.consume_material(MaterialType::Steel, stats.steel_cost);
        nation.remove_resource(ResourceType::Coal, stats.coal_cost);
    }
    let sid = game.alloc_unit_id();
    let new_ship = Ship::new(sid, ship_type, nid);
    if let Some(nation) = game.get_nation_mut(nid) {
        match ship_type.category() {
            ShipCategory::Merchant => nation.military.merchant_fleet.push(new_ship),
            ShipCategory::Warship => nation.military.warships.push(new_ship),
        }
    }

    serialize_game(&game)
}

// ── Mutation: Assign Beachhead ──────────────────────────────────────

/// Assign a nation's warships to establish a beachhead on a specific coastal enemy province.
/// Returns updated game state JSON.
#[wasm_bindgen]
pub fn wasm_assign_beachhead(game_json: &str, nation_id: u32, target_province_id: u32) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target_pid = ProvinceId(target_province_id);

    // Validate the target province is coastal and owned by an enemy at war
    let valid = game.get_province(target_pid).is_some_and(|p| {
        p.coastal && {
            let at_war = game.world.diplomacy.is_at_war(nid, p.owner);
            let target_anarchic = game.get_nation(p.owner).is_some_and(|n| n.diplomacy.is_in_anarchy);
            at_war || target_anarchic
        }
    });
    if !valid {
        return "{\"error\":\"target province is not a valid coastal enemy province\"}".to_string();
    }

    // Must have warships
    let has_warships = game.get_nation(nid).is_some_and(|n| !n.military.warships.is_empty());
    if !has_warships {
        return "{\"error\":\"no warships available\"}".to_string();
    }

    // Sea-zone adjacency: attacker must own at least one coastal province (embarkation point)
    let has_coast = game.get_nation(nid).is_some_and(|n| {
        n.province_ids
            .iter()
            .any(|&pid| game.get_province(pid).is_some_and(|p| p.coastal))
    });
    if !has_coast {
        return "{\"error\":\"you have no coastal provinces to embark from\"}".to_string();
    }

    // Assign all warships to beachhead targeting the specific province
    if let Some(nation) = game.get_nation_mut(nid) {
        for ship in &mut nation.military.warships {
            ship.operation = Some(domain::military::naval::NavalOperation::Beachhead(
                target_pid,
            ));
        }
    }

    serialize_game(&game)
}

// ── Parser helpers ──────────────────────────────────────────────────

fn parse_resource_type(name: &str) -> Option<ResourceType> {
    match name {
        "Timber" => Some(ResourceType::Timber),
        "Coal" => Some(ResourceType::Coal),
        "Iron" => Some(ResourceType::Iron),
        "Cotton" => Some(ResourceType::Cotton),
        "Wool" => Some(ResourceType::Wool),
        "Grain" => Some(ResourceType::Grain),
        "Fruit" => Some(ResourceType::Fruit),
        "Livestock" => Some(ResourceType::Livestock),
        "Horses" => Some(ResourceType::Horses),
        "Oil" => Some(ResourceType::Oil),
        "Gold" => Some(ResourceType::Gold),
        "Gems" => Some(ResourceType::Gems),
        _ => None,
    }
}

fn parse_material_type(name: &str) -> Option<MaterialType> {
    match name {
        "Lumber" => Some(MaterialType::Lumber),
        "Steel" => Some(MaterialType::Steel),
        "Fabric" => Some(MaterialType::Fabric),
        "Paper" => Some(MaterialType::Paper),
        "Arms" => Some(MaterialType::Arms),
        "CannedFood" | "Canned Food" => Some(MaterialType::CannedFood),
        _ => None,
    }
}

fn parse_goods_type(name: &str) -> Option<GoodsType> {
    match name {
        "Furniture" => Some(GoodsType::Furniture),
        "Clothing" => Some(GoodsType::Clothing),
        "Hardware" => Some(GoodsType::Hardware),
        _ => None,
    }
}

fn parse_commodity(
    commodity_type: &str,
    commodity_name: &str,
) -> Option<domain::economy::trade::Commodity> {
    use domain::economy::trade::Commodity;
    match commodity_type {
        "resource" => parse_resource_type(commodity_name).map(Commodity::Resource),
        "material" => parse_material_type(commodity_name).map(Commodity::Material),
        "goods" => parse_goods_type(commodity_name).map(Commodity::Goods),
        _ => None,
    }
}

fn parse_building_type(name: &str) -> Option<BuildingType> {
    match name {
        "Armory" => Some(BuildingType::Armory),
        "Capitol" => Some(BuildingType::Capitol),
        "FoodProcessing" => Some(BuildingType::FoodProcessing),
        "Railyard" => Some(BuildingType::Railyard),
        "Shipyard" => Some(BuildingType::Shipyard),
        "TradeSchool" => Some(BuildingType::TradeSchool),
        "University" => Some(BuildingType::University),
        "Warehouse" => Some(BuildingType::Warehouse),
        "LumberMill" => Some(BuildingType::LumberMill),
        "SteelMill" => Some(BuildingType::SteelMill),
        "TextileMill" => Some(BuildingType::TextileMill),
        "FurnitureFactory" => Some(BuildingType::FurnitureFactory),
        "HardwareFactory" => Some(BuildingType::HardwareFactory),
        "ClothingFactory" => Some(BuildingType::ClothingFactory),
        "OilRefinery" => Some(BuildingType::OilRefinery),
        "PowerPlant" => Some(BuildingType::PowerPlant),
        _ => None,
    }
}

fn parse_treaty_type(name: &str) -> Option<TreatyType> {
    match name {
        "Alliance" => Some(TreatyType::Alliance),
        "NonAggressionPact" => Some(TreatyType::NonAggressionPact),
        "PeaceTreaty" => Some(TreatyType::PeaceTreaty),
        "RequestToJoinEmpire" => Some(TreatyType::RequestToJoinEmpire),
        "WarDeclaration" => Some(TreatyType::WarDeclaration),
        _ => None,
    }
}

// ══════════════════════════════════════════════════════════════════════
// FEATURE 1: Transport Screen
// ══════════════════════════════════════════════════════════════════════

/// Query transport data for a nation.
#[wasm_bindgen]
pub fn wasm_get_transport_data(game_json: &str, nation_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    let transport = &nation.military.transport;
    let (labor_cost, lumber_cost, steel_cost) = TransportSystem::build_freight_car_cost();
    let available_lumber = nation.material_amount(MaterialType::Lumber);
    let available_steel = nation.material_amount(MaterialType::Steel);
    let available_labor = nation.economy.labor.total_labor_units();

    let can_build = available_lumber >= lumber_cost
        && available_steel >= steel_cost
        && available_labor >= labor_cost;

    // Build available resources from warehouse for delivery calculation
    let all_resources = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Grain,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Horses,
        ResourceType::Oil,
        ResourceType::Gold,
        ResourceType::Gems,
    ];

    let available: Vec<(ResourceType, u32)> = all_resources
        .iter()
        .map(|&r| (r, nation.resource_amount(r)))
        .filter(|(_, qty)| *qty > 0)
        .collect();

    let deliveries = transport.calculate_deliveries(&available);

    let allocations_json: Vec<serde_json::Value> = transport
        .allocations
        .iter()
        .map(|(r, pct)| {
            serde_json::json!({
                "resource": format!("{:?}", r),
                "percentage": pct,
            })
        })
        .collect();

    let deliveries_json: Vec<serde_json::Value> = available
        .iter()
        .map(|(r, avail)| {
            let delivered = deliveries
                .iter()
                .find(|(dr, _)| *dr == *r)
                .map(|(_, qty)| *qty)
                .unwrap_or(0);
            serde_json::json!({
                "resource": format!("{:?}", r),
                "available": avail,
                "delivered": delivered,
            })
        })
        .collect();

    serde_json::json!({
        "freight_cars": transport.freight_cars,
        "total_capacity": transport.total_capacity(),
        "military_transport_capacity": transport.military_transport_capacity(),
        "allocations": allocations_json,
        "build_cost": {
            "labor": labor_cost,
            "lumber": lumber_cost,
            "steel": steel_cost,
        },
        "can_build": can_build,
        "available_lumber": available_lumber,
        "available_steel": available_steel,
        "available_labor": available_labor,
        "deliveries": deliveries_json,
    })
    .to_string()
}

/// Build one freight car. Deducts 1 lumber + 1 steel from warehouse.
/// Labor is checked but not consumed (labor is used during turn resolution).
#[wasm_bindgen]
pub fn wasm_build_freight_car(game_json: &str, nation_id: u32) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let (labor_cost, lumber_cost, steel_cost) = TransportSystem::build_freight_car_cost();

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    if nation.economy.labor.total_labor_units() < labor_cost {
        return "{\"error\":\"not enough labor\"}".to_string();
    }
    if nation.material_amount(MaterialType::Lumber) < lumber_cost {
        return "{\"error\":\"not enough lumber\"}".to_string();
    }
    if nation.material_amount(MaterialType::Steel) < steel_cost {
        return "{\"error\":\"not enough steel\"}".to_string();
    }

    nation.consume_material(MaterialType::Lumber, lumber_cost);
    nation.consume_material(MaterialType::Steel, steel_cost);
    nation.military.transport.build_freight_cars(1);

    serialize_game(&game)
}

/// Set transport allocation for a resource type (percentage 0-100).
#[wasm_bindgen]
pub fn wasm_set_transport_allocation(
    game_json: &str,
    nation_id: u32,
    resource: &str,
    percentage: u32,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let res = match parse_resource_type(resource) {
        Some(r) => r,
        None => return "{\"error\":\"unknown resource type\"}".to_string(),
    };

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    nation.military.transport.set_allocation(res, percentage.min(100));
    serialize_game(&game)
}

// ══════════════════════════════════════════════════════════════════════
// FEATURE 2: Industry Screen
// ══════════════════════════════════════════════════════════════════════

/// Query industry/production data for a nation.
#[wasm_bindgen]
pub fn wasm_get_industry_data(game_json: &str, nation_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    // Buildings
    let buildings_json: Vec<serde_json::Value> = nation
        .economy.buildings
        .iter()
        .map(|b| {
            let next_cap = b.next_capacity();
            let (exp_lumber, exp_steel) =
                domain::economy::buildings::Building::expansion_cost(next_cap - b.capacity);
            serde_json::json!({
                "type": format!("{:?}", b.building_type),
                "display_name": format!("{}", b.building_type),
                "capacity": b.capacity,
                "next_capacity": next_cap,
                "is_expanding": b.turns_until_upgrade > 0,
                "turns_remaining": b.turns_until_upgrade,
                "pending_capacity": b.pending_capacity,
                "expansion_cost": { "lumber": exp_lumber, "steel": exp_steel },
            })
        })
        .collect();

    // Warehouse
    let resources_json: serde_json::Value = nation
        .economy.warehouse
        .iter()
        .map(|(r, qty)| (format!("{:?}", r), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    let materials_json: serde_json::Value = nation
        .economy.materials
        .iter()
        .map(|(m, qty)| (format!("{:?}", m), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    let goods_json: serde_json::Value = nation
        .economy.goods
        .iter()
        .map(|(g, qty)| (format!("{:?}", g), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    // Labor
    let labor = &nation.economy.labor;

    // Production forecast for each chain
    let available_lumber_mat = nation.material_amount(MaterialType::Lumber);
    let available_steel_mat = nation.material_amount(MaterialType::Steel);

    // Can-expand map
    let can_expand: serde_json::Value = nation
        .economy.buildings
        .iter()
        .map(|b| {
            let next_cap = b.next_capacity();
            let (exp_lumber, exp_steel) =
                domain::economy::buildings::Building::expansion_cost(next_cap - b.capacity);
            let expandable = b.turns_until_upgrade == 0
                && available_lumber_mat >= exp_lumber
                && available_steel_mat >= exp_steel;
            (
                format!("{:?}", b.building_type),
                serde_json::json!(expandable),
            )
        })
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    // Production forecasts
    let labor_units = labor.total_labor_units();

    // Build resource slices for production functions
    let all_res: Vec<(ResourceType, u32)> = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
    ]
    .iter()
    .map(|&r| (r, nation.resource_amount(r)))
    .collect();

    let lumber_mill_cap = nation
        .economy.buildings
        .iter()
        .find(|b| b.building_type == BuildingType::LumberMill)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let steel_mill_cap = nation
        .economy.buildings
        .iter()
        .find(|b| b.building_type == BuildingType::SteelMill)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let textile_mill_cap = nation
        .economy.buildings
        .iter()
        .find(|b| b.building_type == BuildingType::TextileMill)
        .map(|b| b.capacity)
        .unwrap_or(0);

    let timber_mill = calculate_mill_production(
        ProductionChain::Timber,
        &all_res,
        lumber_mill_cap,
        labor_units,
    );
    let metal_mill = calculate_mill_production(
        ProductionChain::Metal,
        &all_res,
        steel_mill_cap,
        labor_units,
    );
    let textile_mill = calculate_mill_production(
        ProductionChain::Textile,
        &all_res,
        textile_mill_cap,
        labor_units,
    );

    // Build material slices for factory functions
    let all_mats: Vec<(MaterialType, u32)> = [
        MaterialType::Lumber,
        MaterialType::Steel,
        MaterialType::Fabric,
    ]
    .iter()
    .map(|&m| (m, nation.material_amount(m)))
    .collect();

    let furniture_cap = nation
        .economy.buildings
        .iter()
        .find(|b| b.building_type == BuildingType::FurnitureFactory)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let hardware_cap = nation
        .economy.buildings
        .iter()
        .find(|b| b.building_type == BuildingType::HardwareFactory)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let clothing_cap = nation
        .economy.buildings
        .iter()
        .find(|b| b.building_type == BuildingType::ClothingFactory)
        .map(|b| b.capacity)
        .unwrap_or(0);

    let furniture_prod = calculate_factory_production(
        ProductionChain::Timber,
        &all_mats,
        furniture_cap,
        labor_units,
    );
    let hardware_prod =
        calculate_factory_production(ProductionChain::Metal, &all_mats, hardware_cap, labor_units);
    let clothing_prod = calculate_factory_production(
        ProductionChain::Textile,
        &all_mats,
        clothing_cap,
        labor_units,
    );

    serde_json::json!({
        "buildings": buildings_json,
        "warehouse": {
            "resources": resources_json,
            "materials": materials_json,
            "goods": goods_json,
        },
        "labor": {
            "untrained": labor.untrained,
            "trained": labor.trained,
            "expert": labor.expert,
            "total_workers": labor.total_workers(),
            "total_labor_units": labor.total_labor_units(),
        },
        "production_forecast": {
            "timber_chain": {
                "mill_output": timber_mill.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "factory_output": furniture_prod.goods_produced.first().map(|x| x.1).unwrap_or(0),
                "mill_labor": timber_mill.labor_used,
                "factory_labor": furniture_prod.labor_used,
            },
            "metal_chain": {
                "mill_output": metal_mill.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "factory_output": hardware_prod.goods_produced.first().map(|x| x.1).unwrap_or(0),
                "mill_labor": metal_mill.labor_used,
                "factory_labor": hardware_prod.labor_used,
            },
            "textile_chain": {
                "mill_output": textile_mill.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "factory_output": clothing_prod.goods_produced.first().map(|x| x.1).unwrap_or(0),
                "mill_labor": textile_mill.labor_used,
                "factory_labor": clothing_prod.labor_used,
            },
        },
        "can_expand": can_expand,
    })
    .to_string()
}

/// Expand a building to its next capacity tier.
#[wasm_bindgen]
pub fn wasm_expand_building(game_json: &str, nation_id: u32, building_type: &str) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let bt = match parse_building_type(building_type) {
        Some(b) => b,
        None => return "{\"error\":\"unknown building type\"}".to_string(),
    };

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    let building = match nation.economy.buildings.iter().find(|b| b.building_type == bt) {
        Some(b) => b,
        None => return "{\"error\":\"building not found\"}".to_string(),
    };

    if building.turns_until_upgrade > 0 {
        return "{\"error\":\"building is already expanding\"}".to_string();
    }

    let next_cap = building.next_capacity();
    let amount = next_cap - building.capacity;
    let (exp_lumber, exp_steel) = domain::economy::buildings::Building::expansion_cost(amount);

    if nation.material_amount(MaterialType::Lumber) < exp_lumber {
        return "{\"error\":\"not enough lumber\"}".to_string();
    }
    if nation.material_amount(MaterialType::Steel) < exp_steel {
        return "{\"error\":\"not enough steel\"}".to_string();
    }

    nation.consume_material(MaterialType::Lumber, exp_lumber);
    nation.consume_material(MaterialType::Steel, exp_steel);

    // Find the building again mutably and start expansion
    if let Some(b) = nation.economy.buildings.iter_mut().find(|b| b.building_type == bt) {
        b.start_expansion(amount);
    }

    serialize_game(&game)
}

// ══════════════════════════════════════════════════════════════════════
// FEATURE 3: Trade Screen
// ══════════════════════════════════════════════════════════════════════

/// Query trade data for a nation.
#[wasm_bindgen]
pub fn wasm_get_trade_data(game_json: &str, nation_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    // Market prices
    let all_resources = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Grain,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Horses,
        ResourceType::Oil,
    ];
    let market_prices: Vec<serde_json::Value> = all_resources
        .iter()
        .map(|&r| {
            let bp = base_price(r);
            let stock = nation.resource_amount(r);
            serde_json::json!({
                "resource": format!("{:?}", r),
                "base_price": bp.as_dollars(),
                "stock": stock,
            })
        })
        .collect();

    // Trade history (last 20)
    let history: Vec<serde_json::Value> = nation
        .archives.trade_history
        .iter()
        .rev()
        .take(20)
        .map(|entry| {
            let partner_name = game
                .get_nation(entry.partner)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            serde_json::json!({
                "turn": entry.turn.0,
                "partner_name": partner_name,
                "partner_id": entry.partner.0,
                "resource": format!("{:?}", entry.resource),
                "quantity": entry.quantity,
                "total_cost": entry.total_cost.as_dollars(),
                "bought": entry.bought,
            })
        })
        .collect();

    // Subsidies
    let subsidies: Vec<serde_json::Value> = nation
        .diplomacy.trade_subsidies
        .iter()
        .map(|(&target_nid, &amount)| {
            let target_name = game
                .get_nation(target_nid)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let has_consulate = game
                .world.diplomacy
                .get_relation(nid, target_nid)
                .map(|r| r.has_consulate)
                .unwrap_or(false);
            serde_json::json!({
                "nation_id": target_nid.0,
                "nation_name": target_name,
                "amount": amount.as_dollars(),
                "has_consulate": has_consulate,
            })
        })
        .collect();

    // Trade balance from history + auto-sold goods revenue
    let mut total_bought: i64 = 0;
    let mut total_sold: i64 = 0;
    for entry in &nation.archives.trade_history {
        if entry.bought {
            total_bought += entry.total_cost.as_dollars();
        } else {
            total_sold += entry.total_cost.as_dollars();
        }
    }
    total_sold += nation.archives.goods_sales_revenue_dollars;

    // Cargo capacity from merchant fleet
    let total_cargo: u32 = nation
        .military.merchant_fleet
        .iter()
        .map(|s| s.ship_type.stats().cargo)
        .sum();

    // Minor nations with consulates
    let minor_nations: Vec<serde_json::Value> = game
        .world.nations
        .iter()
        .filter(|n| n.nation_type == NationType::MinorNation && n.id != nid)
        .map(|n| {
            let rel = game.world.diplomacy.get_relation(nid, n.id);
            let has_consulate = rel.map(|r| r.has_consulate).unwrap_or(false);
            let has_embassy = rel.map(|r| r.has_embassy).unwrap_or(false);
            // Collect resources available in minor nation's provinces
            let mut mn_resources = Vec::new();
            for &pid in &n.province_ids {
                if let Some(prov) = game.get_province(pid) {
                    for &coord in &prov.tiles {
                        if let Some(tile) = game.world.hex_map.get_tile(coord)
                            && tile.has_visible_resource()
                            && let Some(r) = tile.resource_deposit()
                        {
                            let rs = format!("{:?}", r);
                            if !mn_resources.contains(&rs) {
                                mn_resources.push(rs);
                            }
                        }
                    }
                }
            }
            serde_json::json!({
                "nation_id": n.id.0,
                "name": n.name,
                "has_consulate": has_consulate,
                "has_embassy": has_embassy,
                "resources": mn_resources,
            })
        })
        .collect();

    // Player sell orders
    let cfg = &game.game_data.game_config;
    let player_sell_orders: Vec<serde_json::Value> = nation
        .diplomacy.player_sell_orders
        .iter()
        .map(|o| {
            let (ctype, cname) = match o.commodity {
                Commodity::Resource(r) => ("resource", format!("{:?}", r)),
                Commodity::Material(m) => ("material", format!("{}", m)),
                Commodity::Goods(g) => ("goods", format!("{:?}", g)),
            };
            serde_json::json!({
                "commodity_type": ctype,
                "commodity_name": cname,
                "quantity": o.quantity,
                "price": commodity_price(o.commodity, cfg).as_dollars(),
            })
        })
        .collect();

    // Player buy orders
    let player_buy_orders: Vec<serde_json::Value> = nation
        .diplomacy.player_buy_orders
        .iter()
        .map(|o| {
            serde_json::json!({
                "resource": format!("{:?}", o.resource),
                "quantity": o.quantity,
                "max_price": o.max_price_per_unit.as_dollars(),
            })
        })
        .collect();

    // Available offers from minor nations
    let mut available_offers: Vec<serde_json::Value> =
        domain::economy::trade::generate_minor_nation_offers(
            &game.world.nations,
            &game.world.provinces,
            &game.world.hex_map,
        )
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
                "quantity": o.quantity,
                "price": o.price_per_unit.as_dollars(),
                "is_great_power": false,
            })
        })
        .collect();

    // Add surplus offers from other Great Powers
    for gp in &game.world.nations {
        if gp.id == nid || !gp.is_great_power() {
            continue;
        }
        for (&resource, &qty) in &gp.economy.warehouse {
            if qty > 3 {
                let surplus = qty - 3;
                let price = base_price(resource);
                available_offers.push(serde_json::json!({
                    "seller_id": gp.id.0,
                    "seller_name": gp.name,
                    "resource": format!("{:?}", resource),
                    "quantity": surplus,
                    "price": price.as_dollars(),
                    "is_great_power": true,
                }));
            }
        }
    }

    // Sellable items: resources, materials, goods with stock > 0
    let sellable_resources: Vec<serde_json::Value> = all_resources
        .iter()
        .filter_map(|&r| {
            let stock = nation.resource_amount(r);
            if stock > 0 {
                Some(serde_json::json!({
                    "name": format!("{:?}", r),
                    "stock": stock,
                    "price": base_price(r).as_dollars(),
                }))
            } else {
                None
            }
        })
        .collect();

    let all_materials = [
        MaterialType::Lumber,
        MaterialType::Steel,
        MaterialType::Fabric,
        MaterialType::Paper,
        MaterialType::Arms,
        MaterialType::CannedFood,
    ];
    let sellable_materials: Vec<serde_json::Value> = all_materials
        .iter()
        .filter_map(|&m| {
            let stock = nation.economy.materials.get(&m).copied().unwrap_or(0);
            if stock > 0 {
                Some(serde_json::json!({
                    "name": format!("{}", m),
                    "stock": stock,
                    "price": domain::economy::trade::material_price(m, cfg).as_dollars(),
                }))
            } else {
                None
            }
        })
        .collect();

    let all_goods = [
        GoodsType::Furniture,
        GoodsType::Clothing,
        GoodsType::Hardware,
    ];
    let sellable_goods: Vec<serde_json::Value> = all_goods
        .iter()
        .filter_map(|&g| {
            let stock = nation.economy.goods.get(&g).copied().unwrap_or(0);
            if stock > 0 {
                Some(serde_json::json!({
                    "name": format!("{:?}", g),
                    "stock": stock,
                    "price": domain::economy::trade::goods_price(g, cfg).as_dollars(),
                }))
            } else {
                None
            }
        })
        .collect();

    // Remaining cargo after current orders
    let orders_qty: u32 = nation
        .diplomacy.player_sell_orders
        .iter()
        .map(|o| o.quantity)
        .chain(nation.diplomacy.player_buy_orders.iter().map(|o| o.quantity))
        .sum();
    let remaining_cargo = total_cargo.saturating_sub(orders_qty);

    serde_json::json!({
        "market_prices": market_prices,
        "trade_history": history,
        "subsidies": subsidies,
        "trade_balance": {
            "total_bought": total_bought,
            "total_sold": total_sold,
            "net": total_sold - total_bought,
        },
        "total_cargo": total_cargo,
        "remaining_cargo": remaining_cargo,
        "minor_nations": minor_nations,
        "treasury": nation.economy.treasury.as_dollars(),
        "player_sell_orders": player_sell_orders,
        "player_buy_orders": player_buy_orders,
        "available_offers": available_offers,
        "sellable_resources": sellable_resources,
        "sellable_materials": sellable_materials,
        "sellable_goods": sellable_goods,
    })
    .to_string()
}

/// Set trade subsidy for a minor nation.
#[wasm_bindgen]
pub fn wasm_set_trade_subsidy(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
    amount: i64,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target_nid = NationId(target_nation_id);

    // Validate target nation exists
    if game.get_nation(target_nid).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }
    if game
        .get_nation(target_nid)
        .map(|n| n.nation_type != NationType::MinorNation)
        .unwrap_or(true)
    {
        return "{\"error\":\"subsidies can only be set for minor nations\"}".to_string();
    }

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    if amount <= 0 {
        nation.diplomacy.trade_subsidies.remove(&target_nid);
    } else {
        nation
            .diplomacy.trade_subsidies
            .insert(target_nid, Money::dollars(amount));
    }

    serialize_game(&game)
}

/// Set a player sell order for a commodity.
#[wasm_bindgen]
pub fn wasm_set_player_sell_order(
    game_json: &str,
    nation_id: u32,
    commodity_type: &str,
    commodity_name: &str,
    quantity: u32,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);

    let commodity = match parse_commodity(commodity_type, commodity_name) {
        Some(c) => c,
        None => return r#"{"error":"invalid commodity"}"#.to_string(),
    };

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return r#"{"error":"nation not found"}"#.to_string(),
    };

    // Validate stock
    let available = match commodity {
        Commodity::Resource(r) => nation.resource_amount(r),
        Commodity::Material(m) => nation.economy.materials.get(&m).copied().unwrap_or(0),
        Commodity::Goods(g) => nation.economy.goods.get(&g).copied().unwrap_or(0),
    };
    if quantity > available {
        return r#"{"error":"insufficient stock"}"#.to_string();
    }

    // Validate cargo capacity
    let total_cargo: u32 = nation.total_cargo_capacity();
    let other_orders: u32 = nation
        .diplomacy.player_sell_orders
        .iter()
        .filter(|o| o.commodity != commodity)
        .map(|o| o.quantity)
        .chain(nation.diplomacy.player_buy_orders.iter().map(|o| o.quantity))
        .sum();
    if other_orders + quantity > total_cargo {
        return r#"{"error":"exceeds cargo capacity"}"#.to_string();
    }

    // Upsert: remove existing for this commodity, add new if qty > 0
    nation
        .diplomacy.player_sell_orders
        .retain(|o| o.commodity != commodity);
    if quantity > 0 {
        nation
            .diplomacy.player_sell_orders
            .push(domain::economy::trade::PlayerSellOrder {
                commodity,
                quantity,
            });
    }

    serialize_game(&game)
}

/// Set a player buy order for a resource from minor nations.
#[wasm_bindgen]
pub fn wasm_set_player_buy_order(
    game_json: &str,
    nation_id: u32,
    resource: &str,
    quantity: u32,
    max_price: i64,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);

    let resource_type = match parse_resource_type(resource) {
        Some(r) => r,
        None => return r#"{"error":"invalid resource type"}"#.to_string(),
    };

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return r#"{"error":"nation not found"}"#.to_string(),
    };

    // Validate cargo capacity
    let total_cargo: u32 = nation.total_cargo_capacity();
    let other_orders: u32 = nation
        .diplomacy.player_sell_orders
        .iter()
        .map(|o| o.quantity)
        .chain(
            nation
                .diplomacy.player_buy_orders
                .iter()
                .filter(|o| o.resource != resource_type)
                .map(|o| o.quantity),
        )
        .sum();
    if other_orders + quantity > total_cargo {
        return r#"{"error":"exceeds cargo capacity"}"#.to_string();
    }

    // Default max_price to 120% of base price if not specified
    let max_price_per_unit = if max_price > 0 {
        Money::dollars(max_price)
    } else {
        let bp = base_price(resource_type);
        Money::dollars(bp.as_dollars() * 120 / 100)
    };

    // Upsert: remove existing for this resource, add new if qty > 0
    nation
        .diplomacy.player_buy_orders
        .retain(|o| o.resource != resource_type);
    if quantity > 0 {
        nation
            .diplomacy.player_buy_orders
            .push(domain::economy::trade::PlayerBuyOrder {
                resource: resource_type,
                quantity,
                max_price_per_unit,
            });
    }

    serialize_game(&game)
}

// ══════════════════════════════════════════════════════════════════════
// FEATURE 4: Diplomacy Screen
// ══════════════════════════════════════════════════════════════════════

/// Query diplomacy screen data for a nation.
#[wasm_bindgen]
pub fn wasm_get_diplomacy_screen_data(game_json: &str, nation_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    let player_standing = game.world.diplomacy.standing.get(&nid).copied().unwrap_or(100);
    let treasury = nation.economy.treasury.as_dollars();
    let player_is_gp = nation.nation_type == NationType::GreatPower;
    let player_already_at_war = game.world.diplomacy.is_at_war_with_anyone(nid);
    let player_in_anarchy = nation.diplomacy.is_in_anarchy;

    let relations: Vec<serde_json::Value> = game
        .world.nations
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

            // Any pending proposal in either direction (for action gating, matches backend)
            let any_pending_nap = has_pending_nap
                || game.world.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == TreatyType::NonAggressionPact
                        && p.from == n.id
                        && p.to == nid
                });
            let any_pending_alliance = has_pending_alliance
                || game.world.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == TreatyType::Alliance && p.from == n.id && p.to == nid
                });
            let any_pending_peace = has_pending_peace
                || game.world.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == TreatyType::PeaceTreaty && p.from == n.id && p.to == nid
                });

            // Pre-compute available actions. No diplomatic interaction is
            // possible with a nation in anarchy (card #81); every action is
            // gated on `!target_in_anarchy`. Gating uses `raw_at_war` (the
            // actual backend relation) rather than the anarchy-inflated
            // `display_at_war` so button availability never contradicts
            // what the command handlers will accept.
            let can_build_consulate = !target_in_anarchy && !has_consulate && treasury >= 500;
            let can_build_embassy =
                !target_in_anarchy && has_consulate && !has_embassy && treasury >= 5000;
            let can_propose_nap = !target_in_anarchy
                && has_embassy
                && !raw_at_war
                && !has_nap
                && !has_alliance
                && !any_pending_nap
                && player_standing >= 30;
            let can_propose_alliance = !target_in_anarchy
                && has_embassy
                && !raw_at_war
                && !has_alliance
                && !any_pending_alliance
                && player_standing >= 30
                && player_is_gp
                && target_is_gp;
            let can_declare_war = !target_in_anarchy && !raw_at_war;
            let can_send_grant = !target_in_anarchy && !raw_at_war && treasury > 0;
            let can_break_treaty = !treaties.is_empty();
            let can_propose_peace = !target_in_anarchy && raw_at_war && !any_pending_peace;

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
                    "breakable_treaties": treaties,
                    "can_propose_peace": can_propose_peace,
                },
            })
        })
        .collect();

    serde_json::json!({
        "player_standing": player_standing,
        "treasury": treasury,
        "player_already_at_war": player_already_at_war,
        "relations": relations,
    })
    .to_string()
}

/// Build a consulate with a target nation ($500).
#[wasm_bindgen]
pub fn wasm_diplomacy_build_consulate(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return "{\"error\":\"cannot target self\"}".to_string();
    }
    if game.get_nation(target).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }
    if let Some(err) = reject_if_target_in_anarchy(&game, target) {
        return err;
    }

    // Validate actor nation exists
    if game.get_nation(nid).is_none() {
        return "{\"error\":\"nation not found\"}".to_string();
    }

    // Validate treasury before committing
    let consulate_cost = Money::dollars(500);
    if game
        .get_nation(nid)
        .map(|n| n.economy.treasury.as_dollars() < consulate_cost.as_dollars())
        .unwrap_or(false)
    {
        return "{\"error\":\"not enough treasury\"}".to_string();
    }

    let cost = match game.world.diplomacy.build_consulate(nid, target) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    if let Some(nation) = game.get_nation_mut(nid) {
        nation.economy.treasury -= cost;
    }

    serialize_game(&game)
}

/// Build an embassy with a target nation ($5,000).
#[wasm_bindgen]
pub fn wasm_diplomacy_build_embassy(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return "{\"error\":\"cannot target self\"}".to_string();
    }
    if game.get_nation(target).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }
    if let Some(err) = reject_if_target_in_anarchy(&game, target) {
        return err;
    }

    // Validate actor nation exists
    if game.get_nation(nid).is_none() {
        return "{\"error\":\"nation not found\"}".to_string();
    }

    // Validate treasury before committing
    let embassy_cost = Money::dollars(5000);
    if game
        .get_nation(nid)
        .map(|n| n.economy.treasury.as_dollars() < embassy_cost.as_dollars())
        .unwrap_or(false)
    {
        return "{\"error\":\"not enough treasury\"}".to_string();
    }

    let cost = match game.world.diplomacy.build_embassy(nid, target) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    if let Some(nation) = game.get_nation_mut(nid) {
        nation.economy.treasury -= cost;
    }

    serialize_game(&game)
}

/// Propose a Non-Aggression Pact.
#[wasm_bindgen]
pub fn wasm_diplomacy_propose_nap(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return "{\"error\":\"cannot target self\"}".to_string();
    }
    if game.get_nation(nid).is_none() {
        return "{\"error\":\"nation not found\"}".to_string();
    }
    if game.get_nation(target).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }
    if let Some(err) = reject_if_target_in_anarchy(&game, target) {
        return err;
    }

    let turn = game.turn;
    match game
        .world.diplomacy
        .propose_treaty(nid, target, TreatyType::NonAggressionPact, turn)
    {
        Ok(()) => {}
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    }

    serialize_game(&game)
}

/// Propose an Alliance.
#[wasm_bindgen]
pub fn wasm_diplomacy_propose_alliance(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return "{\"error\":\"cannot target self\"}".to_string();
    }
    if game.get_nation(nid).is_none() {
        return "{\"error\":\"nation not found\"}".to_string();
    }
    if game.get_nation(target).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }
    if let Some(err) = reject_if_target_in_anarchy(&game, target) {
        return err;
    }

    let turn = game.turn;
    match game
        .world.diplomacy
        .propose_treaty(nid, target, TreatyType::Alliance, turn)
    {
        Ok(()) => {}
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    }

    serialize_game(&game)
}

/// Declare war on a target nation.
#[wasm_bindgen]
pub fn wasm_diplomacy_declare_war(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return "{\"error\":\"cannot target self\"}".to_string();
    }
    if game.get_nation(nid).is_none() {
        return "{\"error\":\"nation not found\"}".to_string();
    }
    if game.get_nation(target).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }
    if let Some(err) = reject_if_target_in_anarchy(&game, target) {
        return err;
    }

    if game.world.diplomacy.is_at_war(nid, target) {
        return "{\"error\":\"already at war\"}".to_string();
    }

    let turn = game.turn;
    game.world.diplomacy.declare_war_at(nid, target, turn);
    serialize_game(&game)
}

/// Send a monetary grant to a nation to improve relations.
#[wasm_bindgen]
pub fn wasm_diplomacy_send_grant(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
    amount: i64,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return "{\"error\":\"cannot target self\"}".to_string();
    }
    if amount <= 0 {
        return "{\"error\":\"grant amount must be positive\"}".to_string();
    }

    let money = Money::dollars(amount);

    // Validate target exists
    if game.get_nation(target).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }
    if let Some(err) = reject_if_target_in_anarchy(&game, target) {
        return err;
    }

    // Check treasury
    {
        let nation = match game.get_nation(nid) {
            Some(n) => n,
            None => return "{\"error\":\"nation not found\"}".to_string(),
        };
        if nation.economy.treasury.as_dollars() < amount {
            return "{\"error\":\"not enough treasury\"}".to_string();
        }
    }

    // Deduct from treasury
    if let Some(nation) = game.get_nation_mut(nid) {
        nation.economy.treasury -= money;
    }

    game.world.diplomacy.send_grant(nid, target, money);
    serialize_game(&game)
}

/// Break a treaty with a target nation.
#[wasm_bindgen]
pub fn wasm_diplomacy_break_treaty(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
    treaty_type: &str,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return "{\"error\":\"cannot target self\"}".to_string();
    }
    if game.get_nation(target).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }

    let tt = match parse_treaty_type(treaty_type) {
        Some(t) => t,
        None => return "{\"error\":\"unknown treaty type\"}".to_string(),
    };

    game.world.diplomacy.break_treaty(nid, target, tt);
    serialize_game(&game)
}

/// Propose peace to a nation currently at war.
#[wasm_bindgen]
pub fn wasm_diplomacy_propose_peace(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let target = NationId(target_nation_id);

    if nid == target {
        return "{\"error\":\"cannot target self\"}".to_string();
    }
    if game.get_nation(target).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }
    if let Some(err) = reject_if_target_in_anarchy(&game, target) {
        return err;
    }

    let turn = game.turn;

    match game.world.diplomacy.propose_peace(nid, target, turn) {
        Ok(()) => {}
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    }

    serialize_game(&game)
}

// ══════════════════════════════════════════════════════════════════════
// FEATURE 6: Proposal Modal
// ══════════════════════════════════════════════════════════════════════

/// Get pending diplomatic proposals for a nation.
#[wasm_bindgen]
pub fn wasm_get_pending_proposals(game_json: &str, nation_id: u32) -> String {
    let game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);

    let proposals: Vec<serde_json::Value> = game
        .world.diplomacy
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
                    let attacker_name = p
                        .attacker
                        .and_then(|a| game.get_nation(a))
                        .map(|n| n.name.as_str())
                        .unwrap_or("an aggressor");
                    format!(
                        "{} requests your protection against {}",
                        from_name, attacker_name
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
            })
        })
        .collect();

    serde_json::json!({ "proposals": proposals }).to_string()
}

/// Accept a diplomatic proposal by index.
#[wasm_bindgen]
pub fn wasm_accept_proposal(game_json: &str, nation_id: u32, proposal_index: u32) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let idx = proposal_index as usize;

    if idx >= game.world.diplomacy.pending_proposals.len() {
        return "{\"error\":\"proposal index out of range\"}".to_string();
    }

    let proposal = game.world.diplomacy.pending_proposals[idx].clone();
    if proposal.to != nid {
        return "{\"error\":\"proposal not addressed to you\"}".to_string();
    }

    // Execute the treaty action — propagate errors
    match proposal.proposal_type {
        TreatyType::NonAggressionPact => {
            if let Err(e) = game.world.diplomacy.propose_pact(proposal.from, proposal.to) {
                return format!("{{\"error\":\"{}\"}}", e);
            }
        }
        TreatyType::Alliance => {
            if let Err(e) = game.world.diplomacy.propose_alliance(proposal.from, proposal.to) {
                return format!("{{\"error\":\"{}\"}}", e);
            }
        }
        TreatyType::PeaceTreaty => {
            game.world.diplomacy.queue_peace(proposal.from, proposal.to);
        }
        TreatyType::PactDefenseRequest => {
            if let Some(attacker_id) = proposal.attacker {
                let mut report = domain::turn::TurnReport::empty();
                domain::turn::accept_pact_defense(
                    &mut game,
                    nid,
                    attacker_id,
                    proposal.from,
                    &mut report,
                );
            } else {
                return "{\"error\":\"missing attacker context\"}".to_string();
            }
        }
        _ => {
            return "{\"error\":\"unsupported proposal type\"}".to_string();
        }
    }

    // Remove the proposal
    game.world.diplomacy.pending_proposals.remove(idx);

    serialize_game(&game)
}

/// Reject a diplomatic proposal by index.
#[wasm_bindgen]
pub fn wasm_reject_proposal(game_json: &str, nation_id: u32, proposal_index: u32) -> String {
    let mut game = match deserialize_game(game_json) {
        Ok(g) => g,
        Err(e) => return e,
    };
    let nid = NationId(nation_id);
    let idx = proposal_index as usize;

    if idx >= game.world.diplomacy.pending_proposals.len() {
        return "{\"error\":\"proposal index out of range\"}".to_string();
    }

    if game.world.diplomacy.pending_proposals[idx].to != nid {
        return "{\"error\":\"proposal not addressed to you\"}".to_string();
    }

    let proposal = game.world.diplomacy.pending_proposals.remove(idx);

    // For PactDefenseRequest: continue the cascade with remaining candidates
    if proposal.proposal_type == TreatyType::PactDefenseRequest {
        if let Some(attacker_id) = proposal.attacker {
            let remaining = proposal.cascade_remaining.unwrap_or_default();
            let mut report = domain::turn::TurnReport::empty();
            domain::turn::continue_pact_defense_cascade(
                &mut game,
                attacker_id,
                proposal.from,
                &remaining,
                &mut report,
            );
        }
    }

    serialize_game(&game)
}

/// Return comprehensive ledger/statistics data for a nation.
#[wasm_bindgen]
pub fn wasm_get_ledger_data(game_json: &str, nation_id: u32) -> String {
    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"Nation not found\"}".to_string(),
    };

    // Economy
    let treasury_dollars = nation.economy.treasury.as_dollars();
    let subsidies: Vec<serde_json::Value> = nation
        .diplomacy.trade_subsidies
        .iter()
        .map(|(target_id, amount)| {
            let name = game
                .get_nation(*target_id)
                .map(|n| n.name.as_str())
                .unwrap_or("?");
            serde_json::json!({"nation": name, "amount": amount.as_dollars()})
        })
        .collect();

    // Buildings
    let buildings: Vec<serde_json::Value> = nation
        .economy.buildings
        .iter()
        .map(|b| {
            serde_json::json!({
                "type": format!("{:?}", b.building_type),
                "capacity": b.capacity,
                "upgrading": b.turns_until_upgrade > 0,
            })
        })
        .collect();

    // Resources, materials, goods
    let resources: Vec<serde_json::Value> = nation
        .economy.warehouse
        .iter()
        .filter(|(_, qty)| **qty > 0)
        .map(|(rt, qty)| serde_json::json!({"name": format!("{:?}", rt), "quantity": qty}))
        .collect();
    let materials: Vec<serde_json::Value> = nation
        .economy.materials
        .iter()
        .filter(|(_, qty)| **qty > 0)
        .map(|(mt, qty)| serde_json::json!({"name": format!("{:?}", mt), "quantity": qty}))
        .collect();
    let goods: Vec<serde_json::Value> = nation
        .economy.goods
        .iter()
        .filter(|(_, qty)| **qty > 0)
        .map(|(gt, qty)| serde_json::json!({"name": format!("{:?}", gt), "quantity": qty}))
        .collect();

    // Military — army by type
    let mut army_counts: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    for unit in &nation.military.army {
        let type_name = format!("{:?}", unit.unit_type);
        let fp = unit.unit_type.stats().firepower;
        let entry = army_counts.entry(type_name).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += fp;
    }
    let army_by_type: Vec<serde_json::Value> = army_counts
        .iter()
        .map(|(name, (count, fp))| {
            serde_json::json!({"unit_type": name, "count": count, "firepower": fp})
        })
        .collect();
    let total_army_fp: u32 = army_counts.values().map(|(_, fp)| fp).sum();

    // Warships by type
    let mut warship_counts: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    for ship in &nation.military.warships {
        let type_name = format!("{:?}", ship.ship_type);
        *warship_counts.entry(type_name).or_insert(0) += 1;
    }
    let warships_by_type: Vec<serde_json::Value> = warship_counts
        .iter()
        .map(|(name, count)| serde_json::json!({"ship_type": name, "count": count}))
        .collect();

    // Diplomacy summary
    let standing = game.world.diplomacy.get_standing(nid);
    let mut consulate_count = 0u32;
    let mut embassy_count = 0u32;
    let mut treaties: Vec<serde_json::Value> = Vec::new();
    let mut wars: Vec<String> = Vec::new();

    for other in &game.world.nations {
        if other.id == nid {
            continue;
        }
        if let Some(rel) = game.world.diplomacy.get_relation(nid, other.id) {
            if rel.has_consulate {
                consulate_count += 1;
            }
            if rel.has_embassy {
                embassy_count += 1;
            }
            if rel.at_war {
                wars.push(other.name.clone());
            }
            for t in &rel.active_treaties {
                treaties.push(
                    serde_json::json!({"nation": other.name, "treaty_type": format!("{:?}", t)}),
                );
            }
        }
    }

    let result = serde_json::json!({
        "economy": {
            "treasury": treasury_dollars,
            "goods_revenue": nation.archives.goods_sales_revenue_dollars,
            "subsidies": subsidies,
        },
        "production": {
            "buildings": buildings,
            "resources": resources,
            "materials": materials,
            "goods": goods,
        },
        "military": {
            "army_by_type": army_by_type,
            "total_army_fp": total_army_fp,
            "total_army_count": nation.military.army.len(),
            "field_army_count": nation.field_army_count(),
            "militia_count": nation.military.army.len() - nation.field_army_count(),
            "warships_by_type": warships_by_type,
            "total_warship_count": nation.military.warships.len(),
            "merchant_ships": nation.military.merchant_fleet.len(),
            "total_arms_built": nation.military.total_arms_built,
            "generals_earned": nation.military.generals_earned,
        },
        "diplomacy": {
            "standing": standing,
            "consulates": consulate_count,
            "embassies": embassy_count,
            "treaties": treaties,
            "wars": wars,
        },
        "labor": {
            "untrained": nation.economy.labor.untrained,
            "trained": nation.economy.labor.trained,
            "expert": nation.economy.labor.expert,
            "total": nation.economy.labor.total_workers(),
        },
    });

    serde_json::to_string(&result).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Return ledger data for ALL Great Powers.
#[wasm_bindgen]
pub fn wasm_get_all_gp_ledger_data(game_json: &str) -> String {
    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let entries: Vec<serde_json::Value> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|nation| {
            let nid = nation.id;
            let nation_name = &nation.name;
            let nation_color = format!("{:?}", nation.color);
            let is_human = nid == game.human_player_nation;

            // Per-nation ledger data (same logic as wasm_get_ledger_data)
            let treasury_dollars = nation.economy.treasury.as_dollars();
            let provinces = nation.province_ids.len();

            let mut total_army_fp: u32 = 0;
            let total_army_count = nation.military.army.len();
            for unit in &nation.military.army {
                total_army_fp += unit.unit_type.stats().firepower;
            }
            let total_warship_count = nation.military.warships.len();
            let merchant_ships = nation.military.merchant_fleet.len();

            let building_count = nation.economy.buildings.len();

            let standing = game.world.diplomacy.get_standing(nid);
            let mut consulate_count = 0u32;
            let mut embassy_count = 0u32;
            let mut alliance_count = 0u32;
            let mut war_count = 0u32;
            let mut wars: Vec<String> = Vec::new();
            let mut alliances: Vec<String> = Vec::new();

            for other in &game.world.nations {
                if other.id == nid {
                    continue;
                }
                if let Some(rel) = game.world.diplomacy.get_relation(nid, other.id) {
                    if rel.has_consulate {
                        consulate_count += 1;
                    }
                    if rel.has_embassy {
                        embassy_count += 1;
                    }
                    if rel.at_war {
                        war_count += 1;
                        wars.push(other.name.clone());
                    }
                    if rel.has_treaty(domain::events::TreatyType::Alliance) {
                        alliance_count += 1;
                        alliances.push(other.name.clone());
                    }
                }
            }

            // Resource totals
            let total_resources: u32 = nation.economy.warehouse.values().sum();
            let total_materials: u32 = nation.economy.materials.values().sum();
            let total_goods: u32 = nation.economy.goods.values().sum();

            // Per-resource breakdown
            let resources_detail: serde_json::Map<String, serde_json::Value> = nation
                .economy.warehouse
                .iter()
                .map(|(k, v)| (format!("{:?}", k), serde_json::json!(*v)))
                .collect();

            // Per-material breakdown
            let materials_detail: serde_json::Map<String, serde_json::Value> = nation
                .economy.materials
                .iter()
                .map(|(k, v)| (format!("{:?}", k), serde_json::json!(*v)))
                .collect();

            // Per-goods breakdown
            let goods_detail: serde_json::Map<String, serde_json::Value> = nation
                .economy.goods
                .iter()
                .map(|(k, v)| (format!("{:?}", k), serde_json::json!(*v)))
                .collect();

            // Technology data
            let researched_count = nation.researched_techs.len();
            let researched_names: Vec<String> = nation
                .researched_techs
                .iter()
                .filter_map(|tid| {
                    game.game_data
                        .tech_tree
                        .all_techs()
                        .iter()
                        .find(|t| t.id == *tid)
                        .map(|t| t.name.clone())
                })
                .collect();

            // Per-nation cash-flow breakdown (last processed turn) — read from
            // `game.transient.last_cash_flow`, populated by the turn processor.
            let cash_flow_json = if let Some(flow) = game.transient.last_cash_flow.get(&nid) {
                let income_map: serde_json::Map<String, serde_json::Value> = flow
                    .income_totals_by_source()
                    .into_iter()
                    .map(|(k, v)| (k.label().to_string(), serde_json::json!(v)))
                    .collect();
                let expense_map: serde_json::Map<String, serde_json::Value> = flow
                    .expense_totals_by_sink()
                    .into_iter()
                    .map(|(k, v)| (k.label().to_string(), serde_json::json!(v)))
                    .collect();
                let income_by_cat: serde_json::Map<String, serde_json::Value> = flow
                    .income_by_category()
                    .into_iter()
                    .map(|(k, v)| (k.label().to_string(), serde_json::json!(v)))
                    .collect();
                let expense_by_cat: serde_json::Map<String, serde_json::Value> = flow
                    .expense_by_category()
                    .into_iter()
                    .map(|(k, v)| (k.label().to_string(), serde_json::json!(v)))
                    .collect();
                serde_json::json!({
                    "opening_treasury": flow.opening_treasury.as_dollars(),
                    "closing_treasury": flow.closing_treasury.as_dollars(),
                    "total_income": flow.total_income().as_dollars(),
                    "total_expense": flow.total_expense().as_dollars(),
                    "observed_delta": flow.observed_delta().as_dollars(),
                    "accounted_delta": flow.accounted_delta().as_dollars(),
                    "reconciliation_mismatch": flow.reconciliation_mismatch().as_dollars(),
                    "reconciles": flow.reconciles(),
                    "income_totals": income_map,
                    "expense_totals": expense_map,
                    "income_by_category": income_by_cat,
                    "expense_by_category": expense_by_cat,
                })
            } else {
                serde_json::Value::Null
            };
            let cumulative_income: serde_json::Map<String, serde_json::Value> = nation
                .archives.cash_income_totals
                .iter()
                .map(|(k, v)| (format!("{:?}", k), serde_json::json!(*v)))
                .collect();
            let cumulative_expense: serde_json::Map<String, serde_json::Value> = nation
                .archives.cash_expense_totals
                .iter()
                .map(|(k, v)| (format!("{:?}", k), serde_json::json!(*v)))
                .collect();

            // Resource-flow (last turn) — best-effort visibility, NOT reconciled.
            let resource_flow_json = if let Some(flow) = game.transient.last_resource_flow.get(&nid) {
                let inflow: Vec<serde_json::Value> = flow
                    .inflow
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "stockpile": e.stockpile.label(),
                            "source": e.source.label(),
                            "category": e.source.category().label(),
                            "amount": e.amount,
                        })
                    })
                    .collect();
                let outflow: Vec<serde_json::Value> = flow
                    .outflow
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "stockpile": e.stockpile.label(),
                            "sink": e.sink.label(),
                            "category": e.sink.category().label(),
                            "amount": e.amount,
                        })
                    })
                    .collect();
                // Per-stockpile inflow by category: { "Timber": { "Production": 10, "Trade": 5 } }
                let mut inflow_by_stockpile: serde_json::Map<String, serde_json::Value> =
                    serde_json::Map::new();
                for (stock, by_cat) in flow.inflow_by_stockpile_and_category() {
                    let m: serde_json::Map<String, serde_json::Value> = by_cat
                        .into_iter()
                        .map(|(c, v)| (c.label().to_string(), serde_json::json!(v)))
                        .collect();
                    inflow_by_stockpile.insert(stock.label(), serde_json::Value::Object(m));
                }
                let mut outflow_by_stockpile: serde_json::Map<String, serde_json::Value> =
                    serde_json::Map::new();
                for (stock, by_cat) in flow.outflow_by_stockpile_and_category() {
                    let m: serde_json::Map<String, serde_json::Value> = by_cat
                        .into_iter()
                        .map(|(c, v)| (c.label().to_string(), serde_json::json!(v)))
                        .collect();
                    outflow_by_stockpile.insert(stock.label(), serde_json::Value::Object(m));
                }
                serde_json::json!({
                    "inflow": inflow,
                    "outflow": outflow,
                    "inflow_by_stockpile_category": inflow_by_stockpile,
                    "outflow_by_stockpile_category": outflow_by_stockpile,
                })
            } else {
                serde_json::Value::Null
            };

            serde_json::json!({
                "nation_id": nid.0,
                "nation_name": nation_name,
                "nation_color": nation_color,
                "is_human": is_human,
                "economy": {
                    "treasury": treasury_dollars,
                    "provinces": provinces,
                    "buildings": building_count,
                    "goods_revenue": nation.archives.goods_sales_revenue_dollars,
                    "total_resources": total_resources,
                    "total_materials": total_materials,
                    "total_goods": total_goods,
                },
                "cash_flow": cash_flow_json,
                "resource_flow": resource_flow_json,
                "cumulative": {
                    "income_totals": cumulative_income,
                    "expense_totals": cumulative_expense,
                },
                "labor": {
                    "untrained": nation.economy.labor.untrained,
                    "trained": nation.economy.labor.trained,
                    "expert": nation.economy.labor.expert,
                    "total": nation.economy.labor.total_workers(),
                },
                "military": {
                    "total_army_count": total_army_count,
                    "total_army_fp": total_army_fp,
                    "field_army_count": nation.field_army_count(),
                    "militia_count": total_army_count - nation.field_army_count(),
                    "total_warship_count": total_warship_count,
                    "merchant_ships": merchant_ships,
                    "generals_earned": nation.military.generals_earned,
                    "total_arms_built": nation.military.total_arms_built,
                },
                "diplomacy": {
                    "standing": standing,
                    "consulates": consulate_count,
                    "embassies": embassy_count,
                    "alliances": alliance_count,
                    "alliance_names": alliances,
                    "wars": war_count,
                    "war_names": wars,
                },
                "resources_detail": resources_detail,
                "materials_detail": materials_detail,
                "goods_detail": goods_detail,
                "technology": {
                    "researched_count": researched_count,
                    "researched_names": researched_names,
                },
            })
        })
        .collect();

    serde_json::to_string(&entries).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Return a political-map snapshot for a specific past turn. Each tile is
/// annotated with the owning nation at that turn, plus the display flags
/// needed to render a read-only political view in a modal.
///
/// Returns `{"error": "..."}` if the game can't be deserialized or the
/// requested turn has no snapshot.
#[wasm_bindgen]
pub fn wasm_get_political_snapshot(game_json: &str, turn: u32) -> String {
    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let target = TurnNumber::new(turn);
    let Some((_, snapshot)) = game.archive.political_archive.iter().find(|(t, _)| *t == target) else {
        return format!("{{\"error\":\"no political snapshot for turn {}\"}}", turn);
    };

    // Rebuild province_id → (owner NationId, incorporated_from) at that turn.
    let prov_state: std::collections::HashMap<ProvinceId, (NationId, Option<NationId>)> = snapshot
        .provinces
        .iter()
        .map(|&(pid, owner, inc)| (pid, (owner, inc)))
        .collect();

    let nation_lookup: std::collections::HashMap<NationId, (&str, String, NationType)> = game
        .world.nations
        .iter()
        .map(|n| {
            (
                n.id,
                (n.name.as_str(), format!("{:?}", n.color), n.nation_type),
            )
        })
        .collect();

    // Capital provinces at the archived turn (not current). Capital can move
    // during the game — using current state would mis-place historical markers.
    let country_capital_provinces: std::collections::HashSet<ProvinceId> =
        snapshot.capitals.iter().map(|&(_, pid)| pid).collect();

    let province_name: std::collections::HashMap<ProvinceId, &str> = game
        .world.provinces
        .iter()
        .map(|p| (p.id, p.name.as_str()))
        .collect();

    let map_width = game.world.hex_map.width();
    let map_height = game.world.hex_map.height();

    let tiles: Vec<serde_json::Value> = game
        .world.hex_map
        .all_tiles()
        .map(|(coord, tile)| {
            let (owner_name, owner_color, is_minor, is_incorporated_minor, visual_group) = tile
                .province_id
                .and_then(|pid| prov_state.get(&pid).copied())
                .and_then(|(owner, inc)| {
                    nation_lookup.get(&owner).map(|(name, color, ntype)| {
                        let incorporated = inc.is_some();
                        let is_minor = *ntype == NationType::MinorNation;
                        // Independent minors always render as Beige; incorporated
                        // minors keep the overlord color but lighter.
                        let display_color = if is_minor && !incorporated && !color.is_empty() {
                            "Beige".to_string()
                        } else {
                            color.clone()
                        };
                        // Visual group: incorporated minors keep a separate
                        // border group keyed on the original minor's name, so
                        // they read as distinct countries in the political view.
                        let vg: Option<String> = inc
                            .and_then(|nid| nation_lookup.get(&nid))
                            .map(|(n, _, _)| (*n).to_string());
                        (*name, display_color, is_minor, incorporated, vg)
                    })
                })
                .unwrap_or(("", String::new(), false, false, None));

            let prov_name = tile
                .province_id
                .and_then(|pid| province_name.get(&pid).copied())
                .unwrap_or("");

            let is_country_capital = tile.is_capital
                && tile
                    .province_id
                    .is_some_and(|pid| country_capital_provinces.contains(&pid));

            serde_json::json!({
                "q": coord.q,
                "r": coord.r,
                "terrain": format!("{:?}", tile.terrain()),
                "owner": owner_name,
                "owner_color": owner_color,
                "province": prov_name,
                "is_capital": tile.is_capital,
                "is_country_capital": is_country_capital,
                "is_minor": is_minor,
                "is_incorporated_minor": is_incorporated_minor,
                "visual_group": visual_group,
            })
        })
        .collect();

    let response = serde_json::json!({
        "turn": target.0,
        "year": target.year(),
        "quarter": target.quarter(),
        "map_width": map_width,
        "map_height": map_height,
        "tiles": tiles,
    });
    response.to_string()
}

/// Return the newspaper headline archive for all past turns.
#[wasm_bindgen]
pub fn wasm_get_newspaper_archive(game_json: &str) -> String {
    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let archive: Vec<serde_json::Value> = game
        .archive.newspaper_archive
        .iter()
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

    serde_json::to_string(&archive).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Serialize a land battle result to JSON, resolving nation/province names from game state.
fn serialize_battle(b: &BattleResult, game: &GameState) -> serde_json::Value {
    let attacker_name = game
        .get_nation(b.attacker)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let defender_name = game
        .get_nation(b.defender)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let province_name = game
        .get_province(b.province)
        .map(|p| p.name.as_str())
        .unwrap_or("Unknown");
    let capital_tile = game
        .get_province(b.province)
        .map(|p| serde_json::json!({"q": p.capital_tile.q, "r": p.capital_tile.r}));
    let province_tiles: Vec<serde_json::Value> = game
        .get_province(b.province)
        .map(|p| {
            p.tiles
                .iter()
                .map(|t| serde_json::json!({"q": t.q, "r": t.r}))
                .collect()
        })
        .unwrap_or_default();
    let origin_tiles: Vec<serde_json::Value> = b
        .attacker_origin_provinces
        .iter()
        .filter_map(|pid| {
            game.get_province(*pid)
                .map(|p| serde_json::json!({"q": p.capital_tile.q, "r": p.capital_tile.r}))
        })
        .collect();
    let origin_province_names: Vec<String> = b
        .attacker_origin_provinces
        .iter()
        .filter_map(|pid| game.get_province(*pid).map(|p| p.name.clone()))
        .collect();

    let serialize_units = |units: &[ArmyUnit]| -> Vec<serde_json::Value> {
        units
            .iter()
            .map(|u| {
                serde_json::json!({
                    "unit_type": format!("{:?}", u.unit_type),
                    "health": u.health,
                    "medals": u.medals,
                    "effective_firepower": u.effective_firepower(),
                })
            })
            .collect()
    };

    serde_json::json!({
        "type": "land",
        "attacker": attacker_name,
        "attacker_id": b.attacker.0,
        "defender": defender_name,
        "defender_id": b.defender.0,
        "province": province_name,
        "province_id": b.province.0,
        "attacker_won": b.attacker_won,
        "retreated": b.retreated,
        "attacker_casualties": b.attacker_casualties.iter()
            .map(|c| format!("{:?}", c)).collect::<Vec<_>>(),
        "defender_casualties": b.defender_casualties.iter()
            .map(|c| format!("{:?}", c)).collect::<Vec<_>>(),
        "attacker_survivors": serialize_units(&b.attacker_survivors),
        "defender_survivors": serialize_units(&b.defender_survivors),
        "terrain": b.terrain.map(|t| format!("{:?}", t)),
        "fort_level": b.fort_level,
        "siege_reduced_fort": b.siege_reduced_fort,
        "attacker_initial_count": b.attacker_initial_count,
        "defender_initial_count": b.defender_initial_count,
        "attacker_survivors_count": b.attacker_initial_count.saturating_sub(b.attacker_casualties.len()),
        "defender_survivors_count": b.defender_initial_count.saturating_sub(b.defender_casualties.len()),
        "medal_awards": b.medal_awards.iter()
            .map(|(t, c)| serde_json::json!({"unit_type": format!("{:?}", t), "medals": c}))
            .collect::<Vec<_>>(),
        "capital_tile": capital_tile,
        "province_tiles": province_tiles,
        "origin_tiles": origin_tiles,
        "origin_province_names": origin_province_names,
        "is_naval_landing": b.is_naval_landing,
    })
}

/// Serialize a naval battle result to JSON, resolving nation names from game state.
fn serialize_naval_battle(nb: &NavalBattleResult, game: &GameState) -> serde_json::Value {
    let attacker_name = game
        .get_nation(nb.attacker)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let defender_name = game
        .get_nation(nb.defender)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");

    serde_json::json!({
        "type": "naval",
        "attacker": attacker_name,
        "attacker_id": nb.attacker.0,
        "defender": defender_name,
        "defender_id": nb.defender.0,
        "attacker_won": nb.attacker_won,
        "attacker_ships_lost": nb.attacker_ships_lost.iter()
            .map(|s| format!("{:?}", s)).collect::<Vec<_>>(),
        "defender_ships_lost": nb.defender_ships_lost.iter()
            .map(|s| format!("{:?}", s)).collect::<Vec<_>>(),
        "attacker_survivors_count": nb.attacker_survivors.len(),
        "defender_survivors_count": nb.defender_survivors.len(),
    })
}

/// Return the battle archive for all past turns.
#[wasm_bindgen]
pub fn wasm_get_battle_data(game_json: &str) -> String {
    let game = match game_from_json(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let archive: Vec<serde_json::Value> = game
        .archive.battle_archive
        .iter()
        .map(|(turn, battles, naval_battles)| {
            let land: Vec<serde_json::Value> =
                battles.iter().map(|b| serialize_battle(b, &game)).collect();
            let naval: Vec<serde_json::Value> = naval_battles
                .iter()
                .map(|nb| serialize_naval_battle(nb, &game))
                .collect();
            serde_json::json!({
                "turn": turn.0,
                "year": turn.year(),
                "quarter": turn.quarter(),
                "battles": land,
                "naval_battles": naval,
            })
        })
        .collect();

    serde_json::to_string(&archive).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::diplomacy::DiplomaticProposal;
    use domain::events::TreatyType;

    fn make_game_json() -> String {
        let game = new_game("default", Difficulty::Normal, 0);
        serde_json::to_string(&game).unwrap()
    }

    // ── Parser tests ──────────────────────────────────────────

    #[test]
    fn parse_army_unit_type_valid() {
        assert_eq!(
            parse_army_unit_type("Regulars"),
            Some(ArmyUnitType::Regulars)
        );
        assert_eq!(parse_army_unit_type("Guards"), Some(ArmyUnitType::Guards));
        assert_eq!(parse_army_unit_type("General"), Some(ArmyUnitType::General));
    }

    #[test]
    fn parse_army_unit_type_invalid() {
        assert_eq!(parse_army_unit_type("Wizard"), None);
        assert_eq!(parse_army_unit_type(""), None);
    }

    #[test]
    fn parse_ship_type_valid() {
        assert_eq!(parse_ship_type("Frigate"), Some(ShipType::Frigate));
        assert_eq!(parse_ship_type("Trader"), Some(ShipType::Trader));
    }

    #[test]
    fn parse_ship_type_invalid() {
        assert_eq!(parse_ship_type("Submarine"), None);
    }

    // ── wasm_get_navy_markers tests ───────────────────────────

    /// Build a minimal game state where nation 0 owns a coastal province with
    /// a port, give it two Frigates and one Ironclad, then invoke
    /// `wasm_get_navy_markers` and parse the result.
    fn setup_navy_markers_game() -> (GameState, String) {
        let mut game = new_game("default", Difficulty::Normal, 0);
        game.game_data = domain::data::GameData::default();

        // Promote the first coastal province of the human player to a port and
        // put some warships on it. The map generator already sets `coastal`
        // for coastal provinces.
        let human = game.human_player_nation;
        let coastal_pid: Option<ProvinceId> = game
            .world.provinces
            .iter()
            .find(|p| p.owner == human && p.is_coastal())
            .map(|p| p.id);
        let pid = coastal_pid.expect("test map must have at least one coastal GP province");

        // Pick a tile in that province and mark it as a port.
        let tile_coord = {
            let prov = game.get_province(pid).unwrap();
            prov.tiles.first().copied().unwrap()
        };
        if let Some(t) = game.world.hex_map.get_tile_mut(tile_coord) {
            t.infrastructure.has_port = true;
        }

        // Give the human nation three warships: two Frigates on Patrol and
        // one Ironclad on Escort.
        let nation = game.get_nation_mut(human).unwrap();
        let mk_ship =
            |id: u32, ship_type: ShipType, op: Option<domain::military::naval::NavalOperation>| {
                let mut s = Ship::new(domain::map::UnitId(id), ship_type, human);
                s.operation = op;
                s
            };
        nation.military.warships.clear();
        nation.military.warships.push(mk_ship(
            9000,
            ShipType::Frigate,
            Some(domain::military::naval::NavalOperation::Patrol),
        ));
        nation.military.warships.push(mk_ship(
            9001,
            ShipType::Frigate,
            Some(domain::military::naval::NavalOperation::Patrol),
        ));
        nation.military.warships.push(mk_ship(
            9002,
            ShipType::Ironclad,
            Some(domain::military::naval::NavalOperation::Escort),
        ));

        let json = serde_json::to_string(&game).unwrap();
        (game, json)
    }

    #[test]
    fn navy_markers_emits_fleet_marker_for_human() {
        let (_, json) = setup_navy_markers_game();
        let result = wasm_get_navy_markers(&json, false);
        let markers: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

        let human_markers: Vec<_> = markers
            .iter()
            .filter(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(0))
            .collect();
        assert_eq!(
            human_markers.len(),
            1,
            "human player should have exactly one fleet marker"
        );

        let m = human_markers[0];
        assert_eq!(m["kind"], "fleet");
        assert_eq!(m["ship_count"], 3);
        assert_eq!(m["visible"], true);
        // 2 Frigates + 1 Ironclad grouped into by_type.
        let by_type = m["by_type"].as_object().unwrap();
        assert_eq!(by_type.get("Frigate").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(by_type.get("Ironclad").and_then(|v| v.as_u64()), Some(1));
        // by_operation reports Patrol × 2 + Escort × 1.
        let by_op = m["by_operation"].as_object().unwrap();
        assert_eq!(by_op.get("Patrol").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(by_op.get("Escort").and_then(|v| v.as_u64()), Some(1));
    }

    #[test]
    fn navy_markers_splits_beachhead_into_its_own_marker() {
        let (mut game, _) = setup_navy_markers_game();
        // Re-assign the Ironclad to Beachhead a hostile coastal province.
        let human = game.human_player_nation;
        let beachhead_pid: ProvinceId = game
            .world.provinces
            .iter()
            .find(|p| p.owner != human && p.is_coastal())
            .map(|p| p.id)
            .expect("need a hostile coastal province for beachhead");
        let nation = game.get_nation_mut(human).unwrap();
        nation.military.warships[2].operation = Some(domain::military::naval::NavalOperation::Beachhead(
            beachhead_pid,
        ));
        let json = serde_json::to_string(&game).unwrap();

        let result = wasm_get_navy_markers(&json, false);
        let markers: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        let human_markers: Vec<_> = markers
            .iter()
            .filter(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(0))
            .collect();
        assert_eq!(
            human_markers.len(),
            2,
            "one fleet marker + one beachhead marker"
        );
        assert!(human_markers.iter().any(|m| m["kind"] == "fleet"));
        let beachhead = human_markers
            .iter()
            .find(|m| m["kind"] == "beachhead")
            .expect("beachhead marker present");
        assert_eq!(beachhead["ship_count"], 1);
        assert!(beachhead.get("target_province").is_some());
        assert!(beachhead.get("target_hex").is_some());
    }

    #[test]
    fn navy_markers_fog_hides_invisible_enemy_fleets() {
        // Positive-case fog test. We build a game where a specific non-human
        // nation has a warship, then confirm that its marker is present
        // WITHOUT fog but absent WITH fog (because its anchor hex is not
        // visible to the human player).
        let (mut game, _) = setup_navy_markers_game();

        // Pick an enemy GP whose capital province has been generated as
        // coastal. Some enemies may be inland; skip those.
        let human = game.human_player_nation;
        let enemy_id: NationId = {
            let enemy = game
                .world.nations
                .iter()
                .find(|n| {
                    n.id != human
                        && n.nation_type == NationType::GreatPower
                        && game
                            .world.provinces
                            .iter()
                            .any(|p| p.owner == n.id && p.is_coastal())
                })
                .expect("need a coastal enemy GP for the fog test");
            enemy.id
        };

        // Give the enemy one Frigate on Patrol.
        let mut enemy_ship = Ship::new(domain::map::UnitId(9500), ShipType::Frigate, enemy_id);
        enemy_ship.operation = Some(domain::military::naval::NavalOperation::Patrol);
        let enemy = game.get_nation_mut(enemy_id).unwrap();
        enemy.military.warships.clear();
        enemy.military.warships.push(enemy_ship);

        // Compute where that fleet marker would land and confirm the anchor
        // is outside the human's visible set, so the fog filter is the only
        // thing keeping the marker hidden.
        let enemy_nation = game.get_nation(enemy_id).unwrap();
        let anchor = domain::military::navy_placement::fleet_anchor(
            enemy_nation,
            &game.world.hex_map,
            &game.world.provinces,
        )
        .expect("enemy should have a fleet anchor");
        let visible_hexes = compute_visible_hexes(&game, false);
        assert!(
            !visible_hexes.contains(&anchor),
            "enemy anchor hex must be outside human visibility for this test to be meaningful",
        );

        let json = serde_json::to_string(&game).unwrap();

        // Fogged: enemy marker must be absent.
        let fogged = wasm_get_navy_markers(&json, false);
        let fogged_markers: Vec<serde_json::Value> = serde_json::from_str(&fogged).unwrap();
        assert!(
            !fogged_markers
                .iter()
                .any(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(enemy_id.0 as u64)),
            "fogged call must NOT include the enemy's marker",
        );

        // Unfogged: enemy marker must be present.
        let unfogged = wasm_get_navy_markers(&json, true);
        let unfogged_markers: Vec<serde_json::Value> = serde_json::from_str(&unfogged).unwrap();
        assert!(
            unfogged_markers
                .iter()
                .any(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(enemy_id.0 as u64)),
            "unfogged call must include the enemy's marker",
        );

        // Invariant: emitted markers are always marked visible.
        for m in &fogged_markers {
            assert_eq!(
                m["visible"], true,
                "wasm_get_navy_markers must never emit visible:false",
            );
        }
    }

    #[test]
    fn navy_markers_deterministic_across_runs() {
        let (_, json) = setup_navy_markers_game();
        let a = wasm_get_navy_markers(&json, false);
        let b = wasm_get_navy_markers(&json, false);
        assert_eq!(
            a, b,
            "marker output must be byte-identical for the same game state"
        );
    }

    // ── Move validation tests ─────────────────────────────────

    #[test]
    fn queue_move_rejects_nonexistent_unit() {
        let json = make_game_json();
        let game = game_from_json(&json).unwrap();
        let nid = game.human_player_nation.0;
        let result = wasm_queue_unit_move(&json, nid, 9999999, 1);
        assert!(result.contains("error"));
    }

    #[test]
    fn queue_move_replaces_duplicate() {
        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nid = game.human_player_nation;

        let nation = game.get_nation(nid).unwrap();
        let unit = nation.military.army.iter().find(|u| u.unit_type.can_move());
        if unit.is_none() {
            return;
        }
        let uid = unit.unwrap().id.0;

        let own_provs: Vec<u32> = game
            .world.provinces
            .iter()
            .filter(|p| p.owner == nid)
            .take(2)
            .map(|p| p.id.0)
            .collect();
        if own_provs.len() < 2 {
            return;
        }

        let json = serde_json::to_string(&game).unwrap();
        let result1 = wasm_queue_unit_move(&json, nid.0, uid, own_provs[0]);
        assert!(!result1.contains("error"));

        let result2 = wasm_queue_unit_move(&result1, nid.0, uid, own_provs[1]);
        assert!(!result2.contains("error"));
        let game2 = game_from_json(&result2).unwrap();
        let moves_for_unit = game2
            .transient.pending_moves
            .iter()
            .filter(|(_, id, _)| id.0 == uid)
            .count();
        assert_eq!(moves_for_unit, 1);
    }

    #[test]
    fn wasm_accept_peace_preserves_same_turn_coalition_alliance() {
        let mut game = new_game("wasm_peace", Difficulty::Normal, 0);
        let human = game.human_player_nation;
        let gp_ids: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
        let enemy = gp_ids[1];
        let ally = gp_ids[2];

        game.world.diplomacy.propose_alliance(human, ally).unwrap();
        game.world.diplomacy.declare_war(enemy, human);
        game.world.diplomacy.declare_war(ally, enemy);
        game.world.diplomacy.pending_proposals.push(DiplomaticProposal {
            from: enemy,
            to: human,
            proposal_type: TreatyType::PeaceTreaty,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });
        game.world.diplomacy
            .propose_peace(ally, enemy, game.turn)
            .unwrap();
        game.world.diplomacy.pending_proposals.push(DiplomaticProposal {
            from: enemy,
            to: ally,
            proposal_type: TreatyType::PeaceTreaty,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });

        let accepted_json = wasm_accept_proposal(&serialize_game(&game), human.0, 0);
        let mut accepted_game = game_from_json(&accepted_json).unwrap();

        assert!(
            !accepted_game.world.diplomacy.is_at_war(human, enemy),
            "human peace acceptance should clear the war immediately"
        );
        assert!(
            accepted_game
                .world.diplomacy
                .has_treaty(human, ally, TreatyType::Alliance),
            "alliance should remain pending same-turn reconciliation"
        );

        let report = process_turn(&mut accepted_game);

        assert!(
            accepted_game
                .world.diplomacy
                .has_treaty(human, ally, TreatyType::Alliance),
            "coordinated same-turn coalition peace via wasm should preserve the alliance"
        );
        assert!(
            report
                .newspaper_headlines
                .iter()
                .all(|h| !h.text.contains("breaks its alliance")),
            "coordinated wasm peace should not publish a separate-peace alliance-break headline"
        );
    }

    #[test]
    fn recruit_general_rejected() {
        let json = make_game_json();
        let game = game_from_json(&json).unwrap();
        let nid = game.human_player_nation.0;
        let result = wasm_recruit_army_unit(&json, nid, "General");
        assert!(result.contains("error"));
    }

    #[test]
    fn hire_civilian_insufficient_funds() {
        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nation = game.get_nation_mut(game.human_player_nation).unwrap();
        nation.economy.treasury = Money::ZERO;
        let json = serde_json::to_string(&game).unwrap();

        let result = wasm_hire_civilian(&json, game.human_player_nation.0, "Miner");
        assert!(result.contains("insufficient funds"));
    }

    #[test]
    fn buildable_units_includes_tech_met_for_civilians() {
        let json = make_game_json();
        let game = game_from_json(&json).unwrap();
        let result = wasm_get_buildable_units(&json, game.human_player_nation.0);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let civilians = parsed["civilians"].as_array().unwrap();
        for civ in civilians {
            assert!(civ["tech_met"].as_bool().is_some());
        }
    }

    #[test]
    fn get_civilians_undeployed_has_null_position() {
        let json = make_game_json();
        let game = game_from_json(&json).unwrap();
        let result = wasm_get_civilians(&json, game.human_player_nation.0);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        for civ in parsed["undeployed"].as_array().unwrap_or(&vec![]) {
            assert!(civ.get("position").is_some());
            assert!(civ["position"].is_null());
        }
    }

    #[test]
    fn cancel_move_removes_pending() {
        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();

        let nid = game.human_player_nation;
        game.transient.pending_moves
            .push((nid, domain::map::UnitId(12345), ProvinceId(1)));
        let json = serde_json::to_string(&game).unwrap();

        let result = wasm_cancel_unit_move(&json, 12345);
        assert!(!result.contains("error"));
        let game2 = game_from_json(&result).unwrap();
        assert!(!game2.transient.pending_moves.iter().any(|(_, id, _)| id.0 == 12345));
    }

    // ── F-018: Anarchic target + deploy occupancy tests ───────

    #[test]
    fn valid_move_targets_includes_anarchic_provinces() {
        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nid = game.human_player_nation;

        // Find an enemy nation and make it anarchic
        if let Some(enemy) = game.world.nations.iter_mut().find(|n| n.id != nid) {
            enemy.diplomacy.is_in_anarchy = true;
            let enemy_id = enemy.id;

            // Ensure we have a movable unit
            let nation = game.get_nation(nid).unwrap();
            let unit = nation.military.army.iter().find(|u| u.unit_type.can_move());
            if let Some(unit) = unit {
                let uid = unit.id.0;
                let json = serde_json::to_string(&game).unwrap();
                let result = wasm_get_valid_move_targets(&json, nid.0, uid);
                let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
                let hostile = parsed["hostile"].as_array().unwrap();
                // Anarchic nation's provinces should appear in hostile targets
                let has_anarchic =
                    game.world.provinces.iter().any(|p| p.owner == enemy_id) && !hostile.is_empty();
                if game.world.provinces.iter().any(|p| p.owner == enemy_id) {
                    assert!(
                        !hostile.is_empty(),
                        "Anarchic provinces should appear as hostile targets"
                    );
                }
                let _ = has_anarchic; // avoid unused warning
            }
        }
    }

    #[test]
    fn queue_move_allows_anarchic_target() {
        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nid = game.human_player_nation;

        // Find an enemy province and make its owner anarchic
        let enemy_prov = game
            .world.provinces
            .iter()
            .find(|p| p.owner != nid)
            .map(|p| (p.id, p.owner));
        if let Some((pid, enemy_nid)) = enemy_prov {
            if let Some(enemy) = game.world.nations.iter_mut().find(|n| n.id == enemy_nid) {
                enemy.diplomacy.is_in_anarchy = true;
            }
            let nation = game.get_nation(nid).unwrap();
            if let Some(unit) = nation.military.army.iter().find(|u| u.unit_type.can_move()) {
                let uid = unit.id.0;
                let json = serde_json::to_string(&game).unwrap();
                let result = wasm_queue_unit_move(&json, nid.0, uid, pid.0);
                assert!(
                    !result.contains("error"),
                    "Should allow moving to anarchic province"
                );
            }
        }
    }

    #[test]
    fn queue_move_rejects_neutral_non_anarchic_target() {
        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nid = game.human_player_nation;

        // Find an enemy province not at war and not anarchic
        let enemy_prov = game
            .world.provinces
            .iter()
            .find(|p| {
                p.owner != nid
                    && !game.world.diplomacy.is_at_war(nid, p.owner)
                    && !game.get_nation(p.owner).is_some_and(|n| n.diplomacy.is_in_anarchy)
            })
            .map(|p| p.id);

        if let Some(pid) = enemy_prov {
            let nation = game.get_nation(nid).unwrap();
            if let Some(unit) = nation.military.army.iter().find(|u| u.unit_type.can_move()) {
                let uid = unit.id.0;
                let json = serde_json::to_string(&game).unwrap();
                let result = wasm_queue_unit_move(&json, nid.0, uid, pid.0);
                assert!(
                    result.contains("error"),
                    "Should reject moving to neutral non-anarchic province"
                );
            }
        }
    }

    #[test]
    fn command_error_returns_structured_json() {
        let json = make_game_json();
        let game = game_from_json(&json).unwrap();
        let result = wasm_recruit_army_unit(&json, game.human_player_nation.0, "General");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            parsed["error"].is_string(),
            "Error should be a string field"
        );
    }

    // ── Newspaper archive reason serialization ─────────────────

    #[test]
    fn newspaper_archive_json_includes_reason_for_ai_headlines() {
        use domain::events::{Headline, HeadlineCategory};

        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();

        // Seed the archive with one AI-reasoned headline and one plain headline.
        game.archive.newspaper_archive.push((
            game.turn,
            vec![
                Headline::with_reason(
                    "Testland has declared war!".to_string(),
                    HeadlineCategory::War,
                    "need=2.3, opp=1.1, combined=3.4 > threshold 1.5".to_string(),
                ),
                Headline::new(
                    "The Imperial Times - 1815 Q1".to_string(),
                    HeadlineCategory::Default,
                ),
            ],
        ));

        let game_json = serde_json::to_string(&game).unwrap();
        let archive_json = wasm_get_newspaper_archive(&game_json);
        let parsed: serde_json::Value = serde_json::from_str(&archive_json).unwrap();
        let first_turn = &parsed.as_array().unwrap()[0];
        let headlines = first_turn["headlines"].as_array().unwrap();

        let war = headlines
            .iter()
            .find(|h| h["text"].as_str().unwrap().contains("declared war"))
            .expect("war headline");
        assert_eq!(
            war["reason"].as_str(),
            Some("need=2.3, opp=1.1, combined=3.4 > threshold 1.5"),
            "AI headline must carry reason through WASM"
        );

        let masthead = headlines
            .iter()
            .find(|h| h["text"].as_str().unwrap().contains("Imperial Times"))
            .expect("masthead headline");
        assert!(
            masthead.get("reason").is_none() || masthead["reason"].is_null(),
            "non-AI headline must omit reason field, got: {}",
            masthead
        );
    }

    #[test]
    fn newspaper_archive_json_marks_non_actions() {
        use domain::events::{Headline, HeadlineCategory};

        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();

        game.archive.newspaper_archive.push((
            game.turn,
            vec![
                Headline::non_action(
                    "Testland did not declare war this turn".to_string(),
                    HeadlineCategory::Default,
                    "war cooldown active".to_string(),
                ),
                Headline::with_reason(
                    "Testland declared war on Otherland!".to_string(),
                    HeadlineCategory::War,
                    "combined score above threshold".to_string(),
                ),
            ],
        ));

        let game_json = serde_json::to_string(&game).unwrap();
        let archive_json = wasm_get_newspaper_archive(&game_json);
        let parsed: serde_json::Value = serde_json::from_str(&archive_json).unwrap();
        let headlines = parsed.as_array().unwrap()[0]["headlines"]
            .as_array()
            .unwrap();

        let non_action = headlines
            .iter()
            .find(|h| h["text"].as_str().unwrap().contains("did not declare"))
            .expect("non-action headline");
        assert_eq!(
            non_action["is_non_action"].as_bool(),
            Some(true),
            "non-action headlines must serialize is_non_action=true"
        );

        let action = headlines
            .iter()
            .find(|h| h["text"].as_str().unwrap().contains("declared war on"))
            .expect("action headline");
        assert!(
            action.get("is_non_action").is_none() || action["is_non_action"].is_null(),
            "positive-action headlines must OMIT is_non_action (skip_serializing_if), got: {}",
            action
        );
    }

    #[test]
    fn newspaper_archive_json_includes_nation_ids() {
        use domain::events::{Headline, HeadlineCategory};
        use domain::types::NationId;

        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();

        game.archive.newspaper_archive.push((
            game.turn,
            vec![
                Headline::new("War breaks out!".to_string(), HeadlineCategory::War)
                    .for_nations(&[NationId(1), NationId(2)]),
                Headline::new("The Imperial Times".to_string(), HeadlineCategory::Default),
            ],
        ));

        let game_json = serde_json::to_string(&game).unwrap();
        let archive_json = wasm_get_newspaper_archive(&game_json);
        let parsed: serde_json::Value = serde_json::from_str(&archive_json).unwrap();
        let headlines = parsed.as_array().unwrap()[0]["headlines"].as_array().unwrap();

        let war = headlines
            .iter()
            .find(|h| h["text"].as_str().unwrap().contains("War breaks out"))
            .expect("war headline");
        let ids: Vec<i64> = war["nation_ids"]
            .as_array()
            .expect("nation_ids must be present")
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![1, 2], "nation_ids must survive WASM serialization");

        let masthead = headlines
            .iter()
            .find(|h| h["text"].as_str().unwrap().contains("Imperial Times"))
            .expect("masthead headline");
        assert!(
            masthead.get("nation_ids").is_none() || masthead["nation_ids"].is_null(),
            "headlines without nation_ids must omit the field, got: {}",
            masthead
        );
    }

    #[test]
    fn process_turn_headlines_include_nation_ids() {
        let json = make_game_json();
        let result = wasm_process_turn(&json);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let headlines = parsed["report"]["headlines"].as_array().expect("headlines array");

        // Masthead headline never carries nation_ids
        let masthead = headlines
            .iter()
            .find(|h| h["text"].as_str().unwrap_or("").contains("Imperial Times"))
            .expect("masthead headline");
        assert!(
            masthead.get("nation_ids").is_none() || masthead["nation_ids"].is_null(),
            "masthead must omit nation_ids, got: {}",
            masthead
        );

        // At least one AI-action headline should carry nation_ids (AI nations always act)
        let with_ids: Vec<_> = headlines
            .iter()
            .filter(|h| h.get("nation_ids").is_some() && !h["nation_ids"].is_null())
            .collect();
        assert!(
            !with_ids.is_empty(),
            "at least one headline from a real turn must carry nation_ids"
        );
        for h in &with_ids {
            let ids = h["nation_ids"].as_array().expect("nation_ids must be array");
            assert!(!ids.is_empty(), "nation_ids array must not be empty");
        }
    }

    #[test]
    fn process_turns_headlines_include_nation_ids() {
        let json = make_game_json();
        let result = wasm_process_turns(&json, 1);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let reports = parsed["reports"].as_array().expect("reports array");
        assert!(!reports.is_empty());
        let headlines = reports[0]["headlines"].as_array().expect("headlines array");

        let masthead = headlines
            .iter()
            .find(|h| h["text"].as_str().unwrap_or("").contains("Imperial Times"))
            .expect("masthead headline");
        assert!(
            masthead.get("nation_ids").is_none() || masthead["nation_ids"].is_null(),
            "masthead must omit nation_ids"
        );

        let with_ids: Vec<_> = headlines
            .iter()
            .filter(|h| h.get("nation_ids").is_some() && !h["nation_ids"].is_null())
            .collect();
        assert!(
            !with_ids.is_empty(),
            "at least one headline per turn must carry nation_ids"
        );
    }

    #[test]
    fn wasm_get_battle_data_returns_archive() {
        use domain::military::combat::BattleResult;
        use domain::military::units::ArmyUnitType;

        let mut game = new_game("default", Difficulty::Normal, 0);

        // Manually populate battle archive with test data
        let battle = BattleResult {
            attacker: NationId(0),
            defender: NationId(1),
            province: ProvinceId(0),
            attacker_won: true,
            attacker_casualties: vec![ArmyUnitType::Regulars],
            defender_casualties: vec![ArmyUnitType::Militia, ArmyUnitType::Militia],
            attacker_survivors: Vec::new(), // stripped for archive
            defender_survivors: Vec::new(), // stripped for archive
            terrain: Some(domain::types::TerrainType::Hills),
            fort_level: 1,
            attacker_initial_fp: 100.0,
            defender_initial_fp: 60.0,
            attacker_initial_count: 5,
            defender_initial_count: 3,
            retreated: false,
            siege_reduced_fort: true,
            medal_awards: vec![(ArmyUnitType::Guards, 2)],
            attacker_origin_provinces: vec![ProvinceId(2), ProvinceId(3)],
            is_naval_landing: false,
            defender_retreated: false,
            attacker_retreated_to: Vec::new(),
            defender_retreated_to: Vec::new(),
        };

        game.archive.battle_archive
            .push((TurnNumber::new(1), vec![battle], Vec::new()));

        let game_json = serde_json::to_string(&game).unwrap();
        let result_json = wasm_get_battle_data(&game_json);
        let parsed: serde_json::Value = serde_json::from_str(&result_json).unwrap();

        let archive = parsed.as_array().expect("should be array");
        assert_eq!(archive.len(), 1, "should have one archived turn");

        let entry = &archive[0];
        assert_eq!(entry["turn"].as_u64(), Some(1));
        assert_eq!(entry["year"].as_u64(), Some(1815));
        assert_eq!(entry["quarter"].as_u64(), Some(1));

        let battles = entry["battles"]
            .as_array()
            .expect("should have battles array");
        assert_eq!(battles.len(), 1);

        let b = &battles[0];
        assert_eq!(b["type"].as_str(), Some("land"));
        assert_eq!(b["attacker_won"].as_bool(), Some(true));
        assert_eq!(b["fort_level"].as_u64(), Some(1));
        assert_eq!(b["siege_reduced_fort"].as_bool(), Some(true));
        assert_eq!(b["retreated"].as_bool(), Some(false));
        assert_eq!(b["attacker_initial_count"].as_u64(), Some(5));
        assert_eq!(b["defender_initial_count"].as_u64(), Some(3));
        // Survivor counts derived from initial - casualties
        assert_eq!(b["attacker_survivors_count"].as_u64(), Some(4));
        assert_eq!(b["defender_survivors_count"].as_u64(), Some(1));
        assert_eq!(b["terrain"].as_str(), Some("Hills"));

        // Check origin_tiles are populated (two origin provinces)
        let origin_tiles = b["origin_tiles"]
            .as_array()
            .expect("should have origin_tiles");
        assert_eq!(
            origin_tiles.len(),
            2,
            "should have two origin tiles for two origin provinces"
        );

        // Check medal awards
        let medals = b["medal_awards"]
            .as_array()
            .expect("should have medal_awards");
        assert_eq!(medals.len(), 1);
        assert_eq!(medals[0]["medals"].as_u64(), Some(2));

        // Naval battles should be empty
        let naval = entry["naval_battles"]
            .as_array()
            .expect("should have naval_battles");
        assert!(naval.is_empty());
    }

    #[test]
    fn wasm_get_battle_data_empty_archive() {
        let game_json = make_game_json();
        let result_json = wasm_get_battle_data(&game_json);
        let parsed: serde_json::Value = serde_json::from_str(&result_json).unwrap();
        let archive = parsed.as_array().expect("should be array");
        assert!(
            archive.is_empty(),
            "new game should have empty battle archive"
        );
    }

    #[test]
    fn wasm_get_battle_data_naval_archive() {
        use domain::map::UnitId;
        use domain::military::naval::NavalBattleResult;
        use domain::military::ships::{Ship, ShipType};

        let mut game = new_game("default", Difficulty::Normal, 0);

        let naval = NavalBattleResult {
            attacker: NationId(0),
            defender: NationId(1),
            attacker_won: true,
            attacker_ships_lost: vec![ShipType::Frigate],
            defender_ships_lost: vec![ShipType::ShipOfTheLine, ShipType::Frigate],
            attacker_survivors: vec![Ship {
                id: UnitId(999),
                ship_type: ShipType::ShipOfTheLine,
                owner: NationId(0),
                hull_remaining: 100,
                sea_zone: None,
                operation: None,
            }],
            defender_survivors: Vec::new(),
        };

        game.archive.battle_archive
            .push((TurnNumber::new(2), Vec::new(), vec![naval]));

        let game_json = serde_json::to_string(&game).unwrap();
        let result_json = wasm_get_battle_data(&game_json);
        let parsed: serde_json::Value = serde_json::from_str(&result_json).unwrap();

        let archive = parsed.as_array().unwrap();
        assert_eq!(archive.len(), 1);

        let entry = &archive[0];
        assert_eq!(entry["turn"].as_u64(), Some(2));

        // Land battles should be empty
        let land = entry["battles"].as_array().unwrap();
        assert!(land.is_empty());

        // Naval battles should be populated
        let naval = entry["naval_battles"].as_array().unwrap();
        assert_eq!(naval.len(), 1);

        let nb = &naval[0];
        assert_eq!(nb["type"].as_str(), Some("naval"));
        assert_eq!(nb["attacker_won"].as_bool(), Some(true));
        assert_eq!(nb["attacker_ships_lost"].as_array().unwrap().len(), 1);
        assert_eq!(nb["defender_ships_lost"].as_array().unwrap().len(), 2);
        assert_eq!(nb["attacker_survivors_count"].as_u64(), Some(1));
        assert_eq!(nb["defender_survivors_count"].as_u64(), Some(0));
    }

    // ── Card #31 + F-010: diplomacy screen at-war display vs action gating ──

    /// When the player's own nation is in anarchy, every relation must be
    /// *displayed* as "At War" (card #31) but the action booleans must stay
    /// aligned with the raw backend relation — the peace button is not
    /// meaningfully available against a non-war target, even if presentation
    /// says "At War" because the player is in anarchy.
    #[test]
    fn diplomacy_screen_anarchy_splits_display_from_gating() {
        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        let player_id = game.human_player_nation;

        // Force the player into anarchy without touching relations.
        if let Some(player) = game.get_nation_mut(player_id) {
            player.diplomacy.is_in_anarchy = true;
        }
        // Pick another nation as the counterparty.
        let target_id = game.world.nations.iter().find(|n| n.id != player_id).unwrap().id;
        // Ensure raw_at_war is false for the pair.
        assert!(!game.world.diplomacy.is_at_war(player_id, target_id));

        let json = serde_json::to_string(&game).unwrap();
        let out = wasm_get_diplomacy_screen_data(&json, player_id.0);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let relations = parsed["relations"].as_array().unwrap();
        let rel = relations
            .iter()
            .find(|r| r["nation_id"].as_u64() == Some(target_id.0 as u64))
            .expect("counterparty relation present");

        // Display: anarchy forces At War.
        assert_eq!(rel["at_war"].as_bool(), Some(true));
        assert_eq!(rel["status"].as_str(), Some("At War"));

        // Gating: peace is NOT offered because the underlying relation is
        // not at war — the backend would reject a propose_peace command,
        // so the UI must not advertise it.
        let actions = &rel["actions"];
        assert_eq!(
            actions["can_propose_peace"].as_bool(),
            Some(false),
            "peace must not be gated by anarchy-inflated at_war"
        );
    }

    // ── Political snapshot ─────────────────────────────────────

    #[test]
    fn political_snapshot_returns_tiles_for_archived_turn() {
        use domain::game_state::PoliticalSnapshot;

        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();

        // Seed a snapshot at turn 5 using current province ownership + capitals.
        let provinces: Vec<(ProvinceId, NationId, Option<NationId>)> = game
            .world.provinces
            .iter()
            .map(|p| (p.id, p.owner, p.incorporated_from))
            .collect();
        let capitals: Vec<(NationId, ProvinceId)> = game
            .world.nations
            .iter()
            .map(|n| (n.id, n.capital_province_id))
            .collect();
        game.archive.political_archive.push((
            TurnNumber::new(5),
            PoliticalSnapshot {
                provinces,
                capitals,
            },
        ));

        let game_json = serde_json::to_string(&game).unwrap();
        let out = wasm_get_political_snapshot(&game_json, 5);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

        assert_eq!(parsed["turn"].as_u64(), Some(5));
        let tiles = parsed["tiles"].as_array().expect("tiles array");
        assert_eq!(tiles.len() as i64, game.world.hex_map.tile_count() as i64);
        // At least one tile must show a non-empty owner for a normal game.
        assert!(
            tiles
                .iter()
                .any(|t| t["owner"].as_str().unwrap_or("") != ""),
            "at least one tile should have an owner"
        );
        // At least one country capital should be flagged.
        assert!(
            tiles
                .iter()
                .any(|t| t["is_country_capital"].as_bool() == Some(true)),
            "at least one tile should be flagged as country capital"
        );
    }

    #[test]
    fn political_snapshot_errors_for_missing_turn() {
        let json = make_game_json();
        let out = wasm_get_political_snapshot(&json, 999);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(
            parsed["error"].is_string(),
            "missing snapshot should return an error object, got: {}",
            out
        );
    }

    #[test]
    fn political_snapshot_uses_archived_state_not_live_state() {
        use domain::game_state::PoliticalSnapshot;

        let json = make_game_json();
        let mut game = game_from_json(&json).unwrap();
        game.game_data = domain::data::GameData::default();

        // Archive at turn 5 with the *current* capitals and ownership.
        let provinces: Vec<(ProvinceId, NationId, Option<NationId>)> = game
            .world.provinces
            .iter()
            .map(|p| (p.id, p.owner, p.incorporated_from))
            .collect();
        let capitals: Vec<(NationId, ProvinceId)> = game
            .world.nations
            .iter()
            .map(|n| (n.id, n.capital_province_id))
            .collect();
        let archived_capitals: std::collections::HashSet<ProvinceId> =
            capitals.iter().map(|&(_, pid)| pid).collect();
        game.archive.political_archive.push((
            TurnNumber::new(5),
            PoliticalSnapshot {
                provinces,
                capitals,
            },
        ));

        // Mutate live state AFTER archiving: swap every nation's capital to a
        // province that was not previously a capital, and mark a province as
        // newly incorporated in live state. The archive must ignore both.
        let non_capital_pid = game
            .world.provinces
            .iter()
            .map(|p| p.id)
            .find(|pid| !archived_capitals.contains(pid))
            .expect("at least one non-capital province");
        for n in &mut game.world.nations {
            n.capital_province_id = non_capital_pid;
        }
        // Pick a province and give it a fake `incorporated_from` in live state;
        // archive should NOT pick up this change because the archived tuple
        // was already captured with incorporated=None.
        let mutated_pid = game
            .world.provinces
            .iter()
            .map(|p| p.id)
            .find(|pid| !archived_capitals.contains(pid))
            .expect("province to mutate");
        if let Some(p) = game.get_province_mut(mutated_pid) {
            p.incorporated_from = Some(NationId(999));
        }

        let game_json = serde_json::to_string(&game).unwrap();
        let out = wasm_get_political_snapshot(&game_json, 5);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let tiles = parsed["tiles"].as_array().expect("tiles array");

        // Archived capitals should still be flagged on tiles in archived capital
        // provinces, not on the live non-capital swap target.
        let capital_tile_count = tiles
            .iter()
            .filter(|t| t["is_country_capital"].as_bool() == Some(true))
            .count();
        assert!(
            capital_tile_count > 0,
            "archived capitals should still be flagged after live mutation"
        );

        // No tile in the archive output should show visual_group as the
        // fake NationId(999)-derived name, because that incorporation was
        // applied AFTER the snapshot was taken.
        // Live mutation of province.incorporated_from must not leak into the
        // archived rendering.
        let leaked = tiles.iter().any(|t| {
            t["visual_group"]
                .as_str()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        });
        // The starter map has no incorporated provinces at turn 1, so the
        // archive (captured with all `incorporated_from = None`) must still
        // render all visual_group fields as null/empty.
        assert!(
            !leaked,
            "archived visual_group must not reflect live-state mutation"
        );
    }
}
