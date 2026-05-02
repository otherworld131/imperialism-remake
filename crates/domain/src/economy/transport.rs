use crate::military::ships::Ship;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;
use std::collections::BTreeMap;

/// Per-commodity freight allocation result: how much was requested vs granted (Trello #165).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FreightDemand {
    /// Freight capacity requested for this commodity.
    pub requested: u32,
    /// Freight capacity actually granted (≤ requested, limited by total capacity).
    pub granted: u32,
    /// Unmet freight demand: `requested - granted`.
    pub unmet: u32,
}

/// Explicit logistics state for a nation — tracks freight capacity usage per turn (Trello #165).
///
/// Populated by the turn processor after transport resolution completes.
/// Exposes the codebase answer to "what freight is available?", "what is committed?",
/// and "which resources are starved by transport capacity?".
///
/// Card #84: rail freight (`freight_total`) and merchant-marine cargo
/// (`sea_total`) are tracked as separate components of the unified
/// remote-delivery budget so the UI can show "rail X / sea Y" and the
/// player understands which leg of the supply chain is the bottleneck.
#[derive(Debug, Clone, Default)]
pub struct LogisticsState {
    /// Total *combined* (rail + sea) remote-delivery capacity.
    pub freight_total: u32,
    /// Freight capacity consumed this turn (sum of `granted` across all resources).
    pub freight_committed: u32,
    /// Remaining freight capacity after all deliveries: `freight_total - freight_committed`.
    pub freight_unused: u32,
    /// Card #84: rail-only component of `freight_total` (sum of freight cars).
    pub rail_total: u32,
    /// Card #84: sea-only component of `freight_total` (merchant-marine cargo).
    pub sea_total: u32,
    /// Per-resource freight demand: how much was requested vs actually delivered.
    pub per_resource: BTreeMap<ResourceType, FreightDemand>,
}

impl LogisticsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update logistics state from a completed delivery run.
    ///
    /// `rail_capacity` and `sea_capacity` are the rail and merchant-marine
    /// components; their sum is the total freight capacity. `requested`
    /// lists what each resource wanted; `delivered` lists what was actually
    /// granted. (Card #84.)
    pub fn update(
        &mut self,
        rail_capacity: u32,
        sea_capacity: u32,
        requested: &[(ResourceType, u32)],
        delivered: &[(ResourceType, u32)],
    ) {
        let capacity = rail_capacity + sea_capacity;
        self.freight_total = capacity;
        self.rail_total = rail_capacity;
        self.sea_total = sea_capacity;
        self.per_resource.clear();

        let mut committed = 0u32;
        for &(resource, req) in requested {
            let granted = delivered
                .iter()
                .find(|(r, _)| *r == resource)
                .map(|(_, q)| *q)
                .unwrap_or(0);
            let unmet = req.saturating_sub(granted);
            committed += granted;
            self.per_resource.insert(resource, FreightDemand { requested: req, granted, unmet });
        }
        // Resources delivered but not in `requested` (shouldn't happen but be safe).
        for &(resource, granted) in delivered {
            self.per_resource.entry(resource).or_insert_with(|| {
                committed += granted;
                FreightDemand { requested: granted, granted, unmet: 0 }
            });
        }
        self.freight_committed = committed;
        self.freight_unused = capacity.saturating_sub(committed);
    }
}

/// The transport system for a nation — freight cars carrying resources.
#[derive(Debug, Clone)]
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
    /// Values above 100 are clamped to 100 (fix: Areas-4 finding #8).
    pub fn set_allocation(&mut self, resource: ResourceType, percentage: u32) {
        let pct = percentage.min(100);
        if let Some(entry) = self.allocations.iter_mut().find(|(r, _)| *r == resource) {
            entry.1 = pct;
        } else {
            self.allocations.push((resource, pct));
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

        let mut result: Vec<(ResourceType, u32)> = Vec::new();
        let mut remaining_capacity = capacity;

        // First pass: allocate proportional shares to resources with explicit allocations.
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

        // Second pass: redistribute any wasted capacity and serve unallocated resources.
        // Wasted capacity arises when a resource's availability is less than its share
        // (fix: Areas-4 finding #1). Unallocated resources are those with no explicit
        // allocation entry but with available quantity (fix: Areas-4 finding #2).
        if remaining_capacity > 0 {
            let unallocated: Vec<(ResourceType, u32)> = nonempty
                .iter()
                .filter(|(r, _)| {
                    !self.allocations.iter().any(|(ar, pct)| *ar == *r && *pct > 0)
                })
                .map(|(r, avail)| {
                    // Remaining demand: subtract what was already delivered in pass 1.
                    let already = result.iter().find(|(dr, _)| *dr == *r).map(|(_, q)| *q).unwrap_or(0);
                    (*r, avail.saturating_sub(already))
                })
                .filter(|(_, demand)| *demand > 0)
                .collect();

            // Also give already-allocated resources a second bite if they had leftover demand.
            let second_round: Vec<(ResourceType, u32)> = nonempty
                .iter()
                .filter_map(|(r, avail)| {
                    let already =
                        result.iter().find(|(dr, _)| *dr == *r).map(|(_, q)| *q).unwrap_or(0);
                    let remaining_demand = avail.saturating_sub(already);
                    if remaining_demand > 0 { Some((*r, remaining_demand)) } else { None }
                })
                .filter(|(r, _)| !unallocated.iter().any(|(ur, _)| ur == r))
                .collect();

            let all_leftovers: Vec<(ResourceType, u32)> =
                unallocated.into_iter().chain(second_round).collect();

            if !all_leftovers.is_empty() {
                let extra =
                    Self::distribute_evenly_partial(&all_leftovers, remaining_capacity);
                for (r, qty) in extra {
                    if let Some(entry) = result.iter_mut().find(|(dr, _)| *dr == r) {
                        entry.1 += qty;
                    } else {
                        result.push((r, qty));
                    }
                    remaining_capacity = remaining_capacity.saturating_sub(qty);
                }
            }
        }

        result
    }

    /// 1 army unit per 5 freight cars (integer division).
    pub fn military_transport_capacity(&self) -> u32 {
        self.freight_cars / 5
    }

    /// Distribute `capacity` evenly across `demands`, capping each entry by its
    /// requested amount. Helper for both even-distribution mode and the second
    /// pass of allocation-based distribution.
    fn distribute_evenly_partial(
        demands: &[(ResourceType, u32)],
        capacity: u32,
    ) -> Vec<(ResourceType, u32)> {
        Self::distribute_evenly(demands, capacity)
    }

    /// Distribute capacity evenly across available resources, capped by each
    /// resource's availability.
    ///
    /// Iterates until capacity is exhausted or all demand is met so that capacity
    /// freed by demand-capped resources is redistributed to others rather than wasted.
    fn distribute_evenly(
        available: &[(ResourceType, u32)],
        capacity: u32,
    ) -> Vec<(ResourceType, u32)> {
        let mut remaining_demand: Vec<(ResourceType, u32)> = available
            .iter()
            .filter(|(_, qty)| *qty > 0)
            .copied()
            .collect();
        let mut result: Vec<(ResourceType, u32)> = Vec::new();
        let mut remaining_capacity = capacity;

        loop {
            let active_count = remaining_demand.iter().filter(|(_, d)| *d > 0).count() as u32;
            if active_count == 0 || remaining_capacity == 0 {
                break;
            }
            let base = remaining_capacity / active_count;
            let mut extra = remaining_capacity % active_count;
            let mut granted_this_round = 0u32;

            for (resource, demand) in remaining_demand.iter_mut() {
                if *demand == 0 || remaining_capacity == 0 {
                    continue;
                }
                let mut share = base;
                if extra > 0 {
                    share += 1;
                    extra -= 1;
                }
                let granted = share.min(*demand).min(remaining_capacity);
                if granted > 0 {
                    *demand -= granted;
                    remaining_capacity -= granted;
                    granted_this_round += granted;
                    if let Some(entry) = result.iter_mut().find(|(r, _)| *r == *resource) {
                        entry.1 += granted;
                    } else {
                        result.push((*resource, granted));
                    }
                }
            }

            // If nothing was granted this round (all active resources have demand=0 or
            // capacity=0), avoid infinite loop.
            if granted_this_round == 0 {
                break;
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
pub fn amphibious_force_size(ships: &[Ship], data: &crate::data::GameData) -> u32 {
    ships.iter().map(|s| data.ship_stats(s.ship_type).arms_cost).sum()
}

/// Compute a pre-turn demand forecast for a nation's freight allocation panel.
///
/// Returns a list of `(ResourceType, demand_qty)` representing the expected
/// consumption of each resource next turn, based on:
/// - Food: `total_workers × food_per_worker` split across held food resources
///   (Grain/Fruit/Livestock), or defaulting to Grain if none are held.
/// - Mill inputs: raw-resource consumption at full building capacity.
pub fn compute_demand_forecast(
    nation: &crate::nation::Nation,
    game_data: &crate::data::GameData,
) -> Vec<(ResourceType, u32)> {
    use crate::economy::buildings::BuildingType;
    let mut demand: BTreeMap<ResourceType, u32> = BTreeMap::new();

    // Food demand: workers × food_per_worker shown on one canonical food resource.
    // Priority matches actual consumption order: Grain → Fruit → Livestock → Fish.
    // Using the first held type so the UI shows a single clear demand signal.
    let total_workers = nation.economy.labor.total_workers();
    let food_per_worker = game_data.game_config.food_per_worker;
    if total_workers > 0 && food_per_worker > 0 {
        let food_demand = total_workers.saturating_mul(food_per_worker);
        let food_types = [
            ResourceType::Grain,
            ResourceType::Fruit,
            ResourceType::Livestock,
            ResourceType::Fish,
        ];
        let canonical = food_types
            .iter()
            .copied()
            .find(|&r| nation.resource_amount(r) > 0)
            .unwrap_or(ResourceType::Grain);
        *demand.entry(canonical).or_insert(0) += food_demand;
    }

    // Mill demand: raw-resource consumption at full building capacity.
    for building in &nation.economy.buildings {
        let cap = building.effective_capacity();
        if cap == 0 {
            continue;
        }
        match building.building_type {
            BuildingType::LumberMill => {
                *demand.entry(ResourceType::Timber).or_insert(0) += cap.saturating_mul(2);
            }
            BuildingType::SteelMill => {
                *demand.entry(ResourceType::Coal).or_insert(0) += cap;
                *demand.entry(ResourceType::Iron).or_insert(0) += cap;
            }
            BuildingType::TextileMill | BuildingType::AdvancedTextileMill => {
                *demand.entry(ResourceType::Cotton).or_insert(0) += cap.saturating_mul(2);
            }
            _ => {}
        }
    }

    demand.into_iter().collect()
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

        // Timber gets min(5, 3) = 3 and the unused 2-capacity remainder is
        // redistributed to Coal instead of being dropped.
        let timber = deliveries.iter().find(|(r, _)| *r == ResourceType::Timber);
        let coal = deliveries.iter().find(|(r, _)| *r == ResourceType::Coal);

        assert_eq!(timber, Some(&(ResourceType::Timber, 3)));
        assert_eq!(coal, Some(&(ResourceType::Coal, 7)));
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

        // Timber is capped at 3; the unused 5 capacity from its 80% share is
        // redistributed, so Coal receives 2 + 5 = 7.
        assert_eq!(timber, Some(&(ResourceType::Timber, 3)));
        assert_eq!(coal, Some(&(ResourceType::Coal, 7)));
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
        let data = crate::data::GameData::default();
        let ships: Vec<Ship> = vec![];
        assert_eq!(amphibious_force_size(&ships, &data), 0);
    }

    #[test]
    fn amphibious_force_size_frigates() {
        use crate::map::UnitId;
        use crate::military::ships::ShipType;

        let data = crate::data::GameData::default();
        // 4 frigates, each with arms_cost = 2
        let ships: Vec<Ship> = (0..4)
            .map(|i| Ship::with_data(UnitId(i), ShipType::Frigate, NationId(1), &data))
            .collect();
        assert_eq!(amphibious_force_size(&ships, &data), 8);
    }

    #[test]
    fn amphibious_force_size_mixed_fleet() {
        use crate::map::UnitId;
        use crate::military::ships::ShipType;

        let data = crate::data::GameData::default();
        // Mix of warships with different arms_cost values
        let ships = vec![
            Ship::with_data(UnitId(1), ShipType::Frigate, NationId(1), &data), // arms_cost = 2
            Ship::with_data(UnitId(2), ShipType::ShipOfTheLine, NationId(1), &data), // arms_cost = 5
            Ship::with_data(UnitId(3), ShipType::Dreadnought, NationId(1), &data), // arms_cost = 8
        ];
        assert_eq!(amphibious_force_size(&ships, &data), 2 + 5 + 8);
    }

    #[test]
    fn amphibious_force_size_merchant_ships_have_zero_arms() {
        use crate::map::UnitId;
        use crate::military::ships::ShipType;

        let data = crate::data::GameData::default();
        let ships = vec![
            Ship::with_data(UnitId(1), ShipType::Trader, NationId(1), &data), // arms_cost = 0
            Ship::with_data(UnitId(2), ShipType::Clipper, NationId(1), &data), // arms_cost = 0
        ];
        assert_eq!(amphibious_force_size(&ships, &data), 0);
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

    // ── compute_demand_forecast tests ──

    fn make_test_nation() -> crate::nation::Nation {
        use crate::nation::NationColor;
        crate::nation::Nation::new(
            NationId(1),
            "Test".to_string(),
            NationColor::Blue,
            crate::types::NationType::GreatPower,
            ProvinceId(1),
        )
    }

    #[test]
    fn demand_forecast_no_workers_no_food_demand() {
        let nation = make_test_nation();
        let data = crate::data::GameData::default();
        let forecast = compute_demand_forecast(&nation, &data);
        // No workers → no food demand at all.
        assert!(forecast.is_empty(),
            "no demand expected when nation has 0 workers and no buildings");
    }

    #[test]
    fn demand_forecast_uses_fish_when_only_fish_held() {
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        // Add a worker so food demand is nonzero.
        nation.economy.labor.untrained = 1;
        // Give Fish only (no Grain/Fruit/Livestock).
        nation.add_resource(ResourceType::Fish, 10);
        let forecast = compute_demand_forecast(&nation, &data);
        let fish_demand = forecast.iter().find(|(r, _)| *r == ResourceType::Fish).map(|(_, q)| *q);
        let grain_demand = forecast.iter().find(|(r, _)| *r == ResourceType::Grain).map(|(_, q)| *q);
        assert!(fish_demand.is_some(), "fish demand should appear when only fish is held");
        assert!(grain_demand.is_none(), "grain demand should not appear when fish is canonical food");
    }

    #[test]
    fn demand_forecast_grain_takes_priority_over_fish() {
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        nation.economy.labor.untrained = 1;
        // Grain takes priority over Fish.
        nation.add_resource(ResourceType::Grain, 5);
        nation.add_resource(ResourceType::Fish, 5);
        let forecast = compute_demand_forecast(&nation, &data);
        let grain_demand = forecast.iter().find(|(r, _)| *r == ResourceType::Grain).map(|(_, q)| *q);
        let fish_demand = forecast.iter().find(|(r, _)| *r == ResourceType::Fish).map(|(_, q)| *q);
        assert!(grain_demand.is_some(), "grain is canonical when both grain and fish held");
        assert!(fish_demand.is_none(), "fish demand should not appear when grain is canonical");
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
