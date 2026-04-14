//! RON-serializable definitions for game data.
//!
//! These structs mirror the RON file format and are converted into domain
//! types by the loader module.

use serde::Deserialize;

// ── Technology ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TechDefsFile {
    pub technologies: Vec<TechDef>,
}

#[derive(Debug, Deserialize)]
pub struct TechDef {
    pub id: u32,
    pub name: String,
    pub cost: i64,
    pub earliest_year: u32,
    pub latest_year: u32,
    pub prerequisites: Vec<u32>,
    pub effects: Vec<TechEffectDef>,
}

#[derive(Debug, Deserialize)]
pub enum TechEffectDef {
    UnlockUnit(String),
    UnlockBuilding(String),
    EnableTerrainImprovement { terrain: String, max_level: u8 },
    EnableInfrastructure(String),
    UnlockShip(String),
    UpgradeUnit { from: String, to: String },
    EnableCivilian(String),
    LuaScript(String),
}

// ── Ships ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ShipDefsFile {
    pub ships: Vec<ShipDef>,
}

#[derive(Debug, Deserialize)]
pub struct ShipDef {
    pub name: String,
    pub category: String,
    pub firepower: u32,
    pub range: u32,
    pub armor: u32,
    pub hull: u32,
    pub speed: u32,
    pub cargo: u32,
    pub fabric_cost: u32,
    pub lumber_cost: u32,
    pub arms_cost: u32,
    pub steel_cost: u32,
    pub coal_cost: u32,
    pub prerequisite_tech: Option<String>,
}

// ── Units ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UnitDefsFile {
    pub units: Vec<UnitDef>,
}

#[derive(Debug, Deserialize)]
pub struct UnitDef {
    pub name: String,
    pub category: String,
    pub firepower: u32,
    pub movement: u32,
    pub range: u32,
    pub cost: i64,
    pub arms_required: u32,
    #[serde(default)]
    pub requires_horse: bool,
    pub maintenance_per_turn: i64,
    pub prerequisite_tech: Option<String>,
}

// ── Buildings ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BuildingDefsFile {
    pub buildings: Vec<BuildingDef>,
}

#[derive(Debug, Deserialize)]
pub struct BuildingDef {
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub base_capacity: Option<u32>,
    #[serde(default)]
    pub expansion_cost_per_unit: Option<ExpansionCostDef>,
    #[serde(default)]
    pub purpose: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExpansionCostDef {
    pub lumber: u32,
    pub steel: u32,
}

// ── Terrain ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TerrainDefsFile {
    pub terrain_types: Vec<TerrainDef>,
}

#[derive(Debug, Deserialize)]
pub struct TerrainDef {
    pub name: String,
    pub defense_bonus: f64,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub base_resource: Option<String>,
    #[serde(default)]
    pub requires_prospecting: Option<bool>,
    #[serde(default)]
    pub is_improvable: Option<bool>,
    #[serde(default)]
    pub max_improvement_level: Option<u8>,
}

// ── Nations ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NationDefsFile {
    pub great_powers: Vec<GreatPowerDef>,
    pub minor_nations: Vec<MinorNationDef>,
}

#[derive(Debug, Deserialize)]
pub struct GreatPowerDef {
    pub name: String,
    pub color: String,
    pub ai_personality: String,
}

#[derive(Debug, Deserialize)]
pub struct MinorNationDef {
    pub name: String,
    pub resources: Vec<String>,
}

// ── Production ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ProductionDefsFile {
    pub chains: Vec<ProductionChainDef>,
}

#[derive(Debug, Deserialize)]
pub struct ProductionChainDef {
    pub name: String,
    pub mill_input_ratio: u32,
    pub mill_output_ratio: u32,
    pub factory_input_ratio: u32,
    pub factory_output_ratio: u32,
    pub labor_per_unit: u32,
}

// ── Difficulty / Starting Conditions ────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DifficultyDefsFile {
    pub difficulties: Vec<DifficultyDef>,
}

#[derive(Debug, Deserialize)]
pub struct DifficultyDef {
    pub name: String,
    pub starting_cash: i64,
    pub untrained_workers: u32,
    pub trained_workers: u32,
    pub starting_mills: bool,
    pub starting_factories: bool,
    pub ai_cash_bonus: i64,
    pub starting_resources: Vec<StartingResourceDef>,
}

#[derive(Debug, Deserialize)]
pub struct StartingResourceDef {
    pub resource: String,
    pub amount: u32,
}

// ── Scenarios ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ScenarioDefsFile {
    pub scenarios: Vec<ScenarioDef>,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioDef {
    pub id: String,
    pub name: String,
    pub year: u32,
    pub description: String,
    pub great_powers: Vec<String>,
    #[serde(default)]
    pub difficulty_ratings: Vec<ScenarioDifficultyRating>,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioDifficultyRating {
    pub nation: String,
    pub rating: String,
}
