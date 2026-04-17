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

/// Global game-rule constants. Loaded from `scripts/config/game.lua` when the
/// Lua feature is enabled; otherwise uses hardcoded defaults.
/// These define fundamental mechanics, NOT personality preferences.
#[derive(Debug, Clone)]
pub struct GameConfig {
    // Labor
    pub untrained_labor: u32,
    pub trained_labor: u32,
    pub expert_labor: u32,
    pub labor_per_production: u32,
    pub civilian_costs_expert: bool,
    // Production ratios
    pub resources_per_material: u32,
    pub materials_per_good: u32,
    pub coal_iron_ratio: u32,
    // Food
    pub food_per_worker: u32,
    pub starvation_cap: u32,
    pub canned_food_ratio: u32,
    // Immigration
    pub immigration_canned_food: u32,
    pub immigration_clothing: u32,
    pub immigration_furniture: u32,
    pub provinces_per_immigrant: u32,
    pub provinces_per_immigrant_upgraded: u32,
    // Monetary
    pub gold_value: i64,
    pub gems_value: i64,
    // Buildings
    pub expansion_delay_turns: u8,
    pub use_tier_expansion: bool,
    // Diplomacy costs
    pub consulate_cost: i64,
    pub embassy_cost: i64,
    // Starting conditions
    pub starting_freight_cars: u32,
    // Trade prices — materials (first-level processed)
    pub lumber_price: i64,
    pub steel_price: i64,
    pub fabric_price: i64,
    pub paper_price: i64,
    pub arms_price: i64,
    pub canned_food_price: i64,
    // Trade prices — finished goods (second-level processed)
    pub furniture_price: i64,
    pub clothing_price: i64,
    pub hardware_price: i64,
    // AI trade behaviour
    pub ai_consulate_target: u32,
    pub ai_consulate_priority_score: f64,
    pub ai_consulate_beyond_target_score: f64,
    pub ai_consulate_beyond_target_decay: f64,
    // Map generation
    pub min_food_tile_percent: u32,
    pub food_cluster_chance: u32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            untrained_labor: 1,
            trained_labor: 2,
            expert_labor: 4,
            labor_per_production: 2,
            civilian_costs_expert: true,
            resources_per_material: 2,
            materials_per_good: 2,
            coal_iron_ratio: 1,
            food_per_worker: 1,
            starvation_cap: 2,
            canned_food_ratio: 2,
            immigration_canned_food: 1,
            immigration_clothing: 1,
            immigration_furniture: 1,
            provinces_per_immigrant: 4,
            provinces_per_immigrant_upgraded: 3,
            gold_value: 500,
            gems_value: 1000,
            expansion_delay_turns: 2,
            use_tier_expansion: true,
            consulate_cost: 500,
            embassy_cost: 5000,
            starting_freight_cars: 5,
            lumber_price: 150,
            steel_price: 200,
            fabric_price: 150,
            paper_price: 100,
            arms_price: 300,
            canned_food_price: 100,
            furniture_price: 400,
            clothing_price: 400,
            hardware_price: 500,
            ai_consulate_target: 4,
            ai_consulate_priority_score: 30.0,
            ai_consulate_beyond_target_score: 3.0,
            ai_consulate_beyond_target_decay: 4.0,
            min_food_tile_percent: 20,
            food_cluster_chance: 40,
        }
    }
}

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
    /// Global game-rule constants from scripts/config/game.lua.
    pub game_config: GameConfig,
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

        #[allow(unused_mut)] // mut needed only with cfg(feature = "lua")
        let mut game_config = GameConfig::default();

        #[cfg(feature = "lua")]
        let lua_engine = {
            let engine = LuaEngine::new().ok();
            if let Some(ref e) = engine {
                if let Err(err) = crate::ai::lua_bridge::load_scripts(e) {
                    eprintln!("[GameData] Warning: Lua script loading failed: {}", err);
                }
                game_config = crate::ai::lua_bridge::load_game_config(e);
            }
            engine
        };

        GameData {
            tech_tree,
            unit_stats,
            ship_stats,
            #[cfg(feature = "lua")]
            lua_engine,
            game_config,
        }
    }
}

impl Default for GameData {
    fn default() -> Self {
        #[allow(unused_mut)] // mut needed only with cfg(feature = "lua")
        let mut game_config = GameConfig::default();

        #[cfg(feature = "lua")]
        let lua_engine = {
            let engine = LuaEngine::new().ok();
            if let Some(ref e) = engine {
                if let Err(err) = crate::ai::lua_bridge::load_scripts(e) {
                    eprintln!("[GameData] Warning: Lua script loading failed: {}", err);
                }
                game_config = crate::ai::lua_bridge::load_game_config(e);
            }
            engine
        };

        GameData {
            tech_tree: TechTree::default(),
            unit_stats: default_unit_stats(),
            ship_stats: default_ship_stats(),
            #[cfg(feature = "lua")]
            lua_engine,
            game_config,
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
