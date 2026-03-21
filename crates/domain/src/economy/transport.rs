use crate::military::ships::Ship;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;

/// The transport system for a nation — freight cars carrying resources.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransportSystem {
    /// Total freight cars owned
    pub freight_cars: u32,
    /// Resource allocation: what percentage of capacity goes to each resource type
    pub allocations: Vec<(ResourceType, u32)>, // (resource, percentage 0-100)
}

impl TransportSystem {
    /// Create a new transport system with 0 freight cars and empty allocations.
    pub fn new() -> Self {
        Self {
            freight_cars: 0,
            allocations: Vec::new(),
        }
    }

    /// Each car carries 1 unit, so total capacity equals the number of freight cars.
    pub fn total_capacity(&self) -> u32 {
        self.freight_cars
    }

    /// Returns the cost to build a single freight car: (labor, lumber, steel).
    pub fn build_freight_car_cost() -> (u32, u32, u32) {
        (2, 1, 1)
    }

    /// Add freight cars to the transport system.
    pub fn build_freight_cars(&mut self, count: u32) {
        self.freight_cars += count;
    }

    /// Set the transport priority for a resource type as a percentage (0-100).
    /// If the resource already has an allocation, it is updated.
    pub fn set_allocation(&mut self, resource: ResourceType, percentage: u32) {
        if let Some(entry) = self.allocations.iter_mut().find(|(r, _)| *r == resource) {
            entry.1 = percentage;
        } else {
            self.allocations.push((resource, percentage));
        }
    }

    /// Given available resources from tiles, calculate how many of each resource
    /// get delivered based on capacity and allocations.
    ///
    /// If no allocations are set, resources are distributed evenly across all
    /// available types. Total delivered cannot exceed `freight_cars`.
    pub fn calculate_deliveries(
        &self,
        available: &[(ResourceType, u32)],
    ) -> Vec<(ResourceType, u32)> {
        if self.freight_cars == 0 || available.is_empty() {
            return Vec::new();
        }

        // Filter out resources with zero availability.
        let nonempty: Vec<(ResourceType, u32)> = available
            .iter()
            .filter(|(_, qty)| *qty > 0)
            .copied()
            .collect();

        if nonempty.is_empty() {
            return Vec::new();
        }

        let capacity = self.freight_cars;

        // Check whether any of the available resources have allocations set.
        let has_allocations = nonempty.iter().any(|(r, _)| {
            self.allocations
                .iter()
                .any(|(ar, pct)| *ar == *r && *pct > 0)
        });

        if !has_allocations {
            // Even distribution: split capacity equally, capped by availability.
            return Self::distribute_evenly(&nonempty, capacity);
        }

        // Allocation-based distribution.
        let total_pct: u32 = self
            .allocations
            .iter()
            .filter(|(r, _)| nonempty.iter().any(|(nr, _)| *nr == *r))
            .map(|(_, pct)| pct)
            .sum();

        if total_pct == 0 {
            return Self::distribute_evenly(&nonempty, capacity);
        }

        let mut result = Vec::new();
        let mut remaining_capacity = capacity;

        for (resource, avail) in &nonempty {
            let pct = self
                .allocations
                .iter()
                .find(|(r, _)| *r == *resource)
                .map(|(_, p)| *p)
                .unwrap_or(0);

            if pct == 0 {
                continue;
            }

            // Proportional share of capacity based on allocation percentage.
            let share = (capacity as u64 * pct as u64 / total_pct as u64) as u32;
            let delivered = share.min(*avail).min(remaining_capacity);

            if delivered > 0 {
                result.push((*resource, delivered));
                remaining_capacity -= delivered;
            }
        }

        result
    }

    /// 1 army unit per 5 freight cars (integer division).
    pub fn military_transport_capacity(&self) -> u32 {
        self.freight_cars / 5
    }

    /// Distribute capacity evenly across available resources, capped by each
    /// resource's availability. Uses round-robin to distribute remainders fairly.
    fn distribute_evenly(
        available: &[(ResourceType, u32)],
        capacity: u32,
    ) -> Vec<(ResourceType, u32)> {
        let count = available.len() as u32;
        let base = capacity / count;
        let mut extra = capacity % count;

        let mut result = Vec::new();
        let mut remaining_capacity = capacity;

        for (resource, avail) in available {
            let mut share = base;
            if extra > 0 {
                share += 1;
                extra -= 1;
            }
            let delivered = share.min(*avail).min(remaining_capacity);
            if delivered > 0 {
                result.push((*resource, delivered));
                remaining_capacity -= delivered;
            }
        }

        result
    }
}

/// Calculate how many army units can be transported by rail.
/// 1 unit per 5 freight cars (integer division).
pub fn rail_transport_capacity(freight_cars: u32) -> u32 {
    freight_cars / 5
}

/// Calculate amphibious landing force size.
/// Force size = total arms_cost used to build all ships in the fleet.
pub fn amphibious_force_size(ships: &[Ship]) -> u32 {
    ships.iter().map(|s| s.ship_type.stats().arms_cost).sum()
}

/// Calculate the transport size of an army unit (= arms required to build it).
pub fn army_transport_size(unit: &ArmyUnit) -> u32 {
    unit.unit_type.stats().arms_required
}

/// General counts as 1 transport unit regardless of arms.
pub fn unit_transport_size(unit: &ArmyUnit) -> u32 {
    if unit.unit_type == ArmyUnitType::General {
        1
    } else {
        unit.unit_type.stats().arms_required
    }
}

impl Default for TransportSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Building freight cars ─────────────────────────────────────

    #[test]
    fn new_transport_system_has_zero_cars() {
        let ts = TransportSystem::new();
        assert_eq!(ts.freight_cars, 0);
        assert!(ts.allocations.is_empty());
    }

    #[test]
    fn build_freight_cars_adds_cars() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(3);
        assert_eq!(ts.freight_cars, 3);
    }

    #[test]
    fn build_freight_cars_accumulates() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(2);
        ts.build_freight_cars(5);
        assert_eq!(ts.freight_cars, 7);
    }

    #[test]
    fn build_freight_car_cost_returns_labor_lumber_steel() {
        let (labor, lumber, steel) = TransportSystem::build_freight_car_cost();
        assert_eq!(labor, 2);
        assert_eq!(lumber, 1);
        assert_eq!(steel, 1);
    }

    // ── Capacity calculations ────────────────────────────────────

    #[test]
    fn total_capacity_equals_freight_cars() {
        let mut ts = TransportSystem::new();
        assert_eq!(ts.total_capacity(), 0);
        ts.build_freight_cars(10);
        assert_eq!(ts.total_capacity(), 10);
    }

    // ── Military transport capacity ──────────────────────────────

    #[test]
    fn military_transport_zero_cars() {
        let ts = TransportSystem::new();
        assert_eq!(ts.military_transport_capacity(), 0);
    }

    #[test]
    fn military_transport_four_cars() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(4);
        assert_eq!(ts.military_transport_capacity(), 0);
    }

    #[test]
    fn military_transport_five_cars() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(5);
        assert_eq!(ts.military_transport_capacity(), 1);
    }

    #[test]
    fn military_transport_ten_cars() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);
        assert_eq!(ts.military_transport_capacity(), 2);
    }

    // ── Even distribution when no allocations set ────────────────

    #[test]
    fn even_distribution_single_resource() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(5);

        let available = vec![(ResourceType::Timber, 10)];
        let deliveries = ts.calculate_deliveries(&available);

        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0], (ResourceType::Timber, 5));
    }

    #[test]
    fn even_distribution_two_resources() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);

        let available = vec![(ResourceType::Timber, 20), (ResourceType::Coal, 20)];
        let deliveries = ts.calculate_deliveries(&available);

        assert_eq!(deliveries.len(), 2);
        assert_eq!(deliveries[0], (ResourceType::Timber, 5));
        assert_eq!(deliveries[1], (ResourceType::Coal, 5));
    }

    #[test]
    fn even_distribution_capped_by_availability() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);

        let available = vec![(ResourceType::Timber, 3), (ResourceType::Coal, 20)];
        let deliveries = ts.calculate_deliveries(&available);

        // Timber gets min(5, 3) = 3, Coal gets min(5, 20) = 5
        let timber = deliveries.iter().find(|(r, _)| *r == ResourceType::Timber);
        let coal = deliveries.iter().find(|(r, _)| *r == ResourceType::Coal);

        assert_eq!(timber, Some(&(ResourceType::Timber, 3)));
        assert_eq!(coal, Some(&(ResourceType::Coal, 5)));
    }

    #[test]
    fn even_distribution_no_available_resources() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);

        let available: Vec<(ResourceType, u32)> = vec![];
        let deliveries = ts.calculate_deliveries(&available);
        assert!(deliveries.is_empty());
    }

    #[test]
    fn even_distribution_zero_capacity() {
        let ts = TransportSystem::new();
        let available = vec![(ResourceType::Timber, 10)];
        let deliveries = ts.calculate_deliveries(&available);
        assert!(deliveries.is_empty());
    }

    // ── Delivery calculation with allocations ────────────────────

    #[test]
    fn allocation_based_delivery() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);
        ts.set_allocation(ResourceType::Timber, 70);
        ts.set_allocation(ResourceType::Coal, 30);

        let available = vec![(ResourceType::Timber, 20), (ResourceType::Coal, 20)];
        let deliveries = ts.calculate_deliveries(&available);

        let timber = deliveries.iter().find(|(r, _)| *r == ResourceType::Timber);
        let coal = deliveries.iter().find(|(r, _)| *r == ResourceType::Coal);

        // 70% of 10 = 7, 30% of 10 = 3
        assert_eq!(timber, Some(&(ResourceType::Timber, 7)));
        assert_eq!(coal, Some(&(ResourceType::Coal, 3)));
    }

    #[test]
    fn allocation_capped_by_availability() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);
        ts.set_allocation(ResourceType::Timber, 80);
        ts.set_allocation(ResourceType::Coal, 20);

        let available = vec![
            (ResourceType::Timber, 3), // only 3 available despite 80% allocation
            (ResourceType::Coal, 20),
        ];
        let deliveries = ts.calculate_deliveries(&available);

        let timber = deliveries.iter().find(|(r, _)| *r == ResourceType::Timber);
        let coal = deliveries.iter().find(|(r, _)| *r == ResourceType::Coal);

        assert_eq!(timber, Some(&(ResourceType::Timber, 3)));
        assert_eq!(coal, Some(&(ResourceType::Coal, 2)));
    }

    #[test]
    fn set_allocation_updates_existing() {
        let mut ts = TransportSystem::new();
        ts.set_allocation(ResourceType::Timber, 50);
        ts.set_allocation(ResourceType::Timber, 80);

        assert_eq!(ts.allocations.len(), 1);
        assert_eq!(ts.allocations[0], (ResourceType::Timber, 80));
    }

    #[test]
    fn delivery_total_does_not_exceed_capacity() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(5);
        ts.set_allocation(ResourceType::Timber, 50);
        ts.set_allocation(ResourceType::Coal, 50);

        let available = vec![(ResourceType::Timber, 100), (ResourceType::Coal, 100)];
        let deliveries = ts.calculate_deliveries(&available);

        let total: u32 = deliveries.iter().map(|(_, qty)| qty).sum();
        assert!(total <= 5);
    }

    #[test]
    fn delivery_with_zero_available_filtered_out() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);

        let available = vec![(ResourceType::Timber, 0), (ResourceType::Coal, 10)];
        let deliveries = ts.calculate_deliveries(&available);

        // Timber has 0 available, so only coal should be delivered
        let timber = deliveries.iter().find(|(r, _)| *r == ResourceType::Timber);
        assert!(timber.is_none());

        let coal = deliveries.iter().find(|(r, _)| *r == ResourceType::Coal);
        assert_eq!(coal, Some(&(ResourceType::Coal, 10)));
    }

    #[test]
    fn default_transport_system() {
        let ts = TransportSystem::default();
        assert_eq!(ts.freight_cars, 0);
        assert!(ts.allocations.is_empty());
    }

    // ── Standalone rail_transport_capacity function ──────────────

    #[test]
    fn rail_transport_capacity_ten_cars() {
        assert_eq!(rail_transport_capacity(10), 2);
    }

    #[test]
    fn rail_transport_capacity_three_cars() {
        assert_eq!(rail_transport_capacity(3), 0);
    }

    #[test]
    fn rail_transport_capacity_five_cars() {
        assert_eq!(rail_transport_capacity(5), 1);
    }

    #[test]
    fn rail_transport_capacity_zero_cars() {
        assert_eq!(rail_transport_capacity(0), 0);
    }

    // ── Amphibious force size ──────────────────────────────────

    #[test]
    fn amphibious_force_size_empty_fleet() {
        let ships: Vec<Ship> = vec![];
        assert_eq!(amphibious_force_size(&ships), 0);
    }

    #[test]
    fn amphibious_force_size_frigates() {
        use crate::map::UnitId;
        use crate::military::ships::ShipType;

        // 4 frigates, each with arms_cost = 2
        let ships: Vec<Ship> = (0..4)
            .map(|i| Ship::new(UnitId(i), ShipType::Frigate, NationId(1)))
            .collect();
        assert_eq!(amphibious_force_size(&ships), 8);
    }

    #[test]
    fn amphibious_force_size_mixed_fleet() {
        use crate::map::UnitId;
        use crate::military::ships::ShipType;

        // Mix of warships with different arms_cost values
        let ships = vec![
            Ship::new(UnitId(1), ShipType::Frigate, NationId(1)), // arms_cost = 2
            Ship::new(UnitId(2), ShipType::ShipOfTheLine, NationId(1)), // arms_cost = 5
            Ship::new(UnitId(3), ShipType::Dreadnought, NationId(1)), // arms_cost = 8
        ];
        assert_eq!(amphibious_force_size(&ships), 2 + 5 + 8);
    }

    #[test]
    fn amphibious_force_size_merchant_ships_have_zero_arms() {
        use crate::map::UnitId;
        use crate::military::ships::ShipType;

        let ships = vec![
            Ship::new(UnitId(1), ShipType::Trader, NationId(1)), // arms_cost = 0
            Ship::new(UnitId(2), ShipType::Clipper, NationId(1)), // arms_cost = 0
        ];
        assert_eq!(amphibious_force_size(&ships), 0);
    }

    // ── Army transport size ─────────────────────────────────────

    #[test]
    fn army_transport_size_regulars() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        // Regulars arms_required = 1
        assert_eq!(army_transport_size(&unit), 1);
    }

    #[test]
    fn army_transport_size_guards() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let unit = ArmyUnit::new(UnitId(1), ArmyUnitType::Guards, NationId(1), ProvinceId(1));
        // Guards arms_required = 3
        assert_eq!(army_transport_size(&unit), 3);
    }

    #[test]
    fn unit_transport_size_general_is_one() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let unit = ArmyUnit::new(UnitId(1), ArmyUnitType::General, NationId(1), ProvinceId(1));
        assert_eq!(unit_transport_size(&unit), 1);
    }

    #[test]
    fn unit_transport_size_non_general_equals_arms() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let unit = ArmyUnit::new(UnitId(1), ArmyUnitType::Guards, NationId(1), ProvinceId(1));
        assert_eq!(unit_transport_size(&unit), 3);
    }

    // ── Regression test: barges have no effect (matching original game) ──

    /// Verify that there is no "barge" concept — matching original game.
    /// TransportSystem only has freight_cars — no barges, no water transport fields.
    #[test]
    fn barges_have_no_effect() {
        let ts = TransportSystem::new();
        // TransportSystem only has freight_cars — no barges
        assert_eq!(ts.freight_cars, 0);
        // The struct has exactly two fields: freight_cars and allocations.
        // No water transport / barge functionality exists, matching the original game.
        assert!(ts.allocations.is_empty());
        // Capacity comes solely from freight cars — no barge contribution.
        assert_eq!(ts.total_capacity(), 0);
    }
}
