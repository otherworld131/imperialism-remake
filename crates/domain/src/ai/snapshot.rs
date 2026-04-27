//! Pre-computed per-nation economy snapshot for AI planning (Trello #163).
//!
//! `NationEconomySnapshot::build` captures everything the AI economy
//! planners need from `GameState` in a single immutable struct. Planners
//! read the snapshot instead of reaching into `GameState` directly, which
//! makes them independently testable with synthetic data and prevents
//! mid-tick state drift.

use crate::economy::buildings::BuildingType;
use crate::economy::labor::WorkerType;
use crate::economy::market::Trend;
use crate::economy::observability::PendingEconomyOrder;
use crate::economy::trade::Commodity;
use crate::economy::transport::LogisticsState;
use crate::game_state::GameState;
use crate::types::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryView {
    pub total: u32,
    pub reserved: u32,
    pub available: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreasuryView {
    pub total: Money,
    pub reserved: Money,
    pub available: Money,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaborTierView {
    pub total: u32,
    pub reserved: u32,
    pub available: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaborView {
    pub by_tier: HashMap<WorkerType, LaborTierView>,
    pub total_workers: u32,
    pub reserved_workers: u32,
    pub available_workers: u32,
    pub total_units: u32,
    pub reserved_units: u32,
    pub available_units: u32,
}

/// A lightweight snapshot of a nation's production-relevant economy state.
///
/// All fields are pre-computed once per AI tick. Mutation still goes through
/// `&mut GameState` — this struct is read-only.
#[derive(Debug, Clone)]
pub struct NationEconomySnapshot {
    pub nation_id: NationId,

    // ── Treasury ─────────────────────────────────────────────────────────────
    pub treasury: Money,
    pub treasury_view: TreasuryView,

    // ── Inventory (total stored quantities) ─────────────────────────────────
    pub inventory: HashMap<Commodity, u32>,
    pub inventory_view: HashMap<Commodity, InventoryView>,

    // ── Buildings (type → effective capacity, 0 = not present) ──────────────
    pub buildings: HashMap<BuildingType, u32>,
    /// Pending expansion capacity per building (0 = no expansion in progress).
    pub pending_capacities: HashMap<BuildingType, u32>,

    // ── Labor ────────────────────────────────────────────────────────────────
    pub total_workers: u32,
    pub labor: LaborView,

    // ── Logistics ───────────────────────────────────────────────────────────
    /// Total freight-car transport capacity for this nation.
    pub freight_capacity: u32,
    pub logistics: LogisticsState,

    // ── Pending pre-execution state ─────────────────────────────────────────
    pub pending_orders: Vec<PendingEconomyOrder>,

    // ── Market view (#164) ───────────────────────────────────────────────────
    /// Current market price per commodity (empty until the first trade turn).
    pub market_prices: HashMap<Commodity, Money>,
    /// Price trend per commodity over the last 4 turns.
    pub market_trends: HashMap<Commodity, Trend>,
}

impl NationEconomySnapshot {
    /// Build a snapshot from live game state for the given nation.
    pub fn build(state: &GameState, nation_id: NationId) -> Self {
        let Some(nation) = state.get_nation(nation_id) else {
            return Self::empty(nation_id);
        };

        let inventory: HashMap<Commodity, u32> = nation
            .economy
            .iter_all()
            .filter(|(_, qty)| *qty > 0)
            .collect();
        let mut inventory_view = HashMap::new();
        for commodity in inventory
            .keys()
            .chain(nation.economy.reserved_inventory().keys())
            .copied()
        {
            inventory_view.entry(commodity).or_insert_with(|| InventoryView {
                total: nation.economy.amount(commodity),
                reserved: nation.economy.reserved(commodity),
                available: nation.economy.available(commodity),
            });
        }

        let buildings: HashMap<BuildingType, u32> = nation
            .economy
            .buildings
            .iter()
            .map(|b| (b.building_type, b.effective_capacity()))
            .collect();

        let pending_capacities: HashMap<BuildingType, u32> = nation
            .economy
            .buildings
            .iter()
            .map(|b| (b.building_type, b.pending_capacity))
            .collect();

        // Snapshot market prices and trends for all commodities with history (#164).
        let market_prices: HashMap<Commodity, Money> =
            state.world.market_state.commodities_with_price().collect();
        let market_trends: HashMap<Commodity, Trend> = state
            .world.market_state
            .commodities_with_history()
            .map(|c| (c, state.world.market_state.trend(c, 4)))
            .collect();
        let treasury_view = TreasuryView {
            total: nation.economy.treasury,
            reserved: nation.economy.reserved_treasury_amount(),
            available: nation.economy.available_treasury(),
        };
        let available_labor = nation.economy.available_labor();
        let reserved_labor = nation.economy.reserved_labor();
        let mut labor_by_tier = HashMap::new();
        for (tier, total) in [
            (WorkerType::Untrained, nation.economy.labor.untrained),
            (WorkerType::Trained, nation.economy.labor.trained),
            (WorkerType::Expert, nation.economy.labor.expert),
        ] {
            labor_by_tier.insert(
                tier,
                LaborTierView {
                    total,
                    reserved: reserved_labor.get(&tier).copied().unwrap_or(0),
                    available: available_labor.get(&tier).copied().unwrap_or(0),
                },
            );
        }
        let labor = LaborView {
            by_tier: labor_by_tier,
            total_workers: nation.economy.labor.total_workers(),
            reserved_workers: reserved_labor.values().sum(),
            available_workers: available_labor.values().sum(),
            total_units: nation.economy.labor.total_labor_units_with(
                state.game_data.game_config.untrained_labor,
                state.game_data.game_config.trained_labor,
                state.game_data.game_config.expert_labor,
            ),
            reserved_units: reserved_labor.get(&WorkerType::Untrained).copied().unwrap_or(0)
                * state.game_data.game_config.untrained_labor
                + reserved_labor.get(&WorkerType::Trained).copied().unwrap_or(0)
                    * state.game_data.game_config.trained_labor
                + reserved_labor.get(&WorkerType::Expert).copied().unwrap_or(0)
                    * state.game_data.game_config.expert_labor,
            available_units: nation.economy.available_labor_units_with(
                state.game_data.game_config.untrained_labor,
                state.game_data.game_config.trained_labor,
                state.game_data.game_config.expert_labor,
            ),
        };
        let pending_orders = state
            .transient
            .pending_economy_orders
            .get(&nation_id)
            .cloned()
            .unwrap_or_default();

        Self {
            nation_id,
            treasury: nation.economy.treasury,
            treasury_view,
            inventory,
            inventory_view,
            buildings,
            pending_capacities,
            total_workers: nation.economy.labor.total_workers(),
            labor,
            freight_capacity: nation.military.transport.total_capacity(),
            logistics: nation.economy.logistics.clone(),
            pending_orders,
            market_prices,
            market_trends,
        }
    }

    /// Empty snapshot for eliminated or missing nations.
    fn empty(nation_id: NationId) -> Self {
        Self {
            nation_id,
            treasury: Money::ZERO,
            treasury_view: TreasuryView {
                total: Money::ZERO,
                reserved: Money::ZERO,
                available: Money::ZERO,
            },
            inventory: HashMap::new(),
            inventory_view: HashMap::new(),
            buildings: HashMap::new(),
            pending_capacities: HashMap::new(),
            total_workers: 0,
            labor: LaborView {
                by_tier: HashMap::new(),
                total_workers: 0,
                reserved_workers: 0,
                available_workers: 0,
                total_units: 0,
                reserved_units: 0,
                available_units: 0,
            },
            freight_capacity: 0,
            logistics: LogisticsState::new(),
            pending_orders: Vec::new(),
            market_prices: HashMap::new(),
            market_trends: HashMap::new(),
        }
    }

    /// Whether a building has an expansion in progress.
    pub fn is_expanding(&self, bt: BuildingType) -> bool {
        self.pending_capacities.get(&bt).copied().unwrap_or(0) > 0
    }

    // ── Inventory helpers ─────────────────────────────────────────────────

    /// Stored quantity of a raw resource.
    pub fn resource(&self, r: ResourceType) -> u32 {
        self.inventory
            .get(&Commodity::Resource(r))
            .copied()
            .unwrap_or(0)
    }

    /// Stored quantity of a processed material.
    pub fn material(&self, m: MaterialType) -> u32 {
        self.inventory
            .get(&Commodity::Material(m))
            .copied()
            .unwrap_or(0)
    }

    /// Stored quantity of a finished good.
    pub fn goods(&self, g: GoodsType) -> u32 {
        self.inventory
            .get(&Commodity::Goods(g))
            .copied()
            .unwrap_or(0)
    }

    // ── Building helpers ─────────────────────────────────────────────────

    /// Whether this nation owns a building of the given type.
    pub fn has_building(&self, bt: BuildingType) -> bool {
        self.buildings.contains_key(&bt)
    }

    /// Effective capacity of a building (0 if not present).
    pub fn building_capacity(&self, bt: BuildingType) -> u32 {
        self.buildings.get(&bt).copied().unwrap_or(0)
    }

    // ── Market helpers (#164) ────────────────────────────────────────────────

    /// Current market price for a commodity (Money::ZERO if no data yet).
    pub fn market_price(&self, c: Commodity) -> Money {
        self.market_prices.get(&c).copied().unwrap_or(Money::ZERO)
    }

    /// Price trend for a commodity over the last 4 turns.
    pub fn market_trend(&self, c: Commodity) -> Trend {
        self.market_trends.get(&c).copied().unwrap_or(Trend::Stable)
    }

    // ── Food helpers ─────────────────────────────────────────────────────────

    /// Total food (Grain + Fruit + Livestock) in inventory.
    pub fn total_food(&self) -> u32 {
        self.resource(ResourceType::Grain)
            + self.resource(ResourceType::Fruit)
            + self.resource(ResourceType::Livestock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::diplomacy::DiplomacyState;
    use crate::economy::buildings::Building;
    use crate::game_state::GameState;
    use crate::hex::HexCoord;
    use crate::map::Province;
    use crate::nation::{Nation, NationColor};
    use crate::types::NationType;

    fn minimal_game() -> GameState {
        let coord = HexCoord::new(0, 0);
        let province = Province::new(ProvinceId(1), "Test".to_string(), NationId(1), coord, vec![coord], 4);
        let nation = Nation::new(NationId(1), "Test".to_string(), NationColor::Blue, NationType::GreatPower, ProvinceId(1));
        crate::test_game_state! {
            turn: crate::types::TurnNumber::new(1),
            difficulty: crate::types::Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map: crate::map::hex_map::HexMap::new(5, 5),
            provinces: vec![province],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            political_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
            last_cash_flow: std::collections::HashMap::new(),
            last_resource_flow: std::collections::HashMap::new(),
            pending_ai_cash_spending: Vec::new(),
            pending_ai_cash_income: Vec::new(),
            next_unit_id: 0,
            market_state: crate::economy::market::MarketState::new(),
        }
    }

    #[test]
    fn snapshot_reflects_inventory() {
        let mut game = minimal_game();
        let nation_id = game.world.nations[0].id;
        game.world.nations[0].add_resource(ResourceType::Timber, 10);
        game.world.nations[0].add_material(MaterialType::Lumber, 5);
        game.world.nations[0].add_goods(GoodsType::Furniture, 2);

        let snap = NationEconomySnapshot::build(&game, nation_id);
        assert_eq!(snap.resource(ResourceType::Timber), 10);
        assert_eq!(snap.material(MaterialType::Lumber), 5);
        assert_eq!(snap.goods(GoodsType::Furniture), 2);
    }

    #[test]
    fn snapshot_reflects_buildings() {
        let mut game = minimal_game();
        let nation_id = game.world.nations[0].id;
        game.world.nations[0]
            .economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 4));

        let snap = NationEconomySnapshot::build(&game, nation_id);
        assert!(snap.has_building(BuildingType::LumberMill));
        assert_eq!(snap.building_capacity(BuildingType::LumberMill), 4);
        assert!(!snap.has_building(BuildingType::SteelMill));
    }

    #[test]
    fn snapshot_treasury_matches_nation() {
        let mut game = minimal_game();
        let nation_id = game.world.nations[0].id;
        game.world.nations[0].economy.treasury = Money::dollars(5000);

        let snap = NationEconomySnapshot::build(&game, nation_id);
        assert_eq!(snap.treasury, Money::dollars(5000));
    }

    #[test]
    fn snapshot_empty_for_missing_nation() {
        let game = minimal_game();
        let snap = NationEconomySnapshot::build(&game, NationId(9999));
        assert_eq!(snap.treasury, Money::ZERO);
        assert!(snap.inventory.is_empty());
    }
}
