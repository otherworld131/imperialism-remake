use crate::map::{UnitId, sea_zones::SeaZoneId};
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
    /// Historical era bucket (1–4). Used by the AI to prefer newer ships
    /// when tech allows, and by the UI for grouping.
    pub era: u8,
}

#[derive(Debug, Clone)]
pub struct Ship {
    pub id: UnitId,
    pub ship_type: ShipType,
    pub owner: NationId,
    pub hull_remaining: u32,
    /// Sea zone this ship currently occupies. `None` for ships not yet positioned.
    pub sea_zone: Option<SeaZoneId>,
    /// Current naval operation assignment (Patrol, Blockade, Beachhead, etc.).
    pub operation: Option<crate::military::naval::NavalOperation>,
}

impl ShipType {
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
    /// Create a new ship with the given starting hull and no sea zone assignment.
    /// Callers that want the canonical full-hull value from data should use
    /// [`Ship::with_data`] instead.
    pub fn new(id: UnitId, ship_type: ShipType, owner: NationId, hull: u32) -> Self {
        Self {
            id,
            ship_type,
            owner,
            hull_remaining: hull,
            sea_zone: None,
            operation: None,
        }
    }

    /// Create a new ship with full hull pulled from `GameData`.
    pub fn with_data(
        id: UnitId,
        ship_type: ShipType,
        owner: NationId,
        data: &crate::data::GameData,
    ) -> Self {
        Self::new(id, ship_type, owner, data.ship_stats(ship_type).hull)
    }

    /// Reduce hull by the given amount, saturating at zero.
    pub fn take_damage(&mut self, amount: u32) {
        self.hull_remaining = self.hull_remaining.saturating_sub(amount);
    }

    /// Returns true if the ship has been sunk (hull == 0).
    pub fn is_sunk(&self) -> bool {
        self.hull_remaining == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;

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
    fn category_matches_data_category() {
        let data = GameData::default();
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
                data.ship_stats(ship_type).category,
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
    fn with_data_uses_full_hull_from_data() {
        let data = GameData::default();
        let ship = Ship::with_data(UnitId(1), ShipType::Frigate, NationId(1), &data);
        assert_eq!(ship.hull_remaining, data.ship_stats(ShipType::Frigate).hull);
        assert!(!ship.is_sunk());
        assert_eq!(ship.sea_zone, None);
    }

    #[test]
    fn new_preserves_type_owner_and_hull() {
        let ship = Ship::new(UnitId(42), ShipType::Dreadnought, NationId(3), 80);
        assert_eq!(ship.id, UnitId(42));
        assert_eq!(ship.ship_type, ShipType::Dreadnought);
        assert_eq!(ship.owner, NationId(3));
        assert_eq!(ship.hull_remaining, 80);
    }

    #[test]
    fn take_damage_reduces_hull() {
        let mut ship = Ship::new(UnitId(1), ShipType::Frigate, NationId(1), 35);
        ship.take_damage(10);
        assert_eq!(ship.hull_remaining, 25);
        assert!(!ship.is_sunk());
    }

    #[test]
    fn take_damage_saturates_at_zero() {
        let mut ship = Ship::new(UnitId(1), ShipType::Trader, NationId(1), 25);
        ship.take_damage(100);
        assert_eq!(ship.hull_remaining, 0);
        assert!(ship.is_sunk());
    }

    #[test]
    fn take_exact_hull_damage_sinks() {
        let mut ship = Ship::new(UnitId(1), ShipType::Trader, NationId(1), 25);
        ship.take_damage(25);
        assert_eq!(ship.hull_remaining, 0);
        assert!(ship.is_sunk());
    }

    #[test]
    fn incremental_damage_until_sunk() {
        let mut ship = Ship::new(UnitId(1), ShipType::ShipOfTheLine, NationId(1), 65);
        ship.take_damage(20);
        assert_eq!(ship.hull_remaining, 45);
        ship.take_damage(20);
        assert_eq!(ship.hull_remaining, 25);
        ship.take_damage(20);
        assert_eq!(ship.hull_remaining, 5);
        ship.take_damage(20);
        assert!(ship.is_sunk());
    }

    // ── Data-driven invariants ──────────────────────────────────

    #[test]
    fn merchants_have_zero_firepower_in_data() {
        let data = GameData::default();
        for ship_type in [
            ShipType::Trader,
            ShipType::Indiaman,
            ShipType::Clipper,
            ShipType::Paddlewheeler,
            ShipType::Freighter,
        ] {
            assert_eq!(
                data.ship_stats(ship_type).firepower,
                0,
                "{:?} should have zero firepower",
                ship_type
            );
        }
    }

    #[test]
    fn warships_have_zero_cargo_in_data() {
        let data = GameData::default();
        for ship_type in [
            ShipType::Frigate,
            ShipType::ShipOfTheLine,
            ShipType::Raider,
            ShipType::Ironclad,
            ShipType::AdvancedIronclad,
            ShipType::ArmouredCruiser,
            ShipType::Dreadnought,
            ShipType::Battlecruiser,
        ] {
            assert_eq!(
                data.ship_stats(ship_type).cargo,
                0,
                "{:?} should have zero cargo",
                ship_type
            );
        }
    }
}
