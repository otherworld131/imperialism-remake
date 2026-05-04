#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingType {
    Armory,
    Capitol,
    FoodProcessing,
    Railyard,
    Shipyard,
    TradeSchool,
    University,
    Warehouse,
    // Mills
    LumberMill,
    SteelMill,
    TextileMill,
    // Factories
    FurnitureFactory,
    HardwareFactory,
    ClothingFactory,
    PaperFactory,
    // Late-game
    OilRefinery,
    PowerPlant,
    AdvancedTextileMill,
    ChemicalPlant,
}

impl std::fmt::Display for BuildingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FoodProcessing => write!(f, "Food Processing"),
            Self::TradeSchool => write!(f, "Trade School"),
            Self::LumberMill => write!(f, "Lumber Mill"),
            Self::SteelMill => write!(f, "Steel Mill"),
            Self::TextileMill => write!(f, "Textile Mill"),
            Self::FurnitureFactory => write!(f, "Furniture Factory"),
            Self::HardwareFactory => write!(f, "Hardware Factory"),
            Self::ClothingFactory => write!(f, "Clothing Factory"),
            Self::PaperFactory => write!(f, "Paper Factory"),
            Self::OilRefinery => write!(f, "Oil Refinery"),
            Self::PowerPlant => write!(f, "Power Plant"),
            Self::AdvancedTextileMill => write!(f, "Advanced Textile Mill"),
            Self::ChemicalPlant => write!(f, "Chemical Plant"),
            other => write!(f, "{:?}", other),
        }
    }
}

impl std::str::FromStr for BuildingType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Armory" => Ok(Self::Armory),
            "Capitol" => Ok(Self::Capitol),
            "Food Processing" => Ok(Self::FoodProcessing),
            "Railyard" => Ok(Self::Railyard),
            "Shipyard" => Ok(Self::Shipyard),
            "Trade School" => Ok(Self::TradeSchool),
            "University" => Ok(Self::University),
            "Warehouse" => Ok(Self::Warehouse),
            "Lumber Mill" => Ok(Self::LumberMill),
            "Steel Mill" => Ok(Self::SteelMill),
            "Textile Mill" => Ok(Self::TextileMill),
            "Furniture Factory" => Ok(Self::FurnitureFactory),
            "Hardware Factory" => Ok(Self::HardwareFactory),
            "Clothing Factory" => Ok(Self::ClothingFactory),
            "Paper Factory" => Ok(Self::PaperFactory),
            "Oil Refinery" => Ok(Self::OilRefinery),
            "Power Plant" => Ok(Self::PowerPlant),
            "Advanced Textile Mill" => Ok(Self::AdvancedTextileMill),
            "Chemical Plant" => Ok(Self::ChemicalPlant),
            _ => Err(format!("unknown BuildingType: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Building {
    pub building_type: BuildingType,
    pub capacity: u32,
    pub pending_capacity: u32,
    pub turns_until_upgrade: u8,
}

impl Building {
    /// Create a new building with the given type and initial capacity.
    pub fn new(building_type: BuildingType, initial_capacity: u32) -> Self {
        Self {
            building_type,
            capacity: initial_capacity,
            pending_capacity: 0,
            turns_until_upgrade: 0,
        }
    }

    /// Start expanding this building by the given amount.
    /// Uses the default 2-turn delay before it takes effect.
    /// Returns `false` if an expansion is already in progress.
    pub fn start_expansion(&mut self, amount: u32) -> bool {
        if self.turns_until_upgrade > 0 {
            return false; // already expanding
        }
        self.pending_capacity = amount;
        self.turns_until_upgrade = 2;
        true
    }

    /// Start expanding with a custom delay (from game config).
    /// Returns `false` if an expansion is already in progress.
    pub fn start_expansion_with_delay(&mut self, amount: u32, delay: u8) -> bool {
        if self.turns_until_upgrade > 0 {
            return false;
        }
        self.pending_capacity = amount;
        self.turns_until_upgrade = delay;
        true
    }

    /// Returns the next capacity level following the progression:
    /// 2 -> 4 -> 8 -> 12 -> 16 -> 20 -> ...
    pub fn next_capacity(&self) -> u32 {
        match self.capacity {
            c if c < 2 => 2,
            2 => 4,
            4 => 8,
            8 => 12,
            12 => 16,
            16 => 20,
            _ => self.capacity + 4,
        }
    }

    /// Start expanding this building to its next capacity tier.
    /// Uses the capacity progression (2 -> 4 -> 8 -> 12 -> 16 -> 20 -> ...).
    /// Uses the default 2-turn delay before it takes effect.
    pub fn start_expansion_to_next_tier(&mut self) {
        let next = self.next_capacity();
        let increase = next - self.capacity;
        self.pending_capacity = increase;
        self.turns_until_upgrade = 2;
    }

    /// Start expanding to next tier with a custom delay (from game config).
    pub fn start_expansion_to_next_tier_with_delay(&mut self, delay: u8) {
        let next = self.next_capacity();
        let increase = next - self.capacity;
        self.pending_capacity = increase;
        self.turns_until_upgrade = delay;
    }

    /// Advance the building by one turn. When the upgrade countdown reaches 0,
    /// pending capacity is applied to the building's capacity.
    pub fn tick(&mut self) {
        if self.turns_until_upgrade > 0 {
            self.turns_until_upgrade -= 1;
            if self.turns_until_upgrade == 0 {
                self.capacity += self.pending_capacity;
                self.pending_capacity = 0;
            }
        }
    }

    /// The current effective capacity (does not include pending capacity).
    pub fn effective_capacity(&self) -> u32 {
        self.capacity
    }

    /// Calculate the cost to expand by the given amount.
    /// Returns `(lumber_needed, steel_needed)`: 1 lumber and 1 steel per capacity unit.
    pub fn expansion_cost(amount: u32) -> (u32, u32) {
        (amount, amount)
    }

    /// Whether this building is currently being expanded.
    pub fn is_expanding(&self) -> bool {
        self.turns_until_upgrade > 0
    }

    /// How many turns remain until expansion completes.
    pub fn expansion_turns_remaining(&self) -> u8 {
        self.turns_until_upgrade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ──────────────────────────────────────────────

    #[test]
    fn new_building_has_correct_type_and_capacity() {
        let b = Building::new(BuildingType::LumberMill, 5);
        assert_eq!(b.building_type, BuildingType::LumberMill);
        assert_eq!(b.capacity, 5);
        assert_eq!(b.pending_capacity, 0);
        assert_eq!(b.turns_until_upgrade, 0);
    }

    #[test]
    fn new_building_effective_capacity_equals_initial() {
        let b = Building::new(BuildingType::SteelMill, 3);
        assert_eq!(b.effective_capacity(), 3);
    }

    #[test]
    fn new_building_with_zero_capacity() {
        let b = Building::new(BuildingType::Capitol, 0);
        assert_eq!(b.capacity, 0);
        assert_eq!(b.effective_capacity(), 0);
    }

    // ── Expansion ─────────────────────────────────────────────────

    #[test]
    fn start_expansion_sets_pending_and_countdown() {
        let mut b = Building::new(BuildingType::TextileMill, 2);
        b.start_expansion(3);
        assert_eq!(b.pending_capacity, 3);
        assert_eq!(b.turns_until_upgrade, 2);
        // Effective capacity should not change yet
        assert_eq!(b.effective_capacity(), 2);
    }

    #[test]
    fn expansion_cost_returns_equal_lumber_and_steel() {
        let (lumber, steel) = Building::expansion_cost(4);
        assert_eq!(lumber, 4);
        assert_eq!(steel, 4);
    }

    #[test]
    fn expansion_cost_zero() {
        let (lumber, steel) = Building::expansion_cost(0);
        assert_eq!(lumber, 0);
        assert_eq!(steel, 0);
    }

    // ── Tick behavior / 2-turn delay ──────────────────────────────

    #[test]
    fn tick_without_expansion_is_noop() {
        let mut b = Building::new(BuildingType::Warehouse, 10);
        b.tick();
        assert_eq!(b.capacity, 10);
        assert_eq!(b.pending_capacity, 0);
        assert_eq!(b.turns_until_upgrade, 0);
    }

    #[test]
    fn tick_decrements_countdown() {
        let mut b = Building::new(BuildingType::Armory, 1);
        b.start_expansion(2);
        b.tick(); // turn 1: countdown goes from 2 → 1
        assert_eq!(b.turns_until_upgrade, 1);
        assert_eq!(b.capacity, 1); // not yet applied
        assert_eq!(b.pending_capacity, 2);
    }

    #[test]
    fn tick_applies_pending_capacity_after_two_turns() {
        let mut b = Building::new(BuildingType::FurnitureFactory, 5);
        b.start_expansion(3);

        b.tick(); // turn 1: countdown 2 → 1
        assert_eq!(b.capacity, 5);
        assert_eq!(b.effective_capacity(), 5);

        b.tick(); // turn 2: countdown 1 → 0, capacity applied
        assert_eq!(b.capacity, 8);
        assert_eq!(b.effective_capacity(), 8);
        assert_eq!(b.pending_capacity, 0);
        assert_eq!(b.turns_until_upgrade, 0);
    }

    #[test]
    fn tick_after_expansion_complete_is_noop() {
        let mut b = Building::new(BuildingType::HardwareFactory, 2);
        b.start_expansion(1);
        b.tick();
        b.tick(); // expansion completes
        assert_eq!(b.capacity, 3);

        b.tick(); // extra tick — should be noop
        assert_eq!(b.capacity, 3);
        assert_eq!(b.pending_capacity, 0);
    }

    #[test]
    fn sequential_expansions() {
        let mut b = Building::new(BuildingType::ClothingFactory, 1);

        // First expansion
        b.start_expansion(2);
        b.tick();
        b.tick();
        assert_eq!(b.capacity, 3);

        // Second expansion
        b.start_expansion(4);
        b.tick();
        b.tick();
        assert_eq!(b.capacity, 7);
    }

    #[test]
    fn all_building_types_can_be_constructed() {
        let types = [
            BuildingType::Armory,
            BuildingType::Capitol,
            BuildingType::FoodProcessing,
            BuildingType::Railyard,
            BuildingType::Shipyard,
            BuildingType::TradeSchool,
            BuildingType::University,
            BuildingType::Warehouse,
            BuildingType::LumberMill,
            BuildingType::SteelMill,
            BuildingType::TextileMill,
            BuildingType::FurnitureFactory,
            BuildingType::HardwareFactory,
            BuildingType::ClothingFactory,
            BuildingType::OilRefinery,
            BuildingType::PowerPlant,
        ];
        for bt in types {
            let b = Building::new(bt, 1);
            assert_eq!(b.building_type, bt);
        }
    }

    // ── Capacity progression ──────────────────────────────────────

    #[test]
    fn next_capacity_from_zero() {
        let b = Building::new(BuildingType::LumberMill, 0);
        assert_eq!(b.next_capacity(), 2);
    }

    #[test]
    fn next_capacity_from_one() {
        let b = Building::new(BuildingType::LumberMill, 1);
        assert_eq!(b.next_capacity(), 2);
    }

    #[test]
    fn next_capacity_from_two() {
        let b = Building::new(BuildingType::LumberMill, 2);
        assert_eq!(b.next_capacity(), 4);
    }

    #[test]
    fn next_capacity_from_four() {
        let b = Building::new(BuildingType::LumberMill, 4);
        assert_eq!(b.next_capacity(), 8);
    }

    #[test]
    fn next_capacity_from_eight() {
        let b = Building::new(BuildingType::LumberMill, 8);
        assert_eq!(b.next_capacity(), 12);
    }

    #[test]
    fn next_capacity_from_twelve() {
        let b = Building::new(BuildingType::LumberMill, 12);
        assert_eq!(b.next_capacity(), 16);
    }

    #[test]
    fn next_capacity_from_sixteen() {
        let b = Building::new(BuildingType::LumberMill, 16);
        assert_eq!(b.next_capacity(), 20);
    }

    #[test]
    fn next_capacity_from_twenty() {
        let b = Building::new(BuildingType::LumberMill, 20);
        assert_eq!(b.next_capacity(), 24);
    }

    #[test]
    fn start_expansion_to_next_tier_from_two() {
        let mut b = Building::new(BuildingType::SteelMill, 2);
        b.start_expansion_to_next_tier();
        assert_eq!(b.pending_capacity, 2); // 4 - 2 = 2
        assert_eq!(b.turns_until_upgrade, 2);
        b.tick();
        b.tick();
        assert_eq!(b.capacity, 4);
    }

    #[test]
    fn start_expansion_to_next_tier_from_four() {
        let mut b = Building::new(BuildingType::SteelMill, 4);
        b.start_expansion_to_next_tier();
        assert_eq!(b.pending_capacity, 4); // 8 - 4 = 4
        b.tick();
        b.tick();
        assert_eq!(b.capacity, 8);
    }
}
