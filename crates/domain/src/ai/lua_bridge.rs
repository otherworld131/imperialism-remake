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

// Embed personality scripts at compile time for sandboxing safety.
const BALANCED_LUA: &str = include_str!("../../../../scripts/ai/balanced.lua");
const AGGRESSIVE_LUA: &str = include_str!("../../../../scripts/ai/aggressive.lua");
const DIPLOMATIC_LUA: &str = include_str!("../../../../scripts/ai/diplomatic.lua");
const ECONOMIC_LUA: &str = include_str!("../../../../scripts/ai/economic.lua");

/// Load all four personality scripts into the Lua VM.
/// Each script defines a global table (e.g., `balanced`, `aggressive`) with
/// config fields and methods (`pick_tech`, `evaluate_war`).
pub fn load_personality_scripts(engine: &LuaEngine) -> Result<(), String> {
    engine.exec(BALANCED_LUA)?;
    engine.exec(AGGRESSIVE_LUA)?;
    engine.exec(DIPLOMATIC_LUA)?;
    engine.exec(ECONOMIC_LUA)?;
    Ok(())
}

/// Config parameters read from Lua personality tables.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used when more subsystems integrate Lua config
pub struct LuaAiConfig {
    pub trade_priority: f64,
    pub war_declaration_interval: u32,
    pub alliance_preference: f64,
    pub min_army_size: u32,
    pub max_army_size: u32,
    pub infrastructure_budget: i64,
    pub research_strategy: String,
    pub worker_threshold: u32,
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
#[allow(dead_code)] // Will be used when more subsystems integrate Lua config
pub fn lua_get_config(engine: &LuaEngine, personality: AiPersonality) -> Option<LuaAiConfig> {
    let lua = engine.lua();
    let table_name = personality_table_name(personality);
    let table: mlua::Table = lua.globals().get(table_name).ok()?;

    Some(LuaAiConfig {
        trade_priority: table.get("trade_priority").unwrap_or(0.5),
        war_declaration_interval: table.get("war_declaration_interval").unwrap_or(25),
        alliance_preference: table.get("alliance_preference").unwrap_or(0.5),
        min_army_size: table.get("min_army_size").unwrap_or(3),
        max_army_size: table.get("max_army_size").unwrap_or(7),
        infrastructure_budget: table.get("infrastructure_budget").unwrap_or(2000),
        research_strategy: table
            .get::<String>("research_strategy")
            .unwrap_or_else(|_| "cheapest".to_string()),
        worker_threshold: table.get("worker_threshold").unwrap_or(5),
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

/// Call the Lua `<personality>.evaluate_war(nation_id, target_id, relations)` function.
/// Returns Some(true/false) from Lua, or None if Lua fails.
pub fn lua_evaluate_war(
    game: &GameState,
    personality: AiPersonality,
    nation_id: NationId,
    target_id: NationId,
    relations: i32,
) -> Option<bool> {
    let engine = game.game_data.lua_engine.as_ref()?;
    let lua = engine.lua();
    let table_name = personality_table_name(personality);

    let personality_table: mlua::Table = lua.globals().get(table_name).ok()?;
    let evaluate_war: mlua::Function = personality_table.get("evaluate_war").ok()?;
    let result: bool = evaluate_war
        .call::<bool>((nation_id.0 as i64, target_id.0 as i64, relations))
        .ok()?;
    Some(result)
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
        assert_eq!(config.war_declaration_interval, 20);
        assert!((config.trade_priority - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.min_army_size, 3);
        assert_eq!(config.max_army_size, 7);
    }

    #[test]
    fn read_aggressive_config() {
        let engine = engine_with_scripts();
        let config = lua_get_config(&engine, AiPersonality::Aggressive).unwrap();
        assert_eq!(config.research_strategy, "military");
        assert_eq!(config.war_declaration_interval, 15);
        assert_eq!(config.min_army_size, 5);
        assert_eq!(config.max_army_size, 12);
    }

    #[test]
    fn read_diplomatic_config() {
        let engine = engine_with_scripts();
        let config = lua_get_config(&engine, AiPersonality::Diplomatic).unwrap();
        assert_eq!(config.research_strategy, "economic");
        assert_eq!(config.war_declaration_interval, 40);
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
