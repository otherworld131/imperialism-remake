//! Converts RON definition structs into domain types.

use super::definitions::*;
use crate::economy::buildings::BuildingType;
use crate::economy::civilians::CivilianType;
use crate::events::TechId;
use crate::military::ships::{ShipCategory, ShipStats, ShipType};
use crate::military::units::{ArmyUnitType, UnitCategory, UnitStats};
use crate::tech::tree::{TechEffect, TechTree, Technology};
use crate::types::Money;
use std::collections::HashMap;

/// Parse a RON technology definitions string into a TechTree.
pub fn load_tech_tree(ron_str: &str) -> Result<TechTree, String> {
    let defs: TechDefsFile =
        ron::from_str(ron_str).map_err(|e| format!("Failed to parse technologies RON: {}", e))?;

    let technologies: Vec<Technology> = defs
        .technologies
        .into_iter()
        .map(|def| Technology {
            id: TechId(def.id),
            name: def.name,
            cost: Money::dollars(def.cost),
            earliest_year: def.earliest_year,
            latest_year: def.latest_year,
            prerequisites: def.prerequisites.into_iter().map(TechId).collect(),
            effects: def.effects.into_iter().map(convert_tech_effect).collect(),
        })
        .collect();

    Ok(TechTree::from_technologies(technologies))
}

/// Parse a RON ship definitions string into a HashMap of ShipType → ShipStats.
pub fn load_ship_stats(ron_str: &str) -> Result<HashMap<ShipType, ShipStats>, String> {
    let defs: ShipDefsFile =
        ron::from_str(ron_str).map_err(|e| format!("Failed to parse ships RON: {}", e))?;

    let mut map = HashMap::new();
    for def in defs.ships {
        if def.hull == 0 {
            return Err(format!("Ship '{}' has zero hull", def.name));
        }
        let ship_type = match def.name.as_str() {
            "Trader" => ShipType::Trader,
            "Indiaman" => ShipType::Indiaman,
            "Clipper" => ShipType::Clipper,
            "Paddlewheeler" => ShipType::Paddlewheeler,
            "Freighter" => ShipType::Freighter,
            "Frigate" => ShipType::Frigate,
            "Ship-of-the-Line" => ShipType::ShipOfTheLine,
            "Raider" => ShipType::Raider,
            "Ironclad" => ShipType::Ironclad,
            "Advanced Ironclad" => ShipType::AdvancedIronclad,
            "Armoured Cruiser" => ShipType::ArmouredCruiser,
            "Dreadnought" => ShipType::Dreadnought,
            "Battlecruiser" => ShipType::Battlecruiser,
            other => return Err(format!("Unknown ship type: {}", other)),
        };
        let category = match def.category.as_str() {
            "Merchant" => ShipCategory::Merchant,
            "Warship" => ShipCategory::Warship,
            other => return Err(format!("Unknown ship category: {}", other)),
        };
        map.insert(
            ship_type,
            ShipStats {
                firepower: def.firepower,
                range: def.range,
                armor: def.armor,
                hull: def.hull,
                speed: def.speed,
                cargo: def.cargo,
                category,
                fabric_cost: def.fabric_cost,
                lumber_cost: def.lumber_cost,
                arms_cost: def.arms_cost,
                steel_cost: def.steel_cost,
                coal_cost: def.coal_cost,
                prerequisite_tech: def.prerequisite_tech,
            },
        );
    }
    Ok(map)
}

/// Parse a RON unit definitions string into a HashMap of ArmyUnitType → UnitStats.
pub fn load_unit_stats(ron_str: &str) -> Result<HashMap<ArmyUnitType, UnitStats>, String> {
    let defs: UnitDefsFile =
        ron::from_str(ron_str).map_err(|e| format!("Failed to parse units RON: {}", e))?;

    let mut map = HashMap::new();
    for def in defs.units {
        if def.cost < 0 {
            return Err(format!(
                "Unit '{}' has negative cost: {}",
                def.name, def.cost
            ));
        }
        if def.maintenance_per_turn < 0 {
            return Err(format!(
                "Unit '{}' has negative maintenance: {}",
                def.name, def.maintenance_per_turn
            ));
        }
        let unit_type = match def.name.as_str() {
            "Militia" => ArmyUnitType::Militia,
            "Regulars" => ArmyUnitType::Regulars,
            "Grenadiers" => ArmyUnitType::Grenadiers,
            "RifleInfantry" => ArmyUnitType::RifleInfantry,
            "Guards" => ArmyUnitType::Guards,
            "Sharpshooters" => ArmyUnitType::Sharpshooters,
            "ModernInfantry" => ArmyUnitType::ModernInfantry,
            "MachineGunners" => ArmyUnitType::MachineGunners,
            "Rangers" => ArmyUnitType::Rangers,
            "Cuirassiers" => ArmyUnitType::Cuirassiers,
            "Scouts" => ArmyUnitType::Scouts,
            "CarbineCavalry" => ArmyUnitType::CarbineCavalry,
            "Armour" => ArmyUnitType::Armour,
            "Mechanised" => ArmyUnitType::Mechanised,
            "LightArtillery" => ArmyUnitType::LightArtillery,
            "StandardArtillery" => ArmyUnitType::StandardArtillery,
            "FieldArtillery" => ArmyUnitType::FieldArtillery,
            "SiegeArtillery" => ArmyUnitType::SiegeArtillery,
            "RailroadGun" => ArmyUnitType::RailroadGun,
            "MobileArtillery" => ArmyUnitType::MobileArtillery,
            "Sapper" => ArmyUnitType::Sapper,
            "General" => ArmyUnitType::General,
            other => return Err(format!("Unknown unit type: {}", other)),
        };
        let category = match def.category.as_str() {
            "Garrison" => UnitCategory::Garrison,
            "Infantry" => UnitCategory::Infantry,
            "Cavalry" => UnitCategory::Cavalry,
            "Artillery" => UnitCategory::Artillery,
            "Special" => UnitCategory::Special,
            other => return Err(format!("Unknown unit category: {}", other)),
        };
        map.insert(
            unit_type,
            UnitStats {
                firepower: def.firepower,
                movement: def.movement,
                range: def.range,
                cost: Money::dollars(def.cost),
                arms_required: def.arms_required,
                requires_horse: def.requires_horse,
                category,
                maintenance_per_turn: Money::dollars(def.maintenance_per_turn),
                prerequisite_tech: def.prerequisite_tech,
            },
        );
    }
    Ok(map)
}

fn convert_tech_effect(def: TechEffectDef) -> TechEffect {
    match def {
        TechEffectDef::UnlockUnit(name) => TechEffect::UnlockUnit(
            name.parse::<ArmyUnitType>().unwrap_or_else(|e| panic!("tech data error: {}", e)),
        ),
        TechEffectDef::UnlockBuilding(name) => TechEffect::UnlockBuilding(
            name.parse::<BuildingType>().unwrap_or_else(|e| panic!("tech data error: {}", e)),
        ),
        TechEffectDef::EnableTerrainImprovement { terrain, max_level } => {
            TechEffect::EnableTerrainImprovement { terrain, max_level }
        }
        TechEffectDef::EnableInfrastructure(name) => TechEffect::EnableInfrastructure(name),
        TechEffectDef::UnlockShip(name) => TechEffect::UnlockShip(name),
        TechEffectDef::UpgradeUnit { from, to } => TechEffect::UpgradeUnit {
            from: from.parse::<ArmyUnitType>().unwrap_or_else(|e| panic!("tech data error: {}", e)),
            to: to.parse::<ArmyUnitType>().unwrap_or_else(|e| panic!("tech data error: {}", e)),
        },
        TechEffectDef::EnableCivilian(name) => TechEffect::EnableCivilian(
            name.parse::<CivilianType>().unwrap_or_else(|e| panic!("tech data error: {}", e)),
        ),
        TechEffectDef::LuaScript(script) => TechEffect::LuaScript(script),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_tech_tree_from_ron_string() {
        let ron = r#"(
            technologies: [
                (
                    id: 1,
                    name: "Test Tech",
                    cost: 1000,
                    earliest_year: 1815,
                    latest_year: 1820,
                    prerequisites: [],
                    effects: [UnlockUnit("Regulars")],
                ),
            ],
        )"#;
        let tree = load_tech_tree(ron).unwrap();
        assert_eq!(tree.all_techs().len(), 1);
        assert_eq!(tree.all_techs()[0].name, "Test Tech");
    }

    #[test]
    fn load_tech_tree_with_prerequisites() {
        let ron = r#"(
            technologies: [
                (
                    id: 1,
                    name: "Base Tech",
                    cost: 0,
                    earliest_year: 1815,
                    latest_year: 1815,
                    prerequisites: [],
                    effects: [],
                ),
                (
                    id: 2,
                    name: "Advanced Tech",
                    cost: 5000,
                    earliest_year: 1820,
                    latest_year: 1825,
                    prerequisites: [1],
                    effects: [UnlockShip("Ironclad"), EnableTerrainImprovement(terrain: "Farm", max_level: 2)],
                ),
            ],
        )"#;
        let tree = load_tech_tree(ron).unwrap();
        assert_eq!(tree.all_techs().len(), 2);
        assert_eq!(tree.all_techs()[1].prerequisites, vec![TechId(1)]);
    }

    #[test]
    fn load_tech_tree_invalid_ron_returns_error() {
        let result = load_tech_tree("not valid ron");
        assert!(result.is_err());
    }

    #[test]
    fn ron_tech_tree_matches_hardcoded() {
        let ron_content = include_str!("../../../../data/definitions/technologies.ron");
        let from_ron = load_tech_tree(ron_content).unwrap();
        let hardcoded = TechTree::new();

        assert_eq!(from_ron.all_techs().len(), hardcoded.all_techs().len());
        for (r, h) in from_ron
            .all_techs()
            .iter()
            .zip(hardcoded.all_techs().iter())
        {
            assert_eq!(r.id, h.id, "ID mismatch for tech {}", h.name);
            assert_eq!(r.name, h.name);
            assert_eq!(r.cost, h.cost, "Cost mismatch for {}", h.name);
            assert_eq!(r.earliest_year, h.earliest_year);
            assert_eq!(r.latest_year, h.latest_year);
            assert_eq!(r.prerequisites, h.prerequisites);
            assert_eq!(r.effects, h.effects, "Effects mismatch for {}", h.name);
        }
    }

    #[test]
    fn ron_ship_stats_match_hardcoded() {
        let ron_content = include_str!("../../../../data/definitions/ships.ron");
        let from_ron = load_ship_stats(ron_content).unwrap();

        // Verify all 13 ship types loaded
        assert_eq!(from_ron.len(), 13);

        // Verify each matches hardcoded
        for (ship_type, ron_stats) in &from_ron {
            let hardcoded = ship_type.stats();
            assert_eq!(
                ron_stats.firepower, hardcoded.firepower,
                "Firepower mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.range, hardcoded.range,
                "Range mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.armor, hardcoded.armor,
                "Armor mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.hull, hardcoded.hull,
                "Hull mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.speed, hardcoded.speed,
                "Speed mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.cargo, hardcoded.cargo,
                "Cargo mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.fabric_cost, hardcoded.fabric_cost,
                "Fabric cost mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.lumber_cost, hardcoded.lumber_cost,
                "Lumber cost mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.arms_cost, hardcoded.arms_cost,
                "Arms cost mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.steel_cost, hardcoded.steel_cost,
                "Steel cost mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.coal_cost, hardcoded.coal_cost,
                "Coal cost mismatch for {:?}",
                ship_type
            );
            assert_eq!(
                ron_stats.prerequisite_tech, hardcoded.prerequisite_tech,
                "Tech mismatch for {:?}",
                ship_type
            );
        }
    }

    #[test]
    fn load_unit_stats_rejects_negative_cost() {
        let ron = r#"(units: [(name: "Militia", category: "Garrison", firepower: 1, movement: 0, range: 1, cost: -100, arms_required: 1, maintenance_per_turn: 25, prerequisite_tech: None)])"#;
        let result = load_unit_stats(ron);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("negative cost"));
    }

    #[test]
    fn load_unit_stats_rejects_negative_maintenance() {
        let ron = r#"(units: [(name: "Militia", category: "Garrison", firepower: 1, movement: 0, range: 1, cost: 50, arms_required: 1, maintenance_per_turn: -10, prerequisite_tech: None)])"#;
        let result = load_unit_stats(ron);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("negative maintenance"));
    }

    #[test]
    fn load_ship_stats_rejects_zero_hull() {
        let ron = r#"(ships: [(name: "Frigate", category: "Warship", firepower: 3, range: 2, armor: 2, hull: 0, speed: 3, cargo: 0, fabric_cost: 2, lumber_cost: 5, arms_cost: 3, steel_cost: 0, coal_cost: 0, prerequisite_tech: None)])"#;
        let result = load_ship_stats(ron);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("zero hull"));
    }

    #[test]
    fn ron_unit_stats_match_hardcoded() {
        let ron_content = include_str!("../../../../data/definitions/units.ron");
        let from_ron = load_unit_stats(ron_content).unwrap();

        // Verify all 22 unit types loaded
        assert_eq!(from_ron.len(), 22);

        // Verify each matches hardcoded
        for (unit_type, ron_stats) in &from_ron {
            let hardcoded = unit_type.stats();
            assert_eq!(
                ron_stats.firepower, hardcoded.firepower,
                "Firepower mismatch for {:?}",
                unit_type
            );
            assert_eq!(
                ron_stats.movement, hardcoded.movement,
                "Movement mismatch for {:?}",
                unit_type
            );
            assert_eq!(
                ron_stats.range, hardcoded.range,
                "Range mismatch for {:?}",
                unit_type
            );
            assert_eq!(
                ron_stats.cost, hardcoded.cost,
                "Cost mismatch for {:?}",
                unit_type
            );
            assert_eq!(
                ron_stats.arms_required, hardcoded.arms_required,
                "Arms mismatch for {:?}",
                unit_type
            );
            assert_eq!(
                ron_stats.requires_horse, hardcoded.requires_horse,
                "Horse mismatch for {:?}",
                unit_type
            );
            assert_eq!(
                ron_stats.maintenance_per_turn, hardcoded.maintenance_per_turn,
                "Maintenance mismatch for {:?}",
                unit_type
            );
            assert_eq!(
                ron_stats.prerequisite_tech, hardcoded.prerequisite_tech,
                "Tech mismatch for {:?}",
                unit_type
            );
        }
    }
}
