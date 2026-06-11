//! View models deserialized from `frontend_api` JSON. The shapes mirror the
//! API contract pinned by the fixtures under
//! `crates/wasm-bridge/tests/fixtures/contract/` — they are the only game
//! data the presentation layer ever sees.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

/// One tile from `frontend_api::map::get_map_data`. The struct carries the
/// full tile contract; serde fills every field.
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
    pub civilian_on_tile: Option<CivilianOnTile>,
    pub visible: bool,
    pub visual_group: Option<String>,
}

impl MapTile {
    pub fn is_sea(&self) -> bool {
        self.terrain == "Sea"
    }

    /// Visual group used for country borders (incorporated-minor parent),
    /// falling back to the owner like the web frontend.
    pub fn visual_group_or_owner(&self) -> &str {
        match self.visual_group.as_deref() {
            Some(vg) if !vg.is_empty() => vg,
            _ => &self.owner,
        }
    }
}

/// Civilian standing on a map tile (`civilian_on_tile`).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CivilianOnTile {
    pub id: i64,
    #[serde(rename = "type")]
    pub civ_type: String,
    pub working: bool,
    pub turns_remaining: u32,
    pub build_task: Option<String>,
    pub owner: String,
    pub owner_color: String,
    pub is_human: bool,
}

/// `{q, r}` coordinate pair used by several queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct HexRef {
    pub q: i32,
    pub r: i32,
}

/// One marker from `frontend_api::map::get_navy_markers`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct NavyMarker {
    pub q: i32,
    pub r: i32,
    pub nation_id: i64,
    pub owner_name: String,
    pub owner_color: String,
    /// `"fleet"` or `"beachhead"`.
    pub kind: String,
    pub ship_count: u32,
    pub total_fp: i64,
    pub total_hull: i64,
    pub by_type: BTreeMap<String, u32>,
    pub by_operation: BTreeMap<String, u32>,
    pub visible: bool,
    #[serde(default)]
    pub sea_zone_id: Option<u32>,
    #[serde(default)]
    pub sea_zone_name: Option<String>,
    #[serde(default)]
    pub pending_move_to_zone_id: Option<u32>,
    #[serde(default)]
    pub target_province: Option<String>,
    #[serde(default)]
    pub target_hex: Option<HexRef>,
}

/// One zone from `frontend_api::map::get_sea_zones`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SeaZone {
    pub id: u32,
    pub name: String,
    pub is_lake: bool,
    pub center_q: i32,
    pub center_r: i32,
    pub hexes: Vec<HexRef>,
    pub adjacent_zone_ids: Vec<u32>,
}

/// `frontend_api::map::get_diplomacy_overlay` — relations as seen from a
/// perspective nation.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DiplomacyOverlay {
    pub selected_nation: String,
    pub selected_nation_id: u32,
    pub relations: Vec<DiplomacyRelation>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DiplomacyRelation {
    pub nation_name: String,
    pub nation_id: i64,
    pub nation_color: String,
    pub score: i64,
    pub at_war: bool,
    pub status: String,
    pub treaties: Vec<String>,
    pub has_consulate: bool,
    pub has_embassy: bool,
    pub has_pending_consulate: bool,
    pub has_pending_embassy: bool,
    pub has_pending_war: bool,
    pub pending_grant_amount_dollars: Option<i64>,
    pub pending_break_treaties: Vec<String>,
    pub has_pending_nap: bool,
    pub has_pending_alliance: bool,
    pub has_pending_peace: bool,
}

/// One entry from `frontend_api::map::get_military_overlay`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MilitaryOverlayEntry {
    pub nation_name: String,
    pub nation_id: i64,
    pub nation_color: String,
    pub army_unit_count: u32,
    pub total_army_fp: f64,
    pub total_naval_fp: f64,
    pub warship_count: u32,
}

/// `frontend_api::units::get_civilians` for one nation.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CiviliansVm {
    pub deployed: Vec<CivilianEntry>,
    pub undeployed: Vec<CivilianEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CivilianEntry {
    pub id: i64,
    #[serde(rename = "type")]
    pub civ_type: String,
    pub position: Option<HexRef>,
    pub working: bool,
    pub turns_remaining: u32,
    #[serde(default)]
    pub build_task: Option<String>,
}

pub fn parse_map_tiles(value: serde_json::Value) -> Result<Vec<MapTile>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_navy_markers(value: serde_json::Value) -> Result<Vec<NavyMarker>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_sea_zones(value: serde_json::Value) -> Result<Vec<SeaZone>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_diplomacy_overlay(
    value: serde_json::Value,
) -> Result<DiplomacyOverlay, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_military_overlay(
    value: serde_json::Value,
) -> Result<Vec<MilitaryOverlayEntry>, serde_json::Error> {
    serde_json::from_value(value)
}

pub fn parse_civilians(value: serde_json::Value) -> Result<CiviliansVm, serde_json::Error> {
    serde_json::from_value(value)
}
