use wasm_bindgen::prelude::*;

use domain::ai::common::next_unit_id;
use domain::economy::civilians::{CivilianType, next_civilian_id, parse_civilian_type};
use domain::game_state::{GameState, new_game};
use domain::hex::HexCoord;
use domain::military::ships::{Ship, ShipCategory, ShipType};
use domain::military::units::{ArmyUnit, ArmyUnitType};
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
        Ok(game) => {
            serde_json::to_string(&game).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
        }
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    }
}

/// Process one turn. Accepts game state JSON, returns JSON with updated state + turn report.
#[wasm_bindgen]
pub fn wasm_process_turn(game_json: &str) -> String {
    let mut game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return serde_json::json!({"error": format!("deserialize: {e}")}).to_string(),
    };

    // Reconstruct tech tree (skipped in serialization)
    game.game_data = domain::data::GameData::default();

    let report = process_turn(&mut game);

    // Build response with game state + report summary
    let response = serde_json::json!({
        "game": match serde_json::to_value(&game) {
            Ok(v) => v,
            Err(e) => return serde_json::json!({"error": format!("serialize game: {e}")}).to_string(),
        },
        "report": {
            "turn": format!("{}", report.turn),
            "year": report.year,
            "quarter": report.quarter,
            "headlines": report.newspaper_headlines.iter()
                .map(|(text, cat)| serde_json::json!({"text": text, "category": cat}))
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

    // Build province→nation lookup using Province.owner (the ground truth)
    // and identify country capitals
    let nation_lookup: std::collections::HashMap<NationId, (&str, String)> = game
        .nations
        .iter()
        .map(|n| (n.id, (n.name.as_str(), format!("{:?}", n.color))))
        .collect();
    let mut province_nation: std::collections::HashMap<ProvinceId, (String, String, NationId)> =
        std::collections::HashMap::new();
    for prov in &game.provinces {
        if let Some((name, color)) = nation_lookup.get(&prov.owner) {
            province_nation.insert(prov.id, (name.to_string(), color.clone(), prov.owner));
        }
    }
    let mut country_capital_provinces: std::collections::HashSet<ProvinceId> =
        std::collections::HashSet::new();
    for nation in &game.nations {
        country_capital_provinces.insert(nation.capital_province_id);
    }

    // Build province → (total army FP, unit count) lookup
    let mut province_army: std::collections::HashMap<ProvinceId, (f64, u32)> =
        std::collections::HashMap::new();
    for nation in &game.nations {
        for unit in &nation.army {
            let e = province_army.entry(unit.position).or_insert((0.0, 0));
            e.0 += unit.effective_firepower();
            e.1 += 1;
        }
    }

    // Build nation → (naval FP, warship count) lookup
    let nation_naval: std::collections::HashMap<NationId, (u32, usize)> = game
        .nations
        .iter()
        .map(|n| (n.id, (n.total_naval_firepower(), n.warship_count())))
        .collect();

    // Build hex coord → civilian lookup for human player
    let mut civilian_on_tile: std::collections::HashMap<domain::hex::HexCoord, serde_json::Value> =
        std::collections::HashMap::new();
    if let Some(nation) = game.get_nation(game.human_player_nation) {
        for civ in &nation.civilians {
            if let Some(pos) = civ.position {
                civilian_on_tile.insert(
                    pos,
                    serde_json::json!({
                        "id": civ.id.0,
                        "type": format!("{}", civ.civilian_type),
                        "working": civ.working,
                        "turns_remaining": civ.turns_remaining,
                    }),
                );
            }
        }
    }

    let map_width = game.hex_map.width();

    let tiles: Vec<serde_json::Value> = game
        .hex_map
        .all_tiles()
        .map(|(coord, tile)| {
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

            // A tile is a country capital if it's marked as capital AND is in
            // the nation's capital province
            let is_country_capital = tile.is_capital
                && tile
                    .province_id
                    .is_some_and(|pid| country_capital_provinces.contains(&pid));

            // Strength data — only populated on capital tiles to keep payload small
            let (army_fp, army_count) = if tile.is_capital {
                tile.province_id
                    .and_then(|pid| province_army.get(&pid))
                    .copied()
                    .unwrap_or((0.0, 0))
            } else {
                (0.0, 0)
            };

            let (naval_fp, naval_count) = if is_country_capital {
                nation_naval
                    .get(&NationId(owner_nation_id))
                    .copied()
                    .unwrap_or((0, 0))
            } else {
                (0, 0)
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
                "owner": owner_name,
                "owner_color": owner_color,
                "province": province_name,
                "province_id": tile.province_id.map(|pid| pid.0),
                "has_railroad": tile.infrastructure.has_railroad,
                "has_depot": tile.infrastructure.has_depot,
                "has_port": tile.infrastructure.has_port,
                "has_fort": tile.infrastructure.has_fort,
                "fort_level": tile.infrastructure.fort_level,
                "map_width": map_width,
                "nation_id": owner_nation_id,
                "army_firepower": army_fp,
                "army_unit_count": army_count,
                "naval_firepower": naval_fp,
                "naval_ship_count": naval_count,
                "civilian_on_tile": civilian_on_tile.get(&coord),
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
    game.game_data = domain::data::GameData::default();

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
    let mut game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };
    game.game_data = domain::data::GameData::default();

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
            if nation.treasury.checked_sub(cost).is_none() {
                return "{\"error\":\"insufficient funds\"}".to_string();
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

/// Get diplomacy overlay data for a specific nation's perspective.
/// Returns JSON with relations from the selected nation to all others.
#[wasm_bindgen]
pub fn wasm_get_diplomacy_overlay(game_json: &str, nation_id: u32) -> String {
    let game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let selected_nid = NationId(nation_id);
    let selected_name = game
        .get_nation(selected_nid)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");

    let relations: Vec<serde_json::Value> = game
        .nations
        .iter()
        .filter(|n| n.id != selected_nid)
        .map(|n| {
            let rel = game.diplomacy.get_relation(selected_nid, n.id);
            let (status, score) = match rel {
                Some(r) => {
                    let s = if r.at_war {
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
                None => ("Neutral", 0),
            };
            let treaties: Vec<String> = rel
                .map(|r| {
                    r.active_treaties
                        .iter()
                        .map(|t| format!("{:?}", t))
                        .collect()
                })
                .unwrap_or_default();
            let at_war = rel.map(|r| r.at_war).unwrap_or(false);
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
    let game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let entries: Vec<serde_json::Value> = game
        .nations
        .iter()
        .map(|n| {
            serde_json::json!({
                "nation_name": n.name,
                "nation_id": n.id.0,
                "nation_color": format!("{:?}", n.color),
                "total_army_fp": n.total_military_firepower(),
                "total_naval_fp": n.total_naval_firepower(),
                "army_unit_count": n.army.len(),
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
    let mut game: GameState = serde_json::from_str(game_json)
        .map_err(|e| serde_json::json!({"error": format!("deserialize: {e}")}).to_string())?;
    game.game_data = domain::data::GameData::default();
    Ok(game)
}

fn serialize_game(game: &GameState) -> String {
    serde_json::to_string(game)
        .unwrap_or_else(|e| serde_json::json!({"error": format!("serialize: {e}")}).to_string())
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
    for nation in &game.nations {
        for unit in &nation.army {
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

    for civ in &nation.civilians {
        match civ.position {
            Some(pos) => {
                let tile = game.hex_map.get_tile(pos);
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
        .merchant_fleet
        .iter()
        .map(|s| {
            let stats = s.ship_type.stats();
            serde_json::json!({
                "id": s.id.0,
                "type": format!("{:?}", s.ship_type),
                "hull": s.hull_remaining,
                "hull_max": stats.hull,
                "cargo": stats.cargo,
                "sea_zone": s.sea_zone,
            })
        })
        .collect();

    let warships: Vec<serde_json::Value> = nation
        .warships
        .iter()
        .map(|s| {
            let stats = s.ship_type.stats();
            serde_json::json!({
                "id": s.id.0,
                "type": format!("{:?}", s.ship_type),
                "hull": s.hull_remaining,
                "hull_max": stats.hull,
                "firepower": stats.firepower,
                "sea_zone": s.sea_zone,
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
    let unit = match nation.army.iter().find(|u| u.id == uid) {
        Some(u) => u,
        None => return "{\"error\":\"unit not found\"}".to_string(),
    };
    if !unit.unit_type.can_move() {
        return serde_json::json!({"friendly": [], "hostile": []}).to_string();
    }

    let mut friendly: Vec<serde_json::Value> = Vec::new();
    let mut hostile: Vec<serde_json::Value> = Vec::new();

    for prov in &game.provinces {
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
            let at_war = game.diplomacy.is_at_war(nid, prov.owner);
            let target_anarchic = game.get_nation(prov.owner).is_some_and(|n| n.is_in_anarchy);
            if at_war || target_anarchic {
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
    let treasury = nation.treasury;

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

    let civilians: Vec<serde_json::Value> = all_civilian_types
        .iter()
        .map(|ct| {
            let cost = ct.creation_cost();
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
    let uid = domain::map::UnitId(unit_id);
    let dest = ProvinceId(dest_province_id);

    // Validate unit exists and belongs to nation
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };
    let unit = match nation.army.iter().find(|u| u.id == uid) {
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
    let target_at_war = game.diplomacy.is_at_war(nid, dest_prov.owner);
    let target_anarchic = game
        .get_nation(dest_prov.owner)
        .is_some_and(|n| n.is_in_anarchy);
    if !target_is_own && !target_at_war && !target_anarchic {
        return "{\"error\":\"cannot move to that province\"}".to_string();
    }

    // F-003: Replace existing pending move for this unit (prevent duplicates)
    game.pending_moves.retain(|(_, id, _)| *id != uid);
    game.pending_moves.push((nid, uid, dest));
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
    game.pending_moves.retain(|(_, id, _)| *id != uid);
    serialize_game(&game)
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
    let tile = match game.hex_map.get_tile(coord) {
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
    let civ = match nation.civilians.iter_mut().find(|c| c.id == cid) {
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
    let turns = if improvement_level == 0 { 3 } else { 5 };
    civ.start_work(turns);

    // F-006: Set assigned_civilian on the tile
    if let Some(tile_mut) = game.hex_map.get_tile_mut(coord) {
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
        let civ = match nation.civilians.iter().find(|c| c.id == cid) {
            Some(c) => c,
            None => return "{\"error\":\"civilian not found\"}".to_string(),
        };
        civ.position
    };

    // F-006: Clear assigned_civilian on the old tile
    if let Some(pos) = old_pos
        && let Some(tile_mut) = game.hex_map.get_tile_mut(pos)
    {
        tile_mut.assigned_civilian = None;
    }

    // Now mutate the civilian
    let nation = match game.get_nation_mut(human_nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };
    let civ = match nation.civilians.iter_mut().find(|c| c.id == cid) {
        Some(c) => c,
        None => return "{\"error\":\"civilian not found\"}".to_string(),
    };
    civ.position = None;
    civ.working = false;
    civ.turns_remaining = 0;

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
        if nation.treasury < stats.cost {
            return "{\"error\":\"insufficient funds\"}".to_string();
        }
        if nation.material_amount(MaterialType::Arms) < stats.arms_required {
            return "{\"error\":\"not enough arms\"}".to_string();
        }
    }

    // Deduct costs and create unit
    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };
    nation.treasury -= stats.cost;
    nation.consume_material(MaterialType::Arms, stats.arms_required);
    let capital = nation.capital_province_id;
    let uid = next_unit_id();
    let new_unit = ArmyUnit::new(uid, unit_type, nid, capital);
    nation.army.push(new_unit);

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

    let cost = civ_type.creation_cost();

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };

    if nation.treasury < cost {
        return "{\"error\":\"insufficient funds\"}".to_string();
    }

    nation.treasury -= cost;
    let cid = next_civilian_id();
    let new_civ = domain::economy::civilians::Civilian::new(cid, civ_type, nid);
    nation.civilians.push(new_civ);

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
    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return "{\"error\":\"nation not found\"}".to_string(),
    };
    nation.consume_material(MaterialType::Fabric, stats.fabric_cost);
    nation.consume_material(MaterialType::Lumber, stats.lumber_cost);
    nation.consume_material(MaterialType::Arms, stats.arms_cost);
    nation.consume_material(MaterialType::Steel, stats.steel_cost);
    nation.remove_resource(ResourceType::Coal, stats.coal_cost);

    let sid = next_unit_id();
    let new_ship = Ship::new(sid, ship_type, nid);
    match ship_type.category() {
        ShipCategory::Merchant => nation.merchant_fleet.push(new_ship),
        ShipCategory::Warship => nation.warships.push(new_ship),
    }

    serialize_game(&game)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── Move validation tests ─────────────────────────────────

    #[test]
    fn queue_move_rejects_nonexistent_unit() {
        let json = make_game_json();
        let game: GameState = serde_json::from_str(&json).unwrap();
        let nid = game.human_player_nation.0;
        let result = wasm_queue_unit_move(&json, nid, 9999999, 1);
        assert!(result.contains("error"));
    }

    #[test]
    fn queue_move_replaces_duplicate() {
        let json = make_game_json();
        let mut game: GameState = serde_json::from_str(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nid = game.human_player_nation;

        let nation = game.get_nation(nid).unwrap();
        let unit = nation.army.iter().find(|u| u.unit_type.can_move());
        if unit.is_none() {
            return;
        }
        let uid = unit.unwrap().id.0;

        let own_provs: Vec<u32> = game
            .provinces
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
        let game2: GameState = serde_json::from_str(&result2).unwrap();
        let moves_for_unit = game2
            .pending_moves
            .iter()
            .filter(|(_, id, _)| id.0 == uid)
            .count();
        assert_eq!(moves_for_unit, 1);
    }

    #[test]
    fn recruit_general_rejected() {
        let json = make_game_json();
        let game: GameState = serde_json::from_str(&json).unwrap();
        let nid = game.human_player_nation.0;
        let result = wasm_recruit_army_unit(&json, nid, "General");
        assert!(result.contains("error"));
    }

    #[test]
    fn hire_civilian_insufficient_funds() {
        let json = make_game_json();
        let mut game: GameState = serde_json::from_str(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nation = game.get_nation_mut(game.human_player_nation).unwrap();
        nation.treasury = Money::ZERO;
        let json = serde_json::to_string(&game).unwrap();

        let result = wasm_hire_civilian(&json, game.human_player_nation.0, "Miner");
        assert!(result.contains("insufficient funds"));
    }

    #[test]
    fn buildable_units_includes_tech_met_for_civilians() {
        let json = make_game_json();
        let game: GameState = serde_json::from_str(&json).unwrap();
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
        let game: GameState = serde_json::from_str(&json).unwrap();
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
        let mut game: GameState = serde_json::from_str(&json).unwrap();
        game.game_data = domain::data::GameData::default();

        let nid = game.human_player_nation;
        game.pending_moves
            .push((nid, domain::map::UnitId(12345), ProvinceId(1)));
        let json = serde_json::to_string(&game).unwrap();

        let result = wasm_cancel_unit_move(&json, 12345);
        assert!(!result.contains("error"));
        let game2: GameState = serde_json::from_str(&result).unwrap();
        assert!(!game2.pending_moves.iter().any(|(_, id, _)| id.0 == 12345));
    }

    // ── F-018: Anarchic target + deploy occupancy tests ───────

    #[test]
    fn valid_move_targets_includes_anarchic_provinces() {
        let json = make_game_json();
        let mut game: GameState = serde_json::from_str(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nid = game.human_player_nation;

        // Find an enemy nation and make it anarchic
        if let Some(enemy) = game.nations.iter_mut().find(|n| n.id != nid) {
            enemy.is_in_anarchy = true;
            let enemy_id = enemy.id;

            // Ensure we have a movable unit
            let nation = game.get_nation(nid).unwrap();
            let unit = nation.army.iter().find(|u| u.unit_type.can_move());
            if let Some(unit) = unit {
                let uid = unit.id.0;
                let json = serde_json::to_string(&game).unwrap();
                let result = wasm_get_valid_move_targets(&json, nid.0, uid);
                let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
                let hostile = parsed["hostile"].as_array().unwrap();
                // Anarchic nation's provinces should appear in hostile targets
                let has_anarchic =
                    game.provinces.iter().any(|p| p.owner == enemy_id) && !hostile.is_empty();
                if game.provinces.iter().any(|p| p.owner == enemy_id) {
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
        let mut game: GameState = serde_json::from_str(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nid = game.human_player_nation;

        // Find an enemy province and make its owner anarchic
        let enemy_prov = game
            .provinces
            .iter()
            .find(|p| p.owner != nid)
            .map(|p| (p.id, p.owner));
        if let Some((pid, enemy_nid)) = enemy_prov {
            if let Some(enemy) = game.nations.iter_mut().find(|n| n.id == enemy_nid) {
                enemy.is_in_anarchy = true;
            }
            let nation = game.get_nation(nid).unwrap();
            if let Some(unit) = nation.army.iter().find(|u| u.unit_type.can_move()) {
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
        let mut game: GameState = serde_json::from_str(&json).unwrap();
        game.game_data = domain::data::GameData::default();
        let nid = game.human_player_nation;

        // Find an enemy province not at war and not anarchic
        let enemy_prov = game
            .provinces
            .iter()
            .find(|p| {
                p.owner != nid
                    && !game.diplomacy.is_at_war(nid, p.owner)
                    && !game.get_nation(p.owner).is_some_and(|n| n.is_in_anarchy)
            })
            .map(|p| p.id);

        if let Some(pid) = enemy_prov {
            let nation = game.get_nation(nid).unwrap();
            if let Some(unit) = nation.army.iter().find(|u| u.unit_type.can_move()) {
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
        let game: GameState = serde_json::from_str(&json).unwrap();
        let result = wasm_recruit_army_unit(&json, game.human_player_nation.0, "General");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            parsed["error"].is_string(),
            "Error should be a string field"
        );
    }
}
