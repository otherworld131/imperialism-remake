//! File-system loader for game data definitions.
//!
//! Contains RON definition structs and conversion logic (moved from domain).
//! Reads RON files from disk and passes pre-parsed domain types to
//! `GameData::from_parts()`. Domain never touches ron or the filesystem.
//!
//! Ship and army unit stats are loaded from Lua scripts inside `GameData::from_parts`.
//! Only tech tree data is parsed from RON here.

use domain::data::GameData;
use domain::economy::buildings::BuildingType;
use domain::economy::civilians::CivilianType;
use domain::events::TechId;
use domain::military::units::ArmyUnitType;
use domain::tech::tree::{TechEffect, TechTree, Technology};
use domain::types::Money;
use serde::Deserialize;
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

fn parse_game_data_sources(tech_str: &str) -> GameData {
    let tech_tree =
        load_tech_tree(tech_str).unwrap_or_else(|e| panic!("Failed to load tech tree: {}", e));

    // Unit and ship stats are loaded from Lua scripts inside GameData::from_parts.
    // These defaults are only used if Lua is unavailable.
    let unit_stats = domain::data::default_unit_stats();
    let ship_stats = domain::data::default_ship_stats();

    GameData::from_parts(tech_tree, unit_stats, ship_stats)
}

/// Load game data from RON files in the given data directory.
///
/// `technologies.ron` is required: panics if the file is missing or unreadable.
/// Unit and ship stats come from Lua scripts via `GameData::from_parts`.
pub fn load_game_data(data_dir: &Path) -> GameData {
    let tech_str = read_required(data_dir, "definitions/technologies.ron");
    parse_game_data_sources(&tech_str)
}

/// Load game data from the repo's checked-in RON definitions embedded at compile time.
pub fn load_embedded_game_data() -> GameData {
    parse_game_data_sources(include_str!("../../../data/definitions/technologies.ron"))
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

}
