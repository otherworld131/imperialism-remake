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
    // Diplomatic relationship tuning
    pub voluntary_incorporation_threshold: i32,
    pub trade_relation_improvement_cap: i32,
    pub trade_relation_turn_interval: u32,
    // Starting conditions
    pub starting_freight_cars: u32,
    pub starting_engineers: u32,
    // Infrastructure costs ($)
    pub engineer_cost: i64,
    pub prospector_cost: i64,
    pub miner_cost: i64,
    pub farmer_cost: i64,
    pub rancher_cost: i64,
    pub forester_cost: i64,
    pub driller_cost: i64,
    pub depot_cost: i64,
    pub port_cost: i64,
    pub railroad_cost_grassland: i64,
    pub railroad_cost_forest: i64,
    pub railroad_cost_desert: i64,
    pub railroad_cost_tundra: i64,
    pub railroad_cost_swamp: i64,
    pub railroad_cost_hills: i64,
    pub railroad_cost_mountain: i64,
    pub fort_cost_level_1: i64,
    pub fort_cost_level_2: i64,
    pub fort_cost_level_3: i64,
    // Engineer build task turn counts
    pub build_turns_railroad: u8,
    pub build_turns_depot: u8,
    pub build_turns_port: u8,
    // Tech prerequisites for laying railroad on each land terrain.
    // `None` means the terrain is always buildable; `Some(name)` means the
    // nation must have researched a tech with that name.
    pub railroad_tech_grassland: Option<String>,
    pub railroad_tech_forest: Option<String>,
    pub railroad_tech_desert: Option<String>,
    pub railroad_tech_tundra: Option<String>,
    pub railroad_tech_hills: Option<String>,
    pub railroad_tech_swamp: Option<String>,
    pub railroad_tech_mountain: Option<String>,
    // AI infrastructure planner horizon: number of turns to amortise depot
    // yield over when comparing candidate placements to build costs.
    pub infrastructure_horizon_turns: u32,
    // AI infrastructure planner scoring weights (card #132).
    // `net_score = coverage * horizon * infra_coverage_weight
    //            - path_cost * infra_path_cost_weight - depot_cost`
    pub infra_coverage_weight: f64,
    pub infra_path_cost_weight: f64,
    // Trade-aware demand (cards #131 / #132): how far back to look for
    // import history, and how strongly to discount demand for resources
    // already flowing in via trade.
    //
    // The discount for resource R is:
    //   discount(R) = trade_discount_weight
    //               * ( history_rate(R)      * trade_history_weight
    //                 + consulate_potential(R) * trade_consulate_potential_weight )
    // where history_rate is total recent imports divided by lookback_turns
    // (per-turn decay so a one-off buy 8 turns ago is weighted 1/8 this turn).
    // `trade_discount_weight = 0` disables the whole feature.
    pub trade_lookback_turns: u32,
    pub trade_discount_weight: f64,
    pub trade_history_weight: f64,
    pub trade_consulate_potential_weight: f64,
    // Engineer-hire scoring (drives `score_hire_engineer`).
    pub engineer_hire_max: u32,
    pub engineer_hire_base: u32,
    pub engineer_hire_path_coeff: u32,
    pub engineer_hire_cap: u32,
    // Improver civilian-hire scoring (drives `score_civilian`). Replaces the
    // old fixed 4-step coverage ladder with a continuous saturation formula:
    // each existing civilian "covers" ~`civilian_target_tiles_per_worker`
    // improvable tiles; every unmet tile beyond that capacity adds
    // `civilian_coverage_per_unmet` to the score. Scales with empire size.
    pub civilian_target_tiles_per_worker: u32,
    pub civilian_coverage_per_unmet: f64,
    pub civilian_hire_bootstrap: f64,
    pub civilian_idle_penalty: f64,
    // Backlog scoring weights per personality. `(category, personality)` →
    // points added per turn the category has been neglected.
    // Categories: military, infrastructure, diplomacy, hire_engineer, hire_improver.
    pub backlog_weight_aggressive_military: u32,
    pub backlog_weight_aggressive_infra: u32,
    pub backlog_weight_aggressive_diplomacy: u32,
    pub backlog_weight_aggressive_hire_engineer: u32,
    pub backlog_weight_aggressive_hire_improver: u32,
    pub backlog_weight_balanced_military: u32,
    pub backlog_weight_balanced_infra: u32,
    pub backlog_weight_balanced_diplomacy: u32,
    pub backlog_weight_balanced_hire_engineer: u32,
    pub backlog_weight_balanced_hire_improver: u32,
    pub backlog_weight_economic_military: u32,
    pub backlog_weight_economic_infra: u32,
    pub backlog_weight_economic_diplomacy: u32,
    pub backlog_weight_economic_hire_engineer: u32,
    pub backlog_weight_economic_hire_improver: u32,
    pub backlog_weight_diplomatic_military: u32,
    pub backlog_weight_diplomatic_infra: u32,
    pub backlog_weight_diplomatic_diplomacy: u32,
    pub backlog_weight_diplomatic_hire_engineer: u32,
    pub backlog_weight_diplomatic_hire_improver: u32,
    // Cap on day-1 backlog: prevents starting-state backlog from exploding.
    pub backlog_initial_cap: u32,
    // Score awarded to a Consulate/Embassy with a priority minor target that
    // hasn't been secured yet — large enough to dominate any other category.
    pub priority_minor_target_score: f64,
    // Number of priority-minor diplomacy targets per personality.
    pub priority_minor_targets_aggressive: u32,
    pub priority_minor_targets_balanced: u32,
    pub priority_minor_targets_economic: u32,
    pub priority_minor_targets_diplomatic: u32,
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
    // Garrison (per-province militia) — see manual page 36.
    pub default_garrison_per_province: u32,
    pub minor_default_garrison: u32,
    pub max_garrison_per_province: u32,
    pub garrison_regen_interval_turns: u32,
    pub rest_heal_amount: u8,
    // Naval scoring coefficients for ai_scored_spending (card #112)
    pub spending_naval_base: f64,
    pub spending_naval_war_bonus: f64,
    pub spending_naval_gap_coeff: f64,
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
            voluntary_incorporation_threshold: 90,
            trade_relation_improvement_cap: 2,
            trade_relation_turn_interval: 3,
            starting_freight_cars: 15,
            starting_engineers: 1,
            engineer_cost: 500,
            prospector_cost: 100,
            miner_cost: 1500,
            farmer_cost: 100,
            rancher_cost: 100,
            forester_cost: 100,
            driller_cost: 2000,
            depot_cost: 2000,
            port_cost: 3000,
            railroad_cost_grassland: 100,
            railroad_cost_forest: 100,
            railroad_cost_desert: 150,
            railroad_cost_tundra: 150,
            railroad_cost_swamp: 300,
            railroad_cost_hills: 200,
            railroad_cost_mountain: 500,
            fort_cost_level_1: 5000,
            fort_cost_level_2: 7500,
            fort_cost_level_3: 10000,
            build_turns_railroad: 1,
            build_turns_depot: 2,
            build_turns_port: 3,
            railroad_tech_grassland: None,
            railroad_tech_forest: None,
            railroad_tech_desert: None,
            railroad_tech_tundra: None,
            railroad_tech_hills: None,
            railroad_tech_swamp: Some("Iron Railroad Bridge".to_string()),
            railroad_tech_mountain: Some("Compound Steam Engine".to_string()),
            infrastructure_horizon_turns: 50,
            infra_coverage_weight: 1.0,
            infra_path_cost_weight: 1.0,
            trade_lookback_turns: 8,
            trade_discount_weight: 0.5,
            trade_history_weight: 1.0,
            trade_consulate_potential_weight: 0.25,
            engineer_hire_max: 3,
            engineer_hire_base: 100,
            engineer_hire_path_coeff: 30,
            engineer_hire_cap: 250,
            civilian_target_tiles_per_worker: 3,
            civilian_coverage_per_unmet: 3.0,
            civilian_hire_bootstrap: 15.0,
            civilian_idle_penalty: 8.0,
            backlog_weight_aggressive_military: 50,
            backlog_weight_aggressive_infra: 25,
            backlog_weight_aggressive_diplomacy: 5,
            backlog_weight_aggressive_hire_engineer: 20,
            backlog_weight_aggressive_hire_improver: 15,
            backlog_weight_balanced_military: 30,
            backlog_weight_balanced_infra: 30,
            backlog_weight_balanced_diplomacy: 20,
            backlog_weight_balanced_hire_engineer: 25,
            backlog_weight_balanced_hire_improver: 20,
            backlog_weight_economic_military: 15,
            backlog_weight_economic_infra: 50,
            backlog_weight_economic_diplomacy: 20,
            backlog_weight_economic_hire_engineer: 30,
            backlog_weight_economic_hire_improver: 25,
            backlog_weight_diplomatic_military: 10,
            backlog_weight_diplomatic_infra: 35,
            backlog_weight_diplomatic_diplomacy: 40,
            backlog_weight_diplomatic_hire_engineer: 25,
            backlog_weight_diplomatic_hire_improver: 20,
            backlog_initial_cap: 20,
            priority_minor_target_score: 1000.0,
            priority_minor_targets_aggressive: 3,
            priority_minor_targets_balanced: 4,
            priority_minor_targets_economic: 4,
            priority_minor_targets_diplomatic: 5,
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
            default_garrison_per_province: 4,
            minor_default_garrison: 3,
            max_garrison_per_province: 8,
            garrison_regen_interval_turns: 2,
            rest_heal_amount: 10,
            spending_naval_base: 2.0,
            spending_naval_war_bonus: 10.0,
            spending_naval_gap_coeff: 1.5,
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
