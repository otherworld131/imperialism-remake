//! File-system loader for game data definitions.
//!
//! Contains RON definition structs and conversion logic (moved from domain).
//! Reads RON files from disk and passes pre-parsed domain types to
//! `GameData::from_parts()`. Domain never touches ron or the filesystem.

use domain::data::GameData;
use domain::economy::buildings::BuildingType;
use domain::economy::civilians::CivilianType;
use domain::events::TechId;
use domain::military::ships::{ShipCategory, ShipStats, ShipType};
use domain::military::units::ArmyUnitType;
use domain::tech::tree::{TechEffect, TechTree, Technology};
use domain::types::Money;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

// ── RON definition structs ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TechDefsFile {
    technologies: Vec<TechDef>,
}

#[derive(Debug, Deserialize)]
struct TechDef {
    id: u32,
    name: String,
    cost: i64,
    earliest_year: u32,
    latest_year: u32,
    prerequisites: Vec<u32>,
    effects: Vec<TechEffectDef>,
}

#[derive(Debug, Deserialize)]
enum TechEffectDef {
    UnlockUnit(String),
    UnlockBuilding(String),
    EnableTerrainImprovement { terrain: String, max_level: u8 },
    EnableInfrastructure(String),
    UnlockShip(String),
    UpgradeUnit { from: String, to: String },
    EnableCivilian(String),
    LuaScript(String),
}

#[derive(Debug, Deserialize)]
struct ShipDefsFile {
    ships: Vec<ShipDef>,
}

#[derive(Debug, Deserialize)]
struct ShipDef {
    name: String,
    category: String,
    firepower: u32,
    range: u32,
    armor: u32,
    hull: u32,
    speed: u32,
    cargo: u32,
    fabric_cost: u32,
    lumber_cost: u32,
    arms_cost: u32,
    steel_cost: u32,
    coal_cost: u32,
    prerequisite_tech: Option<String>,
}

// ── RON parsers ──────────────────────────────────────────────────────────

fn load_tech_tree(ron_str: &str) -> Result<TechTree, String> {
    let defs: TechDefsFile =
        ron::from_str(ron_str).map_err(|e| format!("Failed to parse technologies RON: {}", e))?;

    let technologies: Vec<Technology> = defs
        .technologies
        .into_iter()
        .map(|def| Technology {
            id: TechId(def.id),
            name: def.name,
            cost: Money::dollars(def.cost),
            earliest_year: def.earliest_year,
            latest_year: def.latest_year,
            prerequisites: def.prerequisites.into_iter().map(TechId).collect(),
            effects: def.effects.into_iter().map(convert_tech_effect).collect(),
        })
        .collect();

    let tree = TechTree::from_technologies(technologies);
    tree.validate()?;
    Ok(tree)
}

fn load_ship_stats(ron_str: &str) -> Result<HashMap<ShipType, ShipStats>, String> {
    let defs: ShipDefsFile =
        ron::from_str(ron_str).map_err(|e| format!("Failed to parse ships RON: {}", e))?;

    let mut map = HashMap::new();
    for def in defs.ships {
        if def.hull == 0 {
            return Err(format!("Ship '{}' has zero hull", def.name));
        }
        let ship_type = match def.name.as_str() {
            "Trader" => ShipType::Trader,
            "Indiaman" => ShipType::Indiaman,
            "Clipper" => ShipType::Clipper,
            "Paddlewheeler" => ShipType::Paddlewheeler,
            "Freighter" => ShipType::Freighter,
            "Frigate" => ShipType::Frigate,
            "Ship-of-the-Line" => ShipType::ShipOfTheLine,
            "Raider" => ShipType::Raider,
            "Ironclad" => ShipType::Ironclad,
            "Advanced Ironclad" => ShipType::AdvancedIronclad,
            "Armoured Cruiser" => ShipType::ArmouredCruiser,
            "Dreadnought" => ShipType::Dreadnought,
            "Battlecruiser" => ShipType::Battlecruiser,
            other => return Err(format!("Unknown ship type: {}", other)),
        };
        let category = match def.category.as_str() {
            "Merchant" => ShipCategory::Merchant,
            "Warship" => ShipCategory::Warship,
            other => return Err(format!("Unknown ship category: {}", other)),
        };
        map.insert(
            ship_type,
            ShipStats {
                firepower: def.firepower,
                range: def.range,
                armor: def.armor,
                hull: def.hull,
                speed: def.speed,
                cargo: def.cargo,
                category,
                fabric_cost: def.fabric_cost,
                lumber_cost: def.lumber_cost,
                arms_cost: def.arms_cost,
                steel_cost: def.steel_cost,
                coal_cost: def.coal_cost,
                prerequisite_tech: def.prerequisite_tech,
            },
        );
    }
    Ok(map)
}

fn convert_tech_effect(def: TechEffectDef) -> TechEffect {
    match def {
        TechEffectDef::UnlockUnit(name) => TechEffect::UnlockUnit(
            name.parse::<ArmyUnitType>()
                .unwrap_or_else(|e| panic!("tech data error: {}", e)),
        ),
        TechEffectDef::UnlockBuilding(name) => TechEffect::UnlockBuilding(
            name.parse::<BuildingType>()
                .unwrap_or_else(|e| panic!("tech data error: {}", e)),
        ),
        TechEffectDef::EnableTerrainImprovement { terrain, max_level } => {
            TechEffect::EnableTerrainImprovement { terrain, max_level }
        }
        TechEffectDef::EnableInfrastructure(name) => TechEffect::EnableInfrastructure(name),
        TechEffectDef::UnlockShip(name) => TechEffect::UnlockShip(name),
        TechEffectDef::UpgradeUnit { from, to } => TechEffect::UpgradeUnit {
            from: from
                .parse::<ArmyUnitType>()
                .unwrap_or_else(|e| panic!("tech data error: {}", e)),
            to: to
                .parse::<ArmyUnitType>()
                .unwrap_or_else(|e| panic!("tech data error: {}", e)),
        },
        TechEffectDef::EnableCivilian(name) => TechEffect::EnableCivilian(
            name.parse::<CivilianType>()
                .unwrap_or_else(|e| panic!("tech data error: {}", e)),
        ),
        TechEffectDef::LuaScript(script) => TechEffect::LuaScript(script),
    }
}

// ── Public API ───────────────────────────────────────────────────────────

fn parse_game_data_sources(tech_str: &str, ship_str: Option<&str>) -> GameData {
    let tech_tree =
        load_tech_tree(tech_str).unwrap_or_else(|e| panic!("Failed to load tech tree: {}", e));

    // Land-unit stats are loaded from scripts/config/units.lua inside
    // GameData::from_parts; this hardcoded fallback is only used if Lua
    // isn't available.
    let unit_stats = domain::data::default_unit_stats();

    let ship_stats = match ship_str {
        Some(s) => {
            load_ship_stats(s).unwrap_or_else(|e| panic!("Failed to load ship stats: {}", e))
        }
        None => domain::data::default_ship_stats(),
    };

    GameData::from_parts(tech_tree, unit_stats, ship_stats)
}

/// Load game data from RON files in the given data directory.
///
/// `technologies.ron` is required: panics if the file is missing or unreadable.
/// `ships.ron` falls back to hardcoded defaults if absent. Land-unit stats
/// always come from `scripts/config/units.lua` via `GameData::from_parts`.
pub fn load_game_data(data_dir: &Path) -> GameData {
    let tech_str = read_required(data_dir, "definitions/technologies.ron");
    let ship_str = try_read(data_dir, "definitions/ships.ron");
    parse_game_data_sources(&tech_str, ship_str.as_deref())
}

/// Load game data from the repo's checked-in RON definitions embedded at compile time.
pub fn load_embedded_game_data() -> GameData {
    parse_game_data_sources(
        include_str!("../../../data/definitions/technologies.ron"),
        Some(include_str!("../../../data/definitions/ships.ron")),
    )
}

fn read_required(base: &Path, relative: &str) -> String {
    let path = base.join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "Required data file '{}' could not be read: {}",
            path.display(),
            e
        )
    })
}

fn try_read(base: &Path, relative: &str) -> Option<String> {
    let path = base.join(relative);
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_game_data_from_data_dir() {
        let data = load_game_data(Path::new("../../data"));
        assert_eq!(data.tech_tree.all_techs().len(), 28);
    }

    #[test]
    fn load_tech_tree_from_ron_string() {
        let ron = r#"(
            technologies: [
                (
                    id: 1,
                    name: "Test Tech",
                    cost: 1000,
                    earliest_year: 1815,
                    latest_year: 1820,
                    prerequisites: [],
                    effects: [UnlockUnit("Regulars")],
                ),
            ],
        )"#;
        let tree = load_tech_tree(ron).unwrap();
        assert_eq!(tree.all_techs().len(), 1);
        assert_eq!(tree.all_techs()[0].name, "Test Tech");
    }

    #[test]
    fn load_tech_tree_with_prerequisites() {
        let ron = r#"(
            technologies: [
                (
                    id: 1,
                    name: "Base Tech",
                    cost: 0,
                    earliest_year: 1815,
                    latest_year: 1815,
                    prerequisites: [],
                    effects: [],
                ),
                (
                    id: 2,
                    name: "Advanced Tech",
                    cost: 5000,
                    earliest_year: 1820,
                    latest_year: 1825,
                    prerequisites: [1],
                    effects: [UnlockShip("Ironclad"), EnableTerrainImprovement(terrain: "Farm", max_level: 2)],
                ),
            ],
        )"#;
        let tree = load_tech_tree(ron).unwrap();
        assert_eq!(tree.all_techs().len(), 2);
        assert_eq!(tree.all_techs()[1].prerequisites, vec![TechId(1)]);
    }

    #[test]
    fn load_tech_tree_invalid_ron_returns_error() {
        let result = load_tech_tree("not valid ron");
        assert!(result.is_err());
    }

    #[test]
    fn load_ship_stats_rejects_zero_hull() {
        let ron = r#"(ships: [(name: "Frigate", category: "Warship", firepower: 3, range: 2, armor: 2, hull: 0, speed: 3, cargo: 0, fabric_cost: 2, lumber_cost: 5, arms_cost: 3, steel_cost: 0, coal_cost: 0, prerequisite_tech: None)])"#;
        let result = load_ship_stats(ron);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("zero hull"));
    }

    #[test]
    fn ron_ship_stats_define_all_thirteen_types() {
        let ron_content = std::fs::read_to_string("../../data/definitions/ships.ron").unwrap();
        let from_ron = load_ship_stats(&ron_content).unwrap();
        assert_eq!(from_ron.len(), 13);
        for ship_type in [
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
        ] {
            let stats = from_ron
                .get(&ship_type)
                .unwrap_or_else(|| panic!("ships.ron must define {:?}", ship_type));
            assert!(stats.hull > 0, "{:?} must have positive hull", ship_type);
            assert_eq!(
                ship_type.category(),
                stats.category,
                "category mismatch for {:?}",
                ship_type
            );
        }
    }
}
