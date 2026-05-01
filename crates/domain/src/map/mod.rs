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
pub use hex_map::HexMap;
pub use infrastructure::{
    build_depot, build_fort, build_port, build_railroad, fort_cost, is_province_connected,
    railroad_cost,
};
pub use province::{Province, SettlementLevel, compute_coastal, provinces_are_adjacent};
pub use sea_zones::{SeaZone, SeaZoneId, assign_coastal_provinces, compute_sea_zones};
pub use tile::{Infrastructure, Tile, UnitId};
