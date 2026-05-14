use crate::game_state::GameState;
use crate::map::infrastructure::{collectable_hexes, is_province_connected_multi};
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
            self.per_resource.insert(
                resource,
                FreightDemand {
                    requested: req,
                    granted,
                    unmet,
                },
            );
        }
        // Resources delivered but not in `requested` (shouldn't happen but be safe).
        for &(resource, granted) in delivered {
            self.per_resource.entry(resource).or_insert_with(|| {
                committed += granted;
                FreightDemand {
                    requested: granted,
                    granted,
                    unmet: 0,
                }
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
    /// Resource allocation: how many freight units are explicitly assigned.
    pub allocations: Vec<(ResourceType, u32)>, // (resource, assigned units)
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

    /// Set the explicit freight-unit allocation for a resource type.
    /// If the resource already has an allocation, it is updated; zero removes it.
    ///
    /// **No cap is enforced here.** Effective transport capacity is
    /// `freight_cars + merchant_marine_cargo` (rail + sea), but
    /// `TransportSystem` does not know about the merchant marine, so any
    /// rail-only clamp here would prevent valid combined-pool allocations.
    /// `calculate_deliveries` enforces the hard rail-only cap at delivery
    /// time; callers (UI freight panel, AI allocator) are responsible for
    /// keeping the running total within the combined cap.
    pub fn set_allocation(&mut self, resource: ResourceType, units: u32) {
        if units == 0 {
            self.allocations.retain(|(r, _)| *r != resource);
            return;
        }
        if let Some(entry) = self.allocations.iter_mut().find(|(r, _)| *r == resource) {
            entry.1 = units;
        } else {
            self.allocations.push((resource, units));
        }
    }

    /// Given available resources from tiles, calculate how many of each resource
    /// get delivered based on capacity and allocations.
    ///
    /// Only explicitly assigned resources are delivered. Total delivered cannot
    /// exceed `freight_cars`.
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

        // Check whether any of the available resources have explicit allocations set.
        let has_allocations = nonempty.iter().any(|(r, _)| {
            self.allocations
                .iter()
                .any(|(ar, units)| *ar == *r && *units > 0)
        });

        if !has_allocations {
            return Vec::new();
        }

        let mut result: Vec<(ResourceType, u32)> = Vec::new();
        let mut remaining_capacity = capacity;

        // Explicit-unit distribution: honor assigned units directly, capped by
        // availability and remaining system capacity. Unassigned capacity stays unused.
        for (resource, avail) in &nonempty {
            let assigned = self
                .allocations
                .iter()
                .find(|(r, _)| *r == *resource)
                .map(|(_, units)| *units)
                .unwrap_or(0);

            if assigned == 0 || remaining_capacity == 0 {
                continue;
            }

            let delivered = assigned.min(*avail).min(remaining_capacity);

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
}

/// Calculate how many army units can be transported by rail.
/// 1 unit per 5 freight cars (integer division).
pub fn rail_transport_capacity(freight_cars: u32) -> u32 {
    freight_cars / 5
}

/// Calculate amphibious landing force size.
/// Force size = total arms_cost used to build all ships in the fleet.
pub fn amphibious_force_size(ships: &[Ship], data: &crate::data::GameData) -> u32 {
    ships
        .iter()
        .map(|s| data.ship_stats(s.ship_type).arms_cost)
        .sum()
}

/// Compute a pre-turn demand forecast for a nation's freight allocation panel.
///
/// Returns a list of `(ResourceType, demand_qty)` representing the expected
/// consumption of each resource next turn, based on:
/// - Food: workers eat a composite meal (1 grain + 1 fruit + 1 meat per
///   worker per turn), so every worker drives demand for one of each food
///   type. Meat demand is split between livestock and fish — livestock first
///   up to what's already held, then fish covers the rest.
/// - Mill inputs: raw-resource consumption at full building capacity.
pub fn compute_demand_forecast(
    nation: &crate::nation::Nation,
    game_data: &crate::data::GameData,
) -> Vec<(ResourceType, u32)> {
    use crate::economy::buildings::BuildingType;
    let mut demand: BTreeMap<ResourceType, u32> = BTreeMap::new();

    // Worker meal demand (Imperialism-1 ratio): grain = ⌈w/2⌉,
    // meat = ⌊w/4⌋, fruit = w − grain − meat. Canned food is a fallback only
    // and is not counted in the raw-food forecast — transport plans for the
    // primary diet.
    let total_workers = nation.economy.labor.total_workers();
    if total_workers > 0 && game_data.game_config.food_per_worker > 0 {
        let (grain_need, fruit_need, meat_need) =
            crate::economy::labor::worker_food_demand(total_workers);
        if grain_need > 0 {
            *demand.entry(ResourceType::Grain).or_insert(0) += grain_need;
        }
        if fruit_need > 0 {
            *demand.entry(ResourceType::Fruit).or_insert(0) += fruit_need;
        }
        let livestock_held = nation.resource_amount(ResourceType::Livestock);
        let livestock_demand = meat_need.min(livestock_held);
        let fish_demand = meat_need - livestock_demand;
        if livestock_demand > 0 {
            *demand.entry(ResourceType::Livestock).or_insert(0) += livestock_demand;
        }
        if fish_demand > 0 {
            *demand.entry(ResourceType::Fish).or_insert(0) += fish_demand;
        }
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

/// Compute currently collectable raw-resource yields for a nation, split into:
/// - local/free delivery (country-capital collector radii)
/// - remote/freight-gated delivery (connected depot radii and connected ports)
///
/// Returned quantities are base map yields only; caller-owned modifiers such as
/// AI difficulty bonuses should be applied after the split so both local and
/// remote sides stay consistent with turn-production accounting.
pub fn current_collectable_resources(
    game: &GameState,
    nation_id: NationId,
) -> (Vec<(ResourceType, u32)>, Vec<(ResourceType, u32)>) {
    let Some(_nation) = game.get_nation(nation_id) else {
        return (Vec::new(), Vec::new());
    };

    let owned: Vec<&crate::map::Province> = game
        .world
        .provinces
        .iter()
        .filter(|p| p.owner == nation_id)
        .collect();
    if owned.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let capital_tiles: Vec<crate::hex::HexCoord> = owned
        .iter()
        .flat_map(|p| p.tiles.iter().copied())
        .filter(|&coord| {
            game.world
                .hex_map
                .get_tile(coord)
                .is_some_and(|t| t.is_country_capital)
        })
        .collect();
    if capital_tiles.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let connected: std::collections::HashSet<ProvinceId> = owned
        .iter()
        .filter(|p| {
            is_province_connected_multi(
                &game.world.hex_map,
                &capital_tiles,
                p.id,
                &game.world.provinces,
            )
        })
        .map(|p| p.id)
        .collect();

    let collectable = collectable_hexes(&game.world.hex_map, &owned, &connected);
    let owned_tiles: std::collections::HashSet<crate::hex::HexCoord> =
        owned.iter().flat_map(|p| p.tiles.iter().copied()).collect();
    let mut local_hexes: std::collections::HashSet<crate::hex::HexCoord> =
        std::collections::HashSet::new();
    for &capital in &capital_tiles {
        local_hexes.insert(capital);
        for neighbor in capital.neighbors() {
            if owned_tiles.contains(&neighbor) {
                local_hexes.insert(neighbor);
            }
        }
    }

    let mut local: BTreeMap<ResourceType, u32> = BTreeMap::new();
    let mut remote: BTreeMap<ResourceType, u32> = BTreeMap::new();

    let mut add_yield = |coord: crate::hex::HexCoord, resource: ResourceType, qty: u32| {
        if qty == 0 {
            return;
        }
        let bucket = if local_hexes.contains(&coord) {
            &mut local
        } else {
            &mut remote
        };
        *bucket.entry(resource).or_insert(0) += qty;
    };

    for province in &owned {
        for &coord in &province.tiles {
            if !collectable.contains(&coord) {
                continue;
            }
            let Some(tile) = game.world.hex_map.get_tile(coord) else {
                continue;
            };
            if let Some(y) = tile.calculate_yield() {
                add_yield(coord, y.resource, y.quantity);
            }

            let is_port = tile.infrastructure.has_port;
            let is_coastal_capital = tile.is_country_capital
                && coord.neighbors().iter().any(|n| {
                    game.world
                        .hex_map
                        .get_tile(*n)
                        .is_some_and(|t| !t.terrain().is_land())
                });
            if !is_port && !is_coastal_capital {
                continue;
            }
            let fish_qty = coord
                .neighbors()
                .iter()
                .filter(|n| {
                    let Some(t) = game.world.hex_map.get_tile(**n) else {
                        return false;
                    };
                    if t.terrain().is_land() {
                        return false;
                    }
                    !game
                        .world
                        .sea_zones
                        .iter()
                        .any(|z| z.is_lake && z.hexes.contains(*n))
                })
                .count() as u32;
            add_yield(coord, ResourceType::Fish, fish_qty.min(3));
        }
    }

    (local.into_iter().collect(), remote.into_iter().collect())
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

    // ── No delivery without allocations ──────────────────────────

    #[test]
    fn no_delivery_without_allocations_single_resource() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(5);

        let available = vec![(ResourceType::Timber, 10)];
        let deliveries = ts.calculate_deliveries(&available);

        assert!(deliveries.is_empty());
    }

    #[test]
    fn no_delivery_without_allocations_two_resources() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);

        let available = vec![(ResourceType::Timber, 20), (ResourceType::Coal, 20)];
        let deliveries = ts.calculate_deliveries(&available);

        assert!(deliveries.is_empty());
    }

    #[test]
    fn no_delivery_without_allocations_capped_by_availability() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);

        let available = vec![(ResourceType::Timber, 3), (ResourceType::Coal, 20)];
        let deliveries = ts.calculate_deliveries(&available);

        assert!(deliveries.is_empty());
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
        ts.set_allocation(ResourceType::Timber, 7);
        ts.set_allocation(ResourceType::Coal, 3);

        let available = vec![(ResourceType::Timber, 20), (ResourceType::Coal, 20)];
        let deliveries = ts.calculate_deliveries(&available);

        let timber = deliveries.iter().find(|(r, _)| *r == ResourceType::Timber);
        let coal = deliveries.iter().find(|(r, _)| *r == ResourceType::Coal);

        assert_eq!(timber, Some(&(ResourceType::Timber, 7)));
        assert_eq!(coal, Some(&(ResourceType::Coal, 3)));
    }

    #[test]
    fn allocation_capped_by_availability() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(10);
        ts.set_allocation(ResourceType::Timber, 8);
        ts.set_allocation(ResourceType::Coal, 2);

        let available = vec![
            (ResourceType::Timber, 3), // only 3 available despite 8 assigned units
            (ResourceType::Coal, 20),
        ];
        let deliveries = ts.calculate_deliveries(&available);

        let timber = deliveries.iter().find(|(r, _)| *r == ResourceType::Timber);
        let coal = deliveries.iter().find(|(r, _)| *r == ResourceType::Coal);

        // Timber is capped at 3; the unused 5 assigned units remain unused.
        assert_eq!(timber, Some(&(ResourceType::Timber, 3)));
        assert_eq!(coal, Some(&(ResourceType::Coal, 2)));
    }

    #[test]
    fn set_allocation_updates_existing() {
        let mut ts = TransportSystem::new();
        ts.set_allocation(ResourceType::Timber, 5);
        ts.set_allocation(ResourceType::Timber, 8);

        assert_eq!(ts.allocations.len(), 1);
        assert_eq!(ts.allocations[0], (ResourceType::Timber, 8));
    }

    #[test]
    fn set_allocation_does_not_clamp_per_unit() {
        // `set_allocation` no longer caps against `freight_cars` — the cap moved
        // to `calculate_deliveries` so that combined rail+sea allocations work
        // (Trello bug #461). Over-allocation is still bounded at delivery time.
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(5);
        ts.set_allocation(ResourceType::Timber, 4);
        ts.set_allocation(ResourceType::Coal, 4);

        let timber = ts
            .allocations
            .iter()
            .find(|(r, _)| *r == ResourceType::Timber)
            .map(|(_, units)| *units);
        let coal = ts
            .allocations
            .iter()
            .find(|(r, _)| *r == ResourceType::Coal)
            .map(|(_, units)| *units);

        assert_eq!(timber, Some(4));
        assert_eq!(coal, Some(4));

        // Capacity is still enforced at delivery time.
        let available = vec![(ResourceType::Timber, 100), (ResourceType::Coal, 100)];
        let total: u32 = ts
            .calculate_deliveries(&available)
            .iter()
            .map(|(_, q)| *q)
            .sum();
        assert!(total <= 5, "delivery still capped by freight_cars");
    }

    #[test]
    fn delivery_total_does_not_exceed_capacity() {
        let mut ts = TransportSystem::new();
        ts.build_freight_cars(5);
        ts.set_allocation(ResourceType::Timber, 5);
        ts.set_allocation(ResourceType::Coal, 5);

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
    fn army_transport_size_siege_artillery() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::SiegeArtillery,
            NationId(1),
            ProvinceId(1),
        );
        // SiegeArtillery arms_required = 3 in the original-game manual.
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

        // SiegeArtillery arms_required = 3 in the original-game manual.
        let unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::SiegeArtillery,
            NationId(1),
            ProvinceId(1),
        );
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
        assert!(
            forecast.is_empty(),
            "no demand expected when nation has 0 workers and no buildings"
        );
    }

    #[test]
    fn demand_forecast_imperial_ration_demands_all_three_slots() {
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        // 8 workers → ⌈8/2⌉=4 grain, ⌊8/4⌋=2 meat, 8-4-2=2 fruit.
        nation.economy.labor.untrained = 8;
        // No livestock held, so the entire meat slot falls to fish.
        let forecast = compute_demand_forecast(&nation, &data);
        let grain = forecast
            .iter()
            .find(|(r, _)| *r == ResourceType::Grain)
            .map(|(_, q)| *q);
        let fruit = forecast
            .iter()
            .find(|(r, _)| *r == ResourceType::Fruit)
            .map(|(_, q)| *q);
        let fish = forecast
            .iter()
            .find(|(r, _)| *r == ResourceType::Fish)
            .map(|(_, q)| *q);
        let livestock = forecast
            .iter()
            .find(|(r, _)| *r == ResourceType::Livestock)
            .map(|(_, q)| *q);
        assert_eq!(grain, Some(4));
        assert_eq!(fruit, Some(2));
        assert_eq!(fish, Some(2));
        assert_eq!(livestock, None);
    }

    #[test]
    fn demand_forecast_livestock_consumed_first_for_meat_slot() {
        let mut nation = make_test_nation();
        let data = crate::data::GameData::default();
        // 12 workers → meat slot = 3 units. Livestock=1 fills first; fish=2 covers the rest.
        nation.economy.labor.untrained = 12;
        nation.add_resource(ResourceType::Livestock, 1);
        let forecast = compute_demand_forecast(&nation, &data);
        let livestock = forecast
            .iter()
            .find(|(r, _)| *r == ResourceType::Livestock)
            .map(|(_, q)| *q);
        let fish = forecast
            .iter()
            .find(|(r, _)| *r == ResourceType::Fish)
            .map(|(_, q)| *q);
        assert_eq!(
            livestock,
            Some(1),
            "livestock fills meat slot up to held amount"
        );
        assert_eq!(fish, Some(2), "fish covers the remaining meat demand");
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
