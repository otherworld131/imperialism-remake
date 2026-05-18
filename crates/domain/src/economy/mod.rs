pub mod buildings;
pub mod civilians;
pub mod labor;
pub mod ledger;
pub mod market;
pub mod observability;
pub mod production;
pub mod trade;
pub mod transport;

pub use buildings::{Building, BuildingType};
#[cfg(test)]
pub use civilians::next_civilian_id;
pub use civilians::{BuildTask, Civilian, CivilianType, parse_civilian_type};
pub use labor::{LaborPool, TemporaryPenalty, TierState, WorkerType};
pub use ledger::{
    CashEntry, CashFlow, CashSink, CashSource, FlowCategory, ResourceFlow, ResourceIn, ResourceOut,
    Stockpile,
};
pub use market::{MarketState, MarketTick, Trend};
pub use observability::BlockReason;
pub use production::{ProductionChain, ProductionResult};
pub use trade::{
    Commodity, PlayerBuyOrder, PlayerSellOrder, TradeBid, TradeOffer, TradeTransaction,
};
pub use transport::{
    FreightDemand, FreightTarget, LogisticsState, TransportSystem, allocate_town_output_freight,
    compute_demand_forecast, current_collectable_resources, project_town_outputs,
};
