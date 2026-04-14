/// Macro for generating strongly-typed ID wrappers.
macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub struct $name(pub u32);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

define_id!(PlayerId);
define_id!(NationId);
define_id!(ProvinceId);

// ── Turn number ─────────────────────────────────────────────────

/// A turn number. Turn 1 = 1815 Q1, Turn 4 = 1815 Q4, Turn 5 = 1816 Q1, etc.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct TurnNumber(pub u32);

const BASE_YEAR: u32 = 1815;

impl TurnNumber {
    pub const fn new(n: u32) -> Self {
        assert!(n >= 1, "Turn number must be >= 1");
        Self(n)
    }

    /// The calendar year for this turn.
    pub const fn year(self) -> u32 {
        BASE_YEAR + (self.0 - 1) / 4
    }

    /// The quarter (1-4) within the year.
    pub const fn quarter(self) -> u32 {
        (self.0 - 1) % 4 + 1
    }

    /// Create from a year and quarter.
    pub const fn from_year_quarter(year: u32, quarter: u32) -> Self {
        assert!(quarter >= 1 && quarter <= 4, "Quarter must be 1-4");
        assert!(year >= BASE_YEAR, "Year must be >= 1815");
        Self((year - BASE_YEAR) * 4 + quarter)
    }

    /// Whether this turn falls on a decade boundary (Council election).
    pub const fn is_decade_election(self) -> bool {
        let year = self.year();
        // Elections at 1825, 1835, ..., 1915
        year >= 1825 && year % 10 == 5 && self.quarter() == 1
    }

    /// Whether this is the final turn (1915 Q1).
    pub const fn is_game_end(self) -> bool {
        self.year() == 1915 && self.quarter() == 1
    }

    /// Whether a decade election is approaching within `turns_ahead` turns.
    ///
    /// Returns `true` if any turn from now to `self.0 + turns_ahead` (inclusive)
    /// would be a decade election.
    pub fn is_near_decade_election(self, turns_ahead: u32) -> bool {
        for offset in 1..=turns_ahead {
            let future = TurnNumber(self.0 + offset);
            if future.is_decade_election() {
                return true;
            }
        }
        false
    }

    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl std::fmt::Display for TurnNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} Q{}", self.year(), self.quarter())
    }
}

// ── Money ───────────────────────────────────────────────────────

/// Money value in game currency. Stored as integer cents to avoid floating-point issues.
/// Display shows dollars (e.g., $1,500).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Money = Money(0);

    pub const fn dollars(amount: i64) -> Self {
        Self(amount * 100)
    }

    pub const fn from_cents(cents: i64) -> Self {
        Self(cents)
    }

    pub const fn cents(self) -> i64 {
        self.0
    }

    pub const fn as_dollars(self) -> i64 {
        self.0 / 100
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        let result = Self(self.0 - other.0);
        if result.is_negative() {
            None
        } else {
            Some(result)
        }
    }
}

impl std::ops::Add for Money {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Money {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl std::ops::AddAssign for Money {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl std::ops::SubAssign for Money {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl std::ops::Mul<i64> for Money {
    type Output = Self;
    fn mul(self, rhs: i64) -> Self {
        Self(self.0 * rhs)
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${}", self.as_dollars())
    }
}

// ── Resource / Material / Goods enums ───────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ResourceType {
    Timber,
    Coal,
    Iron,
    Cotton,
    Wool,
    Grain,
    Fruit,
    Livestock,
    Horses,
    Oil,
    Gold,
    Gems,
}

impl ResourceType {
    /// Whether this resource can be traded on the international market.
    /// All resources are tradeable — nations can buy food and horses from Minor Nations.
    pub const fn is_tradeable(self) -> bool {
        true
    }

    /// Whether this resource directly converts to money (no processing needed).
    pub const fn is_monetary(self) -> bool {
        matches!(self, Self::Gold | Self::Gems)
    }

    /// Whether this resource is hidden until revealed by prospecting.
    pub const fn requires_prospecting(self) -> bool {
        matches!(
            self,
            Self::Coal | Self::Iron | Self::Gold | Self::Gems | Self::Oil
        )
    }

    /// Whether this resource is a food type.
    pub const fn is_food(self) -> bool {
        matches!(
            self,
            Self::Grain | Self::Fruit | Self::Livestock | Self::Horses
        )
    }

    /// Maximum improvement level for this resource.
    pub const fn max_improvement_level(self) -> u8 {
        match self {
            // Gold/Gems/Oil require level 1+ to produce, can reach level 3
            Self::Gold | Self::Gems | Self::Oil => 3,
            // All other resources: improvable to level 3
            _ => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MaterialType {
    Lumber,
    Steel,
    Fabric,
    Paper,
    Arms,
    CannedFood,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GoodsType {
    Furniture,
    Clothing,
    Hardware,
}

// ── Terrain ─────────────────────────────────────────────────────

/// Landscape terrain types. Resources are a separate overlay on tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TerrainType {
    Grassland,
    Hills,
    Forest,
    Mountain,
    Desert,
    Swamp,
    Tundra,
    Sea,
}

impl TerrainType {
    /// Whether this terrain can host hidden (prospectable) resource deposits.
    pub const fn can_have_deposits(self) -> bool {
        matches!(
            self,
            Self::Hills | Self::Mountain | Self::Desert | Self::Swamp | Self::Tundra
        )
    }

    /// Defense bonus for combat on this terrain.
    pub const fn defense_bonus(self) -> u8 {
        match self {
            Self::Mountain => 50,
            Self::Hills => 30,
            Self::Forest => 20,
            Self::Swamp => 15,
            _ => 0,
        }
    }

    pub const fn is_land(self) -> bool {
        !matches!(self, Self::Sea)
    }
}

// ── ResourceAmount ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResourceAmount {
    pub resource: ResourceType,
    pub quantity: u32,
}

impl ResourceAmount {
    pub const fn new(resource: ResourceType, quantity: u32) -> Self {
        Self { resource, quantity }
    }
}

// ── Nation type ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NationType {
    GreatPower,
    MinorNation,
}

// ── Difficulty ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Difficulty {
    Introductory,
    Easy,
    Normal,
    Hard,
    NighOnImpossible,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── TurnNumber ──────────────────────────────────────────────

    #[test]
    fn turn_1_is_1815_q1() {
        let t = TurnNumber::new(1);
        assert_eq!(t.year(), 1815);
        assert_eq!(t.quarter(), 1);
    }

    #[test]
    fn turn_4_is_1815_q4() {
        let t = TurnNumber::new(4);
        assert_eq!(t.year(), 1815);
        assert_eq!(t.quarter(), 4);
    }

    #[test]
    fn turn_5_is_1816_q1() {
        let t = TurnNumber::new(5);
        assert_eq!(t.year(), 1816);
        assert_eq!(t.quarter(), 1);
    }

    #[test]
    fn from_year_quarter_roundtrip() {
        for turn in 1..=500 {
            let t = TurnNumber::new(turn);
            let reconstructed = TurnNumber::from_year_quarter(t.year(), t.quarter());
            assert_eq!(t, reconstructed, "roundtrip failed for turn {turn}");
        }
    }

    #[test]
    fn turn_401_is_1915_q1() {
        let t = TurnNumber::new(401);
        assert_eq!(t.year(), 1915);
        assert_eq!(t.quarter(), 1);
        assert!(t.is_game_end());
    }

    #[test]
    fn decade_elections() {
        // Elections at 1825, 1835, 1845, ..., 1915 (all Q1)
        let t1825 = TurnNumber::from_year_quarter(1825, 1);
        assert!(t1825.is_decade_election());

        let t1835 = TurnNumber::from_year_quarter(1835, 1);
        assert!(t1835.is_decade_election());

        let t1915 = TurnNumber::from_year_quarter(1915, 1);
        assert!(t1915.is_decade_election());

        // Not elections
        let t1820 = TurnNumber::from_year_quarter(1820, 1);
        assert!(!t1820.is_decade_election());

        let t1825_q2 = TurnNumber::from_year_quarter(1825, 2);
        assert!(!t1825_q2.is_decade_election());

        let t1815 = TurnNumber::from_year_quarter(1815, 1);
        assert!(!t1815.is_decade_election()); // too early
    }

    #[test]
    fn turn_display() {
        assert_eq!(format!("{}", TurnNumber::new(1)), "1815 Q1");
        assert_eq!(format!("{}", TurnNumber::new(42)), "1825 Q2");
    }

    #[test]
    fn turn_next() {
        let t = TurnNumber::new(1);
        assert_eq!(t.next(), TurnNumber::new(2));
    }

    // ── Money ───────────────────────────────────────────────────

    #[test]
    fn money_dollars() {
        let m = Money::dollars(1500);
        assert_eq!(m.as_dollars(), 1500);
        assert_eq!(m.cents(), 150_000);
    }

    #[test]
    fn money_arithmetic() {
        let a = Money::dollars(1000);
        let b = Money::dollars(500);
        assert_eq!((a + b).as_dollars(), 1500);
        assert_eq!((a - b).as_dollars(), 500);
        assert_eq!((b * 3).as_dollars(), 1500);
    }

    #[test]
    fn money_checked_sub_sufficient() {
        let a = Money::dollars(1000);
        let b = Money::dollars(500);
        assert_eq!(a.checked_sub(b), Some(Money::dollars(500)));
    }

    #[test]
    fn money_checked_sub_insufficient() {
        let a = Money::dollars(100);
        let b = Money::dollars(500);
        assert_eq!(a.checked_sub(b), None);
    }

    #[test]
    fn money_display() {
        assert_eq!(format!("{}", Money::dollars(1500)), "$1500");
    }

    #[test]
    fn money_add_assign() {
        let mut m = Money::dollars(100);
        m += Money::dollars(50);
        assert_eq!(m, Money::dollars(150));
    }

    #[test]
    fn money_zero() {
        assert_eq!(Money::ZERO.as_dollars(), 0);
        assert!(!Money::ZERO.is_negative());
    }

    #[test]
    fn money_negative() {
        let m = Money::dollars(100) - Money::dollars(200);
        assert!(m.is_negative());
    }

    // ── ResourceType ────────────────────────────────────────────

    #[test]
    fn all_resources_tradeable() {
        assert!(ResourceType::Grain.is_tradeable());
        assert!(ResourceType::Horses.is_tradeable());
        assert!(ResourceType::Timber.is_tradeable());
    }

    #[test]
    fn timber_tradeable() {
        assert!(ResourceType::Timber.is_tradeable());
    }

    #[test]
    fn gold_is_monetary() {
        assert!(ResourceType::Gold.is_monetary());
    }

    #[test]
    fn gems_is_monetary() {
        assert!(ResourceType::Gems.is_monetary());
    }

    #[test]
    fn iron_not_monetary() {
        assert!(!ResourceType::Iron.is_monetary());
    }

    // ── TerrainType ─────────────────────────────────────────────

    #[test]
    fn hills_can_have_deposits() {
        assert!(TerrainType::Hills.can_have_deposits());
    }

    #[test]
    fn mountain_can_have_deposits() {
        assert!(TerrainType::Mountain.can_have_deposits());
    }

    #[test]
    fn grassland_cannot_have_deposits() {
        assert!(!TerrainType::Grassland.can_have_deposits());
    }

    #[test]
    fn sea_is_not_land() {
        assert!(!TerrainType::Sea.is_land());
    }

    #[test]
    fn grassland_is_land() {
        assert!(TerrainType::Grassland.is_land());
    }

    // ── IDs ─────────────────────────────────────────────────────

    #[test]
    fn id_equality() {
        assert_eq!(NationId(1), NationId(1));
        assert_ne!(NationId(1), NationId(2));
    }

    #[test]
    fn id_display() {
        assert_eq!(format!("{}", NationId(42)), "NationId(42)");
    }

    // ── TurnNumber ordering ────────────────────────────────────

    #[test]
    fn turn_number_ordering() {
        let turn1 = TurnNumber::new(1);
        let turn2 = TurnNumber::new(2);
        assert!(turn1 < turn2);
        assert!(turn2 > turn1);
        assert!(turn1 <= turn1);
        assert!(turn1 >= turn1);
    }

    #[test]
    fn turn_number_ordering_late_game() {
        let t400 = TurnNumber::new(400);
        let t401 = TurnNumber::new(401);
        assert!(t400 < t401);
    }

    // ── Money negative detection ───────────────────────────────

    #[test]
    fn money_negative_detection() {
        assert!(Money::dollars(-1).is_negative());
        assert!(Money::from_cents(-1).is_negative());
        assert!(!Money::dollars(0).is_negative());
        assert!(!Money::dollars(1).is_negative());
    }

    #[test]
    fn money_subtraction_can_go_negative() {
        let result = Money::dollars(100) - Money::dollars(200);
        assert!(result.is_negative());
        assert_eq!(result.as_dollars(), -100);
    }

    // ── Money overflow behavior ────────────────────────────────

    #[test]
    fn money_very_large_amounts() {
        let large = Money::dollars(1_000_000_000); // $1 billion
        assert_eq!(large.as_dollars(), 1_000_000_000);
        assert!(!large.is_negative());

        // Adding two large amounts should not overflow i64
        let doubled = large + large;
        assert_eq!(doubled.as_dollars(), 2_000_000_000);
    }

    #[test]
    fn money_multiply_large() {
        let m = Money::dollars(100_000);
        let result = m * 10_000;
        assert_eq!(result.as_dollars(), 1_000_000_000);
    }

    // ── ResourceType::is_tradeable covers all variants ─────────

    #[test]
    fn is_tradeable_covers_all_variants() {
        let all_resources = [
            ResourceType::Timber,
            ResourceType::Coal,
            ResourceType::Iron,
            ResourceType::Cotton,
            ResourceType::Wool,
            ResourceType::Grain,
            ResourceType::Fruit,
            ResourceType::Livestock,
            ResourceType::Horses,
            ResourceType::Oil,
            ResourceType::Gold,
            ResourceType::Gems,
        ];

        // All resources are tradeable
        let tradeable: Vec<_> = all_resources.iter().filter(|r| r.is_tradeable()).collect();
        assert_eq!(tradeable.len(), 12);
    }

    // ── Money checked_sub edge cases ───────────────────────────

    #[test]
    fn money_checked_sub_equal_amounts() {
        let a = Money::dollars(500);
        assert_eq!(a.checked_sub(a), Some(Money::ZERO));
    }

    #[test]
    fn money_checked_sub_zero() {
        let a = Money::dollars(500);
        assert_eq!(a.checked_sub(Money::ZERO), Some(a));
    }

    // ── Identity tests ────────────────────────────────────────

    #[test]
    fn identity_test_same_id_equal() {
        assert_eq!(NationId(1), NationId(1));
        assert_eq!(ProvinceId(42), ProvinceId(42));
    }

    #[test]
    fn identity_test_different_id_not_equal() {
        assert_ne!(NationId(1), NationId(2));
        assert_ne!(ProvinceId(1), ProvinceId(2));
    }

    // ── Immutability tests ────────────────────────────────────

    #[test]
    fn immutability_money_operations_return_new_value() {
        let a = Money::dollars(100);
        let b = a + Money::dollars(50);
        // 'a' is unchanged (it's Copy, so the original value is preserved)
        assert_eq!(a, Money::dollars(100));
        assert_eq!(b, Money::dollars(150));
    }

    #[test]
    fn immutability_hex_coord_operations_return_new_value() {
        use crate::hex::HexCoord;
        let a = HexCoord::new(1, 2);
        let b = a + HexCoord::new(3, 4);
        assert_eq!(a, HexCoord::new(1, 2));
        assert_eq!(b, HexCoord::new(4, 6));
    }
}
