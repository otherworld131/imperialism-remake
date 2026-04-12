use crate::ai::AiPersonality;
use crate::economy::buildings::{Building, BuildingType};
use crate::economy::civilians::Civilian;
use crate::economy::labor::LaborPool;
use crate::economy::transport::TransportSystem;
use crate::events::TechId;
use crate::military::ships::Ship;
use crate::military::units::ArmyUnit;
use crate::types::*;
use std::collections::HashMap;

/// Colors used to distinguish nations on the map and in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
#[derive(serde::Serialize, serde::Deserialize)]
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
    /// Buildings owned by this nation.
    pub buildings: Vec<Building>,
    /// Labor pool (workers available for production).
    pub labor: LaborPool,
    /// Technologies that have been researched by this nation.
    pub researched_techs: Vec<TechId>,
    /// Army units owned by this nation.
    #[serde(default)]
    pub army: Vec<ArmyUnit>,
    /// Civilian units owned by this nation (Farmers, Foresters, Miners, Engineers).
    #[serde(default)]
    pub civilians: Vec<Civilian>,
    /// Transport system (freight cars, allocations).
    #[serde(default)]
    pub transport: TransportSystem,
    /// Merchant fleet — ships used for trade.
    #[serde(default)]
    pub merchant_fleet: Vec<Ship>,
    /// Warship fleet — military naval vessels.
    #[serde(default)]
    pub warships: Vec<Ship>,
    /// AI personality for this nation (None for human player).
    #[serde(default)]
    pub ai_personality: Option<AiPersonality>,
    /// Trade subsidies: per-Minor-Nation subsidy amounts (GP pays this per turn).
    #[serde(default)]
    pub trade_subsidies: HashMap<NationId, Money>,
    /// Trade history: records of past trade transactions for player reference.
    #[serde(default)]
    pub trade_history: Vec<crate::economy::trade::TradeHistoryEntry>,
    /// Bonus capitol capacity earned from conquering Great Power capitals.
    /// Each +1 improves worker recruitment rate (1 per 3 provinces instead of 4).
    #[serde(default)]
    pub capitol_bonus_capacity: u32,
    /// Total arms built across all army units (tracked for General rewards).
    #[serde(default)]
    pub total_arms_built: u32,
    /// Number of Generals already earned (tracks reward thresholds).
    #[serde(default)]
    pub generals_earned: u32,
    /// Total Ships-of-the-Line built (tracked for Admiral rewards).
    #[serde(default)]
    pub total_ships_of_the_line_built: u32,
    /// Number of Admirals already earned (tracks reward thresholds).
    #[serde(default)]
    pub admirals_earned: u32,
    /// Whether this nation has established its first colony (MN province conquered/incorporated).
    #[serde(default)]
    pub has_colony: bool,
    /// Number of expert worker rewards already earned (tracks reward thresholds at 10 and 30 experts).
    #[serde(default)]
    pub expert_rewards_earned: u8,
    /// Whether this nation has fallen into anarchy (lost its capital province).
    /// An anarchic nation has no economy, diplomacy, or offensive military capability.
    #[serde(default)]
    pub is_in_anarchy: bool,
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
            buildings: Vec::new(),
            labor: LaborPool::new(),
            researched_techs: Vec::new(),
            army: Vec::new(),
            civilians: Vec::new(),
            transport: TransportSystem::new(),
            merchant_fleet: Vec::new(),
            warships: Vec::new(),
            ai_personality: None,
            trade_subsidies: HashMap::new(),
            trade_history: Vec::new(),
            capitol_bonus_capacity: 0,
            total_arms_built: 0,
            generals_earned: 0,
            total_ships_of_the_line_built: 0,
            admirals_earned: 0,
            has_colony: false,
            expert_rewards_earned: 0,
            is_in_anarchy: false,
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

    /// Consume a material from the warehouse.
    /// Returns `false` if the nation does not have enough (no materials removed).
    pub fn consume_material(&mut self, material: MaterialType, amount: u32) -> bool {
        let current = self.materials.entry(material).or_insert(0);
        if *current >= amount {
            *current -= amount;
            true
        } else {
            false
        }
    }

    /// Consume a finished good from the warehouse.
    /// Returns `false` if the nation does not have enough (no goods removed).
    pub fn consume_goods(&mut self, goods: GoodsType, amount: u32) -> bool {
        let current = self.goods.entry(goods).or_insert(0);
        if *current >= amount {
            *current -= amount;
            true
        } else {
            false
        }
    }

    /// The current amount of a material in the warehouse.
    pub fn material_amount(&self, material: MaterialType) -> u32 {
        self.materials.get(&material).copied().unwrap_or(0)
    }

    /// The current amount of a finished good in the warehouse.
    pub fn goods_amount(&self, goods: GoodsType) -> u32 {
        self.goods.get(&goods).copied().unwrap_or(0)
    }

    /// Add materials to the warehouse.
    pub fn add_material(&mut self, material: MaterialType, amount: u32) {
        *self.materials.entry(material).or_insert(0) += amount;
    }

    /// Add finished goods to the warehouse.
    pub fn add_goods(&mut self, goods: GoodsType, amount: u32) {
        *self.goods.entry(goods).or_insert(0) += amount;
    }

    /// Whether this nation is a Great Power.
    pub fn is_great_power(&self) -> bool {
        self.nation_type == NationType::GreatPower
    }

    /// Get a mutable reference to a building by its type.
    pub fn get_building_mut(&mut self, building_type: BuildingType) -> Option<&mut Building> {
        self.buildings
            .iter_mut()
            .find(|b| b.building_type == building_type)
    }

    /// Check whether this nation has a building of the given type.
    pub fn has_building(&self, building_type: BuildingType) -> bool {
        self.buildings
            .iter()
            .any(|b| b.building_type == building_type)
    }

    /// Whether this nation has researched a given technology.
    pub fn has_researched(&self, tech: TechId) -> bool {
        self.researched_techs.contains(&tech)
    }

    /// Returns all army units stationed in a given province.
    pub fn units_in_province(&self, province: ProvinceId) -> Vec<&ArmyUnit> {
        self.army
            .iter()
            .filter(|u| u.position == province)
            .collect()
    }

    /// Sum of effective_firepower() for all army units.
    pub fn total_military_firepower(&self) -> f64 {
        self.army.iter().map(|u| u.effective_firepower()).sum()
    }

    /// Total cargo capacity of all merchant ships in the fleet.
    pub fn total_cargo_capacity(&self) -> u32 {
        self.merchant_fleet
            .iter()
            .map(|s| s.total_cargo_capacity())
            .sum()
    }

    /// Number of merchant ships in the fleet.
    pub fn merchant_ship_count(&self) -> usize {
        self.merchant_fleet.len()
    }

    /// Sum of firepower for all warships in the fleet.
    pub fn total_naval_firepower(&self) -> u32 {
        self.warships
            .iter()
            .map(|s| s.ship_type.stats().firepower)
            .sum()
    }

    /// Number of warships in the fleet.
    pub fn warship_count(&self) -> usize {
        self.warships.len()
    }

    /// Add a technology to this nation's researched list.
    pub fn research_tech(&mut self, tech: TechId) {
        if !self.researched_techs.contains(&tech) {
            self.researched_techs.push(tech);
        }
    }

    /// Returns true if the nation's treasury is negative (in debt).
    pub fn is_bankrupt(&self) -> bool {
        self.treasury < Money::ZERO
    }

    /// Whether this nation is in anarchy (lost its capital).
    pub fn is_in_anarchy(&self) -> bool {
        self.is_in_anarchy
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

    // ── Tech research ─────────────────────────────────────────

    #[test]
    fn new_nation_has_no_researched_techs() {
        let n = sample_great_power();
        assert!(n.researched_techs.is_empty());
    }

    #[test]
    fn has_researched_returns_false_when_empty() {
        let n = sample_great_power();
        assert!(!n.has_researched(TechId(1)));
    }

    #[test]
    fn research_tech_adds_to_list() {
        let mut n = sample_great_power();
        n.research_tech(TechId(5));
        assert!(n.has_researched(TechId(5)));
        assert_eq!(n.researched_techs.len(), 1);
    }

    #[test]
    fn research_tech_does_not_duplicate() {
        let mut n = sample_great_power();
        n.research_tech(TechId(3));
        n.research_tech(TechId(3));
        assert_eq!(n.researched_techs.len(), 1);
    }

    #[test]
    fn research_multiple_techs() {
        let mut n = sample_great_power();
        n.research_tech(TechId(1));
        n.research_tech(TechId(2));
        n.research_tech(TechId(3));
        assert!(n.has_researched(TechId(1)));
        assert!(n.has_researched(TechId(2)));
        assert!(n.has_researched(TechId(3)));
        assert!(!n.has_researched(TechId(4)));
        assert_eq!(n.researched_techs.len(), 3);
    }

    // ── Material management ──────────────────────────────────

    #[test]
    fn add_material_stores_amount() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Lumber, 5);
        assert_eq!(n.material_amount(MaterialType::Lumber), 5);
    }

    #[test]
    fn add_material_accumulates() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Steel, 3);
        n.add_material(MaterialType::Steel, 7);
        assert_eq!(n.material_amount(MaterialType::Steel), 10);
    }

    #[test]
    fn material_amount_defaults_to_zero() {
        let n = sample_great_power();
        assert_eq!(n.material_amount(MaterialType::Fabric), 0);
    }

    #[test]
    fn consume_material_sufficient() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Lumber, 10);
        assert!(n.consume_material(MaterialType::Lumber, 4));
        assert_eq!(n.material_amount(MaterialType::Lumber), 6);
    }

    #[test]
    fn consume_material_insufficient() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Steel, 3);
        assert!(!n.consume_material(MaterialType::Steel, 5));
        assert_eq!(n.material_amount(MaterialType::Steel), 3);
    }

    // ── Goods management ─────────────────────────────────────

    #[test]
    fn add_goods_stores_amount() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Furniture, 2);
        assert_eq!(n.goods_amount(GoodsType::Furniture), 2);
    }

    #[test]
    fn add_goods_accumulates() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Clothing, 3);
        n.add_goods(GoodsType::Clothing, 4);
        assert_eq!(n.goods_amount(GoodsType::Clothing), 7);
    }

    #[test]
    fn goods_amount_defaults_to_zero() {
        let n = sample_great_power();
        assert_eq!(n.goods_amount(GoodsType::Hardware), 0);
    }

    #[test]
    fn consume_goods_sufficient() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Furniture, 5);
        assert!(n.consume_goods(GoodsType::Furniture, 3));
        assert_eq!(n.goods_amount(GoodsType::Furniture), 2);
    }

    #[test]
    fn consume_goods_insufficient() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Clothing, 2);
        assert!(!n.consume_goods(GoodsType::Clothing, 5));
        assert_eq!(n.goods_amount(GoodsType::Clothing), 2);
    }

    // ── Province count after adding/removing ─────────────────

    #[test]
    fn province_count_tracks_additions() {
        let mut n = sample_great_power();
        assert_eq!(n.province_count(), 1); // starts with capital
        n.add_province(ProvinceId(11));
        n.add_province(ProvinceId(12));
        n.add_province(ProvinceId(13));
        assert_eq!(n.province_count(), 4);
    }

    #[test]
    fn province_ids_contains_all_added() {
        let mut n = sample_great_power();
        n.add_province(ProvinceId(11));
        n.add_province(ProvinceId(12));
        assert!(n.province_ids.contains(&ProvinceId(10))); // capital
        assert!(n.province_ids.contains(&ProvinceId(11)));
        assert!(n.province_ids.contains(&ProvinceId(12)));
    }

    // ── Warehouse operations don't go negative ──────────────

    #[test]
    fn remove_resource_does_not_go_negative() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Timber, 3);
        // Try to remove more than available
        let result = n.remove_resource(ResourceType::Timber, 10);
        assert!(!result);
        assert_eq!(n.resource_amount(ResourceType::Timber), 3);
    }

    #[test]
    fn consume_material_does_not_go_negative() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Steel, 2);
        let result = n.consume_material(MaterialType::Steel, 5);
        assert!(!result);
        assert_eq!(n.material_amount(MaterialType::Steel), 2);
    }

    #[test]
    fn consume_goods_does_not_go_negative() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Furniture, 1);
        let result = n.consume_goods(GoodsType::Furniture, 3);
        assert!(!result);
        assert_eq!(n.goods_amount(GoodsType::Furniture), 1);
    }

    #[test]
    fn remove_resource_from_empty_warehouse() {
        let mut n = sample_great_power();
        let result = n.remove_resource(ResourceType::Coal, 1);
        assert!(!result);
        assert_eq!(n.resource_amount(ResourceType::Coal), 0);
    }

    // ── Military firepower calculation with medals ──────────

    #[test]
    fn total_military_firepower_with_no_units() {
        let n = sample_great_power();
        assert!((n.total_military_firepower() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn total_military_firepower_with_medals() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let mut n = sample_great_power();

        // Add a Regulars unit with 2 medals
        let mut unit1 = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(10),
        );
        unit1.award_medal();
        unit1.award_medal();
        // Regulars base fp = 2, 2 medals = 1.5x => 3.0
        n.army.push(unit1);

        // Add a Guards unit with 0 medals
        let unit2 = ArmyUnit::new(UnitId(2), ArmyUnitType::Guards, NationId(1), ProvinceId(10));
        // Guards base fp = 5, 0 medals = 1.0x => 5.0
        n.army.push(unit2);

        // Total: 3.0 + 5.0 = 8.0
        assert!((n.total_military_firepower() - 8.0).abs() < f64::EPSILON);
    }

    // ── Building ownership checks ────────────────────────────

    #[test]
    fn has_building_after_adding() {
        let mut n = sample_great_power();
        n.buildings.push(Building::new(BuildingType::LumberMill, 2));
        assert!(n.has_building(BuildingType::LumberMill));
        assert!(!n.has_building(BuildingType::SteelMill));
    }

    #[test]
    fn get_building_mut_modifies_capacity() {
        let mut n = sample_great_power();
        n.buildings.push(Building::new(BuildingType::SteelMill, 1));
        let mill = n.get_building_mut(BuildingType::SteelMill).unwrap();
        mill.start_expansion(3);
        assert!(
            n.buildings
                .iter()
                .any(|b| b.building_type == BuildingType::SteelMill && b.pending_capacity == 3)
        );
    }

    // ── Trade history ────────────────────────────────────────────

    #[test]
    fn new_nation_has_empty_trade_history() {
        let n = sample_great_power();
        assert!(n.trade_history.is_empty());
    }

    #[test]
    fn trade_history_can_be_appended() {
        use crate::economy::trade::TradeHistoryEntry;

        let mut n = sample_great_power();
        n.trade_history.push(TradeHistoryEntry {
            turn: TurnNumber::new(3),
            partner: NationId(10),
            resource: ResourceType::Timber,
            quantity: 5,
            total_cost: Money::dollars(250),
        });
        assert_eq!(n.trade_history.len(), 1);
        assert_eq!(n.trade_history[0].partner, NationId(10));
        assert_eq!(n.trade_history[0].quantity, 5);
    }

    #[test]
    fn trade_history_survives_serialization() {
        use crate::economy::trade::TradeHistoryEntry;

        let mut n = sample_great_power();
        n.trade_history.push(TradeHistoryEntry {
            turn: TurnNumber::new(7),
            partner: NationId(5),
            resource: ResourceType::Iron,
            quantity: 3,
            total_cost: Money::dollars(225),
        });

        let json = serde_json::to_string(&n).unwrap();
        let deserialized: Nation = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.trade_history.len(), 1);
        assert_eq!(deserialized.trade_history[0].turn, TurnNumber::new(7));
        assert_eq!(deserialized.trade_history[0].partner, NationId(5));
    }

    // ── Bankruptcy ──────────────────────────────────────────────

    #[test]
    fn is_bankrupt_false_at_zero() {
        let n = sample_great_power();
        assert!(!n.is_bankrupt());
    }

    #[test]
    fn is_bankrupt_false_when_positive() {
        let mut n = sample_great_power();
        n.treasury = Money::dollars(1000);
        assert!(!n.is_bankrupt());
    }

    #[test]
    fn is_bankrupt_true_when_negative() {
        let mut n = sample_great_power();
        n.treasury = Money::dollars(-1);
        assert!(n.is_bankrupt());
    }
}
