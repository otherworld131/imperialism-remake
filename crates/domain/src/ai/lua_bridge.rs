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
        starting_freight_cars: table.get("starting_freight_cars").unwrap_or(5),
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
        ai_consulate_beyond_target_score: table.get("ai_consulate_beyond_target_score").unwrap_or(3.0),
        ai_consulate_beyond_target_decay: table.get("ai_consulate_beyond_target_decay").unwrap_or(4.0),
        min_food_tile_percent: table.get("min_food_tile_percent").unwrap_or(20),
        food_cluster_chance: table.get("food_cluster_chance").unwrap_or(40),
    };
    // Sanitize: ensure no zero-or-negative values for fields used as divisors/multipliers
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
        starting_freight_cars: cfg.starting_freight_cars,
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
        ai_consulate_priority_score: if cfg.ai_consulate_priority_score.is_finite() { cfg.ai_consulate_priority_score.clamp(0.0, 1000.0) } else { 30.0 },
        ai_consulate_beyond_target_score: if cfg.ai_consulate_beyond_target_score.is_finite() { cfg.ai_consulate_beyond_target_score.clamp(0.0, 100.0) } else { 3.0 },
        ai_consulate_beyond_target_decay: if cfg.ai_consulate_beyond_target_decay.is_finite() { cfg.ai_consulate_beyond_target_decay.clamp(0.0, 100.0) } else { 4.0 },
        min_food_tile_percent: cfg.min_food_tile_percent.clamp(0, 100),
        food_cluster_chance: cfg.food_cluster_chance.clamp(0, 100),
        ..cfg
    }
}

/// Load game config and all personality scripts into the Lua VM.
pub fn load_scripts(engine: &LuaEngine) -> Result<(), String> {
    engine.exec(GAME_CONFIG_LUA)?;
    engine.exec(BALANCED_LUA)?;
    engine.exec(AGGRESSIVE_LUA)?;
    engine.exec(DIPLOMATIC_LUA)?;
    engine.exec(ECONOMIC_LUA)?;
    Ok(())
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

    // Naval
    pub max_warships_low_treasury: Option<usize>,
    pub max_warships_high_treasury: Option<usize>,
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
        self.max_warships_low_treasury = sanitize_opt_usize(self.max_warships_low_treasury, 0, 50);
        self.max_warships_high_treasury =
            sanitize_opt_usize(self.max_warships_high_treasury, 0, 50);
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
            max_warships_low_treasury: table.get::<usize>("max_warships_low_treasury").ok(),
            max_warships_high_treasury: table.get::<usize>("max_warships_high_treasury").ok(),
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
        }
        .sanitize(),
    )
}

fn serialize_tech_effect(effect: &TechEffect) -> (String, String) {
    match effect {
        TechEffect::UnlockUnit(s) => ("UnlockUnit".to_string(), s.clone()),
        TechEffect::UnlockBuilding(s) => ("UnlockBuilding".to_string(), s.clone()),
        TechEffect::EnableTerrainImprovement { terrain, .. } => {
            ("EnableTerrainImprovement".to_string(), terrain.clone())
        }
        TechEffect::EnableInfrastructure(s) => ("EnableInfrastructure".to_string(), s.clone()),
        TechEffect::UnlockShip(s) => ("UnlockShip".to_string(), s.clone()),
        TechEffect::UpgradeUnit { from, to } => {
            ("UpgradeUnit".to_string(), format!("{}->{}", from, to))
        }
        TechEffect::EnableCivilian(s) => ("EnableCivilian".to_string(), s.clone()),
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
            max_warships_low_treasury: None,
            max_warships_high_treasury: None,
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
            max_warships_low_treasury: None,
            max_warships_high_treasury: None,
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
            max_warships_low_treasury: None,
            max_warships_high_treasury: None,
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
