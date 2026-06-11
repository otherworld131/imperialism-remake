//! Parser helpers shared by frontend API entry points.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals.

use domain::economy::buildings::BuildingType;
use domain::events::TreatyType;
use domain::military::ships::ShipType;
use domain::military::units::ArmyUnitType;
use domain::types::*;

pub fn parse_army_unit_type(name: &str) -> Option<ArmyUnitType> {
    name.parse().ok()
}

pub fn parse_ship_type(name: &str) -> Option<ShipType> {
    match name {
        "Trader" => Some(ShipType::Trader),
        "Indiaman" => Some(ShipType::Indiaman),
        "Clipper" => Some(ShipType::Clipper),
        "Paddlewheeler" => Some(ShipType::Paddlewheeler),
        "Freighter" => Some(ShipType::Freighter),
        "Frigate" => Some(ShipType::Frigate),
        "ShipOfTheLine" => Some(ShipType::ShipOfTheLine),
        "Raider" => Some(ShipType::Raider),
        "Ironclad" => Some(ShipType::Ironclad),
        "AdvancedIronclad" => Some(ShipType::AdvancedIronclad),
        "ArmouredCruiser" => Some(ShipType::ArmouredCruiser),
        "Dreadnought" => Some(ShipType::Dreadnought),
        "Battlecruiser" => Some(ShipType::Battlecruiser),
        _ => None,
    }
}

pub fn parse_resource_type(name: &str) -> Option<ResourceType> {
    match name {
        "Timber" => Some(ResourceType::Timber),
        "Coal" => Some(ResourceType::Coal),
        "Iron" => Some(ResourceType::Iron),
        "Cotton" => Some(ResourceType::Cotton),
        "Wool" => Some(ResourceType::Wool),
        "Grain" => Some(ResourceType::Grain),
        "Fruit" => Some(ResourceType::Fruit),
        "Livestock" => Some(ResourceType::Livestock),
        "Horses" => Some(ResourceType::Horses),
        "Oil" => Some(ResourceType::Oil),
        "Gold" => Some(ResourceType::Gold),
        "Gems" => Some(ResourceType::Gems),
        _ => None,
    }
}

pub fn parse_freight_target(name: &str) -> Option<domain::economy::FreightTarget> {
    if let Some(r) = parse_resource_type(name) {
        return Some(domain::economy::FreightTarget::Resource(r));
    }
    if let Ok(m) = name.parse::<MaterialType>() {
        return Some(domain::economy::FreightTarget::Material(m));
    }
    if let Ok(g) = name.parse::<GoodsType>() {
        return Some(domain::economy::FreightTarget::Goods(g));
    }
    None
}

pub fn parse_building_type(name: &str) -> Option<BuildingType> {
    match name {
        "Armory" => Some(BuildingType::Armory),
        "Capitol" => Some(BuildingType::Capitol),
        "FoodProcessing" => Some(BuildingType::FoodProcessing),
        "Railyard" => Some(BuildingType::Railyard),
        "Shipyard" => Some(BuildingType::Shipyard),
        "TradeSchool" => Some(BuildingType::TradeSchool),
        "University" => Some(BuildingType::University),
        "Warehouse" => Some(BuildingType::Warehouse),
        "LumberMill" => Some(BuildingType::LumberMill),
        "SteelMill" => Some(BuildingType::SteelMill),
        "TextileMill" => Some(BuildingType::TextileMill),
        "FurnitureFactory" => Some(BuildingType::FurnitureFactory),
        "HardwareFactory" => Some(BuildingType::HardwareFactory),
        "ClothingFactory" => Some(BuildingType::ClothingFactory),
        "PaperFactory" => Some(BuildingType::PaperFactory),
        "OilRefinery" => Some(BuildingType::OilRefinery),
        "PowerPlant" => Some(BuildingType::PowerPlant),
        _ => None,
    }
}

pub fn parse_treaty_type(name: &str) -> Option<TreatyType> {
    match name {
        "Alliance" => Some(TreatyType::Alliance),
        "NonAggressionPact" => Some(TreatyType::NonAggressionPact),
        "PeaceTreaty" => Some(TreatyType::PeaceTreaty),
        "RequestToJoinEmpire" => Some(TreatyType::RequestToJoinEmpire),
        "WarDeclaration" => Some(TreatyType::WarDeclaration),
        _ => None,
    }
}
