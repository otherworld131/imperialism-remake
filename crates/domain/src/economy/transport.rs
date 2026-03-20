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
}
