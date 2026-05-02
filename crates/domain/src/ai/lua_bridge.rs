//! Bridge between the Rust AI engine and Lua personality scripts.
//!
//! Loads personality scripts from `scripts/ai/` at compile time and provides
//! functions to query Lua for tech selection, war evaluation, and config parameters.
//! Falls back to Rust logic when Lua is unavailable or returns nil.

use crate::events::TechId;
use crate::game_state::GameState;
use crate::scripting::LuaEngine;
use crate::tech::tree::TechEffect;
use crate::types::*;

use super::common::AiPersonality;

// Embed scripts at compile time for sandboxing safety.
const GAME_CONFIG_LUA: &str = include_str!("../../../../scripts/config/game.lua");
const TECH_TREE_LUA: &str = include_str!("../../../../scripts/config/tech_tree.lua");
const UNITS_LUA: &str = include_str!("../../../../scripts/config/units.lua");
const BALANCED_LUA: &str = include_str!("../../../../scripts/ai/balanced.lua");
const AGGRESSIVE_LUA: &str = include_str!("../../../../scripts/ai/aggressive.lua");
const DIPLOMATIC_LUA: &str = include_str!("../../../../scripts/ai/diplomatic.lua");
const ECONOMIC_LUA: &str = include_str!("../../../../scripts/ai/economic.lua");

use crate::data::GameConfig;

/// Load the game config from the Lua `game_config` global table.
pub fn load_game_config(engine: &LuaEngine) -> GameConfig {
    let lua = engine.lua();
    let table: mlua::Table = match lua.globals().get("game_config") {
        Ok(t) => t,
        Err(_) => return GameConfig::default(),
    };
    let cfg = GameConfig {
        untrained_labor: table.get("untrained_labor").unwrap_or(1),
        trained_labor: table.get("trained_labor").unwrap_or(2),
        expert_labor: table.get("expert_labor").unwrap_or(4),
        labor_per_production: table.get("labor_per_production").unwrap_or(2),
        civilian_costs_expert: table.get("civilian_costs_expert").unwrap_or(true),
        resources_per_material: table.get("resources_per_material").unwrap_or(2),
        materials_per_good: table.get("materials_per_good").unwrap_or(2),
        coal_iron_ratio: table.get("coal_iron_ratio").unwrap_or(1),
        food_per_worker: table.get("food_per_worker").unwrap_or(1),
        starvation_cap: table.get("starvation_cap").unwrap_or(2),
        canned_food_ratio: table.get("canned_food_ratio").unwrap_or(2),
        immigration_canned_food: table.get("immigration_canned_food").unwrap_or(1),
        immigration_clothing: table.get("immigration_clothing").unwrap_or(1),
        immigration_furniture: table.get("immigration_furniture").unwrap_or(1),
        provinces_per_immigrant: table.get("provinces_per_immigrant").unwrap_or(4),
        provinces_per_immigrant_upgraded: table
            .get("provinces_per_immigrant_upgraded")
            .unwrap_or(3),
        gold_value: table.get("gold_value").unwrap_or(500),
        gems_value: table.get("gems_value").unwrap_or(1000),
        expansion_delay_turns: table
            .get::<u32>("expansion_delay_turns")
            .unwrap_or(2)
            .min(255) as u8,
        use_tier_expansion: table.get("use_tier_expansion").unwrap_or(true),
        consulate_cost: table.get("consulate_cost").unwrap_or(500),
        embassy_cost: table.get("embassy_cost").unwrap_or(5000),
        ai_embassy_min_relation: table.get("ai_embassy_min_relation").unwrap_or(50),
        voluntary_incorporation_threshold: table
            .get("voluntary_incorporation_threshold")
            .unwrap_or(90),
        trade_relation_improvement_cap: table.get("trade_relation_improvement_cap").unwrap_or(2),
        trade_relation_turn_interval: table.get("trade_relation_turn_interval").unwrap_or(3),
        starting_freight_cars: table.get("starting_freight_cars").unwrap_or(15),
        starting_engineers: table.get("starting_engineers").unwrap_or(1),
        freight_car_cost: table.get("freight_car_cost").unwrap_or(200),
        army_maintenance_cents_per_arm: table.get("army_maintenance_cents_per_arm").unwrap_or(250),
        engineer_cost: table.get("engineer_cost").unwrap_or(500),
        prospector_cost: table.get("prospector_cost").unwrap_or(100),
        miner_cost: table.get("miner_cost").unwrap_or(1500),
        farmer_cost: table.get("farmer_cost").unwrap_or(100),
        rancher_cost: table.get("rancher_cost").unwrap_or(100),
        forester_cost: table.get("forester_cost").unwrap_or(100),
        driller_cost: table.get("driller_cost").unwrap_or(2000),
        depot_cost: table.get("depot_cost").unwrap_or(2000),
        port_cost: table.get("port_cost").unwrap_or(3000),
        railroad_cost_grassland: table.get("railroad_cost_grassland").unwrap_or(100),
        railroad_cost_forest: table.get("railroad_cost_forest").unwrap_or(100),
        railroad_cost_desert: table.get("railroad_cost_desert").unwrap_or(150),
        railroad_cost_tundra: table.get("railroad_cost_tundra").unwrap_or(150),
        railroad_cost_swamp: table.get("railroad_cost_swamp").unwrap_or(300),
        railroad_cost_hills: table.get("railroad_cost_hills").unwrap_or(200),
        railroad_cost_mountain: table.get("railroad_cost_mountain").unwrap_or(500),
        fort_cost_level_1: table.get("fort_cost_level_1").unwrap_or(5000),
        fort_cost_level_2: table.get("fort_cost_level_2").unwrap_or(7500),
        fort_cost_level_3: table.get("fort_cost_level_3").unwrap_or(10000),
        build_turns_railroad: table
            .get::<u32>("build_turns_railroad")
            .unwrap_or(1)
            .min(255) as u8,
        build_turns_depot: table.get::<u32>("build_turns_depot").unwrap_or(2).min(255) as u8,
        build_turns_port: table.get::<u32>("build_turns_port").unwrap_or(3).min(255) as u8,
        railroad_tech_grassland: table.get("railroad_tech_grassland").ok(),
        railroad_tech_forest: table.get("railroad_tech_forest").ok(),
        railroad_tech_desert: table.get("railroad_tech_desert").ok(),
        railroad_tech_tundra: table.get("railroad_tech_tundra").ok(),
        railroad_tech_hills: table.get("railroad_tech_hills").ok(),
        railroad_tech_swamp: table
            .get("railroad_tech_swamp")
            .ok()
            .or_else(|| Some("Iron Railroad Bridge".to_string())),
        railroad_tech_mountain: table
            .get("railroad_tech_mountain")
            .ok()
            .or_else(|| Some("Compound Steam Engine".to_string())),
        infrastructure_horizon_turns: table.get("infrastructure_horizon_turns").unwrap_or(50),
        infra_coverage_weight: table.get("infra_coverage_weight").unwrap_or(1.0),
        infra_path_cost_weight: table.get("infra_path_cost_weight").unwrap_or(1.0),
        infra_improvability_weight: table.get("infra_improvability_weight").unwrap_or(0.5),
        infra_early_game_bias_turns: table.get("infra_early_game_bias_turns").unwrap_or(5),
        infra_early_game_bias: table.get("infra_early_game_bias").unwrap_or(1.5),
        trade_lookback_turns: table.get("trade_lookback_turns").unwrap_or(8),
        trade_discount_weight: table.get("trade_discount_weight").unwrap_or(0.5),
        trade_history_weight: table.get("trade_history_weight").unwrap_or(1.0),
        trade_consulate_potential_weight: table
            .get("trade_consulate_potential_weight")
            .unwrap_or(0.25),
        engineer_hire_max: table.get("engineer_hire_max").unwrap_or(3),
        engineer_hire_base: table.get("engineer_hire_base").unwrap_or(100),
        engineer_hire_path_coeff: table.get("engineer_hire_path_coeff").unwrap_or(30),
        engineer_hire_cap: table.get("engineer_hire_cap").unwrap_or(250),
        civilian_target_tiles_per_worker: table
            .get("civilian_target_tiles_per_worker")
            .unwrap_or(3),
        civilian_coverage_collectable: table.get("civilian_coverage_collectable").unwrap_or(3.0),
        civilian_coverage_rail_adjacent: table
            .get("civilian_coverage_rail_adjacent")
            .unwrap_or(1.5),
        civilian_coverage_unconnected: table.get("civilian_coverage_unconnected").unwrap_or(0.5),
        civilian_coverage_undiscovered: table.get("civilian_coverage_undiscovered").unwrap_or(1.5),
        civilian_hire_bootstrap: table.get("civilian_hire_bootstrap").unwrap_or(15.0),
        civilian_idle_penalty: table.get("civilian_idle_penalty").unwrap_or(8.0),
        civilian_connectivity_planned_weight: table
            .get("civilian_connectivity_planned_weight")
            .unwrap_or(30.0),
        civilian_connectivity_adjacent_weight: table
            .get("civilian_connectivity_adjacent_weight")
            .unwrap_or(60.0),
        civilian_connectivity_unconnected_weight: table
            .get("civilian_connectivity_unconnected_weight")
            .unwrap_or(100.0),
        civilian_connectivity_softening_threshold: table
            .get("civilian_connectivity_softening_threshold")
            .unwrap_or(20_000),
        // `nil` in Lua propagates to `None` (ungated). Vanilla game.lua sets
        // these explicitly per the manual; modders can ungate by setting to nil.
        civilian_rancher_tech: table.get("civilian_rancher_tech").ok(),
        civilian_forester_tech: table.get("civilian_forester_tech").ok(),
        civilian_driller_tech: table.get("civilian_driller_tech").ok(),
        ai_prospector_per_hexes: table.get("ai_prospector_per_hexes").unwrap_or(10),
        starting_prospectors: table.get("starting_prospectors").unwrap_or(1),
        starting_miners: table.get("starting_miners").unwrap_or(1),
        backlog_weight_aggressive_military: table
            .get("backlog_weight_aggressive_military")
            .unwrap_or(50),
        backlog_weight_aggressive_infra: table.get("backlog_weight_aggressive_infra").unwrap_or(25),
        backlog_weight_aggressive_diplomacy: table
            .get("backlog_weight_aggressive_diplomacy")
            .unwrap_or(5),
        backlog_weight_aggressive_hire_engineer: table
            .get("backlog_weight_aggressive_hire_engineer")
            .unwrap_or(20),
        backlog_weight_aggressive_hire_improver: table
            .get("backlog_weight_aggressive_hire_improver")
            .unwrap_or(15),
        backlog_weight_balanced_military: table
            .get("backlog_weight_balanced_military")
            .unwrap_or(30),
        backlog_weight_balanced_infra: table.get("backlog_weight_balanced_infra").unwrap_or(30),
        backlog_weight_balanced_diplomacy: table
            .get("backlog_weight_balanced_diplomacy")
            .unwrap_or(20),
        backlog_weight_balanced_hire_engineer: table
            .get("backlog_weight_balanced_hire_engineer")
            .unwrap_or(25),
        backlog_weight_balanced_hire_improver: table
            .get("backlog_weight_balanced_hire_improver")
            .unwrap_or(20),
        backlog_weight_economic_military: table
            .get("backlog_weight_economic_military")
            .unwrap_or(15),
        backlog_weight_economic_infra: table.get("backlog_weight_economic_infra").unwrap_or(50),
        backlog_weight_economic_diplomacy: table
            .get("backlog_weight_economic_diplomacy")
            .unwrap_or(20),
        backlog_weight_economic_hire_engineer: table
            .get("backlog_weight_economic_hire_engineer")
            .unwrap_or(30),
        backlog_weight_economic_hire_improver: table
            .get("backlog_weight_economic_hire_improver")
            .unwrap_or(25),
        backlog_weight_diplomatic_military: table
            .get("backlog_weight_diplomatic_military")
            .unwrap_or(10),
        backlog_weight_diplomatic_infra: table.get("backlog_weight_diplomatic_infra").unwrap_or(35),
        backlog_weight_diplomatic_diplomacy: table
            .get("backlog_weight_diplomatic_diplomacy")
            .unwrap_or(40),
        backlog_weight_diplomatic_hire_engineer: table
            .get("backlog_weight_diplomatic_hire_engineer")
            .unwrap_or(25),
        backlog_weight_diplomatic_hire_improver: table
            .get("backlog_weight_diplomatic_hire_improver")
            .unwrap_or(20),
        backlog_initial_cap: table.get("backlog_initial_cap").unwrap_or(20),
        priority_minor_target_score: table.get("priority_minor_target_score").unwrap_or(1000.0),
        priority_minor_targets_aggressive: table
            .get("priority_minor_targets_aggressive")
            .unwrap_or(3),
        priority_minor_targets_balanced: table.get("priority_minor_targets_balanced").unwrap_or(4),
        priority_minor_targets_economic: table.get("priority_minor_targets_economic").unwrap_or(4),
        priority_minor_targets_diplomatic: table
            .get("priority_minor_targets_diplomatic")
            .unwrap_or(5),
        lumber_price: table.get("lumber_price").unwrap_or(150),
        steel_price: table.get("steel_price").unwrap_or(200),
        fabric_price: table.get("fabric_price").unwrap_or(150),
        paper_price: table.get("paper_price").unwrap_or(100),
        arms_price: table.get("arms_price").unwrap_or(300),
        canned_food_price: table.get("canned_food_price").unwrap_or(100),
        furniture_price: table.get("furniture_price").unwrap_or(400),
        clothing_price: table.get("clothing_price").unwrap_or(400),
        hardware_price: table.get("hardware_price").unwrap_or(500),
        ai_consulate_target: table.get("ai_consulate_target").unwrap_or(4),
        ai_consulate_priority_score: table.get("ai_consulate_priority_score").unwrap_or(30.0),
        ai_consulate_beyond_target_score: table
            .get("ai_consulate_beyond_target_score")
            .unwrap_or(3.0),
        ai_consulate_beyond_target_decay: table
            .get("ai_consulate_beyond_target_decay")
            .unwrap_or(4.0),
        min_food_tile_percent: table.get("min_food_tile_percent").unwrap_or(20),
        food_cluster_chance: table.get("food_cluster_chance").unwrap_or(40),
        default_garrison_per_province: table.get("default_garrison_per_province").unwrap_or(4),
        minor_default_garrison: table.get("minor_default_garrison").unwrap_or(3),
        max_garrison_per_province: table.get("max_garrison_per_province").unwrap_or(8),
        garrison_regen_interval_turns: table.get("garrison_regen_interval_turns").unwrap_or(2),
        rest_heal_amount: table.get::<u8>("rest_heal_amount").unwrap_or(10),
        spending_naval_base: table.get::<f64>("spending_naval_base").unwrap_or(2.0),
        spending_naval_war_bonus: table.get::<f64>("spending_naval_war_bonus").unwrap_or(10.0),
        spending_naval_gap_coeff: table.get::<f64>("spending_naval_gap_coeff").unwrap_or(1.5),
        // D-4: pact-defense thresholds
        pact_defense_standing_gate: table.get("pact_defense_standing_gate").unwrap_or(30),
        pact_defense_relationship_weight: table
            .get("pact_defense_relationship_weight")
            .unwrap_or(0.4),
        pact_defense_military_weight: table.get("pact_defense_military_weight").unwrap_or(0.4),
        pact_defense_bias_aggressive: table.get("pact_defense_bias_aggressive").unwrap_or(0.2),
        pact_defense_bias_diplomatic: table.get("pact_defense_bias_diplomatic").unwrap_or(0.1),
        pact_defense_bias_balanced: table.get("pact_defense_bias_balanced").unwrap_or(0.0),
        pact_defense_bias_economic: table.get("pact_defense_bias_economic").unwrap_or(-0.15),
        pact_defense_threshold_aggressive: table
            .get("pact_defense_threshold_aggressive")
            .unwrap_or(0.2),
        pact_defense_threshold_diplomatic: table
            .get("pact_defense_threshold_diplomatic")
            .unwrap_or(0.3),
        pact_defense_threshold_balanced: table
            .get("pact_defense_threshold_balanced")
            .unwrap_or(0.35),
        pact_defense_threshold_economic: table
            .get("pact_defense_threshold_economic")
            .unwrap_or(0.5),
        // D-5: combat terrain/fort bonuses and fp-loss ratios
        terrain_defense_mountain: table.get("terrain_defense_mountain").unwrap_or(0.50),
        terrain_defense_hills: table.get("terrain_defense_hills").unwrap_or(0.30),
        terrain_defense_forest: table.get("terrain_defense_forest").unwrap_or(0.20),
        terrain_defense_swamp: table.get("terrain_defense_swamp").unwrap_or(0.15),
        fort_defense_level1: table.get("fort_defense_level1").unwrap_or(0.20),
        fort_defense_level2: table.get("fort_defense_level2").unwrap_or(0.40),
        fort_defense_level3: table.get("fort_defense_level3").unwrap_or(0.60),
        battle_attacker_fp_loss_ratio: table.get("battle_attacker_fp_loss_ratio").unwrap_or(0.60),
        battle_defender_fp_loss_ratio: table.get("battle_defender_fp_loss_ratio").unwrap_or(2.0),
        // D-6: labor/civilian hiring thresholds
        labor_workers_per_province_base: table.get("labor_workers_per_province_base").unwrap_or(2),
        labor_workers_per_province_wealthy: table
            .get("labor_workers_per_province_wealthy")
            .unwrap_or(3),
        labor_wealthy_treasury_threshold: table
            .get("labor_wealthy_treasury_threshold")
            .unwrap_or(20_000),
        labor_min_workers_floor: table.get("labor_min_workers_floor").unwrap_or(5),
        labor_hire_civilian_tier1_treasury: table
            .get("labor_hire_civilian_tier1_treasury")
            .unwrap_or(1000),
        labor_hire_civilian_tier1_max: table.get("labor_hire_civilian_tier1_max").unwrap_or(2),
        labor_hire_civilian_tier2_treasury: table
            .get("labor_hire_civilian_tier2_treasury")
            .unwrap_or(2000),
        labor_hire_civilian_tier2_max: table.get("labor_hire_civilian_tier2_max").unwrap_or(4),
        // D-7: AI consulate treasury threshold
        ai_consulate_treasury_threshold: table
            .get("ai_consulate_treasury_threshold")
            .unwrap_or(2000),
        // Minor nation trade behaviour
        minor_resource_withhold_chance: table.get("minor_resource_withhold_chance").unwrap_or(20),
        minor_goods_buy_price: table.get("minor_goods_buy_price").unwrap_or(150),
    };
    // Sanitize: ensure no zero-or-negative values for fields used as divisors/multipliers
    let sanitized_default_garrison = cfg.default_garrison_per_province.clamp(0, 20);
    let sanitized_minor_default = cfg.minor_default_garrison.clamp(0, 20);
    GameConfig {
        untrained_labor: cfg.untrained_labor.max(1),
        trained_labor: cfg.trained_labor.max(1),
        expert_labor: cfg.expert_labor.max(1),
        labor_per_production: cfg.labor_per_production.max(1),
        resources_per_material: cfg.resources_per_material.max(1),
        materials_per_good: cfg.materials_per_good.max(1),
        coal_iron_ratio: cfg.coal_iron_ratio.max(1),
        food_per_worker: cfg.food_per_worker.max(1),
        starvation_cap: cfg.starvation_cap.max(1),
        canned_food_ratio: cfg.canned_food_ratio.max(1),
        provinces_per_immigrant: cfg.provinces_per_immigrant.max(1),
        provinces_per_immigrant_upgraded: cfg.provinces_per_immigrant_upgraded.max(1),
        gold_value: cfg.gold_value.clamp(0, 1_000_000),
        gems_value: cfg.gems_value.clamp(0, 1_000_000),
        consulate_cost: cfg.consulate_cost.clamp(0, 1_000_000),
        embassy_cost: cfg.embassy_cost.clamp(0, 1_000_000),
        ai_embassy_min_relation: cfg.ai_embassy_min_relation.clamp(-100, 100),
        voluntary_incorporation_threshold: cfg.voluntary_incorporation_threshold.clamp(-100, 100),
        trade_relation_improvement_cap: cfg.trade_relation_improvement_cap.max(0),
        trade_relation_turn_interval: cfg.trade_relation_turn_interval.max(1),
        starting_freight_cars: cfg.starting_freight_cars,
        starting_engineers: cfg.starting_engineers,
        freight_car_cost: cfg.freight_car_cost.clamp(0, 1_000_000),
        army_maintenance_cents_per_arm: cfg.army_maintenance_cents_per_arm.clamp(0, 1_000_000),
        engineer_cost: cfg.engineer_cost.clamp(0, 1_000_000),
        prospector_cost: cfg.prospector_cost.clamp(0, 1_000_000),
        miner_cost: cfg.miner_cost.clamp(0, 1_000_000),
        farmer_cost: cfg.farmer_cost.clamp(0, 1_000_000),
        rancher_cost: cfg.rancher_cost.clamp(0, 1_000_000),
        forester_cost: cfg.forester_cost.clamp(0, 1_000_000),
        driller_cost: cfg.driller_cost.clamp(0, 1_000_000),
        depot_cost: cfg.depot_cost.clamp(0, 1_000_000),
        port_cost: cfg.port_cost.clamp(0, 1_000_000),
        railroad_cost_grassland: cfg.railroad_cost_grassland.clamp(0, 1_000_000),
        railroad_cost_forest: cfg.railroad_cost_forest.clamp(0, 1_000_000),
        railroad_cost_desert: cfg.railroad_cost_desert.clamp(0, 1_000_000),
        railroad_cost_tundra: cfg.railroad_cost_tundra.clamp(0, 1_000_000),
        railroad_cost_swamp: cfg.railroad_cost_swamp.clamp(0, 1_000_000),
        railroad_cost_hills: cfg.railroad_cost_hills.clamp(0, 1_000_000),
        railroad_cost_mountain: cfg.railroad_cost_mountain.clamp(0, 1_000_000),
        fort_cost_level_1: cfg.fort_cost_level_1.clamp(0, 1_000_000),
        fort_cost_level_2: cfg.fort_cost_level_2.clamp(0, 1_000_000),
        fort_cost_level_3: cfg.fort_cost_level_3.clamp(0, 1_000_000),
        build_turns_railroad: cfg.build_turns_railroad.max(1),
        build_turns_depot: cfg.build_turns_depot.max(1),
        build_turns_port: cfg.build_turns_port.max(1),
        lumber_price: cfg.lumber_price.clamp(1, 1_000_000),
        steel_price: cfg.steel_price.clamp(1, 1_000_000),
        fabric_price: cfg.fabric_price.clamp(1, 1_000_000),
        paper_price: cfg.paper_price.clamp(1, 1_000_000),
        arms_price: cfg.arms_price.clamp(1, 1_000_000),
        canned_food_price: cfg.canned_food_price.clamp(1, 1_000_000),
        furniture_price: cfg.furniture_price.clamp(1, 1_000_000),
        clothing_price: cfg.clothing_price.clamp(1, 1_000_000),
        hardware_price: cfg.hardware_price.clamp(1, 1_000_000),
        ai_consulate_target: cfg.ai_consulate_target.clamp(0, 20),
        ai_consulate_priority_score: if cfg.ai_consulate_priority_score.is_finite() {
            cfg.ai_consulate_priority_score.clamp(0.0, 1000.0)
        } else {
            30.0
        },
        ai_consulate_beyond_target_score: if cfg.ai_consulate_beyond_target_score.is_finite() {
            cfg.ai_consulate_beyond_target_score.clamp(0.0, 100.0)
        } else {
            3.0
        },
        ai_consulate_beyond_target_decay: if cfg.ai_consulate_beyond_target_decay.is_finite() {
            cfg.ai_consulate_beyond_target_decay.clamp(0.0, 100.0)
        } else {
            4.0
        },
        min_food_tile_percent: cfg.min_food_tile_percent.clamp(0, 100),
        food_cluster_chance: cfg.food_cluster_chance.clamp(0, 100),
        // Garrison tunables: sanitize defaults first, then bound the cap
        // using those sanitized values. Using the raw pre-clamp values as
        // the lower bound can panic (min > max) when Lua sets defaults
        // above 20.
        default_garrison_per_province: sanitized_default_garrison,
        minor_default_garrison: sanitized_minor_default,
        max_garrison_per_province: cfg
            .max_garrison_per_province
            .clamp(sanitized_default_garrison.max(sanitized_minor_default), 20),
        // `0` keeps its "disabled" semantic — the regen phase early-returns
        // on zero and also guards against modulo-by-zero.
        garrison_regen_interval_turns: cfg.garrison_regen_interval_turns.min(200),
        rest_heal_amount: cfg.rest_heal_amount.clamp(1, 100),
        spending_naval_base: if cfg.spending_naval_base.is_finite() {
            cfg.spending_naval_base.clamp(0.0, 100.0)
        } else {
            2.0
        },
        spending_naval_war_bonus: if cfg.spending_naval_war_bonus.is_finite() {
            cfg.spending_naval_war_bonus.clamp(0.0, 100.0)
        } else {
            10.0
        },
        spending_naval_gap_coeff: if cfg.spending_naval_gap_coeff.is_finite() {
            cfg.spending_naval_gap_coeff.clamp(0.0, 10.0)
        } else {
            1.5
        },
        // D-4: pact-defense thresholds
        pact_defense_standing_gate: cfg.pact_defense_standing_gate.clamp(-100, 100),
        pact_defense_relationship_weight: if cfg.pact_defense_relationship_weight.is_finite() {
            cfg.pact_defense_relationship_weight.clamp(0.0, 1.0)
        } else {
            0.4
        },
        pact_defense_military_weight: if cfg.pact_defense_military_weight.is_finite() {
            cfg.pact_defense_military_weight.clamp(0.0, 1.0)
        } else {
            0.4
        },
        pact_defense_bias_aggressive: if cfg.pact_defense_bias_aggressive.is_finite() {
            cfg.pact_defense_bias_aggressive.clamp(-1.0, 1.0)
        } else {
            0.2
        },
        pact_defense_bias_diplomatic: if cfg.pact_defense_bias_diplomatic.is_finite() {
            cfg.pact_defense_bias_diplomatic.clamp(-1.0, 1.0)
        } else {
            0.1
        },
        pact_defense_bias_balanced: if cfg.pact_defense_bias_balanced.is_finite() {
            cfg.pact_defense_bias_balanced.clamp(-1.0, 1.0)
        } else {
            0.0
        },
        pact_defense_bias_economic: if cfg.pact_defense_bias_economic.is_finite() {
            cfg.pact_defense_bias_economic.clamp(-1.0, 1.0)
        } else {
            -0.15
        },
        pact_defense_threshold_aggressive: if cfg.pact_defense_threshold_aggressive.is_finite() {
            cfg.pact_defense_threshold_aggressive.clamp(0.0, 1.0)
        } else {
            0.2
        },
        pact_defense_threshold_diplomatic: if cfg.pact_defense_threshold_diplomatic.is_finite() {
            cfg.pact_defense_threshold_diplomatic.clamp(0.0, 1.0)
        } else {
            0.3
        },
        pact_defense_threshold_balanced: if cfg.pact_defense_threshold_balanced.is_finite() {
            cfg.pact_defense_threshold_balanced.clamp(0.0, 1.0)
        } else {
            0.35
        },
        pact_defense_threshold_economic: if cfg.pact_defense_threshold_economic.is_finite() {
            cfg.pact_defense_threshold_economic.clamp(0.0, 1.0)
        } else {
            0.5
        },
        // D-5: terrain/fort bonus and fp-loss sanitization
        terrain_defense_mountain: if cfg.terrain_defense_mountain.is_finite() {
            cfg.terrain_defense_mountain.clamp(0.0, 2.0)
        } else {
            0.50
        },
        terrain_defense_hills: if cfg.terrain_defense_hills.is_finite() {
            cfg.terrain_defense_hills.clamp(0.0, 2.0)
        } else {
            0.30
        },
        terrain_defense_forest: if cfg.terrain_defense_forest.is_finite() {
            cfg.terrain_defense_forest.clamp(0.0, 2.0)
        } else {
            0.20
        },
        terrain_defense_swamp: if cfg.terrain_defense_swamp.is_finite() {
            cfg.terrain_defense_swamp.clamp(0.0, 2.0)
        } else {
            0.15
        },
        fort_defense_level1: if cfg.fort_defense_level1.is_finite() {
            cfg.fort_defense_level1.clamp(0.0, 2.0)
        } else {
            0.20
        },
        fort_defense_level2: if cfg.fort_defense_level2.is_finite() {
            cfg.fort_defense_level2.clamp(0.0, 2.0)
        } else {
            0.40
        },
        fort_defense_level3: if cfg.fort_defense_level3.is_finite() {
            cfg.fort_defense_level3.clamp(0.0, 2.0)
        } else {
            0.60
        },
        battle_attacker_fp_loss_ratio: if cfg.battle_attacker_fp_loss_ratio.is_finite() {
            cfg.battle_attacker_fp_loss_ratio.clamp(0.0, 10.0)
        } else {
            0.60
        },
        battle_defender_fp_loss_ratio: if cfg.battle_defender_fp_loss_ratio.is_finite() {
            cfg.battle_defender_fp_loss_ratio.clamp(0.0, 10.0)
        } else {
            2.0
        },
        // D-6: labor/civilian thresholds
        labor_workers_per_province_base: cfg.labor_workers_per_province_base.clamp(1, 10),
        labor_workers_per_province_wealthy: cfg.labor_workers_per_province_wealthy.clamp(1, 10),
        labor_wealthy_treasury_threshold: cfg.labor_wealthy_treasury_threshold.clamp(0, 1_000_000),
        labor_min_workers_floor: cfg.labor_min_workers_floor.clamp(1, 50),
        labor_hire_civilian_tier1_treasury: cfg
            .labor_hire_civilian_tier1_treasury
            .clamp(0, 1_000_000),
        labor_hire_civilian_tier1_max: cfg.labor_hire_civilian_tier1_max.clamp(0, 20),
        labor_hire_civilian_tier2_treasury: cfg
            .labor_hire_civilian_tier2_treasury
            .clamp(0, 1_000_000),
        labor_hire_civilian_tier2_max: cfg.labor_hire_civilian_tier2_max.clamp(0, 20),
        // D-7: AI consulate treasury threshold
        ai_consulate_treasury_threshold: cfg.ai_consulate_treasury_threshold.clamp(0, 1_000_000),
        // Minor nation trade tunables
        minor_resource_withhold_chance: cfg.minor_resource_withhold_chance.min(100),
        minor_goods_buy_price: cfg.minor_goods_buy_price.clamp(1, 1_000_000),
        ..cfg
    }
}

/// Load game config and all personality scripts into the Lua VM.
pub fn load_scripts(engine: &LuaEngine) -> Result<(), String> {
    engine.exec(GAME_CONFIG_LUA)?;
    engine.exec(TECH_TREE_LUA)?;
    engine.exec(UNITS_LUA)?;
    engine.exec(BALANCED_LUA)?;
    engine.exec(AGGRESSIVE_LUA)?;
    engine.exec(DIPLOMATIC_LUA)?;
    engine.exec(ECONOMIC_LUA)?;
    Ok(())
}

/// Load land-unit stats from the Lua `units` global table populated by
/// `scripts/config/units.lua`.
///
/// Returns `None` if the table is absent or contains no rows; callers fall
/// back to `default_unit_stats()` (which mirrors the same numbers in Rust).
/// Per-row parse failures are logged and skipped — a typo on one unit must
/// not silently disable the rest.
pub fn load_unit_stats(
    engine: &LuaEngine,
) -> Option<
    std::collections::HashMap<
        crate::military::units::ArmyUnitType,
        crate::military::units::UnitStats,
    >,
> {
    use crate::military::units::{ArmyUnitType, Era, UnitCategory, UnitStats};
    use std::collections::HashMap;

    let lua = engine.lua();
    let table: mlua::Table = lua.globals().get("units").ok()?;

    let mut map: HashMap<ArmyUnitType, UnitStats> = HashMap::new();
    for row_res in table.sequence_values::<mlua::Table>() {
        let row = match row_res {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[units.lua] skipping malformed row: {}", e);
                continue;
            }
        };
        let name: String = match row.get("name") {
            Ok(s) => s,
            Err(_) => continue,
        };
        let unit_type: ArmyUnitType = match name.parse() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[units.lua] unknown unit '{}': {}", name, e);
                continue;
            }
        };
        let category_str: String = match row.get("category") {
            Ok(s) => s,
            Err(_) => continue,
        };
        let category = match category_str.as_str() {
            "Garrison" => UnitCategory::Garrison,
            "Infantry" => UnitCategory::Infantry,
            "Cavalry" => UnitCategory::Cavalry,
            "Artillery" => UnitCategory::Artillery,
            "Special" => UnitCategory::Special,
            other => {
                eprintln!("[units.lua] '{}' has unknown category '{}'", name, other);
                continue;
            }
        };
        let era_n: u8 = row.get::<u8>("era").unwrap_or(1);
        let era = match era_n {
            1 => Era::One,
            2 => Era::Two,
            3 => Era::Three,
            other => {
                eprintln!("[units.lua] '{}' has invalid era {}", name, other);
                continue;
            }
        };
        let cost_dollars: i64 = row.get("cost").unwrap_or(0);
        let maint_dollars: i64 = row.get("maintenance_per_turn").unwrap_or(0);
        let stats = UnitStats {
            firepower: row.get("firepower").unwrap_or(0),
            firepower_mounted: row.get("firepower_mounted").unwrap_or(0),
            defense: row.get("defense").unwrap_or(0),
            defense_terrain_bonus: row.get("defense_terrain_bonus").unwrap_or(0),
            range: row.get("range").unwrap_or(0),
            movement: row.get("movement").unwrap_or(0),
            arms_required: row.get("arms_required").unwrap_or(0),
            requires_horse: row.get("requires_horse").unwrap_or(false),
            category,
            cost: Money::dollars(cost_dollars),
            maintenance_per_turn: Money::dollars(maint_dollars),
            prerequisite_tech: row.get::<String>("prerequisite_tech").ok(),
            era,
        };
        map.insert(unit_type, stats);
    }
    if map.is_empty() { None } else { Some(map) }
}

/// Load the tech tree from the Lua `tech_tree` global table.
///
/// Returns `None` if the table is missing or malformed; callers fall back
/// to the hardcoded Rust default. We log per-row parse failures so a typo
/// in tech_tree.lua doesn't silently disable everything else.
pub fn load_tech_tree(engine: &LuaEngine) -> Option<crate::tech::tree::TechTree> {
    use crate::economy::buildings::BuildingType;
    use crate::economy::civilians::CivilianType;
    use crate::military::units::ArmyUnitType;
    use crate::tech::tree::{TechEffect, Technology};

    let lua = engine.lua();
    let table: mlua::Table = lua.globals().get("tech_tree").ok()?;

    let mut techs: Vec<Technology> = Vec::new();
    for pair in table.sequence_values::<mlua::Table>() {
        let row = match pair {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[tech_tree.lua] skipping malformed row: {}", e);
                continue;
            }
        };
        let id: u32 = match row.get("id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let name: String = match row.get("name") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let cost: i64 = row.get("cost").unwrap_or(0);
        let earliest_year: u32 = row.get("earliest_year").unwrap_or(1815);
        let latest_year: u32 = row.get("latest_year").unwrap_or(1900);
        let prerequisites: Vec<u32> = row
            .get::<mlua::Table>("prerequisites")
            .map(|t| t.sequence_values::<u32>().filter_map(|v| v.ok()).collect())
            .unwrap_or_default();

        let mut effects: Vec<TechEffect> = Vec::new();
        if let Ok(effects_t) = row.get::<mlua::Table>("effects") {
            for eff_pair in effects_t.sequence_values::<mlua::Table>() {
                let eff = match eff_pair {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let kind: String = match eff.get("kind") {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let parsed = match kind.as_str() {
                    "EnableInfrastructure" => eff
                        .get::<String>("value")
                        .ok()
                        .map(TechEffect::EnableInfrastructure),
                    "EnableTerrainImprovement" => {
                        let terrain: Option<String> = eff.get("terrain").ok();
                        let max_level: Option<u8> = eff.get("max_level").ok();
                        match (terrain, max_level) {
                            (Some(terrain), Some(max_level)) => {
                                Some(TechEffect::EnableTerrainImprovement { terrain, max_level })
                            }
                            _ => None,
                        }
                    }
                    "UnlockShip" => eff.get::<String>("value").ok().map(TechEffect::UnlockShip),
                    "UnlockBuilding" => eff
                        .get::<String>("value")
                        .ok()
                        .and_then(|s| s.parse::<BuildingType>().ok())
                        .map(TechEffect::UnlockBuilding),
                    "UnlockUnit" => eff
                        .get::<String>("value")
                        .ok()
                        .and_then(|s| s.parse::<ArmyUnitType>().ok())
                        .map(TechEffect::UnlockUnit),
                    "UpgradeUnit" => {
                        let from: Option<String> = eff.get("from").ok();
                        let to: Option<String> = eff.get("to").ok();
                        match (
                            from.and_then(|s| s.parse().ok()),
                            to.and_then(|s| s.parse().ok()),
                        ) {
                            (Some(from), Some(to)) => Some(TechEffect::UpgradeUnit { from, to }),
                            _ => None,
                        }
                    }
                    "EnableCivilian" => eff
                        .get::<String>("value")
                        .ok()
                        .and_then(|s| s.parse::<CivilianType>().ok())
                        .map(TechEffect::EnableCivilian),
                    "LuaScript" => eff.get::<String>("value").ok().map(TechEffect::LuaScript),
                    other => {
                        eprintln!("[tech_tree.lua] unknown effect kind: {}", other);
                        None
                    }
                };
                if let Some(p) = parsed {
                    effects.push(p);
                }
            }
        }

        techs.push(Technology {
            id: TechId(id),
            name,
            cost: Money::dollars(cost),
            earliest_year,
            latest_year,
            prerequisites: prerequisites.into_iter().map(TechId).collect(),
            effects,
        });
    }

    if techs.is_empty() {
        return None;
    }
    Some(crate::tech::tree::TechTree::from_technologies(techs))
}

/// Load all four personality scripts into the Lua VM (used by tests).
#[cfg(test)]
pub fn load_personality_scripts(engine: &LuaEngine) -> Result<(), String> {
    load_scripts(engine)
}

/// Config parameters read from Lua personality tables.
/// Core fields always present (with defaults); extended fields are optional
/// and fall back to hardcoded personality defaults when absent.
#[derive(Debug, Clone)]
pub struct LuaAiConfig {
    // Core (always populated with Lua defaults — used by research/labor subsystems and tests)
    #[allow(dead_code)]
    pub trade_priority: f64,
    #[allow(dead_code)]
    pub alliance_preference: f64,
    #[allow(dead_code)]
    pub min_army_size: u32,
    #[allow(dead_code)]
    pub max_army_size: u32,
    #[allow(dead_code)]
    pub infrastructure_budget: i64,
    #[allow(dead_code)]
    pub research_strategy: String,
    #[allow(dead_code)]
    pub worker_threshold: u32,

    // War (replaces interval-based system)
    pub war_cooldown: Option<u32>,
    pub war_threshold: Option<f64>,
    pub army_min_for_war: Option<usize>,
    pub opportunism_weight: Option<f64>,
    pub min_artillery_for_minor_war: Option<usize>,
    // Decaying opportunity gate + resource-bonus knobs (card #97)
    pub min_opportunity_start: Option<f64>,
    pub min_opportunity_end: Option<f64>,
    pub min_opportunity_decay_turns: Option<u32>,
    pub resource_bonus_per_missing: Option<f64>,
    pub resource_bonus_cap: Option<f64>,

    // Army building tiers
    pub tier1_army_max: Option<usize>,
    pub tier2_army_max: Option<usize>,
    pub tier3_army_max: Option<usize>,
    pub tier1_treasury: Option<i64>,
    pub tier2_treasury: Option<i64>,
    pub tier3_treasury: Option<i64>,
    pub tier4_treasury: Option<i64>,

    // Diplomacy
    pub consulate_max_per_turn: Option<u32>,
    pub propose_pacts: Option<bool>,
    pub propose_alliances: Option<bool>,
    pub grant_amount: Option<i64>,
    pub grant_interval: Option<u32>,
    pub embassy_treasury_threshold: Option<i64>,
    pub max_alliances: Option<usize>,

    // Naval. Card #112 removed the warship caps: naval growth now flows
    // through the scored-spending rotation and is gated only by treasury
    // + materials.
    pub max_merchant_ships: Option<usize>,
    /// Minimum army size before attempting a naval invasion.
    pub min_army_naval_invasion: Option<usize>,

    // Economy
    pub expansion_threshold_multiplier: Option<u32>,
    pub use_tier_expansion: Option<bool>,
    pub high_treasury_expansion_threshold: Option<i64>,
    pub trade_resource_reserve: Option<u32>,
    pub trade_treasury_cap: Option<i64>,
    pub goods_sell_treasury_threshold: Option<i64>,
    pub goods_reserve: Option<u32>,
    pub food_processing_expansion_threshold: Option<u32>,
    pub infra_budget_scale_threshold: Option<i64>,

    // Spending weights (need-based scoring system)
    pub spending_military_weight: Option<f64>,
    pub spending_economy_weight: Option<f64>,
    pub spending_diplomacy_weight: Option<f64>,
    pub treasury_reserve: Option<i64>,
    pub min_score_threshold: Option<f64>,

    // Worker training thresholds
    pub worker_train_threshold: Option<u32>,
    pub worker_promote_threshold: Option<u32>,

    // Tactical
    pub peace_war_duration_threshold: Option<u32>,
    pub peace_province_loss_ratio: Option<f64>,
    pub fort_strategy: Option<String>,

    // Coalition assessment weights
    pub coalition_mil_weight: Option<f64>,
    pub coalition_prov_weight: Option<f64>,
    pub coalition_econ_weight: Option<f64>,
    pub coalition_momentum_weight: Option<f64>,
    pub coalition_naval_weight: Option<f64>,
    pub coalition_sigmoid_steepness: Option<f64>,

    // Economic-score multipliers (used by `nation_economic_score`).
    // Treasury contribution is `treasury_dollars / econ_score_treasury_divisor`;
    // each building adds `econ_score_buildings_multiplier`; each worker
    // adds `econ_score_workers_multiplier`.
    pub econ_score_treasury_divisor: Option<f64>,
    pub econ_score_buildings_multiplier: Option<f64>,
    pub econ_score_workers_multiplier: Option<f64>,

    // Peace proposal thresholds
    pub peace_accept_threshold: Option<f64>,
    pub peace_reject_threshold: Option<f64>,
    pub peace_stalemate_duration: Option<u32>,

    // War worthiness thresholds
    pub won_enough_captures: Option<usize>,
    pub won_enough_marginal: Option<f64>,
    pub lost_enough_losses: Option<usize>,
    pub lost_enough_likelihood: Option<f64>,

    // Treaty evaluation thresholds
    pub nap_accept_threshold: Option<f64>,
    pub alliance_accept_threshold: Option<f64>,
    pub alliance_rival_penalty: Option<f64>,
    pub alliance_overcommit_penalty: Option<f64>,
    pub treaty_personality_bias: Option<f64>,

    // Field-army distribution (cards #5, #9)
    pub capital_reserve_normal: Option<usize>,
    pub capital_reserve_threatened: Option<usize>,
    pub max_redeploys_per_turn: Option<usize>,

    // Retreat (card #18)
    pub retreat_prebattle_ratio: Option<f64>,
    pub retreat_postbattle_fp_loss: Option<f64>,

    // Naval landing gate (card #7)
    pub naval_min_adjacent_strength_ratio: Option<f64>,

    // Attack acceptance (card #99, phase 2): minimum ratio of our forward
    // firepower to the defender's local firepower required to attack. A
    // value of `1.0` means "only attack if our forward FP matches the
    // defender's raw local FP". Lower values permit more aggressive
    // engagements. Applied separately to minor and great-power targets.
    pub attack_fp_vs_minor: Option<f64>,
    pub attack_fp_vs_gp: Option<f64>,

    // Trello card #20: minimum unit health (0–100) required for the AI to
    // count a unit toward its forward attack firepower. Units below this
    // threshold sit out and heal via the end-of-turn rest-heal pass.
    pub rest_health_threshold: Option<u8>,

    // Trello card #8: extra score penalty added when the AI considers
    // attacking an enemy's capital province while that enemy still owns
    // reachable non-capital provinces. Higher = more strongly save the
    // capital for last (avoid flipping a minor into anarchy too early).
    pub capital_save_for_last_penalty: Option<i32>,

    // Trello card #112: optional per-personality override of the naval
    // spending weight in `ai_scored_spending`. If `None`, naval uses
    // `spending_military_weight`.
    pub spending_naval_weight: Option<f64>,
}

/// Clamp an f64 to a finite range, replacing NaN/inf with the default.
fn sanitize_f64(val: f64, min: f64, max: f64, default: f64) -> f64 {
    if val.is_finite() {
        val.clamp(min, max)
    } else {
        default
    }
}

/// Sanitize an optional f64, returning None for NaN/inf, clamped otherwise.
fn sanitize_opt_f64(val: Option<f64>, min: f64, max: f64) -> Option<f64> {
    val.and_then(|v| {
        if v.is_finite() {
            Some(v.clamp(min, max))
        } else {
            None
        }
    })
}

/// Sanitize an optional i64, clamping to [min, max].
fn sanitize_opt_i64(val: Option<i64>, min: i64, max: i64) -> Option<i64> {
    val.map(|v| v.clamp(min, max))
}

/// Sanitize an optional u32, clamping to [min, max].
fn sanitize_opt_u32(val: Option<u32>, min: u32, max: u32) -> Option<u32> {
    val.map(|v| v.clamp(min, max))
}

/// Sanitize an optional usize, clamping to [min, max].
fn sanitize_opt_usize(val: Option<usize>, min: usize, max: usize) -> Option<usize> {
    val.map(|v| v.clamp(min, max))
}

/// Sanitize an optional string, returning None if not in the allowed set.
fn sanitize_opt_string(val: Option<String>, allowed: &[&str]) -> Option<String> {
    val.filter(|v| allowed.iter().any(|a| a == v))
}

impl LuaAiConfig {
    /// Validate and clamp all fields to sane ranges, replacing NaN/inf with None.
    fn sanitize(mut self) -> Self {
        // Core f64 fields
        self.trade_priority = sanitize_f64(self.trade_priority, 0.0, 1.0, 0.5);
        self.alliance_preference = sanitize_f64(self.alliance_preference, 0.0, 1.0, 0.5);

        // Core u32 fields
        self.min_army_size = self.min_army_size.clamp(0, 100);
        self.max_army_size = self.max_army_size.clamp(1, 100);
        self.worker_threshold = self.worker_threshold.clamp(0, 100);

        // Core i64 field
        self.infrastructure_budget = self.infrastructure_budget.clamp(0, 10_000_000);

        // Core string field
        if !["cheapest", "expensive", "military", "economic", "balanced"]
            .contains(&self.research_strategy.as_str())
        {
            self.research_strategy = "cheapest".to_string();
        }

        // War
        self.war_cooldown = sanitize_opt_u32(self.war_cooldown, 0, 100);
        self.war_threshold = sanitize_opt_f64(self.war_threshold, 0.0, 1.0);
        self.army_min_for_war = sanitize_opt_usize(self.army_min_for_war, 0, 100);
        self.opportunism_weight = sanitize_opt_f64(self.opportunism_weight, 0.0, 10.0);
        self.min_artillery_for_minor_war =
            sanitize_opt_usize(self.min_artillery_for_minor_war, 0, 20);
        // Opportunity gate + resource-bonus (card #97)
        self.min_opportunity_start = sanitize_opt_f64(self.min_opportunity_start, 0.0, 1.0);
        self.min_opportunity_end = sanitize_opt_f64(self.min_opportunity_end, 0.0, 1.0);
        // Enforce monotonic decay: end must not exceed start, otherwise the
        // "decay" would become an "increase" as the game progresses.
        if let (Some(start), Some(end)) = (self.min_opportunity_start, self.min_opportunity_end)
            && end > start
        {
            self.min_opportunity_end = Some(start);
        }
        self.min_opportunity_decay_turns =
            sanitize_opt_u32(self.min_opportunity_decay_turns, 1, 100);
        self.resource_bonus_per_missing =
            sanitize_opt_f64(self.resource_bonus_per_missing, 0.0, 1.0);
        self.resource_bonus_cap = sanitize_opt_f64(self.resource_bonus_cap, 0.0, 1.0);

        // Army tiers (usize)
        self.tier1_army_max = sanitize_opt_usize(self.tier1_army_max, 0, 100);
        self.tier2_army_max = sanitize_opt_usize(self.tier2_army_max, 0, 100);
        self.tier3_army_max = sanitize_opt_usize(self.tier3_army_max, 0, 100);

        // Diplomacy
        self.consulate_max_per_turn = sanitize_opt_u32(self.consulate_max_per_turn, 0, 20);
        self.grant_amount = sanitize_opt_i64(self.grant_amount, 0, 100_000);
        self.grant_interval = sanitize_opt_u32(self.grant_interval, 1, 100);
        self.embassy_treasury_threshold =
            sanitize_opt_i64(self.embassy_treasury_threshold, 0, 1_000_000);
        self.max_alliances = sanitize_opt_usize(self.max_alliances, 0, 10);

        // Naval
        self.max_merchant_ships = sanitize_opt_usize(self.max_merchant_ships, 0, 50);
        self.min_army_naval_invasion = sanitize_opt_usize(self.min_army_naval_invasion, 1, 20);

        // Economy
        self.expansion_threshold_multiplier =
            sanitize_opt_u32(self.expansion_threshold_multiplier, 1, 20);
        self.trade_resource_reserve = sanitize_opt_u32(self.trade_resource_reserve, 0, 100);
        self.goods_reserve = sanitize_opt_u32(self.goods_reserve, 0, 100);
        self.food_processing_expansion_threshold =
            sanitize_opt_u32(self.food_processing_expansion_threshold, 0, 1000);

        // Treasury thresholds
        self.tier1_treasury = sanitize_opt_i64(self.tier1_treasury, 0, 10_000_000);
        self.tier2_treasury = sanitize_opt_i64(self.tier2_treasury, 0, 10_000_000);
        self.tier3_treasury = sanitize_opt_i64(self.tier3_treasury, 0, 10_000_000);
        self.tier4_treasury = sanitize_opt_i64(self.tier4_treasury, 0, 10_000_000);
        self.treasury_reserve = sanitize_opt_i64(self.treasury_reserve, 0, 10_000_000);
        self.high_treasury_expansion_threshold =
            sanitize_opt_i64(self.high_treasury_expansion_threshold, 0, 10_000_000);
        self.trade_treasury_cap = sanitize_opt_i64(self.trade_treasury_cap, 0, 10_000_000);
        self.goods_sell_treasury_threshold =
            sanitize_opt_i64(self.goods_sell_treasury_threshold, 0, 10_000_000);
        self.infra_budget_scale_threshold =
            sanitize_opt_i64(self.infra_budget_scale_threshold, 0, 10_000_000);

        // Spending weights (f64)
        self.spending_military_weight = sanitize_opt_f64(self.spending_military_weight, 0.0, 10.0);
        self.spending_economy_weight = sanitize_opt_f64(self.spending_economy_weight, 0.0, 10.0);
        self.spending_diplomacy_weight =
            sanitize_opt_f64(self.spending_diplomacy_weight, 0.0, 10.0);
        self.min_score_threshold = sanitize_opt_f64(self.min_score_threshold, 0.0, 100.0);

        // Coalition assessment weights
        self.coalition_mil_weight = sanitize_opt_f64(self.coalition_mil_weight, 0.0, 10.0);
        self.coalition_prov_weight = sanitize_opt_f64(self.coalition_prov_weight, 0.0, 10.0);
        self.coalition_econ_weight = sanitize_opt_f64(self.coalition_econ_weight, 0.0, 10.0);
        self.coalition_momentum_weight =
            sanitize_opt_f64(self.coalition_momentum_weight, 0.0, 10.0);
        self.coalition_naval_weight = sanitize_opt_f64(self.coalition_naval_weight, 0.0, 10.0);
        self.coalition_sigmoid_steepness =
            sanitize_opt_f64(self.coalition_sigmoid_steepness, 0.1, 20.0);

        // Economic-score multipliers — divisor must stay > 0 to avoid div-by-zero.
        self.econ_score_treasury_divisor =
            sanitize_opt_f64(self.econ_score_treasury_divisor, 1.0, 1_000_000.0);
        self.econ_score_buildings_multiplier =
            sanitize_opt_f64(self.econ_score_buildings_multiplier, 0.0, 100.0);
        self.econ_score_workers_multiplier =
            sanitize_opt_f64(self.econ_score_workers_multiplier, 0.0, 100.0);

        // Tactical
        self.peace_war_duration_threshold =
            sanitize_opt_u32(self.peace_war_duration_threshold, 0, 200);
        self.fort_strategy = sanitize_opt_string(
            self.fort_strategy,
            &["capital", "border", "offensive", "none"],
        );

        // Worker training
        self.worker_train_threshold = sanitize_opt_u32(self.worker_train_threshold, 0, 100);
        self.worker_promote_threshold = sanitize_opt_u32(self.worker_promote_threshold, 0, 100);

        // Peace/war thresholds (f64 in [0, 1])
        self.peace_accept_threshold = sanitize_opt_f64(self.peace_accept_threshold, 0.0, 1.0);
        self.peace_reject_threshold = sanitize_opt_f64(self.peace_reject_threshold, 0.0, 1.0);
        self.peace_province_loss_ratio = sanitize_opt_f64(self.peace_province_loss_ratio, 0.0, 1.0);
        self.peace_stalemate_duration = sanitize_opt_u32(self.peace_stalemate_duration, 0, 200);
        self.won_enough_captures = sanitize_opt_usize(self.won_enough_captures, 0, 50);
        self.won_enough_marginal = sanitize_opt_f64(self.won_enough_marginal, 0.0, 10.0);
        self.lost_enough_losses = sanitize_opt_usize(self.lost_enough_losses, 0, 50);
        self.lost_enough_likelihood = sanitize_opt_f64(self.lost_enough_likelihood, 0.0, 1.0);

        // Cross-field invariants
        if self.min_army_size > self.max_army_size {
            self.min_army_size = self.max_army_size;
        }

        // Treaty thresholds
        self.nap_accept_threshold = sanitize_opt_f64(self.nap_accept_threshold, 0.0, 1.0);
        self.alliance_accept_threshold = sanitize_opt_f64(self.alliance_accept_threshold, 0.0, 1.0);
        self.alliance_rival_penalty = sanitize_opt_f64(self.alliance_rival_penalty, 0.0, 10.0);
        self.alliance_overcommit_penalty =
            sanitize_opt_f64(self.alliance_overcommit_penalty, 0.0, 10.0);
        self.treaty_personality_bias = sanitize_opt_f64(self.treaty_personality_bias, -5.0, 5.0);

        // Field-army distribution
        self.capital_reserve_normal = sanitize_opt_usize(self.capital_reserve_normal, 0, 50);
        self.capital_reserve_threatened =
            sanitize_opt_usize(self.capital_reserve_threatened, 0, 50);
        self.max_redeploys_per_turn = sanitize_opt_usize(self.max_redeploys_per_turn, 0, 50);

        // Retreat
        self.retreat_prebattle_ratio = sanitize_opt_f64(self.retreat_prebattle_ratio, 1.0, 10.0);
        self.retreat_postbattle_fp_loss =
            sanitize_opt_f64(self.retreat_postbattle_fp_loss, 0.0, 1.0);

        // Naval gate
        self.naval_min_adjacent_strength_ratio =
            sanitize_opt_f64(self.naval_min_adjacent_strength_ratio, 0.5, 10.0);

        // Attack acceptance (card #99 phase 2)
        self.attack_fp_vs_minor = sanitize_opt_f64(self.attack_fp_vs_minor, 0.1, 5.0);
        self.attack_fp_vs_gp = sanitize_opt_f64(self.attack_fp_vs_gp, 0.1, 5.0);

        // Trello card #20: rest-heal threshold
        self.rest_health_threshold = self.rest_health_threshold.map(|v| v.min(100));

        // Trello card #8: capital-save-for-last penalty
        self.capital_save_for_last_penalty = self
            .capital_save_for_last_penalty
            .map(|v| v.clamp(0, 1_000));

        // Trello card #112: naval spending weight override
        self.spending_naval_weight = sanitize_opt_f64(self.spending_naval_weight, 0.0, 10.0);

        self
    }
}

fn personality_table_name(personality: AiPersonality) -> &'static str {
    match personality {
        AiPersonality::Aggressive => "aggressive",
        AiPersonality::Diplomatic => "diplomatic",
        AiPersonality::Economic => "economic",
        AiPersonality::Balanced => "balanced",
    }
}

/// Read the config table for a personality from Lua.
pub fn lua_get_config(engine: &LuaEngine, personality: AiPersonality) -> Option<LuaAiConfig> {
    let lua = engine.lua();
    let table_name = personality_table_name(personality);
    let table: mlua::Table = lua.globals().get(table_name).ok()?;

    Some(
        LuaAiConfig {
            // Core
            trade_priority: table.get("trade_priority").unwrap_or(0.5),
            alliance_preference: table.get("alliance_preference").unwrap_or(0.5),
            min_army_size: table.get("min_army_size").unwrap_or(3),
            max_army_size: table.get("max_army_size").unwrap_or(7),
            infrastructure_budget: table.get("infrastructure_budget").unwrap_or(2000),
            research_strategy: table
                .get::<String>("research_strategy")
                .unwrap_or_else(|_| "cheapest".to_string()),
            worker_threshold: table.get("worker_threshold").unwrap_or(5),
            // War
            war_cooldown: table.get("war_cooldown").ok(),
            war_threshold: table.get("war_threshold").ok(),
            army_min_for_war: table.get::<usize>("army_min_for_war").ok(),
            opportunism_weight: table.get("opportunism_weight").ok(),
            min_artillery_for_minor_war: table.get::<usize>("min_artillery_for_minor_war").ok(),
            // Opportunity gate + resource-bonus (card #97)
            min_opportunity_start: table.get("min_opportunity_start").ok(),
            min_opportunity_end: table.get("min_opportunity_end").ok(),
            min_opportunity_decay_turns: table.get("min_opportunity_decay_turns").ok(),
            resource_bonus_per_missing: table.get("resource_bonus_per_missing").ok(),
            resource_bonus_cap: table.get("resource_bonus_cap").ok(),
            // Army tiers
            tier1_army_max: table.get::<usize>("tier1_army_max").ok(),
            tier2_army_max: table.get::<usize>("tier2_army_max").ok(),
            tier3_army_max: table.get::<usize>("tier3_army_max").ok(),
            tier1_treasury: table.get("tier1_treasury").ok(),
            tier2_treasury: table.get("tier2_treasury").ok(),
            tier3_treasury: table.get("tier3_treasury").ok(),
            tier4_treasury: table.get("tier4_treasury").ok(),
            // Diplomacy
            consulate_max_per_turn: table.get("consulate_max_per_turn").ok(),
            propose_pacts: table.get("propose_pacts").ok(),
            propose_alliances: table.get("propose_alliances").ok(),
            grant_amount: table.get("grant_amount").ok(),
            grant_interval: table.get("grant_interval").ok(),
            embassy_treasury_threshold: table.get("embassy_treasury_threshold").ok(),
            max_alliances: table.get::<usize>("max_alliances").ok(),
            // Naval
            max_merchant_ships: table.get::<usize>("max_merchant_ships").ok(),
            min_army_naval_invasion: table.get::<usize>("min_army_naval_invasion").ok(),
            // Economy
            expansion_threshold_multiplier: table.get("expansion_threshold_multiplier").ok(),
            use_tier_expansion: table.get("use_tier_expansion").ok(),
            high_treasury_expansion_threshold: table.get("high_treasury_expansion_threshold").ok(),
            trade_resource_reserve: table.get("trade_resource_reserve").ok(),
            trade_treasury_cap: table.get("trade_treasury_cap").ok(),
            goods_sell_treasury_threshold: table.get("goods_sell_treasury_threshold").ok(),
            goods_reserve: table.get("goods_reserve").ok(),
            food_processing_expansion_threshold: table
                .get("food_processing_expansion_threshold")
                .ok(),
            infra_budget_scale_threshold: table.get("infra_budget_scale_threshold").ok(),
            // Spending weights
            spending_military_weight: table.get("spending_military_weight").ok(),
            spending_economy_weight: table.get("spending_economy_weight").ok(),
            spending_diplomacy_weight: table.get("spending_diplomacy_weight").ok(),
            treasury_reserve: table.get("treasury_reserve").ok(),
            min_score_threshold: table.get("min_score_threshold").ok(),
            // Worker training
            worker_train_threshold: table.get("worker_train_threshold").ok(),
            worker_promote_threshold: table.get("worker_promote_threshold").ok(),
            // Tactical
            peace_war_duration_threshold: table.get("peace_war_duration_threshold").ok(),
            peace_province_loss_ratio: table.get("peace_province_loss_ratio").ok(),
            fort_strategy: table.get::<String>("fort_strategy").ok(),
            // Coalition assessment weights
            coalition_mil_weight: table.get("coalition_mil_weight").ok(),
            coalition_prov_weight: table.get("coalition_prov_weight").ok(),
            coalition_econ_weight: table.get("coalition_econ_weight").ok(),
            coalition_momentum_weight: table.get("coalition_momentum_weight").ok(),
            coalition_naval_weight: table.get("coalition_naval_weight").ok(),
            coalition_sigmoid_steepness: table.get("coalition_sigmoid_steepness").ok(),
            // Economic-score multipliers
            econ_score_treasury_divisor: table.get("econ_score_treasury_divisor").ok(),
            econ_score_buildings_multiplier: table.get("econ_score_buildings_multiplier").ok(),
            econ_score_workers_multiplier: table.get("econ_score_workers_multiplier").ok(),
            // Peace proposal thresholds
            peace_accept_threshold: table.get("peace_accept_threshold").ok(),
            peace_reject_threshold: table.get("peace_reject_threshold").ok(),
            peace_stalemate_duration: table.get("peace_stalemate_duration").ok(),
            // War worthiness thresholds
            won_enough_captures: table.get::<usize>("won_enough_captures").ok(),
            won_enough_marginal: table.get("won_enough_marginal").ok(),
            lost_enough_losses: table.get::<usize>("lost_enough_losses").ok(),
            lost_enough_likelihood: table.get("lost_enough_likelihood").ok(),
            // Treaty evaluation thresholds
            nap_accept_threshold: table.get("nap_accept_threshold").ok(),
            alliance_accept_threshold: table.get("alliance_accept_threshold").ok(),
            alliance_rival_penalty: table.get("alliance_rival_penalty").ok(),
            alliance_overcommit_penalty: table.get("alliance_overcommit_penalty").ok(),
            treaty_personality_bias: table.get("treaty_personality_bias").ok(),
            // Field-army distribution (cards #5, #9)
            capital_reserve_normal: table.get::<usize>("capital_reserve_normal").ok(),
            capital_reserve_threatened: table.get::<usize>("capital_reserve_threatened").ok(),
            max_redeploys_per_turn: table.get::<usize>("max_redeploys_per_turn").ok(),
            // Retreat (card #18)
            retreat_prebattle_ratio: table.get("retreat_prebattle_ratio").ok(),
            retreat_postbattle_fp_loss: table.get("retreat_postbattle_fp_loss").ok(),
            // Naval landing gate (card #7)
            naval_min_adjacent_strength_ratio: table.get("naval_min_adjacent_strength_ratio").ok(),
            // Attack acceptance (card #99 phase 2)
            attack_fp_vs_minor: table.get("attack_fp_vs_minor").ok(),
            attack_fp_vs_gp: table.get("attack_fp_vs_gp").ok(),
            // Trello cards #8 / #20 / #112
            rest_health_threshold: table.get("rest_health_threshold").ok(),
            capital_save_for_last_penalty: table.get("capital_save_for_last_penalty").ok(),
            spending_naval_weight: table.get("spending_naval_weight").ok(),
        }
        .sanitize(),
    )
}

fn serialize_tech_effect(effect: &TechEffect) -> (String, String) {
    match effect {
        TechEffect::UnlockUnit(u) => ("UnlockUnit".to_string(), format!("{:?}", u)),
        TechEffect::UnlockBuilding(b) => ("UnlockBuilding".to_string(), format!("{:?}", b)),
        TechEffect::EnableTerrainImprovement { terrain, .. } => {
            ("EnableTerrainImprovement".to_string(), terrain.clone())
        }
        TechEffect::EnableInfrastructure(s) => ("EnableInfrastructure".to_string(), s.clone()),
        TechEffect::UnlockShip(s) => ("UnlockShip".to_string(), s.clone()),
        TechEffect::UpgradeUnit { from, to } => {
            ("UpgradeUnit".to_string(), format!("{:?}->{:?}", from, to))
        }
        TechEffect::EnableCivilian(c) => ("EnableCivilian".to_string(), format!("{:?}", c)),
        TechEffect::LuaScript(s) => ("LuaScript".to_string(), s.clone()),
    }
}

/// Call the Lua `<personality>.pick_tech(available_techs)` function.
/// Returns the TechId selected by Lua, or None if Lua fails or returns nil.
pub fn lua_pick_tech(
    game: &GameState,
    personality: AiPersonality,
    available: &[(TechId, Money, String, Vec<TechEffect>)],
) -> Option<TechId> {
    let engine = game.game_data.lua_engine.as_ref()?;
    let lua = engine.lua();
    let table_name = personality_table_name(personality);

    // Build the Lua table of available techs
    let techs_table = lua.create_table().ok()?;
    for (i, (tech_id, cost, name, effects)) in available.iter().enumerate() {
        let t = lua.create_table().ok()?;
        t.set("id", tech_id.0).ok()?;
        t.set("name", name.as_str()).ok()?;
        t.set("cost", cost.cents()).ok()?;

        let effects_table = lua.create_table().ok()?;
        for (j, effect) in effects.iter().enumerate() {
            let e = lua.create_table().ok()?;
            let (etype, evalue) = serialize_tech_effect(effect);
            e.set("type", etype).ok()?;
            e.set("value", evalue).ok()?;
            effects_table.set(j + 1, e).ok()?;
        }
        t.set("effects", effects_table).ok()?;
        techs_table.set(i + 1, t).ok()?;
    }

    // Call <personality>.pick_tech(techs_table)
    let personality_table: mlua::Table = lua.globals().get(table_name).ok()?;
    let pick_tech: mlua::Function = personality_table.get("pick_tech").ok()?;
    let result: mlua::Table = pick_tech.call(techs_table).ok()?;
    let tech_id: u32 = result.get("id").ok()?;
    Some(TechId(tech_id))
}

/// Call the Lua `<personality>.evaluate_war(nation_id, target_id, relations, need, opportunity)`.
/// Returns Some(true/false) from Lua, or None if Lua fails.
/// Extra params are silently ignored by Lua scripts that don't use them.
pub fn lua_evaluate_war(
    game: &GameState,
    personality: AiPersonality,
    nation_id: NationId,
    target_id: NationId,
    relations: i32,
    need_score: f64,
    opportunity_score: f64,
) -> Option<bool> {
    let engine = game.game_data.lua_engine.as_ref()?;
    let lua = engine.lua();
    let table_name = personality_table_name(personality);

    let personality_table: mlua::Table = lua.globals().get(table_name).ok()?;
    let evaluate_war: mlua::Function = personality_table.get("evaluate_war").ok()?;
    let result: bool = evaluate_war
        .call::<bool>((
            nation_id.0 as i64,
            target_id.0 as i64,
            relations,
            need_score,
            opportunity_score,
        ))
        .ok()?;
    Some(result)
}

/// Call the Lua `<personality>.evaluate_peace(nation_id, enemy_id, win_likelihood, captured, lost, duration)`.
/// Returns Some(true) to propose peace, Some(false) to not propose, or None to fall through.
#[allow(clippy::too_many_arguments)]
pub fn lua_evaluate_peace(
    game: &GameState,
    personality: AiPersonality,
    nation_id: NationId,
    enemy_id: NationId,
    win_likelihood: f64,
    captured: usize,
    lost: usize,
    duration: u32,
) -> Option<bool> {
    let engine = game.game_data.lua_engine.as_ref()?;
    let lua = engine.lua();
    let table_name = personality_table_name(personality);

    let personality_table: mlua::Table = lua.globals().get(table_name).ok()?;
    let evaluate_peace: mlua::Function = personality_table.get("evaluate_peace").ok()?;
    let result: mlua::Value = evaluate_peace
        .call::<mlua::Value>((
            nation_id.0 as i64,
            enemy_id.0 as i64,
            win_likelihood,
            captured as i64,
            lost as i64,
            duration as i64,
        ))
        .ok()?;
    match result {
        mlua::Value::Boolean(b) => Some(b),
        _ => None, // nil = fall through to Rust logic
    }
}

/// Call the Lua `<personality>.evaluate_treaty_response(nation_id, proposer_id, treaty_type, relationship, power_ratio)`.
/// Returns Some(true) to accept, Some(false) to reject, or None to fall through.
pub fn lua_evaluate_treaty_response(
    game: &GameState,
    personality: AiPersonality,
    nation_id: NationId,
    proposer_id: NationId,
    treaty_type_str: &str,
    relationship: i32,
    power_ratio: f64,
) -> Option<bool> {
    let engine = game.game_data.lua_engine.as_ref()?;
    let lua = engine.lua();
    let table_name = personality_table_name(personality);

    let personality_table: mlua::Table = lua.globals().get(table_name).ok()?;
    let evaluate_treaty: mlua::Function = personality_table.get("evaluate_treaty_response").ok()?;
    let result: mlua::Value = evaluate_treaty
        .call::<mlua::Value>((
            nation_id.0 as i64,
            proposer_id.0 as i64,
            treaty_type_str.to_string(),
            relationship,
            power_ratio,
        ))
        .ok()?;
    match result {
        mlua::Value::Boolean(b) => Some(b),
        _ => None, // nil = fall through to Rust logic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripting::LuaEngine;

    fn engine_with_scripts() -> LuaEngine {
        let engine = LuaEngine::new().unwrap();
        load_personality_scripts(&engine).unwrap();
        engine
    }

    #[test]
    fn load_all_personality_scripts() {
        let engine = LuaEngine::new().unwrap();
        load_personality_scripts(&engine).unwrap();
    }

    #[test]
    fn read_balanced_config() {
        let engine = engine_with_scripts();
        let config = lua_get_config(&engine, AiPersonality::Balanced).unwrap();
        assert_eq!(config.research_strategy, "cheapest");
        assert!((config.trade_priority - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.min_army_size, 3);
        assert_eq!(config.max_army_size, 7);
        // Extended fields now populated from scripts
        assert_eq!(config.war_cooldown, Some(12));
        assert_eq!(config.tier1_army_max, Some(3));
        assert_eq!(config.tier1_treasury, Some(2000));
        assert_eq!(config.consulate_max_per_turn, Some(2));
        assert_eq!(config.fort_strategy.as_deref(), Some("border"));
    }

    #[test]
    fn read_aggressive_config() {
        let engine = engine_with_scripts();
        let config = lua_get_config(&engine, AiPersonality::Aggressive).unwrap();
        assert_eq!(config.research_strategy, "military");
        assert_eq!(config.min_army_size, 5);
        assert_eq!(config.max_army_size, 12);
    }

    #[test]
    fn read_diplomatic_config() {
        let engine = engine_with_scripts();
        let config = lua_get_config(&engine, AiPersonality::Diplomatic).unwrap();
        assert_eq!(config.research_strategy, "economic");
        assert!((config.alliance_preference - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn read_economic_config() {
        let engine = engine_with_scripts();
        let config = lua_get_config(&engine, AiPersonality::Economic).unwrap();
        assert_eq!(config.research_strategy, "expensive");
        assert_eq!(config.infrastructure_budget, 3000);
    }

    #[test]
    fn balanced_evaluate_war_low_relations() {
        let engine = engine_with_scripts();
        let lua = engine.lua();
        let table: mlua::Table = lua.globals().get("balanced").unwrap();
        let func: mlua::Function = table.get("evaluate_war").unwrap();
        let result: bool = func.call::<bool>((1i64, 2i64, -60i32)).unwrap();
        assert!(
            result,
            "Balanced should declare war at relations -60 (< -50)"
        );
    }

    #[test]
    fn balanced_evaluate_war_ok_relations() {
        let engine = engine_with_scripts();
        let lua = engine.lua();
        let table: mlua::Table = lua.globals().get("balanced").unwrap();
        let func: mlua::Function = table.get("evaluate_war").unwrap();
        let result: bool = func.call::<bool>((1i64, 2i64, -30i32)).unwrap();
        assert!(
            !result,
            "Balanced should not declare war at relations -30 (> -50)"
        );
    }

    #[test]
    fn aggressive_pick_tech_prefers_military() {
        let engine = engine_with_scripts();
        let lua = engine.lua();

        // Build a table with one military and one economic tech
        let techs = lua.create_table().unwrap();

        let t1 = lua.create_table().unwrap();
        t1.set("id", 1).unwrap();
        t1.set("name", "Seed Drill").unwrap();
        t1.set("cost", 0).unwrap();
        let effects1 = lua.create_table().unwrap();
        let e1 = lua.create_table().unwrap();
        e1.set("type", "EnableTerrainImprovement").unwrap();
        e1.set("value", "Farm").unwrap();
        effects1.set(1, e1).unwrap();
        t1.set("effects", effects1).unwrap();
        techs.set(1, t1).unwrap();

        let t2 = lua.create_table().unwrap();
        t2.set("id", 2).unwrap();
        t2.set("name", "Rifled Muskets").unwrap();
        t2.set("cost", 1000).unwrap();
        let effects2 = lua.create_table().unwrap();
        let e2 = lua.create_table().unwrap();
        e2.set("type", "UnlockUnit").unwrap();
        e2.set("value", "RifleInfantry").unwrap();
        effects2.set(1, e2).unwrap();
        t2.set("effects", effects2).unwrap();
        techs.set(2, t2).unwrap();

        let table: mlua::Table = lua.globals().get("aggressive").unwrap();
        let func: mlua::Function = table.get("pick_tech").unwrap();
        let result: mlua::Table = func.call(techs).unwrap();
        let picked_id: u32 = result.get("id").unwrap();
        assert_eq!(picked_id, 2, "Aggressive should pick military tech");
    }

    #[test]
    fn economic_pick_tech_prefers_expensive() {
        let engine = engine_with_scripts();
        let lua = engine.lua();

        let techs = lua.create_table().unwrap();

        let t1 = lua.create_table().unwrap();
        t1.set("id", 1).unwrap();
        t1.set("name", "Cheap Tech").unwrap();
        t1.set("cost", 500).unwrap();
        t1.set("effects", lua.create_table().unwrap()).unwrap();
        techs.set(1, t1).unwrap();

        let t2 = lua.create_table().unwrap();
        t2.set("id", 2).unwrap();
        t2.set("name", "Expensive Tech").unwrap();
        t2.set("cost", 3000).unwrap();
        t2.set("effects", lua.create_table().unwrap()).unwrap();
        techs.set(2, t2).unwrap();

        let table: mlua::Table = lua.globals().get("economic").unwrap();
        let func: mlua::Function = table.get("pick_tech").unwrap();
        let result: mlua::Table = func.call(techs).unwrap();
        let picked_id: u32 = result.get("id").unwrap();
        assert_eq!(picked_id, 2, "Economic should pick most expensive tech");
    }

    #[test]
    fn sanitize_clamps_nan_and_inf() {
        let cfg = LuaAiConfig {
            trade_priority: f64::NAN,
            alliance_preference: f64::INFINITY,
            min_army_size: 3,
            max_army_size: 7,
            infrastructure_budget: 2000,
            research_strategy: "cheapest".to_string(),
            worker_threshold: 5,
            war_cooldown: None,
            war_threshold: Some(f64::NAN),
            army_min_for_war: None,
            opportunism_weight: Some(f64::NEG_INFINITY),
            min_artillery_for_minor_war: None,
            min_opportunity_start: None,
            min_opportunity_end: None,
            min_opportunity_decay_turns: None,
            resource_bonus_per_missing: None,
            resource_bonus_cap: None,
            tier1_army_max: None,
            tier2_army_max: None,
            tier3_army_max: None,
            tier1_treasury: None,
            tier2_treasury: None,
            tier3_treasury: None,
            tier4_treasury: None,
            consulate_max_per_turn: None,
            propose_pacts: None,
            propose_alliances: None,
            grant_amount: Some(-500),
            grant_interval: None,
            embassy_treasury_threshold: None,
            max_alliances: None,
            max_merchant_ships: None,
            min_army_naval_invasion: None,
            expansion_threshold_multiplier: None,
            use_tier_expansion: None,
            high_treasury_expansion_threshold: None,
            trade_resource_reserve: None,
            trade_treasury_cap: None,
            goods_sell_treasury_threshold: None,
            goods_reserve: None,
            food_processing_expansion_threshold: None,
            infra_budget_scale_threshold: None,
            spending_military_weight: Some(f64::INFINITY),
            spending_economy_weight: None,
            spending_diplomacy_weight: None,
            treasury_reserve: None,
            min_score_threshold: None,
            worker_train_threshold: None,
            worker_promote_threshold: None,
            peace_war_duration_threshold: None,
            peace_province_loss_ratio: None,
            fort_strategy: None,
            coalition_mil_weight: None,
            coalition_prov_weight: None,
            coalition_econ_weight: None,
            coalition_momentum_weight: None,
            coalition_naval_weight: None,
            coalition_sigmoid_steepness: Some(f64::NAN),
            econ_score_treasury_divisor: None,
            econ_score_buildings_multiplier: None,
            econ_score_workers_multiplier: None,
            peace_accept_threshold: None,
            peace_reject_threshold: None,
            peace_stalemate_duration: None,
            won_enough_captures: None,
            won_enough_marginal: None,
            lost_enough_losses: None,
            lost_enough_likelihood: None,
            nap_accept_threshold: None,
            alliance_accept_threshold: None,
            alliance_rival_penalty: None,
            alliance_overcommit_penalty: None,
            treaty_personality_bias: None,
            capital_reserve_normal: None,
            capital_reserve_threatened: None,
            max_redeploys_per_turn: None,
            retreat_prebattle_ratio: None,
            retreat_postbattle_fp_loss: None,
            naval_min_adjacent_strength_ratio: None,
            attack_fp_vs_minor: None,
            attack_fp_vs_gp: None,
            rest_health_threshold: None,
            capital_save_for_last_penalty: None,
            spending_naval_weight: None,
        };

        let sanitized = cfg.sanitize();

        // NaN/inf core fields fall back to defaults
        assert_eq!(sanitized.trade_priority, 0.5);
        assert_eq!(sanitized.alliance_preference, 0.5);

        // NaN/inf optional fields become None
        assert!(sanitized.war_threshold.is_none());
        assert!(sanitized.opportunism_weight.is_none());
        assert!(sanitized.spending_military_weight.is_none());
        assert!(sanitized.coalition_sigmoid_steepness.is_none());

        // Negative i64 gets clamped to 0
        assert_eq!(sanitized.grant_amount, Some(0));
    }

    #[test]
    fn sanitize_clamps_out_of_range() {
        let cfg = LuaAiConfig {
            trade_priority: 5.0,       // should clamp to 1.0
            alliance_preference: -1.0, // should clamp to 0.0
            min_army_size: 3,
            max_army_size: 7,
            infrastructure_budget: 2000,
            research_strategy: "cheapest".to_string(),
            worker_threshold: 5,
            war_cooldown: None,
            war_threshold: Some(2.0), // should clamp to 1.0
            army_min_for_war: None,
            opportunism_weight: Some(50.0), // should clamp to 10.0
            min_artillery_for_minor_war: None,
            min_opportunity_start: None,
            min_opportunity_end: None,
            min_opportunity_decay_turns: None,
            resource_bonus_per_missing: None,
            resource_bonus_cap: None,
            tier1_army_max: None,
            tier2_army_max: None,
            tier3_army_max: None,
            tier1_treasury: None,
            tier2_treasury: None,
            tier3_treasury: None,
            tier4_treasury: None,
            consulate_max_per_turn: None,
            propose_pacts: None,
            propose_alliances: None,
            grant_amount: Some(500_000), // should clamp to 100_000
            grant_interval: None,
            embassy_treasury_threshold: None,
            max_alliances: None,
            max_merchant_ships: None,
            min_army_naval_invasion: None,
            expansion_threshold_multiplier: None,
            use_tier_expansion: None,
            high_treasury_expansion_threshold: None,
            trade_resource_reserve: None,
            trade_treasury_cap: None,
            goods_sell_treasury_threshold: None,
            goods_reserve: None,
            food_processing_expansion_threshold: None,
            infra_budget_scale_threshold: None,
            spending_military_weight: None,
            spending_economy_weight: None,
            spending_diplomacy_weight: None,
            treasury_reserve: None,
            min_score_threshold: None,
            worker_train_threshold: None,
            worker_promote_threshold: None,
            peace_war_duration_threshold: None,
            peace_province_loss_ratio: None,
            fort_strategy: None,
            coalition_mil_weight: None,
            coalition_prov_weight: None,
            coalition_econ_weight: None,
            coalition_momentum_weight: None,
            coalition_naval_weight: None,
            coalition_sigmoid_steepness: None,
            econ_score_treasury_divisor: None,
            econ_score_buildings_multiplier: None,
            econ_score_workers_multiplier: None,
            peace_accept_threshold: None,
            peace_reject_threshold: None,
            peace_stalemate_duration: None,
            won_enough_captures: None,
            won_enough_marginal: None,
            lost_enough_losses: None,
            lost_enough_likelihood: None,
            nap_accept_threshold: None,
            alliance_accept_threshold: None,
            alliance_rival_penalty: None,
            alliance_overcommit_penalty: None,
            treaty_personality_bias: None,
            capital_reserve_normal: None,
            capital_reserve_threatened: None,
            max_redeploys_per_turn: None,
            retreat_prebattle_ratio: None,
            retreat_postbattle_fp_loss: None,
            naval_min_adjacent_strength_ratio: None,
            attack_fp_vs_minor: None,
            attack_fp_vs_gp: None,
            rest_health_threshold: None,
            capital_save_for_last_penalty: None,
            spending_naval_weight: None,
        };

        let sanitized = cfg.sanitize();
        assert_eq!(sanitized.trade_priority, 1.0);
        assert_eq!(sanitized.alliance_preference, 0.0);
        assert_eq!(sanitized.war_threshold, Some(1.0));
        assert_eq!(sanitized.opportunism_weight, Some(10.0));
        assert_eq!(sanitized.grant_amount, Some(100_000));
    }

    #[test]
    fn game_config_sanitizes_extreme_lua_values() {
        let engine = LuaEngine::new().unwrap();
        // Set extreme game_config values via Lua
        engine
            .exec(
                r#"
                game_config = {
                    gold_value = 999999999,
                    gems_value = -500,
                    consulate_cost = 999999999,
                    embassy_cost = -1,
                    expansion_delay_turns = 300,
                    untrained_labor = 0,
                    food_per_worker = -5,
                    provinces_per_immigrant = 0,
                    min_food_tile_percent = 200,
                    food_cluster_chance = -10,
                }
                "#,
            )
            .unwrap();

        let cfg = load_game_config(&engine);

        // Upper bounds clamped
        assert_eq!(cfg.gold_value, 1_000_000);
        assert_eq!(cfg.consulate_cost, 1_000_000);

        // Negative values clamped to 0
        assert_eq!(cfg.gems_value, 0);
        assert_eq!(cfg.embassy_cost, 0);

        // u32→u8 saturation: 300 -> 255
        assert_eq!(cfg.expansion_delay_turns, 255);

        // Min-1 enforcement for divisor fields
        assert_eq!(cfg.untrained_labor, 1);
        assert_eq!(cfg.food_per_worker, 1);
        assert_eq!(cfg.provinces_per_immigrant, 1);

        // Percent clamping
        assert_eq!(cfg.min_food_tile_percent, 100);
        // Negative Lua value for u32 field falls back to default (40), then clamped to [0,100]
        assert_eq!(cfg.food_cluster_chance, 40);
    }

    #[test]
    fn sanitize_enforces_cross_field_invariants() {
        let cfg = LuaAiConfig {
            trade_priority: 0.5,
            alliance_preference: 0.5,
            min_army_size: 10, // > max_army_size
            max_army_size: 5,
            infrastructure_budget: 2000,
            research_strategy: "cheapest".to_string(),
            worker_threshold: 5,
            war_cooldown: None,
            war_threshold: None,
            army_min_for_war: None,
            opportunism_weight: None,
            min_artillery_for_minor_war: None,
            min_opportunity_start: None,
            min_opportunity_end: None,
            min_opportunity_decay_turns: None,
            resource_bonus_per_missing: None,
            resource_bonus_cap: None,
            tier1_army_max: None,
            tier2_army_max: None,
            tier3_army_max: None,
            tier1_treasury: None,
            tier2_treasury: None,
            tier3_treasury: None,
            tier4_treasury: None,
            consulate_max_per_turn: None,
            propose_pacts: None,
            propose_alliances: None,
            grant_amount: None,
            grant_interval: None,
            embassy_treasury_threshold: None,
            max_alliances: None,
            max_merchant_ships: None,
            min_army_naval_invasion: None,
            expansion_threshold_multiplier: None,
            use_tier_expansion: None,
            high_treasury_expansion_threshold: None,
            trade_resource_reserve: None,
            trade_treasury_cap: None,
            goods_sell_treasury_threshold: None,
            goods_reserve: None,
            food_processing_expansion_threshold: None,
            infra_budget_scale_threshold: None,
            spending_military_weight: None,
            spending_economy_weight: None,
            spending_diplomacy_weight: None,
            treasury_reserve: None,
            min_score_threshold: None,
            worker_train_threshold: None,
            worker_promote_threshold: None,
            peace_war_duration_threshold: None,
            peace_province_loss_ratio: None,
            fort_strategy: None,
            coalition_mil_weight: None,
            coalition_prov_weight: None,
            coalition_econ_weight: None,
            coalition_momentum_weight: None,
            coalition_naval_weight: None,
            coalition_sigmoid_steepness: None,
            econ_score_treasury_divisor: None,
            econ_score_buildings_multiplier: None,
            econ_score_workers_multiplier: None,
            peace_accept_threshold: None,
            peace_reject_threshold: None,
            peace_stalemate_duration: None,
            won_enough_captures: None,
            won_enough_marginal: None,
            lost_enough_losses: None,
            lost_enough_likelihood: None,
            nap_accept_threshold: None,
            alliance_accept_threshold: None,
            alliance_rival_penalty: None,
            alliance_overcommit_penalty: None,
            treaty_personality_bias: None,
            capital_reserve_normal: None,
            capital_reserve_threatened: None,
            max_redeploys_per_turn: None,
            retreat_prebattle_ratio: None,
            retreat_postbattle_fp_loss: None,
            naval_min_adjacent_strength_ratio: None,
            attack_fp_vs_minor: None,
            attack_fp_vs_gp: None,
            rest_health_threshold: None,
            capital_save_for_last_penalty: None,
            spending_naval_weight: None,
        };

        let sanitized = cfg.sanitize();
        assert!(
            sanitized.min_army_size <= sanitized.max_army_size,
            "min_army_size ({}) should be <= max_army_size ({})",
            sanitized.min_army_size,
            sanitized.max_army_size
        );
    }
}
