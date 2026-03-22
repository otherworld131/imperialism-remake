//! Data-driven game definitions.
//!
//! The `GameData` struct holds all game configuration that was previously
//! hardcoded — tech tree, unit stats, ship stats, etc. It can be constructed
//! from hardcoded defaults (`GameData::default()`) or loaded from RON
//! strings (`GameData::from_ron_strings()`).

pub mod definitions;
pub mod loader;

use crate::military::ships::{ShipStats, ShipType};
use crate::military::units::{ArmyUnitType, UnitStats};
#[cfg(feature = "lua")]
use crate::scripting::LuaEngine;
use crate::tech::TechTree;
use std::collections::HashMap;

/// Aggregate container for all data-driven game definitions.
///
/// Stored in `GameState` and threaded through the game via `&self` references.
/// Skipped during serialization — always reconstructed on load.
pub struct GameData {
    pub tech_tree: TechTree,
    pub unit_stats: HashMap<ArmyUnitType, UnitStats>,
    pub ship_stats: HashMap<ShipType, ShipStats>,
    #[cfg(feature = "lua")]
    pub lua_engine: Option<LuaEngine>,
}

impl GameData {
    /// Construct GameData from RON strings, falling back to hardcoded
    /// defaults for any section that is `None` or fails to parse.
    pub fn from_ron_strings(
        tech_ron: Option<&str>,
        units_ron: Option<&str>,
        ships_ron: Option<&str>,
    ) -> Self {
        let tech_tree = tech_ron
            .and_then(|s| loader::load_tech_tree(s).ok())
            .unwrap_or_default();

        let unit_stats = units_ron
            .and_then(|s| loader::load_unit_stats(s).ok())
            .unwrap_or_else(default_unit_stats);

        let ship_stats = ships_ron
            .and_then(|s| loader::load_ship_stats(s).ok())
            .unwrap_or_else(default_ship_stats);

        GameData {
            tech_tree,
            unit_stats,
            ship_stats,
            #[cfg(feature = "lua")]
            lua_engine: LuaEngine::new().ok(),
        }
    }
}

impl Default for GameData {
    fn default() -> Self {
        GameData {
            tech_tree: TechTree::default(),
            unit_stats: default_unit_stats(),
            ship_stats: default_ship_stats(),
            #[cfg(feature = "lua")]
            lua_engine: LuaEngine::new().ok(),
        }
    }
}

/// Build default unit stats from the hardcoded `ArmyUnitType::stats()` method.
fn default_unit_stats() -> HashMap<ArmyUnitType, UnitStats> {
    use ArmyUnitType::*;
    let all_types = [
        Militia,
        Regulars,
        Grenadiers,
        RifleInfantry,
        Guards,
        Sharpshooters,
        ModernInfantry,
        MachineGunners,
        Rangers,
        Cuirassiers,
        Scouts,
        CarbineCavalry,
        Armour,
        Mechanised,
        LightArtillery,
        StandardArtillery,
        FieldArtillery,
        SiegeArtillery,
        RailroadGun,
        MobileArtillery,
        Sapper,
        General,
    ];
    all_types.into_iter().map(|t| (t, t.stats())).collect()
}

/// Build default ship stats from the hardcoded `ShipType::stats()` method.
fn default_ship_stats() -> HashMap<ShipType, ShipStats> {
    use ShipType::*;
    let all_types = [
        Trader,
        Indiaman,
        Clipper,
        Paddlewheeler,
        Freighter,
        Frigate,
        ShipOfTheLine,
        Raider,
        Ironclad,
        AdvancedIronclad,
        ArmouredCruiser,
        Dreadnought,
        Battlecruiser,
    ];
    all_types.into_iter().map(|t| (t, t.stats())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_game_data_has_28_techs() {
        let data = GameData::default();
        assert_eq!(data.tech_tree.all_techs().len(), 28);
    }

    #[test]
    fn default_game_data_has_22_unit_types() {
        let data = GameData::default();
        assert_eq!(data.unit_stats.len(), 22);
    }

    #[test]
    fn default_game_data_has_13_ship_types() {
        let data = GameData::default();
        assert_eq!(data.ship_stats.len(), 13);
    }

    #[test]
    fn from_ron_strings_with_none_uses_defaults() {
        let data = GameData::from_ron_strings(None, None, None);
        assert_eq!(data.tech_tree.all_techs().len(), 28);
        assert_eq!(data.unit_stats.len(), 22);
        assert_eq!(data.ship_stats.len(), 13);
    }

    #[test]
    fn from_ron_strings_with_valid_tech_ron() {
        let ron = r#"(
            technologies: [
                (
                    id: 1,
                    name: "Only Tech",
                    cost: 0,
                    earliest_year: 1815,
                    latest_year: 1815,
                    prerequisites: [],
                    effects: [],
                ),
            ],
        )"#;
        let data = GameData::from_ron_strings(Some(ron), None, None);
        assert_eq!(data.tech_tree.all_techs().len(), 1);
        // Other sections fall back to defaults
        assert_eq!(data.unit_stats.len(), 22);
        assert_eq!(data.ship_stats.len(), 13);
    }

    #[test]
    fn from_ron_strings_with_invalid_ron_falls_back_to_default() {
        let data = GameData::from_ron_strings(Some("invalid"), Some("invalid"), Some("invalid"));
        assert_eq!(data.tech_tree.all_techs().len(), 28);
        assert_eq!(data.unit_stats.len(), 22);
        assert_eq!(data.ship_stats.len(), 13);
    }

    #[test]
    fn default_unit_stats_match_hardcoded() {
        let data = GameData::default();
        // Spot-check a few units against hardcoded values
        let militia = &data.unit_stats[&ArmyUnitType::Militia];
        assert_eq!(militia.firepower, 1);
        assert_eq!(militia.movement, 0);
        assert_eq!(militia.cost, crate::types::Money::dollars(50));

        let guards = &data.unit_stats[&ArmyUnitType::Guards];
        assert_eq!(guards.firepower, 5);
        assert_eq!(guards.range, 2);
        assert_eq!(
            guards.prerequisite_tech.as_deref(),
            Some("Professional Army")
        );
    }

    #[test]
    fn default_ship_stats_match_hardcoded() {
        let data = GameData::default();
        let frigate = &data.ship_stats[&ShipType::Frigate];
        assert_eq!(frigate.firepower, 3);
        assert_eq!(frigate.hull, 35);

        let dreadnought = &data.ship_stats[&ShipType::Dreadnought];
        assert_eq!(dreadnought.firepower, 15);
        assert_eq!(dreadnought.hull, 80);
    }
}
