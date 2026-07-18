use crate::hex::HexCoord;
use crate::types::{
    GoodsType, MaterialType, Money, NationId, ReservationId, ResourceType, TurnNumber,
};
use domain::economy as d;
use std::collections::{BTreeMap, HashMap};

// ── Labor ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WorkerType {
    Untrained,
    Trained,
    Expert,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TemporaryPenalty {
    pub fraction: f32,
    pub expires: TurnNumber,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TierState {
    pub healthy: u32,
    pub sick: u32,
    pub training_to: Option<(WorkerType, TurnNumber)>,
    pub recent_origin: Option<NationId>,
    pub temporary_penalty: Option<TemporaryPenalty>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LaborPool {
    pub untrained: u32,
    pub trained: u32,
    pub expert: u32,
    #[serde(default)]
    pub tier_meta: HashMap<WorkerType, TierState>,
}

// ── Transport ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FreightDemand {
    pub requested: u32,
    pub granted: u32,
    pub unmet: u32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LogisticsState {
    pub freight_total: u32,
    pub freight_committed: u32,
    pub freight_unused: u32,
    #[serde(default)]
    pub rail_total: u32,
    #[serde(default)]
    pub sea_total: u32,
    #[serde(default)]
    pub per_resource: BTreeMap<ResourceType, FreightDemand>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TransportSystem {
    pub freight_cars: u32,
    pub allocations: Vec<(FreightTarget, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FreightTarget {
    Resource(ResourceType),
    Material(MaterialType),
    Goods(GoodsType),
}

// ── Buildings ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BuildingType {
    Armory,
    Capitol,
    FoodProcessing,
    Railyard,
    Shipyard,
    TradeSchool,
    University,
    Warehouse,
    LumberMill,
    SteelMill,
    TextileMill,
    FurnitureFactory,
    HardwareFactory,
    ClothingFactory,
    PaperFactory,
    OilRefinery,
    PowerPlant,
    AdvancedTextileMill,
    ChemicalPlant,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Building {
    pub building_type: BuildingType,
    pub capacity: u32,
    pub pending_capacity: u32,
    pub turns_until_upgrade: u8,
}

// ── Civilians ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BuildTask {
    Railroad { to: crate::hex::HexCoord },
    Depot,
    Port,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CivilianType {
    Prospector,
    Miner,
    Engineer,
    Farmer,
    Rancher,
    Forester,
    Driller,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Civilian {
    pub id: u32,
    pub civilian_type: CivilianType,
    pub owner: NationId,
    pub position: Option<HexCoord>,
    pub working: bool,
    pub turns_remaining: u8,
    #[serde(default)]
    pub build_task: Option<BuildTask>,
    #[serde(default)]
    pub arrived_this_turn: bool,
}

// ── Ledger ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FlowCategory {
    Production,
    Trade,
    Consumption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CashSource {
    GoldGemsConversion,
    TradeExportRevenue,
    GoodsAutoSales,
    AiGoodsSale,
    BankruptcyWriteoff,
    Tariff,
    Tribute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CashSink {
    ArmyMaintenance,
    TradePurchase,
    Subsidy,
    ConstructionInfrastructure,
    AiBuildingConstruction,
    AiArmyBuild,
    AiWarshipBuild,
    AiCivilianBuild,
    AiDiplomacyConsulate,
    AiDiplomacyEmbassy,
    AiGrant,
    AiInfrastructure,
    AiResearch,
    AiSpendingOther,
    AiTacticalMovement,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CashEntry {
    pub amount: Money,
    pub partner: Option<NationId>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CashFlow {
    pub opening_treasury: Money,
    pub closing_treasury: Money,
    pub income: HashMap<CashSource, Vec<CashEntry>>,
    pub expense: HashMap<CashSink, Vec<CashEntry>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Stockpile {
    Resource(ResourceType),
    Material(MaterialType),
    Goods(GoodsType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ResourceIn {
    HomeProduction,
    MillOutput,
    FactoryOutput,
    TownProduced,
    TradeImport,
    FoodProcessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ResourceOut {
    DisconnectedLoss,
    TransportOverflow,
    WorkerFood,
    MillConsumed,
    FactoryConsumed,
    FoodProcessedInput,
    TradeExport,
    ImmigrationConsumed,
    ConstructionConsumed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceInflowEntry {
    pub stockpile: Stockpile,
    pub source: ResourceIn,
    pub amount: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceOutflowEntry {
    pub stockpile: Stockpile,
    pub sink: ResourceOut,
    pub amount: u32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ResourceFlow {
    pub inflow: Vec<ResourceInflowEntry>,
    pub outflow: Vec<ResourceOutflowEntry>,
}

// ── Market ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarketTick {
    pub turn: TurnNumber,
    pub price: Money,
    pub supply: u32,
    pub demand: u32,
    pub unmet_demand: u32,
    pub sold: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MarketState {
    #[serde(default)]
    pub resource_prices: BTreeMap<ResourceType, Money>,
    #[serde(default)]
    pub material_prices: BTreeMap<MaterialType, Money>,
    #[serde(default)]
    pub goods_prices: BTreeMap<GoodsType, Money>,
    #[serde(default)]
    pub resource_history: BTreeMap<ResourceType, Vec<MarketTick>>,
    #[serde(default)]
    pub material_history: BTreeMap<MaterialType, Vec<MarketTick>>,
    #[serde(default)]
    pub goods_history: BTreeMap<GoodsType, Vec<MarketTick>>,
}

// ── Trade ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TradeHistoryEntry {
    pub turn: TurnNumber,
    pub partner: NationId,
    pub resource: ResourceType,
    /// Human-readable commodity label. Defaults to the resource name for resource
    /// trades; overridden to the material/goods name for manufactured-good sales.
    #[serde(default)]
    pub commodity_label: String,
    pub quantity: u32,
    pub total_cost: Money,
    #[serde(default)]
    pub bought: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Commodity {
    Resource(ResourceType),
    Material(MaterialType),
    Goods(GoodsType),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlayerSellOrder {
    pub resource: ResourceType,
    pub quantity: u32,
}

// ── Observability ─────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum BlockReason {
    InsufficientInventory {
        commodity: Commodity,
        needed: u32,
        available: u32,
    },
    InsufficientLabor {
        tier: WorkerType,
        needed: u32,
        available: u32,
    },
    InsufficientFreight {
        needed: u32,
        available: u32,
    },
    InsufficientTreasury {
        needed: Money,
        available: Money,
    },
    MissingPrerequisite(String),
}

// ═══════════════════════════════════════════════════════════════════
// From impls
// ═══════════════════════════════════════════════════════════════════

// ── WorkerType ───────────────────────────────────────────────────

impl From<d::labor::WorkerType> for WorkerType {
    fn from(v: d::labor::WorkerType) -> Self {
        match v {
            d::labor::WorkerType::Untrained => Self::Untrained,
            d::labor::WorkerType::Trained => Self::Trained,
            d::labor::WorkerType::Expert => Self::Expert,
        }
    }
}
impl From<WorkerType> for d::labor::WorkerType {
    fn from(v: WorkerType) -> Self {
        match v {
            WorkerType::Untrained => Self::Untrained,
            WorkerType::Trained => Self::Trained,
            WorkerType::Expert => Self::Expert,
        }
    }
}

// ── TemporaryPenalty ─────────────────────────────────────────────

impl From<&d::labor::TemporaryPenalty> for TemporaryPenalty {
    fn from(v: &d::labor::TemporaryPenalty) -> Self {
        Self {
            fraction: v.fraction,
            expires: v.expires.into(),
        }
    }
}
impl From<TemporaryPenalty> for d::labor::TemporaryPenalty {
    fn from(v: TemporaryPenalty) -> Self {
        Self {
            fraction: v.fraction,
            expires: v.expires.into(),
        }
    }
}

// ── TierState ─────────────────────────────────────────────────────

impl From<&d::labor::TierState> for TierState {
    fn from(v: &d::labor::TierState) -> Self {
        Self {
            healthy: v.healthy,
            sick: v.sick,
            training_to: v.training_to.map(|(wt, tn)| (wt.into(), tn.into())),
            recent_origin: v.recent_origin.map(Into::into),
            temporary_penalty: v.temporary_penalty.as_ref().map(Into::into),
        }
    }
}
impl From<TierState> for d::labor::TierState {
    fn from(v: TierState) -> Self {
        Self {
            healthy: v.healthy,
            sick: v.sick,
            training_to: v.training_to.map(|(wt, tn)| (wt.into(), tn.into())),
            recent_origin: v.recent_origin.map(Into::into),
            temporary_penalty: v.temporary_penalty.map(Into::into),
        }
    }
}

// ── LaborPool ─────────────────────────────────────────────────────

impl From<&d::labor::LaborPool> for LaborPool {
    fn from(v: &d::labor::LaborPool) -> Self {
        Self {
            untrained: v.untrained,
            trained: v.trained,
            expert: v.expert,
            tier_meta: v
                .tier_meta
                .iter()
                .map(|(k, s)| ((*k).into(), s.into()))
                .collect(),
        }
    }
}
impl From<LaborPool> for d::labor::LaborPool {
    fn from(v: LaborPool) -> Self {
        Self {
            untrained: v.untrained,
            trained: v.trained,
            expert: v.expert,
            tier_meta: v
                .tier_meta
                .into_iter()
                .map(|(k, s)| (k.into(), s.into()))
                .collect(),
        }
    }
}

// ── FreightDemand ─────────────────────────────────────────────────

impl From<&d::transport::FreightDemand> for FreightDemand {
    fn from(v: &d::transport::FreightDemand) -> Self {
        Self {
            requested: v.requested,
            granted: v.granted,
            unmet: v.unmet,
        }
    }
}
impl From<FreightDemand> for d::transport::FreightDemand {
    fn from(v: FreightDemand) -> Self {
        Self {
            requested: v.requested,
            granted: v.granted,
            unmet: v.unmet,
        }
    }
}

// ── LogisticsState ────────────────────────────────────────────────

impl From<&d::transport::LogisticsState> for LogisticsState {
    fn from(v: &d::transport::LogisticsState) -> Self {
        Self {
            freight_total: v.freight_total,
            freight_committed: v.freight_committed,
            freight_unused: v.freight_unused,
            rail_total: v.rail_total,
            sea_total: v.sea_total,
            per_resource: v
                .per_resource
                .iter()
                .map(|(k, fd)| ((*k).into(), fd.into()))
                .collect(),
        }
    }
}
impl From<LogisticsState> for d::transport::LogisticsState {
    fn from(v: LogisticsState) -> Self {
        Self {
            freight_total: v.freight_total,
            freight_committed: v.freight_committed,
            freight_unused: v.freight_unused,
            rail_total: v.rail_total,
            sea_total: v.sea_total,
            per_resource: v
                .per_resource
                .into_iter()
                .map(|(k, fd)| (k.into(), fd.into()))
                .collect(),
        }
    }
}

// ── TransportSystem ───────────────────────────────────────────────

impl From<d::transport::FreightTarget> for FreightTarget {
    fn from(v: d::transport::FreightTarget) -> Self {
        match v {
            d::transport::FreightTarget::Resource(r) => Self::Resource(r.into()),
            d::transport::FreightTarget::Material(m) => Self::Material(m.into()),
            d::transport::FreightTarget::Goods(g) => Self::Goods(g.into()),
        }
    }
}
impl From<FreightTarget> for d::transport::FreightTarget {
    fn from(v: FreightTarget) -> Self {
        match v {
            FreightTarget::Resource(r) => Self::Resource(r.into()),
            FreightTarget::Material(m) => Self::Material(m.into()),
            FreightTarget::Goods(g) => Self::Goods(g.into()),
        }
    }
}

impl From<&d::transport::TransportSystem> for TransportSystem {
    fn from(v: &d::transport::TransportSystem) -> Self {
        Self {
            freight_cars: v.freight_cars,
            allocations: v
                .allocations
                .iter()
                .map(|(rt, n)| ((*rt).into(), *n))
                .collect(),
        }
    }
}
impl From<TransportSystem> for d::transport::TransportSystem {
    fn from(v: TransportSystem) -> Self {
        d::transport::TransportSystem {
            freight_cars: v.freight_cars,
            allocations: v
                .allocations
                .into_iter()
                .map(|(rt, n)| (rt.into(), n))
                .collect(),
        }
    }
}

// ── BuildingType ──────────────────────────────────────────────────

impl From<d::buildings::BuildingType> for BuildingType {
    fn from(v: d::buildings::BuildingType) -> Self {
        use d::buildings::BuildingType as D;
        match v {
            D::Armory => Self::Armory,
            D::Capitol => Self::Capitol,
            D::FoodProcessing => Self::FoodProcessing,
            D::Railyard => Self::Railyard,
            D::Shipyard => Self::Shipyard,
            D::TradeSchool => Self::TradeSchool,
            D::University => Self::University,
            D::Warehouse => Self::Warehouse,
            D::LumberMill => Self::LumberMill,
            D::SteelMill => Self::SteelMill,
            D::TextileMill => Self::TextileMill,
            D::FurnitureFactory => Self::FurnitureFactory,
            D::HardwareFactory => Self::HardwareFactory,
            D::ClothingFactory => Self::ClothingFactory,
            D::PaperFactory => Self::PaperFactory,
            D::OilRefinery => Self::OilRefinery,
            D::PowerPlant => Self::PowerPlant,
            D::AdvancedTextileMill => Self::AdvancedTextileMill,
            D::ChemicalPlant => Self::ChemicalPlant,
        }
    }
}
impl From<BuildingType> for d::buildings::BuildingType {
    fn from(v: BuildingType) -> Self {
        use d::buildings::BuildingType as D;
        match v {
            BuildingType::Armory => D::Armory,
            BuildingType::Capitol => D::Capitol,
            BuildingType::FoodProcessing => D::FoodProcessing,
            BuildingType::Railyard => D::Railyard,
            BuildingType::Shipyard => D::Shipyard,
            BuildingType::TradeSchool => D::TradeSchool,
            BuildingType::University => D::University,
            BuildingType::Warehouse => D::Warehouse,
            BuildingType::LumberMill => D::LumberMill,
            BuildingType::SteelMill => D::SteelMill,
            BuildingType::TextileMill => D::TextileMill,
            BuildingType::FurnitureFactory => D::FurnitureFactory,
            BuildingType::HardwareFactory => D::HardwareFactory,
            BuildingType::ClothingFactory => D::ClothingFactory,
            BuildingType::PaperFactory => D::PaperFactory,
            BuildingType::OilRefinery => D::OilRefinery,
            BuildingType::PowerPlant => D::PowerPlant,
            BuildingType::AdvancedTextileMill => D::AdvancedTextileMill,
            BuildingType::ChemicalPlant => D::ChemicalPlant,
        }
    }
}

// ── Building ──────────────────────────────────────────────────────

impl From<&d::buildings::Building> for Building {
    fn from(v: &d::buildings::Building) -> Self {
        Self {
            building_type: v.building_type.into(),
            capacity: v.capacity,
            pending_capacity: v.pending_capacity,
            turns_until_upgrade: v.turns_until_upgrade,
        }
    }
}
impl From<Building> for d::buildings::Building {
    fn from(v: Building) -> Self {
        Self {
            building_type: v.building_type.into(),
            capacity: v.capacity,
            pending_capacity: v.pending_capacity,
            turns_until_upgrade: v.turns_until_upgrade,
        }
    }
}

// ── BuildTask ─────────────────────────────────────────────────────

impl From<d::civilians::BuildTask> for BuildTask {
    fn from(v: d::civilians::BuildTask) -> Self {
        match v {
            d::civilians::BuildTask::Railroad { to } => Self::Railroad { to: to.into() },
            d::civilians::BuildTask::Depot => Self::Depot,
            d::civilians::BuildTask::Port => Self::Port,
        }
    }
}
impl From<BuildTask> for d::civilians::BuildTask {
    fn from(v: BuildTask) -> Self {
        match v {
            BuildTask::Railroad { to } => Self::Railroad { to: to.into() },
            BuildTask::Depot => Self::Depot,
            BuildTask::Port => Self::Port,
        }
    }
}

// ── CivilianType ──────────────────────────────────────────────────

impl From<d::civilians::CivilianType> for CivilianType {
    fn from(v: d::civilians::CivilianType) -> Self {
        use d::civilians::CivilianType as D;
        match v {
            D::Prospector => Self::Prospector,
            D::Miner => Self::Miner,
            D::Engineer => Self::Engineer,
            D::Farmer => Self::Farmer,
            D::Rancher => Self::Rancher,
            D::Forester => Self::Forester,
            D::Driller => Self::Driller,
        }
    }
}
impl From<CivilianType> for d::civilians::CivilianType {
    fn from(v: CivilianType) -> Self {
        use d::civilians::CivilianType as D;
        match v {
            CivilianType::Prospector => D::Prospector,
            CivilianType::Miner => D::Miner,
            CivilianType::Engineer => D::Engineer,
            CivilianType::Farmer => D::Farmer,
            CivilianType::Rancher => D::Rancher,
            CivilianType::Forester => D::Forester,
            CivilianType::Driller => D::Driller,
        }
    }
}

// ── Civilian ──────────────────────────────────────────────────────

impl From<&d::civilians::Civilian> for Civilian {
    fn from(v: &d::civilians::Civilian) -> Self {
        Self {
            id: v.id.0,
            civilian_type: v.civilian_type.into(),
            owner: v.owner.into(),
            position: v.position.map(Into::into),
            working: v.working,
            turns_remaining: v.turns_remaining,
            build_task: v.build_task.map(Into::into),
            arrived_this_turn: v.arrived_this_turn,
        }
    }
}
impl From<Civilian> for d::civilians::Civilian {
    fn from(v: Civilian) -> Self {
        use domain::map::UnitId;
        Self {
            id: UnitId(v.id),
            civilian_type: v.civilian_type.into(),
            owner: v.owner.into(),
            position: v.position.map(Into::into),
            working: v.working,
            turns_remaining: v.turns_remaining,
            build_task: v.build_task.map(Into::into),
            arrived_this_turn: v.arrived_this_turn,
        }
    }
}

// ── Ledger ────────────────────────────────────────────────────────

impl From<d::ledger::FlowCategory> for FlowCategory {
    fn from(v: d::ledger::FlowCategory) -> Self {
        match v {
            d::ledger::FlowCategory::Production => Self::Production,
            d::ledger::FlowCategory::Trade => Self::Trade,
            d::ledger::FlowCategory::Consumption => Self::Consumption,
        }
    }
}
impl From<FlowCategory> for d::ledger::FlowCategory {
    fn from(v: FlowCategory) -> Self {
        match v {
            FlowCategory::Production => Self::Production,
            FlowCategory::Trade => Self::Trade,
            FlowCategory::Consumption => Self::Consumption,
        }
    }
}

impl From<d::ledger::CashSource> for CashSource {
    fn from(v: d::ledger::CashSource) -> Self {
        use d::ledger::CashSource as D;
        match v {
            D::GoldGemsConversion => Self::GoldGemsConversion,
            D::TradeExportRevenue => Self::TradeExportRevenue,
            D::GoodsAutoSales => Self::GoodsAutoSales,
            D::AiGoodsSale => Self::AiGoodsSale,
            D::BankruptcyWriteoff => Self::BankruptcyWriteoff,
            D::Tariff => Self::Tariff,
            D::Tribute => Self::Tribute,
        }
    }
}
impl From<CashSource> for d::ledger::CashSource {
    fn from(v: CashSource) -> Self {
        use d::ledger::CashSource as D;
        match v {
            CashSource::GoldGemsConversion => D::GoldGemsConversion,
            CashSource::TradeExportRevenue => D::TradeExportRevenue,
            CashSource::GoodsAutoSales => D::GoodsAutoSales,
            CashSource::AiGoodsSale => D::AiGoodsSale,
            CashSource::BankruptcyWriteoff => D::BankruptcyWriteoff,
            CashSource::Tariff => D::Tariff,
            CashSource::Tribute => D::Tribute,
        }
    }
}

impl From<d::ledger::CashSink> for CashSink {
    fn from(v: d::ledger::CashSink) -> Self {
        use d::ledger::CashSink as D;
        match v {
            D::ArmyMaintenance => Self::ArmyMaintenance,
            D::TradePurchase => Self::TradePurchase,
            D::Subsidy => Self::Subsidy,
            D::ConstructionInfrastructure => Self::ConstructionInfrastructure,
            D::AiBuildingConstruction => Self::AiBuildingConstruction,
            D::AiArmyBuild => Self::AiArmyBuild,
            D::AiWarshipBuild => Self::AiWarshipBuild,
            D::AiCivilianBuild => Self::AiCivilianBuild,
            D::AiDiplomacyConsulate => Self::AiDiplomacyConsulate,
            D::AiDiplomacyEmbassy => Self::AiDiplomacyEmbassy,
            D::AiGrant => Self::AiGrant,
            D::AiInfrastructure => Self::AiInfrastructure,
            D::AiResearch => Self::AiResearch,
            D::AiSpendingOther => Self::AiSpendingOther,
            D::AiTacticalMovement => Self::AiTacticalMovement,
        }
    }
}
impl From<CashSink> for d::ledger::CashSink {
    fn from(v: CashSink) -> Self {
        use d::ledger::CashSink as D;
        match v {
            CashSink::ArmyMaintenance => D::ArmyMaintenance,
            CashSink::TradePurchase => D::TradePurchase,
            CashSink::Subsidy => D::Subsidy,
            CashSink::ConstructionInfrastructure => D::ConstructionInfrastructure,
            CashSink::AiBuildingConstruction => D::AiBuildingConstruction,
            CashSink::AiArmyBuild => D::AiArmyBuild,
            CashSink::AiWarshipBuild => D::AiWarshipBuild,
            CashSink::AiCivilianBuild => D::AiCivilianBuild,
            CashSink::AiDiplomacyConsulate => D::AiDiplomacyConsulate,
            CashSink::AiDiplomacyEmbassy => D::AiDiplomacyEmbassy,
            CashSink::AiGrant => D::AiGrant,
            CashSink::AiInfrastructure => D::AiInfrastructure,
            CashSink::AiResearch => D::AiResearch,
            CashSink::AiSpendingOther => D::AiSpendingOther,
            CashSink::AiTacticalMovement => D::AiTacticalMovement,
        }
    }
}

impl From<&d::ledger::CashEntry> for CashEntry {
    fn from(v: &d::ledger::CashEntry) -> Self {
        Self {
            amount: v.amount.into(),
            partner: v.partner.map(Into::into),
        }
    }
}
impl From<CashEntry> for d::ledger::CashEntry {
    fn from(v: CashEntry) -> Self {
        Self {
            amount: v.amount.into(),
            partner: v.partner.map(Into::into),
        }
    }
}

impl From<&d::ledger::CashFlow> for CashFlow {
    fn from(v: &d::ledger::CashFlow) -> Self {
        Self {
            opening_treasury: v.opening_treasury.into(),
            closing_treasury: v.closing_treasury.into(),
            income: v
                .income
                .iter()
                .map(|(k, entries)| ((*k).into(), entries.iter().map(Into::into).collect()))
                .collect(),
            expense: v
                .expense
                .iter()
                .map(|(k, entries)| ((*k).into(), entries.iter().map(Into::into).collect()))
                .collect(),
        }
    }
}
impl From<CashFlow> for d::ledger::CashFlow {
    fn from(v: CashFlow) -> Self {
        let mut cf = d::ledger::CashFlow::new(v.opening_treasury.into());
        cf.closing_treasury = v.closing_treasury.into();
        cf.income = v
            .income
            .into_iter()
            .map(|(k, entries)| (k.into(), entries.into_iter().map(Into::into).collect()))
            .collect();
        cf.expense = v
            .expense
            .into_iter()
            .map(|(k, entries)| (k.into(), entries.into_iter().map(Into::into).collect()))
            .collect();
        cf
    }
}

// ── Stockpile / ResourceIn / ResourceOut ──────────────────────────

impl From<d::ledger::Stockpile> for Stockpile {
    fn from(v: d::ledger::Stockpile) -> Self {
        match v {
            d::ledger::Stockpile::Resource(r) => Self::Resource(r.into()),
            d::ledger::Stockpile::Material(m) => Self::Material(m.into()),
            d::ledger::Stockpile::Goods(g) => Self::Goods(g.into()),
        }
    }
}
impl From<Stockpile> for d::ledger::Stockpile {
    fn from(v: Stockpile) -> Self {
        match v {
            Stockpile::Resource(r) => Self::Resource(r.into()),
            Stockpile::Material(m) => Self::Material(m.into()),
            Stockpile::Goods(g) => Self::Goods(g.into()),
        }
    }
}

impl From<d::ledger::ResourceIn> for ResourceIn {
    fn from(v: d::ledger::ResourceIn) -> Self {
        use d::ledger::ResourceIn as D;
        match v {
            D::HomeProduction => Self::HomeProduction,
            D::MillOutput => Self::MillOutput,
            D::FactoryOutput => Self::FactoryOutput,
            D::TownProduced => Self::TownProduced,
            D::TradeImport => Self::TradeImport,
            D::FoodProcessed => Self::FoodProcessed,
        }
    }
}
impl From<ResourceIn> for d::ledger::ResourceIn {
    fn from(v: ResourceIn) -> Self {
        use d::ledger::ResourceIn as D;
        match v {
            ResourceIn::HomeProduction => D::HomeProduction,
            ResourceIn::MillOutput => D::MillOutput,
            ResourceIn::FactoryOutput => D::FactoryOutput,
            ResourceIn::TownProduced => D::TownProduced,
            ResourceIn::TradeImport => D::TradeImport,
            ResourceIn::FoodProcessed => D::FoodProcessed,
        }
    }
}

impl From<d::ledger::ResourceOut> for ResourceOut {
    fn from(v: d::ledger::ResourceOut) -> Self {
        use d::ledger::ResourceOut as D;
        match v {
            D::DisconnectedLoss => Self::DisconnectedLoss,
            D::TransportOverflow => Self::TransportOverflow,
            D::WorkerFood => Self::WorkerFood,
            D::MillConsumed => Self::MillConsumed,
            D::FactoryConsumed => Self::FactoryConsumed,
            D::FoodProcessedInput => Self::FoodProcessedInput,
            D::TradeExport => Self::TradeExport,
            D::ImmigrationConsumed => Self::ImmigrationConsumed,
            D::ConstructionConsumed => Self::ConstructionConsumed,
        }
    }
}
impl From<ResourceOut> for d::ledger::ResourceOut {
    fn from(v: ResourceOut) -> Self {
        use d::ledger::ResourceOut as D;
        match v {
            ResourceOut::DisconnectedLoss => D::DisconnectedLoss,
            ResourceOut::TransportOverflow => D::TransportOverflow,
            ResourceOut::WorkerFood => D::WorkerFood,
            ResourceOut::MillConsumed => D::MillConsumed,
            ResourceOut::FactoryConsumed => D::FactoryConsumed,
            ResourceOut::FoodProcessedInput => D::FoodProcessedInput,
            ResourceOut::TradeExport => D::TradeExport,
            ResourceOut::ImmigrationConsumed => D::ImmigrationConsumed,
            ResourceOut::ConstructionConsumed => D::ConstructionConsumed,
        }
    }
}

impl From<&d::ledger::ResourceInflowEntry> for ResourceInflowEntry {
    fn from(v: &d::ledger::ResourceInflowEntry) -> Self {
        Self {
            stockpile: v.stockpile.into(),
            source: v.source.into(),
            amount: v.amount,
        }
    }
}
impl From<ResourceInflowEntry> for d::ledger::ResourceInflowEntry {
    fn from(v: ResourceInflowEntry) -> Self {
        Self {
            stockpile: v.stockpile.into(),
            source: v.source.into(),
            amount: v.amount,
        }
    }
}

impl From<&d::ledger::ResourceOutflowEntry> for ResourceOutflowEntry {
    fn from(v: &d::ledger::ResourceOutflowEntry) -> Self {
        Self {
            stockpile: v.stockpile.into(),
            sink: v.sink.into(),
            amount: v.amount,
        }
    }
}
impl From<ResourceOutflowEntry> for d::ledger::ResourceOutflowEntry {
    fn from(v: ResourceOutflowEntry) -> Self {
        Self {
            stockpile: v.stockpile.into(),
            sink: v.sink.into(),
            amount: v.amount,
        }
    }
}

impl From<&d::ledger::ResourceFlow> for ResourceFlow {
    fn from(v: &d::ledger::ResourceFlow) -> Self {
        Self {
            inflow: v.inflow.iter().map(Into::into).collect(),
            outflow: v.outflow.iter().map(Into::into).collect(),
        }
    }
}
impl From<ResourceFlow> for d::ledger::ResourceFlow {
    fn from(v: ResourceFlow) -> Self {
        Self {
            inflow: v.inflow.into_iter().map(Into::into).collect(),
            outflow: v.outflow.into_iter().map(Into::into).collect(),
        }
    }
}

// ── MarketTick / Trend / MarketState ─────────────────────────────

impl From<&d::market::MarketTick> for MarketTick {
    fn from(v: &d::market::MarketTick) -> Self {
        Self {
            turn: v.turn.into(),
            price: v.price.into(),
            supply: v.supply,
            demand: v.demand,
            unmet_demand: v.unmet_demand,
            sold: v.sold,
        }
    }
}
impl From<MarketTick> for d::market::MarketTick {
    fn from(v: MarketTick) -> Self {
        Self {
            turn: v.turn.into(),
            price: v.price.into(),
            supply: v.supply,
            demand: v.demand,
            unmet_demand: v.unmet_demand,
            sold: v.sold,
        }
    }
}

impl From<&d::market::MarketState> for MarketState {
    fn from(v: &d::market::MarketState) -> Self {
        Self {
            resource_prices: v
                .resource_prices
                .iter()
                .map(|(k, p)| ((*k).into(), (*p).into()))
                .collect(),
            material_prices: v
                .material_prices
                .iter()
                .map(|(k, p)| ((*k).into(), (*p).into()))
                .collect(),
            goods_prices: v
                .goods_prices
                .iter()
                .map(|(k, p)| ((*k).into(), (*p).into()))
                .collect(),
            resource_history: v
                .resource_history
                .iter()
                .map(|(k, h)| ((*k).into(), h.iter().map(Into::into).collect()))
                .collect(),
            material_history: v
                .material_history
                .iter()
                .map(|(k, h)| ((*k).into(), h.iter().map(Into::into).collect()))
                .collect(),
            goods_history: v
                .goods_history
                .iter()
                .map(|(k, h)| ((*k).into(), h.iter().map(Into::into).collect()))
                .collect(),
        }
    }
}
impl From<MarketState> for d::market::MarketState {
    fn from(v: MarketState) -> Self {
        let mut ms = d::market::MarketState::new();
        for (k, p) in v.resource_prices {
            ms.resource_prices.insert(k.into(), p.into());
        }
        for (k, p) in v.material_prices {
            ms.material_prices.insert(k.into(), p.into());
        }
        for (k, p) in v.goods_prices {
            ms.goods_prices.insert(k.into(), p.into());
        }
        for (k, h) in v.resource_history {
            ms.resource_history
                .insert(k.into(), h.into_iter().map(Into::into).collect());
        }
        for (k, h) in v.material_history {
            ms.material_history
                .insert(k.into(), h.into_iter().map(Into::into).collect());
        }
        for (k, h) in v.goods_history {
            ms.goods_history
                .insert(k.into(), h.into_iter().map(Into::into).collect());
        }
        ms
    }
}

// ── Trade types ───────────────────────────────────────────────────

impl From<&d::trade::TradeHistoryEntry> for TradeHistoryEntry {
    fn from(v: &d::trade::TradeHistoryEntry) -> Self {
        Self {
            turn: v.turn.into(),
            partner: v.partner.into(),
            resource: v.resource.into(),
            commodity_label: v.commodity_label.clone(),
            quantity: v.quantity,
            total_cost: v.total_cost.into(),
            bought: v.bought,
        }
    }
}
impl From<TradeHistoryEntry> for d::trade::TradeHistoryEntry {
    fn from(v: TradeHistoryEntry) -> Self {
        Self {
            turn: v.turn.into(),
            partner: v.partner.into(),
            resource: v.resource.into(),
            commodity_label: v.commodity_label,
            quantity: v.quantity,
            total_cost: v.total_cost.into(),
            bought: v.bought,
        }
    }
}

impl From<d::trade::Commodity> for Commodity {
    fn from(v: d::trade::Commodity) -> Self {
        match v {
            d::trade::Commodity::Resource(r) => Self::Resource(r.into()),
            d::trade::Commodity::Material(m) => Self::Material(m.into()),
            d::trade::Commodity::Goods(g) => Self::Goods(g.into()),
        }
    }
}
impl From<Commodity> for d::trade::Commodity {
    fn from(v: Commodity) -> Self {
        match v {
            Commodity::Resource(r) => Self::Resource(r.into()),
            Commodity::Material(m) => Self::Material(m.into()),
            Commodity::Goods(g) => Self::Goods(g.into()),
        }
    }
}

impl From<&d::trade::PlayerSellOrder> for PlayerSellOrder {
    fn from(v: &d::trade::PlayerSellOrder) -> Self {
        Self {
            resource: v.resource.into(),
            quantity: v.quantity,
        }
    }
}
impl From<PlayerSellOrder> for d::trade::PlayerSellOrder {
    fn from(v: PlayerSellOrder) -> Self {
        Self {
            resource: v.resource.into(),
            quantity: v.quantity,
        }
    }
}

// ── BlockReason ───────────────────────────────────────────────────

impl From<&d::observability::BlockReason> for BlockReason {
    fn from(v: &d::observability::BlockReason) -> Self {
        use d::observability::BlockReason as D;
        match v {
            D::InsufficientInventory {
                commodity,
                needed,
                available,
            } => Self::InsufficientInventory {
                commodity: (*commodity).into(),
                needed: *needed,
                available: *available,
            },
            D::InsufficientLabor {
                tier,
                needed,
                available,
            } => Self::InsufficientLabor {
                tier: (*tier).into(),
                needed: *needed,
                available: *available,
            },
            D::InsufficientFreight { needed, available } => Self::InsufficientFreight {
                needed: *needed,
                available: *available,
            },
            D::InsufficientTreasury { needed, available } => Self::InsufficientTreasury {
                needed: (*needed).into(),
                available: (*available).into(),
            },
            D::MissingPrerequisite(s) => Self::MissingPrerequisite(s.clone()),
        }
    }
}
impl From<BlockReason> for d::observability::BlockReason {
    fn from(v: BlockReason) -> Self {
        use d::observability::BlockReason as D;
        match v {
            BlockReason::InsufficientInventory {
                commodity,
                needed,
                available,
            } => D::InsufficientInventory {
                commodity: commodity.into(),
                needed,
                available,
            },
            BlockReason::InsufficientLabor {
                tier,
                needed,
                available,
            } => D::InsufficientLabor {
                tier: tier.into(),
                needed,
                available,
            },
            BlockReason::InsufficientFreight { needed, available } => {
                D::InsufficientFreight { needed, available }
            }
            BlockReason::InsufficientTreasury { needed, available } => D::InsufficientTreasury {
                needed: needed.into(),
                available: available.into(),
            },
            BlockReason::MissingPrerequisite(s) => D::MissingPrerequisite(s),
        }
    }
}

// ── Nation-level fields (used from nation.rs) ─────────────────────

fn default_true() -> bool {
    true
}

/// Snapshot of player-controlled production chain output targets.
/// Each field is a target output quantity (u32::MAX = unlimited).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ChainOutputTargets {
    #[serde(default)]
    pub timber_mill: u32,
    #[serde(default)]
    pub metal_mill: u32,
    #[serde(default)]
    pub textile_mill: u32,
    #[serde(default)]
    pub lumber_factory: u32,
    #[serde(default)]
    pub steel_factory: u32,
    #[serde(default)]
    pub garment_factory: u32,
    #[serde(default)]
    pub armory: u32,
    #[serde(default)]
    pub paper_factory: u32,
    #[serde(default)]
    pub canned_food_factory: u32,
}

impl From<&domain::nation::ChainOutputTargets> for ChainOutputTargets {
    fn from(v: &domain::nation::ChainOutputTargets) -> Self {
        Self {
            timber_mill: v.timber_mill,
            metal_mill: v.metal_mill,
            textile_mill: v.textile_mill,
            lumber_factory: v.lumber_factory,
            steel_factory: v.steel_factory,
            garment_factory: v.garment_factory,
            armory: v.armory,
            paper_factory: v.paper_factory,
            canned_food_factory: v.canned_food_factory,
        }
    }
}

impl From<ChainOutputTargets> for domain::nation::ChainOutputTargets {
    fn from(v: ChainOutputTargets) -> Self {
        Self {
            timber_mill: v.timber_mill,
            metal_mill: v.metal_mill,
            textile_mill: v.textile_mill,
            lumber_factory: v.lumber_factory,
            steel_factory: v.steel_factory,
            garment_factory: v.garment_factory,
            armory: v.armory,
            paper_factory: v.paper_factory,
            canned_food_factory: v.canned_food_factory,
        }
    }
}

/// Snapshot of a nation's full economy for use by `nation.rs`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NationEconomy {
    pub treasury: Money,
    pub warehouse: BTreeMap<ResourceType, u32>,
    pub materials: BTreeMap<MaterialType, u32>,
    pub goods: BTreeMap<GoodsType, u32>,
    pub buildings: Vec<Building>,
    pub labor: LaborPool,
    #[serde(default)]
    pub logistics: LogisticsState,
    #[serde(default)]
    pub transport: TransportSystem,
    #[serde(default)]
    pub chain_targets: ChainOutputTargets,
    #[serde(default)]
    pub pending_civilian_hires: HashMap<CivilianType, u32>,
    #[serde(default)]
    pub pending_train_to_trained: u32,
    #[serde(default)]
    pub pending_train_to_expert: u32,
    #[serde(default)]
    pub pending_immigration: u32,
    #[serde(default)]
    pub pending_freight_cars: u32,
    #[serde(default)]
    pub pending_ships: Vec<String>,
    #[serde(default)]
    pub pending_army_recruits: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_trade_with_minors: bool,
    #[serde(default)]
    pub reserved_treasury: Money,
    #[serde(default)]
    pub reserved_warehouse: BTreeMap<ResourceType, u32>,
    #[serde(default)]
    pub reserved_materials: BTreeMap<MaterialType, u32>,
    #[serde(default)]
    pub reserved_goods: BTreeMap<GoodsType, u32>,
    #[serde(default)]
    pub reservation_ledger: BTreeMap<ReservationId, (Commodity, u32)>,
    #[serde(default)]
    pub next_reservation_id: u64,
    #[serde(default)]
    pub reserved_labor: HashMap<WorkerType, u32>,
}

impl From<&domain::nation::NationEconomy> for NationEconomy {
    fn from(v: &domain::nation::NationEconomy) -> Self {
        Self {
            treasury: v.treasury.into(),
            warehouse: v.warehouse.iter().map(|(k, n)| ((*k).into(), *n)).collect(),
            materials: v.materials.iter().map(|(k, n)| ((*k).into(), *n)).collect(),
            goods: v.goods.iter().map(|(k, n)| ((*k).into(), *n)).collect(),
            buildings: v.buildings.iter().map(Into::into).collect(),
            labor: (&v.labor).into(),
            logistics: (&v.logistics).into(),
            transport: (&v.transport).into(),
            chain_targets: (&v.chain_targets).into(),
            pending_civilian_hires: v
                .pending_civilian_hires
                .iter()
                .map(|(k, n)| ((*k).into(), *n))
                .collect(),
            pending_train_to_trained: v.pending_train_to_trained,
            pending_train_to_expert: v.pending_train_to_expert,
            pending_immigration: v.pending_immigration,
            pending_freight_cars: v.pending_freight_cars,
            pending_ships: v.pending_ships.clone(),
            pending_army_recruits: v.pending_army_recruits.clone(),
            auto_trade_with_minors: v.auto_trade_with_minors,
            reserved_treasury: v.snapshot_reserved_treasury().into(),
            reserved_warehouse: v
                .snapshot_reserved_warehouse()
                .iter()
                .map(|(k, n)| ((*k).into(), *n))
                .collect(),
            reserved_materials: v
                .snapshot_reserved_materials()
                .iter()
                .map(|(k, n)| ((*k).into(), *n))
                .collect(),
            reserved_goods: v
                .snapshot_reserved_goods()
                .iter()
                .map(|(k, n)| ((*k).into(), *n))
                .collect(),
            reservation_ledger: v
                .snapshot_reservation_ledger()
                .iter()
                .map(|(k, (c, n))| ((*k).into(), ((*c).into(), *n)))
                .collect(),
            next_reservation_id: v.snapshot_next_reservation_id(),
            reserved_labor: v
                .snapshot_reserved_labor()
                .iter()
                .map(|(k, n)| ((*k).into(), *n))
                .collect(),
        }
    }
}
impl From<NationEconomy> for domain::nation::NationEconomy {
    fn from(v: NationEconomy) -> Self {
        let mut ne = domain::nation::NationEconomy::default();
        ne.treasury = v.treasury.into();
        ne.warehouse = v
            .warehouse
            .into_iter()
            .map(|(k, n)| (k.into(), n))
            .collect();
        ne.materials = v
            .materials
            .into_iter()
            .map(|(k, n)| (k.into(), n))
            .collect();
        ne.goods = v.goods.into_iter().map(|(k, n)| (k.into(), n)).collect();
        ne.buildings = v.buildings.into_iter().map(Into::into).collect();
        ne.labor = v.labor.into();
        ne.logistics = v.logistics.into();
        ne.transport = v.transport.into();
        ne.chain_targets = v.chain_targets.into();
        ne.pending_civilian_hires = v
            .pending_civilian_hires
            .into_iter()
            .map(|(k, n)| (k.into(), n))
            .collect();
        ne.pending_train_to_trained = v.pending_train_to_trained;
        ne.pending_train_to_expert = v.pending_train_to_expert;
        ne.pending_immigration = v.pending_immigration;
        ne.pending_freight_cars = v.pending_freight_cars;
        ne.pending_ships = v.pending_ships;
        ne.pending_army_recruits = v.pending_army_recruits;
        ne.auto_trade_with_minors = v.auto_trade_with_minors;
        ne.restore_reservation_state(domain::nation::ReservationStateSnapshot {
            reserved_treasury: v.reserved_treasury.into(),
            reserved_warehouse: v
                .reserved_warehouse
                .into_iter()
                .map(|(k, n)| (k.into(), n))
                .collect(),
            reserved_materials: v
                .reserved_materials
                .into_iter()
                .map(|(k, n)| (k.into(), n))
                .collect(),
            reserved_goods: v
                .reserved_goods
                .into_iter()
                .map(|(k, n)| (k.into(), n))
                .collect(),
            reservation_ledger: v
                .reservation_ledger
                .into_iter()
                .map(|(k, (c, n))| (k.into(), (c.into(), n)))
                .collect(),
            next_reservation_id: v.next_reservation_id,
            reserved_labor: v
                .reserved_labor
                .into_iter()
                .map(|(k, n)| (k.into(), n))
                .collect(),
        });
        ne
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_targets_defaults_to_zero_when_missing() {
        let json = r#"{"treasury":0,"warehouse":{},"materials":{},"goods":{},"buildings":[],"labor":{"untrained":0,"trained":0,"expert":0}}"#;
        let economy: NationEconomy = serde_json::from_str(json).unwrap();
        assert_eq!(economy.chain_targets.timber_mill, 0);
        assert_eq!(economy.chain_targets.metal_mill, 0);
        assert_eq!(economy.chain_targets.garment_factory, 0);
    }

    #[test]
    fn chain_targets_partial_fields_use_defaults() {
        let json = r#"{"treasury":0,"warehouse":{},"materials":{},"goods":{},"buildings":[],"labor":{"untrained":0,"trained":0,"expert":0},"chain_targets":{"timber_mill":30}}"#;
        let economy: NationEconomy = serde_json::from_str(json).unwrap();
        assert_eq!(economy.chain_targets.timber_mill, 30);
        assert_eq!(economy.chain_targets.metal_mill, 0);
        assert_eq!(economy.chain_targets.garment_factory, 0);
    }
}
