use wasm_bindgen::prelude::*;

use domain::ai::common::next_unit_id;
use domain::economy::buildings::BuildingType;
use domain::economy::civilians::{CivilianType, next_civilian_id, parse_civilian_type};
use domain::economy::production::{
    ProductionChain, calculate_factory_production, calculate_mill_production,
};
use domain::economy::trade::{Commodity, base_price, commodity_price};
use domain::economy::transport::TransportSystem;
use domain::events::TreatyType;
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
                .map(|h| {
                    let mut obj = serde_json::json!({"text": &h.text, "category": &h.category});
                    if let Some(ref reason) = h.reason {
                        obj["reason"] = serde_json::json!(reason);
                    }
                    if h.is_non_action {
                        obj["is_non_action"] = serde_json::json!(true);
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

    // Build hex coord → civilian lookup for ALL nations
    let mut civilian_on_tile: std::collections::HashMap<domain::hex::HexCoord, serde_json::Value> =
        std::collections::HashMap::new();
    for nation in &game.nations {
        let (nation_name, nation_color) = nation_lookup
            .get(&nation.id)
            .map(|(name, color)| (*name, color.as_str()))
            .unwrap_or(("", ""));
        let is_human = nation.id == game.human_player_nation;
        for civ in &nation.civilians {
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
                        "owner": nation_name,
                        "owner_color": nation_color,
                        "is_human": is_human,
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
                // Adjacency check: nation must own a province adjacent to
                // the target, or have an active landing site (matching backend logic).
                let nation_adjacent = nation.province_ids.iter().any(|&our_pid| {
                    game.get_province(our_pid).is_some_and(|our_prov| {
                        domain::map::provinces_are_adjacent(&game.hex_map, our_prov, prov)
                    })
                });
                let has_landing = game.pending_landings.iter().any(|(lid, pid, established)| {
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
            let at_war = game.diplomacy.is_at_war(nid, p.owner);
            let target_anarchic = game.get_nation(p.owner).is_some_and(|n| n.is_in_anarchy);
            at_war || target_anarchic
        }
    });
    if !valid {
        return "{\"error\":\"target province is not a valid coastal enemy province\"}".to_string();
    }

    // Must have warships
    let has_warships = game.get_nation(nid).is_some_and(|n| !n.warships.is_empty());
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
        for ship in &mut nation.warships {
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

    let transport = &nation.transport;
    let (labor_cost, lumber_cost, steel_cost) = TransportSystem::build_freight_car_cost();
    let available_lumber = nation.material_amount(MaterialType::Lumber);
    let available_steel = nation.material_amount(MaterialType::Steel);
    let available_labor = nation.labor.total_labor_units();

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

    if nation.labor.total_labor_units() < labor_cost {
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
    nation.transport.build_freight_cars(1);

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

    nation.transport.set_allocation(res, percentage.min(100));
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
        .buildings
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
        .warehouse
        .iter()
        .map(|(r, qty)| (format!("{:?}", r), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    let materials_json: serde_json::Value = nation
        .materials
        .iter()
        .map(|(m, qty)| (format!("{:?}", m), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    let goods_json: serde_json::Value = nation
        .goods
        .iter()
        .map(|(g, qty)| (format!("{:?}", g), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    // Labor
    let labor = &nation.labor;

    // Production forecast for each chain
    let available_lumber_mat = nation.material_amount(MaterialType::Lumber);
    let available_steel_mat = nation.material_amount(MaterialType::Steel);

    // Can-expand map
    let can_expand: serde_json::Value = nation
        .buildings
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
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::LumberMill)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let steel_mill_cap = nation
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::SteelMill)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let textile_mill_cap = nation
        .buildings
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
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::FurnitureFactory)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let hardware_cap = nation
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::HardwareFactory)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let clothing_cap = nation
        .buildings
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

    let building = match nation.buildings.iter().find(|b| b.building_type == bt) {
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
    if let Some(b) = nation.buildings.iter_mut().find(|b| b.building_type == bt) {
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
        .trade_history
        .iter()
        .rev()
        .take(20)
        .map(|entry| {
            let partner_name = game
                .get_nation(entry.partner)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            serde_json::json!({
                "turn": entry.turn,
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
        .trade_subsidies
        .iter()
        .map(|(&target_nid, &amount)| {
            let target_name = game
                .get_nation(target_nid)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let has_consulate = game
                .diplomacy
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
    for entry in &nation.trade_history {
        if entry.bought {
            total_bought += entry.total_cost.as_dollars();
        } else {
            total_sold += entry.total_cost.as_dollars();
        }
    }
    total_sold += nation.goods_sales_revenue_dollars;

    // Cargo capacity from merchant fleet
    let total_cargo: u32 = nation
        .merchant_fleet
        .iter()
        .map(|s| s.ship_type.stats().cargo)
        .sum();

    // Minor nations with consulates
    let minor_nations: Vec<serde_json::Value> = game
        .nations
        .iter()
        .filter(|n| n.nation_type == NationType::MinorNation && n.id != nid)
        .map(|n| {
            let rel = game.diplomacy.get_relation(nid, n.id);
            let has_consulate = rel.map(|r| r.has_consulate).unwrap_or(false);
            let has_embassy = rel.map(|r| r.has_embassy).unwrap_or(false);
            // Collect resources available in minor nation's provinces
            let mut mn_resources = Vec::new();
            for &pid in &n.province_ids {
                if let Some(prov) = game.get_province(pid) {
                    for &coord in &prov.tiles {
                        if let Some(tile) = game.hex_map.get_tile(coord)
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
        .player_sell_orders
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
        .player_buy_orders
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
    let available_offers: Vec<serde_json::Value> =
        domain::economy::trade::generate_minor_nation_offers(
            &game.nations,
            &game.provinces,
            &game.hex_map,
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
            })
        })
        .collect();

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
            let stock = nation.materials.get(&m).copied().unwrap_or(0);
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
            let stock = nation.goods.get(&g).copied().unwrap_or(0);
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
        .player_sell_orders
        .iter()
        .map(|o| o.quantity)
        .chain(nation.player_buy_orders.iter().map(|o| o.quantity))
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
        "treasury": nation.treasury.as_dollars(),
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
        nation.trade_subsidies.remove(&target_nid);
    } else {
        nation
            .trade_subsidies
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
        Commodity::Material(m) => nation.materials.get(&m).copied().unwrap_or(0),
        Commodity::Goods(g) => nation.goods.get(&g).copied().unwrap_or(0),
    };
    if quantity > available {
        return r#"{"error":"insufficient stock"}"#.to_string();
    }

    // Validate cargo capacity
    let total_cargo: u32 = nation.total_cargo_capacity();
    let other_orders: u32 = nation
        .player_sell_orders
        .iter()
        .filter(|o| o.commodity != commodity)
        .map(|o| o.quantity)
        .chain(nation.player_buy_orders.iter().map(|o| o.quantity))
        .sum();
    if other_orders + quantity > total_cargo {
        return r#"{"error":"exceeds cargo capacity"}"#.to_string();
    }

    // Upsert: remove existing for this commodity, add new if qty > 0
    nation
        .player_sell_orders
        .retain(|o| o.commodity != commodity);
    if quantity > 0 {
        nation
            .player_sell_orders
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
        .player_sell_orders
        .iter()
        .map(|o| o.quantity)
        .chain(
            nation
                .player_buy_orders
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
        .player_buy_orders
        .retain(|o| o.resource != resource_type);
    if quantity > 0 {
        nation
            .player_buy_orders
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

    let player_standing = game.diplomacy.standing.get(&nid).copied().unwrap_or(100);
    let treasury = nation.treasury.as_dollars();
    let player_is_gp = nation.nation_type == NationType::GreatPower;

    let relations: Vec<serde_json::Value> = game
        .nations
        .iter()
        .filter(|n| n.id != nid)
        .map(|n| {
            let rel = game.diplomacy.get_relation(nid, n.id);
            let score = rel.map(|r| r.score).unwrap_or(0);
            let at_war = rel.map(|r| r.at_war).unwrap_or(false);
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

            let status = if at_war {
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
            let has_pending_nap = game.diplomacy.pending_proposals.iter().any(|p| {
                p.proposal_type == TreatyType::NonAggressionPact && p.from == nid && p.to == n.id
            });
            let has_pending_alliance =
                game.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == TreatyType::Alliance && p.from == nid && p.to == n.id
                });
            let has_pending_peace = game.diplomacy.pending_proposals.iter().any(|p| {
                p.proposal_type == TreatyType::PeaceTreaty && p.from == nid && p.to == n.id
            });

            // Any pending proposal in either direction (for action gating, matches backend)
            let any_pending_nap = has_pending_nap
                || game.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == TreatyType::NonAggressionPact
                        && p.from == n.id
                        && p.to == nid
                });
            let any_pending_alliance = has_pending_alliance
                || game.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == TreatyType::Alliance && p.from == n.id && p.to == nid
                });
            let any_pending_peace = has_pending_peace
                || game.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == TreatyType::PeaceTreaty && p.from == n.id && p.to == nid
                });

            // Pre-compute available actions
            let can_build_consulate = !has_consulate && treasury >= 500;
            let can_build_embassy = has_consulate && !has_embassy && treasury >= 5000;
            let can_propose_nap = has_embassy
                && !at_war
                && !has_nap
                && !has_alliance
                && !any_pending_nap
                && player_standing >= 30;
            let can_propose_alliance = has_embassy
                && !at_war
                && !has_alliance
                && !any_pending_alliance
                && player_standing >= 30
                && player_is_gp
                && target_is_gp;
            let can_declare_war = !at_war;
            let can_send_grant = !at_war && treasury > 0;
            let can_break_treaty = !treaties.is_empty();
            let can_propose_peace = at_war && !any_pending_peace;

            serde_json::json!({
                "nation_id": n.id.0,
                "nation_name": n.name,
                "nation_color": format!("{:?}", n.color),
                "nation_type": format!("{:?}", n.nation_type),
                "score": score,
                "at_war": at_war,
                "status": status,
                "treaties": treaties,
                "has_consulate": has_consulate,
                "has_embassy": has_embassy,
                "has_pending_nap": has_pending_nap,
                "has_pending_alliance": has_pending_alliance,
                "has_pending_peace": has_pending_peace,
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

    // Validate actor nation exists
    if game.get_nation(nid).is_none() {
        return "{\"error\":\"nation not found\"}".to_string();
    }

    // Validate treasury before committing
    let consulate_cost = Money::dollars(500);
    if game
        .get_nation(nid)
        .map(|n| n.treasury.as_dollars() < consulate_cost.as_dollars())
        .unwrap_or(false)
    {
        return "{\"error\":\"not enough treasury\"}".to_string();
    }

    let cost = match game.diplomacy.build_consulate(nid, target) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    if let Some(nation) = game.get_nation_mut(nid) {
        nation.treasury -= cost;
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

    // Validate actor nation exists
    if game.get_nation(nid).is_none() {
        return "{\"error\":\"nation not found\"}".to_string();
    }

    // Validate treasury before committing
    let embassy_cost = Money::dollars(5000);
    if game
        .get_nation(nid)
        .map(|n| n.treasury.as_dollars() < embassy_cost.as_dollars())
        .unwrap_or(false)
    {
        return "{\"error\":\"not enough treasury\"}".to_string();
    }

    let cost = match game.diplomacy.build_embassy(nid, target) {
        Ok(c) => c,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    if let Some(nation) = game.get_nation_mut(nid) {
        nation.treasury -= cost;
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

    let turn = game.turn;
    match game
        .diplomacy
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

    let turn = game.turn;
    match game
        .diplomacy
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
    if game.get_nation(target).is_none() {
        return "{\"error\":\"target nation not found\"}".to_string();
    }

    if game.diplomacy.is_at_war(nid, target) {
        return "{\"error\":\"already at war\"}".to_string();
    }

    game.diplomacy.declare_war(nid, target);
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

    // Check treasury
    {
        let nation = match game.get_nation(nid) {
            Some(n) => n,
            None => return "{\"error\":\"nation not found\"}".to_string(),
        };
        if nation.treasury.as_dollars() < amount {
            return "{\"error\":\"not enough treasury\"}".to_string();
        }
    }

    // Deduct from treasury
    if let Some(nation) = game.get_nation_mut(nid) {
        nation.treasury -= money;
    }

    game.diplomacy.send_grant(nid, target, money);
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

    game.diplomacy.break_treaty(nid, target, tt);
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

    let turn = game.turn;

    match game.diplomacy.propose_peace(nid, target, turn) {
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
        .diplomacy
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
                "turn_proposed": p.turn_proposed,
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

    if idx >= game.diplomacy.pending_proposals.len() {
        return "{\"error\":\"proposal index out of range\"}".to_string();
    }

    let proposal = game.diplomacy.pending_proposals[idx].clone();
    if proposal.to != nid {
        return "{\"error\":\"proposal not addressed to you\"}".to_string();
    }

    // Execute the treaty action — propagate errors
    match proposal.proposal_type {
        TreatyType::NonAggressionPact => {
            if let Err(e) = game.diplomacy.propose_pact(proposal.from, proposal.to) {
                return format!("{{\"error\":\"{}\"}}", e);
            }
        }
        TreatyType::Alliance => {
            if let Err(e) = game.diplomacy.propose_alliance(proposal.from, proposal.to) {
                return format!("{{\"error\":\"{}\"}}", e);
            }
        }
        TreatyType::PeaceTreaty => {
            game.diplomacy.make_peace(proposal.from, proposal.to);
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
    game.diplomacy.pending_proposals.remove(idx);

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

    if idx >= game.diplomacy.pending_proposals.len() {
        return "{\"error\":\"proposal index out of range\"}".to_string();
    }

    if game.diplomacy.pending_proposals[idx].to != nid {
        return "{\"error\":\"proposal not addressed to you\"}".to_string();
    }

    let proposal = game.diplomacy.pending_proposals.remove(idx);

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
    let game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return "{\"error\":\"Nation not found\"}".to_string(),
    };

    // Economy
    let treasury_dollars = nation.treasury.as_dollars();
    let subsidies: Vec<serde_json::Value> = nation
        .trade_subsidies
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
        .buildings
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
        .warehouse
        .iter()
        .filter(|(_, qty)| **qty > 0)
        .map(|(rt, qty)| serde_json::json!({"name": format!("{:?}", rt), "quantity": qty}))
        .collect();
    let materials: Vec<serde_json::Value> = nation
        .materials
        .iter()
        .filter(|(_, qty)| **qty > 0)
        .map(|(mt, qty)| serde_json::json!({"name": format!("{:?}", mt), "quantity": qty}))
        .collect();
    let goods: Vec<serde_json::Value> = nation
        .goods
        .iter()
        .filter(|(_, qty)| **qty > 0)
        .map(|(gt, qty)| serde_json::json!({"name": format!("{:?}", gt), "quantity": qty}))
        .collect();

    // Military — army by type
    let mut army_counts: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    for unit in &nation.army {
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
    for ship in &nation.warships {
        let type_name = format!("{:?}", ship.ship_type);
        *warship_counts.entry(type_name).or_insert(0) += 1;
    }
    let warships_by_type: Vec<serde_json::Value> = warship_counts
        .iter()
        .map(|(name, count)| serde_json::json!({"ship_type": name, "count": count}))
        .collect();

    // Diplomacy summary
    let standing = game.diplomacy.get_standing(nid);
    let mut consulate_count = 0u32;
    let mut embassy_count = 0u32;
    let mut treaties: Vec<serde_json::Value> = Vec::new();
    let mut wars: Vec<String> = Vec::new();

    for other in &game.nations {
        if other.id == nid {
            continue;
        }
        if let Some(rel) = game.diplomacy.get_relation(nid, other.id) {
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
            "goods_revenue": nation.goods_sales_revenue_dollars,
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
            "total_army_count": nation.army.len(),
            "warships_by_type": warships_by_type,
            "total_warship_count": nation.warships.len(),
            "merchant_ships": nation.merchant_fleet.len(),
            "total_arms_built": nation.total_arms_built,
            "generals_earned": nation.generals_earned,
        },
        "diplomacy": {
            "standing": standing,
            "consulates": consulate_count,
            "embassies": embassy_count,
            "treaties": treaties,
            "wars": wars,
        },
        "labor": {
            "untrained": nation.labor.untrained,
            "trained": nation.labor.trained,
            "expert": nation.labor.expert,
            "total": nation.labor.total_workers(),
        },
    });

    serde_json::to_string(&result).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Return ledger data for ALL Great Powers.
#[wasm_bindgen]
pub fn wasm_get_all_gp_ledger_data(game_json: &str) -> String {
    let game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let entries: Vec<serde_json::Value> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|nation| {
            let nid = nation.id;
            let nation_name = &nation.name;
            let nation_color = format!("{:?}", nation.color);
            let is_human = nid == game.human_player_nation;

            // Per-nation ledger data (same logic as wasm_get_ledger_data)
            let treasury_dollars = nation.treasury.as_dollars();
            let provinces = nation.province_ids.len();

            let mut total_army_fp: u32 = 0;
            let total_army_count = nation.army.len();
            for unit in &nation.army {
                total_army_fp += unit.unit_type.stats().firepower;
            }
            let total_warship_count = nation.warships.len();
            let merchant_ships = nation.merchant_fleet.len();

            let building_count = nation.buildings.len();

            let standing = game.diplomacy.get_standing(nid);
            let mut consulate_count = 0u32;
            let mut embassy_count = 0u32;
            let mut alliance_count = 0u32;
            let mut war_count = 0u32;
            let mut wars: Vec<String> = Vec::new();
            let mut alliances: Vec<String> = Vec::new();

            for other in &game.nations {
                if other.id == nid {
                    continue;
                }
                if let Some(rel) = game.diplomacy.get_relation(nid, other.id) {
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
            let total_resources: u32 = nation.warehouse.values().sum();
            let total_materials: u32 = nation.materials.values().sum();
            let total_goods: u32 = nation.goods.values().sum();

            serde_json::json!({
                "nation_id": nid.0,
                "nation_name": nation_name,
                "nation_color": nation_color,
                "is_human": is_human,
                "economy": {
                    "treasury": treasury_dollars,
                    "provinces": provinces,
                    "buildings": building_count,
                    "goods_revenue": nation.goods_sales_revenue_dollars,
                    "total_resources": total_resources,
                    "total_materials": total_materials,
                    "total_goods": total_goods,
                },
                "labor": {
                    "untrained": nation.labor.untrained,
                    "trained": nation.labor.trained,
                    "expert": nation.labor.expert,
                    "total": nation.labor.total_workers(),
                },
                "military": {
                    "total_army_count": total_army_count,
                    "total_army_fp": total_army_fp,
                    "total_warship_count": total_warship_count,
                    "merchant_ships": merchant_ships,
                    "generals_earned": nation.generals_earned,
                    "total_arms_built": nation.total_arms_built,
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
            })
        })
        .collect();

    serde_json::to_string(&entries).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// Return the newspaper headline archive for all past turns.
#[wasm_bindgen]
pub fn wasm_get_newspaper_archive(game_json: &str) -> String {
    let game: GameState = match serde_json::from_str(game_json) {
        Ok(g) => g,
        Err(e) => return format!("{{\"error\":\"{}\"}}", e),
    };

    let archive: Vec<serde_json::Value> = game
        .newspaper_archive
        .iter()
        .map(|(turn, headlines)| {
            let items: Vec<serde_json::Value> = headlines
                .iter()
                .map(|h| {
                    let mut obj = serde_json::json!({"text": &h.text, "category": &h.category});
                    if let Some(ref reason) = h.reason {
                        obj["reason"] = serde_json::json!(reason);
                    }
                    if h.is_non_action {
                        obj["is_non_action"] = serde_json::json!(true);
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

    // ── Newspaper archive reason serialization ─────────────────

    #[test]
    fn newspaper_archive_json_includes_reason_for_ai_headlines() {
        use domain::events::{Headline, HeadlineCategory};

        let json = make_game_json();
        let mut game: GameState = serde_json::from_str(&json).unwrap();
        game.game_data = domain::data::GameData::default();

        // Seed the archive with one AI-reasoned headline and one plain headline.
        game.newspaper_archive.push((
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
}
