//! Data-driven game definitions.
//!
//! The `GameData` struct holds all game configuration that was previously
//! hardcoded — tech tree, unit stats, ship stats, etc. It is constructed
//! from hardcoded defaults (`GameData::default()`) or from parsed data
//! supplied by infrastructure (`GameData::from_parts()`).

use crate::military::ships::{ShipStats, ShipType};
use crate::military::units::{ArmyUnitType, UnitStats};
#[cfg(feature = "lua")]
use crate::scripting::LuaEngine;
use crate::tech::TechTree;
use std::collections::HashMap;

#[cfg(test)]
#[derive(Debug, serde::Deserialize)]
struct TestTechDefsFile {
    technologies: Vec<TestTechDef>,
}

#[cfg(test)]
#[derive(Debug, serde::Deserialize)]
struct TestTechDef {
    id: u32,
    name: String,
    cost: i64,
    earliest_year: u32,
    latest_year: u32,
    prerequisites: Vec<u32>,
    effects: Vec<TestTechEffectDef>,
}

#[cfg(test)]
#[derive(Debug, serde::Deserialize)]
enum TestTechEffectDef {
    UnlockUnit(String),
    UnlockBuilding(String),
    EnableTerrainImprovement { terrain: String, max_level: u8 },
    EnableInfrastructure(String),
    UnlockShip(String),
    UpgradeUnit { from: String, to: String },
    EnableCivilian(String),
    LuaScript(String),
}

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
    // Minimum relationship score (-100..=100) before AI upgrades a consulate
    // to an embassy. Consulates are cheap and themselves grant a relationship
    // bonus, so the AI should defer the (expensive) embassy until the score
    // already shows real warmth. Priority-minor targets bypass this gate.
    pub ai_embassy_min_relation: i32,
    // Diplomatic relationship tuning
    pub voluntary_incorporation_threshold: i32,
    pub trade_relation_improvement_cap: i32,
    pub trade_relation_turn_interval: u32,
    // Starting conditions
    pub starting_freight_cars: u32,
    pub starting_engineers: u32,
    // Cost to build a single freight car ($).
    pub freight_car_cost: i64,
    // Per-turn army maintenance, in cents per `arms_required` slot. Card #216
    // dropped this from the original $25/arm (2500 ¢) to $2.50/arm (250 ¢).
    // Garrison units (militia, garrison artillery) are exempt and always pay 0.
    pub army_maintenance_cents_per_arm: i64,
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
    // Card #217: depot-planner weight on improvable-but-not-yet-improved tiers
    // (0 disables, 1.0 = 1 demand-weighted point per unimproved tier).
    pub infra_improvability_weight: f64,
    // Card #217: early-game bias multiplier on `score_infrastructure` for the
    // first `infra_early_game_bias_turns` turns of the game.
    pub infra_early_game_bias_turns: u32,
    pub infra_early_game_bias: f64,
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
    // Improver civilian-hire scoring (drives `score_civilian`). Continuous
    // saturation formula: each existing civilian "covers" weighted demand
    // worth ~`civilian_target_tiles_per_worker` units; unmet weighted
    // demand drives the score. The per-tile weight depends on the tile's
    // connectivity (see civilian_coverage_* fields below) so disconnected
    // tiles don't pull as hard for additional improvers as connected ones.
    pub civilian_target_tiles_per_worker: u32,
    // Card #217 follow-up: per-tile coverage weights. See `score_civilian`
    // and `scripts/config/game.lua` for the bucket definitions.
    pub civilian_coverage_collectable: f64,
    pub civilian_coverage_rail_adjacent: f64,
    pub civilian_coverage_unconnected: f64,
    pub civilian_coverage_undiscovered: f64,
    pub civilian_hire_bootstrap: f64,
    pub civilian_idle_penalty: f64,
    // Card #217: improver-deployment connectivity bucket weights. See
    // `ai_deploy_civilians` and game.lua for semantics.
    pub civilian_connectivity_planned_weight: f64,
    pub civilian_connectivity_adjacent_weight: f64,
    pub civilian_connectivity_unconnected_weight: f64,
    pub civilian_connectivity_softening_threshold: i64,
    // Tech prerequisites for hiring specific civilian types — must match the
    // tech tree. `None` means the civilian is available from turn 1.
    // Per the original Imperialism manual (p.27–28): Rancher needs Feed Grasses,
    // Forester needs Iron Railroad Bridge, Driller needs Oil Drilling.
    pub civilian_rancher_tech: Option<String>,
    pub civilian_forester_tech: Option<String>,
    pub civilian_driller_tech: Option<String>,
    // AI prospector hiring: target one prospector per N undiscovered hexes
    // owned by the nation. 0 disables prospector hiring entirely.
    pub ai_prospector_per_hexes: u32,
    // Starting civilians per Great Power.
    pub starting_prospectors: u32,
    pub starting_miners: u32,
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
    // D-4: Pact-defense evaluation thresholds (evaluate_pact_defense)
    pub pact_defense_standing_gate: i32,
    pub pact_defense_relationship_weight: f64,
    pub pact_defense_military_weight: f64,
    pub pact_defense_bias_aggressive: f64,
    pub pact_defense_bias_diplomatic: f64,
    pub pact_defense_bias_balanced: f64,
    pub pact_defense_bias_economic: f64,
    pub pact_defense_threshold_aggressive: f64,
    pub pact_defense_threshold_diplomatic: f64,
    pub pact_defense_threshold_balanced: f64,
    pub pact_defense_threshold_economic: f64,
    // D-5: Combat terrain / fort defense bonuses
    pub terrain_defense_mountain: f64,
    pub terrain_defense_hills: f64,
    pub terrain_defense_forest: f64,
    pub terrain_defense_swamp: f64,
    pub fort_defense_level1: f64,
    pub fort_defense_level2: f64,
    pub fort_defense_level3: f64,
    pub battle_attacker_fp_loss_ratio: f64,
    pub battle_defender_fp_loss_ratio: f64,
    // D-6: Civilian/worker hiring thresholds
    pub labor_workers_per_province_base: u32,
    pub labor_workers_per_province_wealthy: u32,
    pub labor_wealthy_treasury_threshold: i64,
    pub labor_min_workers_floor: u32,
    pub labor_hire_civilian_tier1_treasury: i64,
    pub labor_hire_civilian_tier1_max: u32,
    pub labor_hire_civilian_tier2_treasury: i64,
    pub labor_hire_civilian_tier2_max: u32,
    // D-7: AI consulate treasury threshold
    pub ai_consulate_treasury_threshold: i64,
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
            ai_embassy_min_relation: 50,
            voluntary_incorporation_threshold: 90,
            trade_relation_improvement_cap: 2,
            trade_relation_turn_interval: 3,
            starting_freight_cars: 15,
            starting_engineers: 1,
            freight_car_cost: 200,
            army_maintenance_cents_per_arm: 250,
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
            infra_improvability_weight: 0.5,
            infra_early_game_bias_turns: 5,
            infra_early_game_bias: 1.5,
            trade_lookback_turns: 8,
            trade_discount_weight: 0.5,
            trade_history_weight: 1.0,
            trade_consulate_potential_weight: 0.25,
            engineer_hire_max: 3,
            engineer_hire_base: 100,
            engineer_hire_path_coeff: 30,
            engineer_hire_cap: 250,
            civilian_target_tiles_per_worker: 8,
            civilian_coverage_collectable: 3.0,
            civilian_coverage_rail_adjacent: 1.5,
            civilian_coverage_unconnected: 0.5,
            civilian_coverage_undiscovered: 1.5,
            civilian_hire_bootstrap: 15.0,
            civilian_idle_penalty: 8.0,
            civilian_connectivity_planned_weight: 30.0,
            civilian_connectivity_adjacent_weight: 60.0,
            civilian_connectivity_unconnected_weight: 100.0,
            civilian_connectivity_softening_threshold: 20_000,
            civilian_rancher_tech: Some("Feed Grasses".to_string()),
            civilian_forester_tech: Some("Iron Railroad Bridge".to_string()),
            civilian_driller_tech: Some("Oil Drilling".to_string()),
            ai_prospector_per_hexes: 10,
            starting_prospectors: 1,
            starting_miners: 1,
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
            pact_defense_standing_gate: 30,
            pact_defense_relationship_weight: 0.4,
            pact_defense_military_weight: 0.4,
            pact_defense_bias_aggressive: 0.2,
            pact_defense_bias_diplomatic: 0.1,
            pact_defense_bias_balanced: 0.0,
            pact_defense_bias_economic: -0.15,
            pact_defense_threshold_aggressive: 0.2,
            pact_defense_threshold_diplomatic: 0.3,
            pact_defense_threshold_balanced: 0.35,
            pact_defense_threshold_economic: 0.5,
            terrain_defense_mountain: 0.50,
            terrain_defense_hills: 0.30,
            terrain_defense_forest: 0.20,
            terrain_defense_swamp: 0.15,
            fort_defense_level1: 0.20,
            fort_defense_level2: 0.40,
            fort_defense_level3: 0.60,
            battle_attacker_fp_loss_ratio: 0.60,
            battle_defender_fp_loss_ratio: 2.0,
            labor_workers_per_province_base: 2,
            labor_workers_per_province_wealthy: 3,
            labor_wealthy_treasury_threshold: 20_000,
            labor_min_workers_floor: 5,
            labor_hire_civilian_tier1_treasury: 1000,
            labor_hire_civilian_tier1_max: 2,
            labor_hire_civilian_tier2_treasury: 2000,
            labor_hire_civilian_tier2_max: 4,
            ai_consulate_treasury_threshold: 2000,
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
    /// Construct GameData from pre-parsed parts supplied by infrastructure.
    ///
    /// Infrastructure parses RON files and calls this constructor so that
    /// domain never needs a ron dependency.
    pub fn from_parts(
        tech_tree: TechTree,
        unit_stats: HashMap<ArmyUnitType, UnitStats>,
        ship_stats: HashMap<ShipType, ShipStats>,
    ) -> Self {
        #[allow(unused_mut)]
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
    /// Returns minimal GameData with an empty tech tree and hardcoded unit/ship stats.
    ///
    /// Used as a placeholder during snapshot restore; infrastructure replaces
    /// `game_data` with the full data loaded from RON files after deserialization.
    fn default() -> Self {
        #[allow(unused_mut)]
        let mut game_config = GameConfig::default();
        #[allow(unused_mut)]
        let mut tech_tree = TechTree::default(); // empty until Lua populates

        #[cfg(feature = "lua")]
        let lua_engine = {
            let engine = LuaEngine::new().ok();
            if let Some(ref e) = engine {
                if let Err(err) = crate::ai::lua_bridge::load_scripts(e) {
                    eprintln!("[GameData] Warning: Lua script loading failed: {}", err);
                }
                game_config = crate::ai::lua_bridge::load_game_config(e);
                match crate::ai::lua_bridge::load_tech_tree(e) {
                    Some(loaded) => tech_tree = loaded,
                    None => eprintln!(
                        "[GameData] ERROR: scripts/config/tech_tree.lua failed to load. \
                         Tech tree is empty; tech-gating and research will not function."
                    ),
                }
            }
            engine
        };

        GameData {
            tech_tree,
            unit_stats: default_unit_stats(),
            ship_stats: default_ship_stats(),
            #[cfg(feature = "lua")]
            lua_engine,
            game_config,
        }
    }
}

/// Build default unit stats from the hardcoded `ArmyUnitType::stats()` method.
pub fn default_unit_stats() -> HashMap<ArmyUnitType, UnitStats> {
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
pub fn default_ship_stats() -> HashMap<ShipType, ShipStats> {
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
fn convert_test_tech_effect(def: TestTechEffectDef) -> crate::tech::tree::TechEffect {
    use crate::economy::buildings::BuildingType;
    use crate::economy::civilians::CivilianType;
    use crate::tech::tree::TechEffect;

    match def {
        TestTechEffectDef::UnlockUnit(name) => {
            TechEffect::UnlockUnit(name.parse().expect("valid army unit in test data"))
        }
        TestTechEffectDef::UnlockBuilding(name) => TechEffect::UnlockBuilding(
            name.parse::<BuildingType>().expect("valid building type in test data"),
        ),
        TestTechEffectDef::EnableTerrainImprovement { terrain, max_level } => {
            TechEffect::EnableTerrainImprovement { terrain, max_level }
        }
        TestTechEffectDef::EnableInfrastructure(name) => TechEffect::EnableInfrastructure(name),
        TestTechEffectDef::UnlockShip(name) => TechEffect::UnlockShip(name),
        TestTechEffectDef::UpgradeUnit { from, to } => TechEffect::UpgradeUnit {
            from: from.parse().expect("valid army unit in test data"),
            to: to.parse().expect("valid army unit in test data"),
        },
        TestTechEffectDef::EnableCivilian(name) => TechEffect::EnableCivilian(
            name.parse::<CivilianType>().expect("valid civilian type in test data"),
        ),
        TestTechEffectDef::LuaScript(script) => TechEffect::LuaScript(script),
    }
}

#[cfg(test)]
pub fn test_game_data() -> GameData {
    use crate::events::TechId;
    use crate::tech::tree::Technology;
    use crate::types::Money;

    let defs: TestTechDefsFile = ron::from_str(include_str!("../../../../data/definitions/technologies.ron"))
        .expect("technologies.ron must be valid");
    let tech_tree = TechTree::from_technologies(
        defs.technologies
            .into_iter()
            .map(|def| Technology {
                id: TechId(def.id),
                name: def.name,
                cost: Money::dollars(def.cost),
                earliest_year: def.earliest_year,
                latest_year: def.latest_year,
                prerequisites: def.prerequisites.into_iter().map(TechId).collect(),
                effects: def.effects.into_iter().map(convert_test_tech_effect).collect(),
            })
            .collect(),
    );
    tech_tree
        .validate()
        .expect("embedded tech tree test data should validate");
    GameData::from_parts(tech_tree, default_unit_stats(), default_ship_stats())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn default_unit_stats_spot_check() {
        let data = GameData::default();
        let militia = &data.unit_stats[&ArmyUnitType::Militia];
        assert_eq!(militia.firepower, 1);
        assert_eq!(militia.movement, 0);
        assert_eq!(militia.cost, crate::types::Money::dollars(50));
    }

    #[test]
    fn default_ship_stats_spot_check() {
        let data = GameData::default();
        let frigate = &data.ship_stats[&ShipType::Frigate];
        assert_eq!(frigate.firepower, 3);
        assert_eq!(frigate.hull, 35);
    }

}
