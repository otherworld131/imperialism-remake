use crate::map::UnitId;
use crate::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShipCategory {
    Merchant,
    Warship,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShipType {
    // Merchant
    Trader,
    Indiaman,
    Clipper,
    Paddlewheeler,
    Freighter,
    // Warship
    Frigate,
    ShipOfTheLine,
    Raider,
    Ironclad,
    AdvancedIronclad,
    ArmouredCruiser,
    Dreadnought,
    Battlecruiser,
}

#[derive(Debug, Clone)]
pub struct ShipStats {
    pub firepower: u32,
    pub range: u32,
    pub armor: u32,
    pub hull: u32,
    pub speed: u32,
    pub cargo: u32,
    pub category: ShipCategory,
    // Construction costs
    pub fabric_cost: u32,
    pub lumber_cost: u32,
    pub arms_cost: u32,
    pub steel_cost: u32,
    pub coal_cost: u32,
    pub prerequisite_tech: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: UnitId,
    pub ship_type: ShipType,
    pub owner: NationId,
    pub hull_remaining: u32,
    pub sea_zone: Option<u32>,
    /// Current naval operation assignment (Patrol, Blockade, Beachhead, etc.).
    pub operation: Option<crate::military::naval::NavalOperation>,
}

impl ShipType {
    /// Returns the base stats for each ship type.
    pub fn stats(&self) -> ShipStats {
        match self {
            // ── Merchant ships ─────────────────────────────────────
            ShipType::Trader => ShipStats {
                firepower: 0,
                range: 0,
                armor: 0,
                hull: 25,
                speed: 0,
                cargo: 2,
                category: ShipCategory::Merchant,
                fabric_cost: 2,
                lumber_cost: 4,
                arms_cost: 0,
                steel_cost: 0,
                coal_cost: 0,
                prerequisite_tech: None,
            },
            ShipType::Indiaman => ShipStats {
                firepower: 0,
                range: 0,
                armor: 5,
                hull: 40,
                speed: 0,
                cargo: 4,
                category: ShipCategory::Merchant,
                fabric_cost: 3,
                lumber_cost: 7,
                arms_cost: 0,
                steel_cost: 0,
                coal_cost: 0,
                prerequisite_tech: None,
            },
            ShipType::Clipper => ShipStats {
                firepower: 0,
                range: 0,
                armor: 0,
                hull: 25,
                speed: 0,
                cargo: 4,
                category: ShipCategory::Merchant,
                fabric_cost: 2,
                lumber_cost: 6,
                arms_cost: 0,
                steel_cost: 0,
                coal_cost: 0,
                prerequisite_tech: Some("Streamlined Hulls".to_string()),
            },
            ShipType::Paddlewheeler => ShipStats {
                firepower: 0,
                range: 0,
                armor: 5,
                hull: 35,
                speed: 0,
                cargo: 8,
                category: ShipCategory::Merchant,
                fabric_cost: 0,
                lumber_cost: 6,
                arms_cost: 0,
                steel_cost: 2,
                coal_cost: 10,
                prerequisite_tech: Some("Paddlewheels".to_string()),
            },
            ShipType::Freighter => ShipStats {
                firepower: 0,
                range: 0,
                armor: 10,
                hull: 50,
                speed: 0,
                cargo: 12,
                category: ShipCategory::Merchant,
                fabric_cost: 0,
                lumber_cost: 8,
                arms_cost: 0,
                steel_cost: 4,
                coal_cost: 15,
                prerequisite_tech: Some("Marine Engineering".to_string()),
            },

            // ── Warships ───────────────────────────────────────────
            ShipType::Frigate => ShipStats {
                firepower: 3,
                range: 5,
                armor: 10,
                hull: 35,
                speed: 4,
                cargo: 0,
                category: ShipCategory::Warship,
                fabric_cost: 2,
                lumber_cost: 5,
                arms_cost: 2,
                steel_cost: 0,
                coal_cost: 0,
                prerequisite_tech: None,
            },
            ShipType::ShipOfTheLine => ShipStats {
                firepower: 6,
                range: 6,
                armor: 20,
                hull: 65,
                speed: 3,
                cargo: 0,
                category: ShipCategory::Warship,
                fabric_cost: 3,
                lumber_cost: 8,
                arms_cost: 5,
                steel_cost: 0,
                coal_cost: 0,
                prerequisite_tech: None,
            },
            ShipType::Raider => ShipStats {
                firepower: 3,
                range: 7,
                armor: 20,
                hull: 30,
                speed: 7,
                cargo: 0,
                category: ShipCategory::Warship,
                fabric_cost: 0,
                lumber_cost: 6,
                arms_cost: 3,
                steel_cost: 0,
                coal_cost: 10,
                prerequisite_tech: Some("Paddlewheels".to_string()),
            },
            ShipType::Ironclad => ShipStats {
                firepower: 8,
                range: 7,
                armor: 30,
                hull: 50,
                speed: 5,
                cargo: 0,
                category: ShipCategory::Warship,
                fabric_cost: 0,
                lumber_cost: 6,
                arms_cost: 4,
                steel_cost: 3,
                coal_cost: 12,
                prerequisite_tech: Some("Advanced Iron Working".to_string()),
            },
            ShipType::AdvancedIronclad => ShipStats {
                firepower: 10,
                range: 8,
                armor: 40,
                hull: 60,
                speed: 5,
                cargo: 0,
                category: ShipCategory::Warship,
                fabric_cost: 0,
                lumber_cost: 6,
                arms_cost: 5,
                steel_cost: 4,
                coal_cost: 15,
                prerequisite_tech: Some("Steel Armour Plate".to_string()),
            },
            ShipType::ArmouredCruiser => ShipStats {
                firepower: 8,
                range: 9,
                armor: 35,
                hull: 55,
                speed: 7,
                cargo: 0,
                category: ShipCategory::Warship,
                fabric_cost: 0,
                lumber_cost: 7,
                arms_cost: 4,
                steel_cost: 5,
                coal_cost: 15,
                prerequisite_tech: Some("Marine Engineering".to_string()),
            },
            ShipType::Dreadnought => ShipStats {
                firepower: 15,
                range: 10,
                armor: 50,
                hull: 80,
                speed: 6,
                cargo: 0,
                category: ShipCategory::Warship,
                fabric_cost: 0,
                lumber_cost: 10,
                arms_cost: 8,
                steel_cost: 8,
                coal_cost: 20,
                prerequisite_tech: Some("Improved Range-Finding".to_string()),
            },
            ShipType::Battlecruiser => ShipStats {
                firepower: 12,
                range: 10,
                armor: 40,
                hull: 65,
                speed: 8,
                cargo: 0,
                category: ShipCategory::Warship,
                fabric_cost: 0,
                lumber_cost: 8,
                arms_cost: 6,
                steel_cost: 6,
                coal_cost: 18,
                prerequisite_tech: Some("Improved Range-Finding".to_string()),
            },
        }
    }

    /// Returns the ship category for this ship type.
    pub fn category(&self) -> ShipCategory {
        match self {
            ShipType::Trader
            | ShipType::Indiaman
            | ShipType::Clipper
            | ShipType::Paddlewheeler
            | ShipType::Freighter => ShipCategory::Merchant,

            ShipType::Frigate
            | ShipType::ShipOfTheLine
            | ShipType::Raider
            | ShipType::Ironclad
            | ShipType::AdvancedIronclad
            | ShipType::ArmouredCruiser
            | ShipType::Dreadnought
            | ShipType::Battlecruiser => ShipCategory::Warship,
        }
    }

    /// Returns the ship type that obsoletes this one, if any.
    ///
    /// Obsolescence chains:
    /// - Merchant: Trader -> Clipper -> Freighter
    /// - Merchant: Indiaman -> Paddlewheeler -> Freighter
    /// - Warship: Frigate -> Ironclad
    /// - Warship: ShipOfTheLine -> AdvancedIronclad
    /// - Warship: Raider -> ArmouredCruiser
    /// - Warship: Ironclad -> Dreadnought
    /// - Warship: AdvancedIronclad -> Dreadnought
    /// - Warship: ArmouredCruiser -> Battlecruiser
    pub fn is_obsoleted_by(&self) -> Option<ShipType> {
        match self {
            // Merchant obsolescence
            ShipType::Trader => Some(ShipType::Clipper),
            ShipType::Indiaman => Some(ShipType::Paddlewheeler),
            ShipType::Clipper => Some(ShipType::Freighter),
            ShipType::Paddlewheeler => Some(ShipType::Freighter),

            // Warship obsolescence
            ShipType::Frigate => Some(ShipType::Ironclad),
            ShipType::ShipOfTheLine => Some(ShipType::AdvancedIronclad),
            ShipType::Raider => Some(ShipType::ArmouredCruiser),
            ShipType::Ironclad => Some(ShipType::Dreadnought),
            ShipType::AdvancedIronclad => Some(ShipType::Dreadnought),
            ShipType::ArmouredCruiser => Some(ShipType::Battlecruiser),

            // End-of-line ships
            ShipType::Freighter => None,
            ShipType::Dreadnought => None,
            ShipType::Battlecruiser => None,
        }
    }
}

impl std::str::FromStr for ShipType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Trader" => Ok(Self::Trader),
            "Indiaman" => Ok(Self::Indiaman),
            "Clipper" => Ok(Self::Clipper),
            "Paddlewheeler" => Ok(Self::Paddlewheeler),
            "Freighter" => Ok(Self::Freighter),
            "Frigate" => Ok(Self::Frigate),
            "ShipOfTheLine" => Ok(Self::ShipOfTheLine),
            "Raider" => Ok(Self::Raider),
            "Ironclad" => Ok(Self::Ironclad),
            "AdvancedIronclad" => Ok(Self::AdvancedIronclad),
            "ArmouredCruiser" => Ok(Self::ArmouredCruiser),
            "Dreadnought" => Ok(Self::Dreadnought),
            "Battlecruiser" => Ok(Self::Battlecruiser),
            other => Err(format!("unknown ship type: {}", other)),
        }
    }
}

impl Ship {
    /// Create a new ship with full hull and no sea zone assignment.
    pub fn new(id: UnitId, ship_type: ShipType, owner: NationId) -> Self {
        let stats = ship_type.stats();
        Self {
            id,
            ship_type,
            owner,
            hull_remaining: stats.hull,
            sea_zone: None,
            operation: None,
        }
    }

    /// Reduce hull by the given amount, saturating at zero.
    pub fn take_damage(&mut self, amount: u32) {
        self.hull_remaining = self.hull_remaining.saturating_sub(amount);
    }

    /// Returns true if the ship has been sunk (hull == 0).
    pub fn is_sunk(&self) -> bool {
        self.hull_remaining == 0
    }

    /// Returns the total cargo capacity for this ship.
    pub fn total_cargo_capacity(&self) -> u32 {
        self.ship_type.stats().cargo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Stats verification ──────────────────────────────────────

    #[test]
    fn trader_stats() {
        let stats = ShipType::Trader.stats();
        assert_eq!(stats.firepower, 0);
        assert_eq!(stats.range, 0);
        assert_eq!(stats.armor, 0);
        assert_eq!(stats.hull, 25);
        assert_eq!(stats.speed, 0);
        assert_eq!(stats.cargo, 2);
        assert_eq!(stats.category, ShipCategory::Merchant);
        assert_eq!(stats.fabric_cost, 2);
        assert_eq!(stats.lumber_cost, 4);
        assert_eq!(stats.arms_cost, 0);
        assert_eq!(stats.steel_cost, 0);
        assert_eq!(stats.coal_cost, 0);
        assert!(stats.prerequisite_tech.is_none());
    }

    #[test]
    fn indiaman_stats() {
        let stats = ShipType::Indiaman.stats();
        assert_eq!(stats.firepower, 0);
        assert_eq!(stats.range, 0);
        assert_eq!(stats.armor, 5);
        assert_eq!(stats.hull, 40);
        assert_eq!(stats.speed, 0);
        assert_eq!(stats.cargo, 4);
        assert_eq!(stats.category, ShipCategory::Merchant);
        assert_eq!(stats.fabric_cost, 3);
        assert_eq!(stats.lumber_cost, 7);
        assert!(stats.prerequisite_tech.is_none());
    }

    #[test]
    fn clipper_stats() {
        let stats = ShipType::Clipper.stats();
        assert_eq!(stats.hull, 25);
        assert_eq!(stats.cargo, 4);
        assert_eq!(stats.fabric_cost, 2);
        assert_eq!(stats.lumber_cost, 6);
        assert_eq!(
            stats.prerequisite_tech,
            Some("Streamlined Hulls".to_string())
        );
    }

    #[test]
    fn paddlewheeler_stats() {
        let stats = ShipType::Paddlewheeler.stats();
        assert_eq!(stats.armor, 5);
        assert_eq!(stats.hull, 35);
        assert_eq!(stats.cargo, 8);
        assert_eq!(stats.fabric_cost, 0);
        assert_eq!(stats.lumber_cost, 6);
        assert_eq!(stats.steel_cost, 2);
        assert_eq!(stats.coal_cost, 10);
        assert_eq!(stats.prerequisite_tech, Some("Paddlewheels".to_string()));
    }

    #[test]
    fn freighter_stats() {
        let stats = ShipType::Freighter.stats();
        assert_eq!(stats.armor, 10);
        assert_eq!(stats.hull, 50);
        assert_eq!(stats.cargo, 12);
        assert_eq!(stats.steel_cost, 4);
        assert_eq!(stats.coal_cost, 15);
        assert_eq!(
            stats.prerequisite_tech,
            Some("Marine Engineering".to_string())
        );
    }

    #[test]
    fn frigate_stats() {
        let stats = ShipType::Frigate.stats();
        assert_eq!(stats.firepower, 3);
        assert_eq!(stats.range, 5);
        assert_eq!(stats.armor, 10);
        assert_eq!(stats.hull, 35);
        assert_eq!(stats.speed, 4);
        assert_eq!(stats.cargo, 0);
        assert_eq!(stats.category, ShipCategory::Warship);
        assert_eq!(stats.fabric_cost, 2);
        assert_eq!(stats.lumber_cost, 5);
        assert_eq!(stats.arms_cost, 2);
        assert!(stats.prerequisite_tech.is_none());
    }

    #[test]
    fn ship_of_the_line_stats() {
        let stats = ShipType::ShipOfTheLine.stats();
        assert_eq!(stats.firepower, 6);
        assert_eq!(stats.range, 6);
        assert_eq!(stats.armor, 20);
        assert_eq!(stats.hull, 65);
        assert_eq!(stats.speed, 3);
        assert_eq!(stats.arms_cost, 5);
        assert!(stats.prerequisite_tech.is_none());
    }

    #[test]
    fn raider_stats() {
        let stats = ShipType::Raider.stats();
        assert_eq!(stats.firepower, 3);
        assert_eq!(stats.range, 7);
        assert_eq!(stats.armor, 20);
        assert_eq!(stats.hull, 30);
        assert_eq!(stats.speed, 7);
        assert_eq!(stats.coal_cost, 10);
        assert_eq!(stats.prerequisite_tech, Some("Paddlewheels".to_string()));
    }

    #[test]
    fn ironclad_stats() {
        let stats = ShipType::Ironclad.stats();
        assert_eq!(stats.firepower, 8);
        assert_eq!(stats.range, 7);
        assert_eq!(stats.armor, 30);
        assert_eq!(stats.hull, 50);
        assert_eq!(stats.speed, 5);
        assert_eq!(stats.arms_cost, 4);
        assert_eq!(stats.steel_cost, 3);
        assert_eq!(stats.coal_cost, 12);
        assert_eq!(
            stats.prerequisite_tech,
            Some("Advanced Iron Working".to_string())
        );
    }

    #[test]
    fn advanced_ironclad_stats() {
        let stats = ShipType::AdvancedIronclad.stats();
        assert_eq!(stats.firepower, 10);
        assert_eq!(stats.range, 8);
        assert_eq!(stats.armor, 40);
        assert_eq!(stats.hull, 60);
        assert_eq!(stats.speed, 5);
        assert_eq!(stats.arms_cost, 5);
        assert_eq!(stats.steel_cost, 4);
        assert_eq!(stats.coal_cost, 15);
        assert_eq!(
            stats.prerequisite_tech,
            Some("Steel Armour Plate".to_string())
        );
    }

    #[test]
    fn armoured_cruiser_stats() {
        let stats = ShipType::ArmouredCruiser.stats();
        assert_eq!(stats.firepower, 8);
        assert_eq!(stats.range, 9);
        assert_eq!(stats.armor, 35);
        assert_eq!(stats.hull, 55);
        assert_eq!(stats.speed, 7);
        assert_eq!(stats.steel_cost, 5);
        assert_eq!(stats.coal_cost, 15);
        assert_eq!(
            stats.prerequisite_tech,
            Some("Marine Engineering".to_string())
        );
    }

    #[test]
    fn dreadnought_stats() {
        let stats = ShipType::Dreadnought.stats();
        assert_eq!(stats.firepower, 15);
        assert_eq!(stats.range, 10);
        assert_eq!(stats.armor, 50);
        assert_eq!(stats.hull, 80);
        assert_eq!(stats.speed, 6);
        assert_eq!(stats.lumber_cost, 10);
        assert_eq!(stats.arms_cost, 8);
        assert_eq!(stats.steel_cost, 8);
        assert_eq!(stats.coal_cost, 20);
        assert_eq!(
            stats.prerequisite_tech,
            Some("Improved Range-Finding".to_string())
        );
    }

    #[test]
    fn battlecruiser_stats() {
        let stats = ShipType::Battlecruiser.stats();
        assert_eq!(stats.firepower, 12);
        assert_eq!(stats.range, 10);
        assert_eq!(stats.armor, 40);
        assert_eq!(stats.hull, 65);
        assert_eq!(stats.speed, 8);
        assert_eq!(stats.lumber_cost, 8);
        assert_eq!(stats.arms_cost, 6);
        assert_eq!(stats.steel_cost, 6);
        assert_eq!(stats.coal_cost, 18);
        assert_eq!(
            stats.prerequisite_tech,
            Some("Improved Range-Finding".to_string())
        );
    }

    // ── Category ────────────────────────────────────────────────

    #[test]
    fn merchant_categories() {
        assert_eq!(ShipType::Trader.category(), ShipCategory::Merchant);
        assert_eq!(ShipType::Indiaman.category(), ShipCategory::Merchant);
        assert_eq!(ShipType::Clipper.category(), ShipCategory::Merchant);
        assert_eq!(ShipType::Paddlewheeler.category(), ShipCategory::Merchant);
        assert_eq!(ShipType::Freighter.category(), ShipCategory::Merchant);
    }

    #[test]
    fn warship_categories() {
        assert_eq!(ShipType::Frigate.category(), ShipCategory::Warship);
        assert_eq!(ShipType::ShipOfTheLine.category(), ShipCategory::Warship);
        assert_eq!(ShipType::Raider.category(), ShipCategory::Warship);
        assert_eq!(ShipType::Ironclad.category(), ShipCategory::Warship);
        assert_eq!(ShipType::AdvancedIronclad.category(), ShipCategory::Warship);
        assert_eq!(ShipType::ArmouredCruiser.category(), ShipCategory::Warship);
        assert_eq!(ShipType::Dreadnought.category(), ShipCategory::Warship);
        assert_eq!(ShipType::Battlecruiser.category(), ShipCategory::Warship);
    }

    #[test]
    fn category_matches_stats_category() {
        let all_types = [
            ShipType::Trader,
            ShipType::Indiaman,
            ShipType::Clipper,
            ShipType::Paddlewheeler,
            ShipType::Freighter,
            ShipType::Frigate,
            ShipType::ShipOfTheLine,
            ShipType::Raider,
            ShipType::Ironclad,
            ShipType::AdvancedIronclad,
            ShipType::ArmouredCruiser,
            ShipType::Dreadnought,
            ShipType::Battlecruiser,
        ];
        for ship_type in all_types {
            assert_eq!(
                ship_type.category(),
                ship_type.stats().category,
                "Category mismatch for {:?}",
                ship_type
            );
        }
    }

    // ── Obsolescence ────────────────────────────────────────────

    #[test]
    fn trader_obsoleted_by_clipper() {
        assert_eq!(ShipType::Trader.is_obsoleted_by(), Some(ShipType::Clipper));
    }

    #[test]
    fn indiaman_obsoleted_by_paddlewheeler() {
        assert_eq!(
            ShipType::Indiaman.is_obsoleted_by(),
            Some(ShipType::Paddlewheeler)
        );
    }

    #[test]
    fn clipper_obsoleted_by_freighter() {
        assert_eq!(
            ShipType::Clipper.is_obsoleted_by(),
            Some(ShipType::Freighter)
        );
    }

    #[test]
    fn paddlewheeler_obsoleted_by_freighter() {
        assert_eq!(
            ShipType::Paddlewheeler.is_obsoleted_by(),
            Some(ShipType::Freighter)
        );
    }

    #[test]
    fn freighter_not_obsoleted() {
        assert_eq!(ShipType::Freighter.is_obsoleted_by(), None);
    }

    #[test]
    fn ship_of_the_line_obsoleted_by_advanced_ironclad() {
        assert_eq!(
            ShipType::ShipOfTheLine.is_obsoleted_by(),
            Some(ShipType::AdvancedIronclad)
        );
    }

    #[test]
    fn frigate_obsoleted_by_ironclad() {
        assert_eq!(
            ShipType::Frigate.is_obsoleted_by(),
            Some(ShipType::Ironclad)
        );
    }

    #[test]
    fn raider_obsoleted_by_armoured_cruiser() {
        assert_eq!(
            ShipType::Raider.is_obsoleted_by(),
            Some(ShipType::ArmouredCruiser)
        );
    }

    #[test]
    fn ironclad_obsoleted_by_dreadnought() {
        assert_eq!(
            ShipType::Ironclad.is_obsoleted_by(),
            Some(ShipType::Dreadnought)
        );
    }

    #[test]
    fn advanced_ironclad_obsoleted_by_dreadnought() {
        assert_eq!(
            ShipType::AdvancedIronclad.is_obsoleted_by(),
            Some(ShipType::Dreadnought)
        );
    }

    #[test]
    fn armoured_cruiser_obsoleted_by_battlecruiser() {
        assert_eq!(
            ShipType::ArmouredCruiser.is_obsoleted_by(),
            Some(ShipType::Battlecruiser)
        );
    }

    #[test]
    fn dreadnought_not_obsoleted() {
        assert_eq!(ShipType::Dreadnought.is_obsoleted_by(), None);
    }

    #[test]
    fn battlecruiser_not_obsoleted() {
        assert_eq!(ShipType::Battlecruiser.is_obsoleted_by(), None);
    }

    // ── Ship construction & damage ──────────────────────────────

    #[test]
    fn new_ship_has_full_hull() {
        let ship = Ship::new(UnitId(1), ShipType::Frigate, NationId(1));
        assert_eq!(ship.hull_remaining, 35);
        assert!(!ship.is_sunk());
    }

    #[test]
    fn new_ship_has_no_sea_zone() {
        let ship = Ship::new(UnitId(1), ShipType::Trader, NationId(1));
        assert_eq!(ship.sea_zone, None);
    }

    #[test]
    fn new_ship_preserves_type_and_owner() {
        let ship = Ship::new(UnitId(42), ShipType::Dreadnought, NationId(3));
        assert_eq!(ship.id, UnitId(42));
        assert_eq!(ship.ship_type, ShipType::Dreadnought);
        assert_eq!(ship.owner, NationId(3));
    }

    #[test]
    fn take_damage_reduces_hull() {
        let mut ship = Ship::new(UnitId(1), ShipType::Frigate, NationId(1));
        assert_eq!(ship.hull_remaining, 35);
        ship.take_damage(10);
        assert_eq!(ship.hull_remaining, 25);
        assert!(!ship.is_sunk());
    }

    #[test]
    fn take_damage_saturates_at_zero() {
        let mut ship = Ship::new(UnitId(1), ShipType::Trader, NationId(1));
        assert_eq!(ship.hull_remaining, 25);
        ship.take_damage(100);
        assert_eq!(ship.hull_remaining, 0);
        assert!(ship.is_sunk());
    }

    #[test]
    fn take_exact_hull_damage_sinks() {
        let mut ship = Ship::new(UnitId(1), ShipType::Trader, NationId(1));
        ship.take_damage(25);
        assert_eq!(ship.hull_remaining, 0);
        assert!(ship.is_sunk());
    }

    #[test]
    fn incremental_damage_until_sunk() {
        let mut ship = Ship::new(UnitId(1), ShipType::ShipOfTheLine, NationId(1));
        assert_eq!(ship.hull_remaining, 65);

        ship.take_damage(20);
        assert_eq!(ship.hull_remaining, 45);
        assert!(!ship.is_sunk());

        ship.take_damage(20);
        assert_eq!(ship.hull_remaining, 25);
        assert!(!ship.is_sunk());

        ship.take_damage(20);
        assert_eq!(ship.hull_remaining, 5);
        assert!(!ship.is_sunk());

        ship.take_damage(20);
        assert_eq!(ship.hull_remaining, 0);
        assert!(ship.is_sunk());
    }

    // ── Cargo capacity ──────────────────────────────────────────

    #[test]
    fn merchant_ships_have_cargo() {
        assert_eq!(
            Ship::new(UnitId(1), ShipType::Trader, NationId(1)).total_cargo_capacity(),
            2
        );
        assert_eq!(
            Ship::new(UnitId(2), ShipType::Indiaman, NationId(1)).total_cargo_capacity(),
            4
        );
        assert_eq!(
            Ship::new(UnitId(3), ShipType::Clipper, NationId(1)).total_cargo_capacity(),
            4
        );
        assert_eq!(
            Ship::new(UnitId(4), ShipType::Paddlewheeler, NationId(1)).total_cargo_capacity(),
            8
        );
        assert_eq!(
            Ship::new(UnitId(5), ShipType::Freighter, NationId(1)).total_cargo_capacity(),
            12
        );
    }

    #[test]
    fn warships_have_no_cargo() {
        let warship_types = [
            ShipType::Frigate,
            ShipType::ShipOfTheLine,
            ShipType::Raider,
            ShipType::Ironclad,
            ShipType::AdvancedIronclad,
            ShipType::ArmouredCruiser,
            ShipType::Dreadnought,
            ShipType::Battlecruiser,
        ];
        for (i, ship_type) in warship_types.iter().enumerate() {
            let ship = Ship::new(UnitId(i as u32 + 10), *ship_type, NationId(1));
            assert_eq!(
                ship.total_cargo_capacity(),
                0,
                "{:?} should have no cargo",
                ship_type
            );
        }
    }

    // ── Merchants have zero firepower ───────────────────────────

    #[test]
    fn all_merchants_have_zero_firepower() {
        let merchant_types = [
            ShipType::Trader,
            ShipType::Indiaman,
            ShipType::Clipper,
            ShipType::Paddlewheeler,
            ShipType::Freighter,
        ];
        for ship_type in merchant_types {
            assert_eq!(
                ship_type.stats().firepower,
                0,
                "{:?} should have zero firepower",
                ship_type
            );
        }
    }

    // ── Hull values for all ship types ──────────────────────────

    #[test]
    fn all_hull_values_match_table() {
        let expected: &[(ShipType, u32)] = &[
            (ShipType::Trader, 25),
            (ShipType::Indiaman, 40),
            (ShipType::Clipper, 25),
            (ShipType::Paddlewheeler, 35),
            (ShipType::Freighter, 50),
            (ShipType::Frigate, 35),
            (ShipType::ShipOfTheLine, 65),
            (ShipType::Raider, 30),
            (ShipType::Ironclad, 50),
            (ShipType::AdvancedIronclad, 60),
            (ShipType::ArmouredCruiser, 55),
            (ShipType::Dreadnought, 80),
            (ShipType::Battlecruiser, 65),
        ];
        for &(ship_type, hull) in expected {
            let ship = Ship::new(UnitId(1), ship_type, NationId(1));
            assert_eq!(
                ship.hull_remaining, hull,
                "{:?} should start with hull {}",
                ship_type, hull
            );
        }
    }
}
