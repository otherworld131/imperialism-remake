pub mod generator;
pub mod hex_map;
pub mod infrastructure;
mod province;
pub mod tile;

pub use generator::{GeneratedMap, NationSetup, generate_map, validate_map};
pub use hex_map::HexMap;
pub use infrastructure::{
    build_depot, build_fort, build_port, build_railroad, fort_cost, is_province_connected,
    railroad_cost,
};
pub use province::{Province, SettlementLevel, compute_coastal, provinces_are_adjacent};
pub use tile::{Infrastructure, Tile, UnitId};
