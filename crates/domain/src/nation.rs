use crate::types::*;
use std::collections::HashMap;

/// Colors used to distinguish nations on the map and in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NationColor {
    // Great Power colors
    Yellow,
    Orange,
    LightBlue,
    Red,
    Green,
    Purple,
    Blue,
    // Minor nation colors
    Gray,
    Brown,
    Pink,
    Teal,
    Olive,
    Maroon,
    Navy,
    Cyan,
    Lime,
    Coral,
    Lavender,
    Tan,
    Salmon,
    Khaki,
    Indigo,
}

/// A nation in the game — either a Great Power (player-controlled or AI)
/// or a Minor Nation (AI-only, can be annexed or allied).
pub struct Nation {
    pub id: NationId,
    pub name: String,
    pub color: NationColor,
    pub nation_type: NationType,
    pub treasury: Money,
    pub province_ids: Vec<ProvinceId>,
    pub capital_province_id: ProvinceId,
    /// Resource warehouse — stores raw resources.
    pub warehouse: HashMap<ResourceType, u32>,
    /// Processed materials warehouse.
    pub materials: HashMap<MaterialType, u32>,
    /// Finished goods warehouse.
    pub goods: HashMap<GoodsType, u32>,
}

impl Nation {
    /// Create a new nation with an empty treasury and empty warehouses.
    pub fn new(
        id: NationId,
        name: String,
        color: NationColor,
        nation_type: NationType,
        capital_province_id: ProvinceId,
    ) -> Self {
        Self {
            id,
            name,
            color,
            nation_type,
            treasury: Money::ZERO,
            province_ids: vec![capital_province_id],
            capital_province_id,
            warehouse: HashMap::new(),
            materials: HashMap::new(),
            goods: HashMap::new(),
        }
    }

    /// Add a province to this nation's control.
    pub fn add_province(&mut self, province_id: ProvinceId) {
        if !self.province_ids.contains(&province_id) {
            self.province_ids.push(province_id);
        }
    }

    /// The number of provinces controlled by this nation.
    pub fn province_count(&self) -> usize {
        self.province_ids.len()
    }

    /// Add raw resources to the warehouse.
    pub fn add_resource(&mut self, resource: ResourceType, amount: u32) {
        *self.warehouse.entry(resource).or_insert(0) += amount;
    }

    /// Remove raw resources from the warehouse.
    /// Returns `false` if the nation does not have enough of the resource
    /// (no resources are removed in that case).
    pub fn remove_resource(&mut self, resource: ResourceType, amount: u32) -> bool {
        let current = self.warehouse.entry(resource).or_insert(0);
        if *current >= amount {
            *current -= amount;
            true
        } else {
            false
        }
    }

    /// The current amount of a raw resource in the warehouse.
    pub fn resource_amount(&self, resource: ResourceType) -> u32 {
        self.warehouse.get(&resource).copied().unwrap_or(0)
    }

    /// Whether this nation is a Great Power.
    pub fn is_great_power(&self) -> bool {
        self.nation_type == NationType::GreatPower
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a sample Great Power nation for testing.
    fn sample_great_power() -> Nation {
        Nation::new(
            NationId(1),
            "France".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(10),
        )
    }

    /// Helper: build a sample Minor Nation for testing.
    fn sample_minor_nation() -> Nation {
        Nation::new(
            NationId(8),
            "Bavaria".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        )
    }

    // ── Construction ───────────────────────────────────────────

    #[test]
    fn new_nation_has_correct_fields() {
        let n = sample_great_power();
        assert_eq!(n.id, NationId(1));
        assert_eq!(n.name, "France");
        assert_eq!(n.color, NationColor::Blue);
        assert_eq!(n.nation_type, NationType::GreatPower);
        assert_eq!(n.capital_province_id, ProvinceId(10));
    }

    #[test]
    fn new_nation_starts_with_zero_treasury() {
        let n = sample_great_power();
        assert_eq!(n.treasury, Money::ZERO);
    }

    #[test]
    fn new_nation_has_capital_in_provinces() {
        let n = sample_great_power();
        assert!(n.province_ids.contains(&ProvinceId(10)));
        assert_eq!(n.province_count(), 1);
    }

    #[test]
    fn new_nation_has_empty_warehouses() {
        let n = sample_great_power();
        assert!(n.warehouse.is_empty());
        assert!(n.materials.is_empty());
        assert!(n.goods.is_empty());
    }

    // ── is_great_power ────────────────────────────────────────

    #[test]
    fn great_power_returns_true() {
        let n = sample_great_power();
        assert!(n.is_great_power());
    }

    #[test]
    fn minor_nation_returns_false() {
        let n = sample_minor_nation();
        assert!(!n.is_great_power());
    }

    // ── Province management ───────────────────────────────────

    #[test]
    fn add_province_increases_count() {
        let mut n = sample_great_power();
        assert_eq!(n.province_count(), 1);
        n.add_province(ProvinceId(11));
        assert_eq!(n.province_count(), 2);
        n.add_province(ProvinceId(12));
        assert_eq!(n.province_count(), 3);
    }

    #[test]
    fn add_duplicate_province_does_not_increase_count() {
        let mut n = sample_great_power();
        n.add_province(ProvinceId(11));
        assert_eq!(n.province_count(), 2);
        n.add_province(ProvinceId(11));
        assert_eq!(n.province_count(), 2);
    }

    #[test]
    fn add_capital_province_again_does_not_duplicate() {
        let mut n = sample_great_power();
        n.add_province(ProvinceId(10)); // capital already present
        assert_eq!(n.province_count(), 1);
    }

    // ── Resource management ───────────────────────────────────

    #[test]
    fn add_resource_stores_amount() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Timber, 5);
        assert_eq!(n.resource_amount(ResourceType::Timber), 5);
    }

    #[test]
    fn add_resource_accumulates() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Iron, 3);
        n.add_resource(ResourceType::Iron, 7);
        assert_eq!(n.resource_amount(ResourceType::Iron), 10);
    }

    #[test]
    fn resource_amount_defaults_to_zero() {
        let n = sample_great_power();
        assert_eq!(n.resource_amount(ResourceType::Coal), 0);
    }

    #[test]
    fn remove_resource_sufficient() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Cotton, 10);
        let result = n.remove_resource(ResourceType::Cotton, 4);
        assert!(result);
        assert_eq!(n.resource_amount(ResourceType::Cotton), 6);
    }

    #[test]
    fn remove_resource_exact_amount() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Grain, 5);
        let result = n.remove_resource(ResourceType::Grain, 5);
        assert!(result);
        assert_eq!(n.resource_amount(ResourceType::Grain), 0);
    }

    #[test]
    fn remove_resource_insufficient() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Gold, 3);
        let result = n.remove_resource(ResourceType::Gold, 5);
        assert!(!result);
        // Amount should remain unchanged
        assert_eq!(n.resource_amount(ResourceType::Gold), 3);
    }

    #[test]
    fn remove_resource_not_present() {
        let mut n = sample_great_power();
        let result = n.remove_resource(ResourceType::Oil, 1);
        assert!(!result);
    }

    #[test]
    fn multiple_resource_types_independent() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Timber, 10);
        n.add_resource(ResourceType::Coal, 20);
        n.add_resource(ResourceType::Iron, 15);

        assert_eq!(n.resource_amount(ResourceType::Timber), 10);
        assert_eq!(n.resource_amount(ResourceType::Coal), 20);
        assert_eq!(n.resource_amount(ResourceType::Iron), 15);

        n.remove_resource(ResourceType::Coal, 5);
        assert_eq!(n.resource_amount(ResourceType::Coal), 15);
        // Others unchanged
        assert_eq!(n.resource_amount(ResourceType::Timber), 10);
        assert_eq!(n.resource_amount(ResourceType::Iron), 15);
    }
}
