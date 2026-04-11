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
    GameConfig {
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
        provinces_per_immigrant_upgraded: table.get("provinces_per_immigrant_upgraded").unwrap_or(3),
        gold_value: table.get("gold_value").unwrap_or(500),
        gems_value: table.get("gems_value").unwrap_or(1000),
        expansion_delay_turns: table.get::<u32>("expansion_delay_turns").unwrap_or(2) as u8,
        use_tier_expansion: table.get("use_tier_expansion").unwrap_or(true),
        consulate_cost: table.get("consulate_cost").unwrap_or(500),
        embassy_cost: table.get("embassy_cost").unwrap_or(5000),
        starting_freight_cars: table.get("starting_freight_cars").unwrap_or(5),
        min_food_tile_percent: table.get("min_food_tile_percent").unwrap_or(20),
        food_cluster_chance: table.get("food_cluster_chance").unwrap_or(40),
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

    Some(LuaAiConfig {
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
        // Economy
        expansion_threshold_multiplier: table.get("expansion_threshold_multiplier").ok(),
        use_tier_expansion: table.get("use_tier_expansion").ok(),
        high_treasury_expansion_threshold: table.get("high_treasury_expansion_threshold").ok(),
        trade_resource_reserve: table.get("trade_resource_reserve").ok(),
        trade_treasury_cap: table.get("trade_treasury_cap").ok(),
        goods_sell_treasury_threshold: table.get("goods_sell_treasury_threshold").ok(),
        goods_reserve: table.get("goods_reserve").ok(),
        food_processing_expansion_threshold: table.get("food_processing_expansion_threshold").ok(),
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
    })
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
}
