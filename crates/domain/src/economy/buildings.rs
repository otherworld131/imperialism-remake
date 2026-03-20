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
    // Late-game
    OilRefinery,
    PowerPlant,
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
    /// Sets pending_capacity and a 2-turn delay before it takes effect.
    pub fn start_expansion(&mut self, amount: u32) {
        self.pending_capacity = amount;
        self.turns_until_upgrade = 2;
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
}
