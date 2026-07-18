pub mod bounds;
pub mod generator;
pub mod hex_map;
pub mod infrastructure;
mod province;
pub mod sea_zones;
pub mod tile;

pub use bounds::MapBounds;
pub use generator::{
    GeneratedMap, MapGenConfig, NationSetup, TerrainMix, generate_map, generate_map_with_config,
    validate_map,
};
pub use hex_map::{HexMap, RailLink, canonical_link};
pub use infrastructure::{
    build_depot, build_fort, build_port, build_rail_link, fort_cost, is_province_connected,
    rail_link_cost, rail_reach, railroad_cost,
};
pub use province::{Province, SettlementLevel, compute_coastal, provinces_are_adjacent};
pub use sea_zones::{SeaZone, SeaZoneId, assign_coastal_provinces, compute_sea_zones};
pub use tile::{Infrastructure, Tile, UnitId};
