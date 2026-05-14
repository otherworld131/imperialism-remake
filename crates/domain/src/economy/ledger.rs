//! Per-turn cash-flow ledger types.
//!
//! Surfaces income and expense by source so players and — primarily — AI
//! debugging can see where money comes from and where it goes each turn.
//! Resolution of the reconciliation invariant
//! `Σincome − Σexpense == closing_treasury − opening_treasury` is verified
//! by tests; when it fails, a source is missing from the aggregator below.

use crate::types::{GoodsType, MaterialType, Money, NationId, ResourceType};
use std::collections::HashMap;

/// Three-bucket classification for per-turn flows. This is the mental
/// model players and AI-debuggers care about: "money/goods came from
/// producing stuff vs. trading vs. we consumed/spent it".
///
/// Applied uniformly to both `CashSource`/`CashSink` and
/// `ResourceIn`/`ResourceOut` so CLI, batch JSON, and web UI can group
/// flows into the same buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlowCategory {
    /// Output of the nation's own economy — things you produced, or
    /// investments into building more productive capacity.
    Production,
    /// Market transactions — goods moving in/out via the trade system.
    Trade,
    /// Things consumed or spent: maintenance, worker food, upkeep,
    /// losses, debt forgiveness.
    Consumption,
}

impl FlowCategory {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Production => "Production",
            Self::Trade => "Trade",
            Self::Consumption => "Consumption",
        }
    }
}

/// Every per-turn cash income source we currently account for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CashSource {
    /// Gold and Gems resources auto-converted to treasury ($500/$1000 per unit).
    GoldGemsConversion,
    /// Revenue from selling resources on the world market (player or auto).
    TradeExportRevenue,
    /// Revenue from auto-selling materials and goods on the world market.
    GoodsAutoSales,
    /// AI nation sold goods/materials directly for cash (ai/economy.rs paths).
    AiGoodsSale,
    /// Debt forgiven by the bankruptcy clamp at the end of a turn. The clamp
    /// resets a negative treasury to $0; we account for the forgiveness as
    /// income so the reconciliation invariant closes.
    BankruptcyWriteoff,
    // Placeholders — not yet implemented in the economy.
    Tariff,
    Tribute,
}

impl CashSource {
    /// Human-readable label shared by CLI, batch JSON, and the web UI.
    pub const fn label(self) -> &'static str {
        match self {
            Self::GoldGemsConversion => "Gold/Gems conversion",
            Self::TradeExportRevenue => "Trade exports",
            Self::GoodsAutoSales => "Goods auto-sales",
            Self::AiGoodsSale => "AI goods sale",
            Self::BankruptcyWriteoff => "Debt forgiven (bankruptcy)",
            Self::Tariff => "Tariffs",
            Self::Tribute => "Tributes",
        }
    }

    pub const ALL: [CashSource; 7] = [
        Self::GoldGemsConversion,
        Self::TradeExportRevenue,
        Self::GoodsAutoSales,
        Self::AiGoodsSale,
        Self::BankruptcyWriteoff,
        Self::Tariff,
        Self::Tribute,
    ];

    pub const fn category(self) -> FlowCategory {
        match self {
            // Money earned from what the nation produced (mines, factories).
            Self::GoldGemsConversion | Self::GoodsAutoSales | Self::AiGoodsSale => {
                FlowCategory::Production
            }
            // Direct trade-system transactions and trade-adjacent flows.
            Self::TradeExportRevenue | Self::Tariff | Self::Tribute => FlowCategory::Trade,
            // Debt forgiveness is consumption after the fact.
            Self::BankruptcyWriteoff => FlowCategory::Consumption,
        }
    }
}

/// Every per-turn cash expense bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

impl CashSink {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ArmyMaintenance => "Army maintenance",
            Self::TradePurchase => "Trade purchases",
            Self::Subsidy => "Subsidies",
            Self::ConstructionInfrastructure => "Construction",
            Self::AiBuildingConstruction => "AI: building construction",
            Self::AiArmyBuild => "AI: army build",
            Self::AiWarshipBuild => "AI: warship build",
            Self::AiCivilianBuild => "AI: civilian build",
            Self::AiDiplomacyConsulate => "AI: consulate",
            Self::AiDiplomacyEmbassy => "AI: embassy",
            Self::AiGrant => "AI: grant",
            Self::AiInfrastructure => "AI: infrastructure",
            Self::AiResearch => "AI: research",
            Self::AiSpendingOther => "AI: other spending",
            Self::AiTacticalMovement => "AI: tactical move",
        }
    }

    pub const ALL: [CashSink; 15] = [
        Self::ArmyMaintenance,
        Self::TradePurchase,
        Self::Subsidy,
        Self::ConstructionInfrastructure,
        Self::AiBuildingConstruction,
        Self::AiArmyBuild,
        Self::AiWarshipBuild,
        Self::AiCivilianBuild,
        Self::AiDiplomacyConsulate,
        Self::AiDiplomacyEmbassy,
        Self::AiGrant,
        Self::AiInfrastructure,
        Self::AiResearch,
        Self::AiSpendingOther,
        Self::AiTacticalMovement,
    ];

    pub const fn category(self) -> FlowCategory {
        match self {
            // Trade-system transactions.
            Self::TradePurchase => FlowCategory::Trade,
            // Investment in productive capacity (construction, new units,
            // tech, buildings) — grouped under Production per the user's
            // mental model ("money spent to build more production").
            Self::ConstructionInfrastructure
            | Self::AiBuildingConstruction
            | Self::AiArmyBuild
            | Self::AiWarshipBuild
            | Self::AiCivilianBuild
            | Self::AiInfrastructure
            | Self::AiResearch => FlowCategory::Production,
            // Everything else is recurring consumption / upkeep.
            Self::ArmyMaintenance
            | Self::Subsidy
            | Self::AiDiplomacyConsulate
            | Self::AiDiplomacyEmbassy
            | Self::AiGrant
            | Self::AiSpendingOther
            | Self::AiTacticalMovement => FlowCategory::Consumption,
        }
    }
}

/// One atomic cash movement. `partner` is set when the counterparty is known
/// (trade partner, subsidy target) and `None` otherwise.
#[derive(Debug, Clone)]
pub struct CashEntry {
    pub amount: Money,
    pub partner: Option<NationId>,
}

/// Per-nation, per-turn cash ledger.
#[derive(Debug, Clone)]
pub struct CashFlow {
    pub opening_treasury: Money,
    pub closing_treasury: Money,
    pub income: HashMap<CashSource, Vec<CashEntry>>,
    pub expense: HashMap<CashSink, Vec<CashEntry>>,
}

impl Default for CashFlow {
    fn default() -> Self {
        Self::new(Money::ZERO)
    }
}

impl CashFlow {
    pub fn new(opening: Money) -> Self {
        Self {
            opening_treasury: opening,
            closing_treasury: opening,
            income: HashMap::new(),
            expense: HashMap::new(),
        }
    }

    pub fn add_income(&mut self, source: CashSource, amount: Money, partner: Option<NationId>) {
        if amount == Money::ZERO {
            return;
        }
        self.income
            .entry(source)
            .or_default()
            .push(CashEntry { amount, partner });
    }

    pub fn add_expense(&mut self, sink: CashSink, amount: Money, partner: Option<NationId>) {
        if amount == Money::ZERO {
            return;
        }
        self.expense
            .entry(sink)
            .or_default()
            .push(CashEntry { amount, partner });
    }

    pub fn total_income(&self) -> Money {
        let cents: i64 = self
            .income
            .values()
            .flatten()
            .map(|e| e.amount.cents())
            .sum();
        Money::from_cents(cents)
    }

    pub fn total_expense(&self) -> Money {
        let cents: i64 = self
            .expense
            .values()
            .flatten()
            .map(|e| e.amount.cents())
            .sum();
        Money::from_cents(cents)
    }

    /// Sum per income source (dollars).
    pub fn income_totals_by_source(&self) -> HashMap<CashSource, i64> {
        self.income
            .iter()
            .map(|(k, v)| {
                let dollars: i64 = v.iter().map(|e| e.amount.as_dollars()).sum();
                (*k, dollars)
            })
            .collect()
    }

    /// Sum per expense sink (dollars).
    pub fn expense_totals_by_sink(&self) -> HashMap<CashSink, i64> {
        self.expense
            .iter()
            .map(|(k, v)| {
                let dollars: i64 = v.iter().map(|e| e.amount.as_dollars()).sum();
                (*k, dollars)
            })
            .collect()
    }

    /// Treasury delta observed between opening and closing snapshot.
    pub fn observed_delta(&self) -> Money {
        Money::from_cents(self.closing_treasury.cents() - self.opening_treasury.cents())
    }

    /// Delta implied by accounted-for sources (income − expense).
    pub fn accounted_delta(&self) -> Money {
        Money::from_cents(self.total_income().cents() - self.total_expense().cents())
    }

    /// Reconciliation mismatch: `observed − accounted`.
    /// Zero means every treasury mutation is captured by a source/sink.
    pub fn reconciliation_mismatch(&self) -> Money {
        Money::from_cents(self.observed_delta().cents() - self.accounted_delta().cents())
    }

    pub fn reconciles(&self) -> bool {
        self.reconciliation_mismatch() == Money::ZERO
    }

    /// Bucketed income: sum of income dollars per `FlowCategory`
    /// (Production, Trade, Consumption). Missing categories are 0.
    pub fn income_by_category(&self) -> HashMap<FlowCategory, i64> {
        let mut out: HashMap<FlowCategory, i64> = HashMap::new();
        for (source, entries) in &self.income {
            let cat = source.category();
            let sum: i64 = entries.iter().map(|e| e.amount.as_dollars()).sum();
            *out.entry(cat).or_insert(0) += sum;
        }
        out
    }

    /// Bucketed expense: sum of expense dollars per `FlowCategory`.
    pub fn expense_by_category(&self) -> HashMap<FlowCategory, i64> {
        let mut out: HashMap<FlowCategory, i64> = HashMap::new();
        for (sink, entries) in &self.expense {
            let cat = sink.category();
            let sum: i64 = entries.iter().map(|e| e.amount.as_dollars()).sum();
            *out.entry(cat).or_insert(0) += sum;
        }
        out
    }
}

// ──────────────────────────────────────────────────────────────────
// Resource-flow ledger
//
// This is the stockpile counterpart of `CashFlow`. Unlike cash, which has
// a small, well-bounded set of mutation sites, resource stockpiles are
// mutated in ~60 places across `ai/*` and `economy/*`. Full reconciliation
// would require wrapping every single site. For now this is a **best-effort
// visibility ledger**: it aggregates from the already-populated fields on
// `TurnReport` so the CLI, batch, and UI can show meaningful per-turn
// inflow/outflow breakdowns without a 60-site refactor.
//
// Consumers must NOT treat `reconciles()` on a `ResourceFlow` as a
// correctness guarantee — only key categories are tracked.
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stockpile {
    Resource(ResourceType),
    Material(MaterialType),
    Goods(GoodsType),
}

impl Stockpile {
    pub fn label(self) -> String {
        match self {
            Self::Resource(r) => format!("{:?}", r),
            Self::Material(m) => format!("{:?}", m),
            Self::Goods(g) => format!("{:?}", g),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceIn {
    /// Raw resources gathered from owned tiles this turn.
    HomeProduction,
    /// Mill output (Timber→Lumber, Coal+Iron→Steel, Cotton+Wool→Fabric).
    MillOutput,
    /// Factory output (Lumber→Furniture, Steel→Hardware, Fabric→Clothing).
    FactoryOutput,
    /// Village/Town auto-produced materials or goods.
    TownProduced,
    /// Resources received via trade purchases.
    TradeImport,
    /// CannedFood produced from grain/fruit/livestock in `process_food`.
    FoodProcessed,
}

impl ResourceIn {
    pub const fn label(self) -> &'static str {
        match self {
            Self::HomeProduction => "Home production",
            Self::MillOutput => "Mill output",
            Self::FactoryOutput => "Factory output",
            Self::TownProduced => "Town auto-produced",
            Self::TradeImport => "Trade import",
            Self::FoodProcessed => "Food processed",
        }
    }

    pub const fn category(self) -> FlowCategory {
        match self {
            Self::HomeProduction
            | Self::MillOutput
            | Self::FactoryOutput
            | Self::TownProduced
            | Self::FoodProcessed => FlowCategory::Production,
            Self::TradeImport => FlowCategory::Trade,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceOut {
    /// Lost because owning province was disconnected from capital.
    DisconnectedLoss,
    /// Lost to freight-car capacity cap.
    TransportOverflow,
    /// Consumed by worker food eating (grain/fruit/livestock/canned).
    WorkerFood,
    /// Consumed by mill as input raw material.
    MillConsumed,
    /// Consumed by factory as input material.
    FactoryConsumed,
    /// Consumed by food processing (grain/fruit/livestock → canned).
    FoodProcessedInput,
    /// Sent out via trade sales.
    TradeExport,
    /// Consumed by immigration (canned food + clothing + furniture).
    ImmigrationConsumed,
    /// Consumed by AI construction/build paths (mill expansion, freight cars,
    /// ships, training paper). Materials used to build a unit/building/asset
    /// rather than feed a production chain.
    ConstructionConsumed,
}

impl ResourceOut {
    pub const fn label(self) -> &'static str {
        match self {
            Self::DisconnectedLoss => "Lost (disconnected)",
            Self::TransportOverflow => "Lost (transport cap)",
            Self::WorkerFood => "Worker food",
            Self::MillConsumed => "Mill consumed",
            Self::FactoryConsumed => "Factory consumed",
            Self::FoodProcessedInput => "Food processing",
            Self::TradeExport => "Trade export",
            Self::ImmigrationConsumed => "Immigration consumed",
            Self::ConstructionConsumed => "Construction consumed",
        }
    }

    pub const fn category(self) -> FlowCategory {
        match self {
            // Market outflows.
            Self::TradeExport => FlowCategory::Trade,
            // Used up by the economy (or lost outright).
            Self::DisconnectedLoss
            | Self::TransportOverflow
            | Self::WorkerFood
            | Self::MillConsumed
            | Self::FactoryConsumed
            | Self::FoodProcessedInput
            | Self::ImmigrationConsumed
            | Self::ConstructionConsumed => FlowCategory::Consumption,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResourceInflowEntry {
    pub stockpile: Stockpile,
    pub source: ResourceIn,
    pub amount: u32,
}

#[derive(Debug, Clone)]
pub struct ResourceOutflowEntry {
    pub stockpile: Stockpile,
    pub sink: ResourceOut,
    pub amount: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ResourceFlow {
    pub inflow: Vec<ResourceInflowEntry>,
    pub outflow: Vec<ResourceOutflowEntry>,
}

impl ResourceFlow {
    pub fn add_inflow(&mut self, stock: Stockpile, source: ResourceIn, amount: u32) {
        if amount == 0 {
            return;
        }
        if let Some(existing) = self
            .inflow
            .iter_mut()
            .find(|e| e.stockpile == stock && e.source == source)
        {
            existing.amount += amount;
        } else {
            self.inflow.push(ResourceInflowEntry {
                stockpile: stock,
                source,
                amount,
            });
        }
    }

    pub fn add_outflow(&mut self, stock: Stockpile, sink: ResourceOut, amount: u32) {
        if amount == 0 {
            return;
        }
        if let Some(existing) = self
            .outflow
            .iter_mut()
            .find(|e| e.stockpile == stock && e.sink == sink)
        {
            existing.amount += amount;
        } else {
            self.outflow.push(ResourceOutflowEntry {
                stockpile: stock,
                sink,
                amount,
            });
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inflow.is_empty() && self.outflow.is_empty()
    }

    /// Sum of inflows per stockpile (totals across all sources).
    pub fn inflow_totals_by_stockpile(&self) -> HashMap<Stockpile, u32> {
        let mut out: HashMap<Stockpile, u32> = HashMap::new();
        for e in &self.inflow {
            *out.entry(e.stockpile).or_insert(0) += e.amount;
        }
        out
    }

    /// Sum of outflows per stockpile.
    pub fn outflow_totals_by_stockpile(&self) -> HashMap<Stockpile, u32> {
        let mut out: HashMap<Stockpile, u32> = HashMap::new();
        for e in &self.outflow {
            *out.entry(e.stockpile).or_insert(0) += e.amount;
        }
        out
    }

    /// Per-stockpile inflow totals split by `FlowCategory`. Nested map so
    /// CLI/UI can show e.g. `Timber: +40 production, +5 trade`.
    pub fn inflow_by_stockpile_and_category(
        &self,
    ) -> HashMap<Stockpile, HashMap<FlowCategory, u32>> {
        let mut out: HashMap<Stockpile, HashMap<FlowCategory, u32>> = HashMap::new();
        for e in &self.inflow {
            *out.entry(e.stockpile)
                .or_default()
                .entry(e.source.category())
                .or_insert(0) += e.amount;
        }
        out
    }

    /// Per-stockpile outflow totals split by `FlowCategory`.
    pub fn outflow_by_stockpile_and_category(
        &self,
    ) -> HashMap<Stockpile, HashMap<FlowCategory, u32>> {
        let mut out: HashMap<Stockpile, HashMap<FlowCategory, u32>> = HashMap::new();
        for e in &self.outflow {
            *out.entry(e.stockpile)
                .or_default()
                .entry(e.sink.category())
                .or_insert(0) += e.amount;
        }
        out
    }

    /// Cross-stockpile totals by category for a summary "In: X production,
    /// Y trade | Out: Z consumption, W trade" line.
    pub fn inflow_by_category(&self) -> HashMap<FlowCategory, u32> {
        let mut out: HashMap<FlowCategory, u32> = HashMap::new();
        for e in &self.inflow {
            *out.entry(e.source.category()).or_insert(0) += e.amount;
        }
        out
    }

    pub fn outflow_by_category(&self) -> HashMap<FlowCategory, u32> {
        let mut out: HashMap<FlowCategory, u32> = HashMap::new();
        for e in &self.outflow {
            *out.entry(e.sink.category()).or_insert(0) += e.amount;
        }
        out
    }
}

/// Structured per-turn tracking of material- and goods-stockpile movements.
///
/// `finalize_resource_flow` reads these vectors and folds them into the
/// per-nation `ResourceFlow` so the Materials ledger can show the same
/// production / trade / consumption breakdown the Resources ledger has.
#[derive(Debug, Default)]
pub struct StockpileFlowTracking {
    /// Resources consumed by mills as input (e.g. Timber → Lumber).
    pub mill_consumed_resources: Vec<(NationId, ResourceType, u32)>,
    /// Materials produced by mills.
    pub mill_produced_materials: Vec<(NationId, MaterialType, u32)>,
    /// Materials consumed by factories (e.g. Lumber → Furniture).
    pub factory_consumed_materials: Vec<(NationId, MaterialType, u32)>,
    /// Goods produced by factories.
    pub factory_produced_goods: Vec<(NationId, GoodsType, u32)>,
    /// Materials produced via village/town auto-production.
    pub town_produced_materials: Vec<(NationId, MaterialType, u32)>,
    /// Goods produced via village/town auto-production.
    pub town_produced_goods: Vec<(NationId, GoodsType, u32)>,
    /// Raw food (grain/fruit/livestock) consumed by `process_food` to make canned food.
    pub food_processed_inputs: Vec<(NationId, ResourceType, u32)>,
    /// Canned food produced by `process_food`.
    pub canned_food_produced: Vec<(NationId, u32)>,
    /// Raw food consumed by workers eating.
    pub worker_food_consumed: Vec<(NationId, ResourceType, u32)>,
    /// Canned food consumed by workers eating (fallback).
    pub worker_canned_food_consumed: Vec<(NationId, u32)>,
    /// Materials auto-sold to the world market via player sell orders.
    pub auto_sold_materials: Vec<(NationId, MaterialType, u32)>,
    /// Goods auto-sold to the world market via player sell orders.
    pub auto_sold_goods: Vec<(NationId, GoodsType, u32)>,
    /// Materials consumed by immigration recruitment (CannedFood per immigrant).
    pub immigration_consumed_materials: Vec<(NationId, MaterialType, u32)>,
    /// Goods consumed by immigration recruitment (Clothing + Furniture per immigrant).
    pub immigration_consumed_goods: Vec<(NationId, GoodsType, u32)>,
}
