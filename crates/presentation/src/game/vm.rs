//! View models deserialized from `frontend_api` JSON. The shapes mirror the
//! API contract pinned by the fixtures under
//! `crates/wasm-bridge/tests/fixtures/contract/` — they are the only game
//! data the presentation layer ever sees.

use serde::Deserialize;
use std::collections::HashMap;

/// One tile from `frontend_api::map::get_map_data`. The struct carries the
/// full tile contract even though M2 reads only a subset; serde fills every
/// field, later milestones consume the rest.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MapTile {
    pub q: i32,
    pub r: i32,
    pub map_width: i32,
    pub map_height: i32,
    pub terrain: String,
    pub owner: String,
    pub owner_color: String,
    pub nation_id: i64,
    pub province: String,
    pub province_id: Option<u64>,
    pub is_capital: bool,
    pub is_country_capital: bool,
    pub is_minor: bool,
    pub is_incorporated_minor: bool,
    pub incorporated_nation_id: Option<i64>,
    pub is_anarchic: bool,
    pub is_prospected: bool,
    pub resource: Option<String>,
    pub resource_hidden: bool,
    pub improvement_level: u32,
    pub max_improvement_level: u32,
    pub has_railroad: bool,
    pub has_depot: bool,
    pub has_port: bool,
    pub has_fort: bool,
    pub has_river: bool,
    pub fort_level: u32,
    pub port_blockaded: bool,
    pub army_unit_count: u32,
    pub army_firepower: f64,
    pub army_composition: Option<HashMap<String, u32>>,
    pub naval_ship_count: u32,
    pub naval_firepower: i64,
    pub civilian_on_tile: Option<serde_json::Value>,
    pub visible: bool,
    pub visual_group: Option<serde_json::Value>,
}

pub fn parse_map_tiles(value: serde_json::Value) -> Result<Vec<MapTile>, serde_json::Error> {
    serde_json::from_value(value)
}
