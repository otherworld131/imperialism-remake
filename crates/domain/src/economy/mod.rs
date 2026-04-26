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
pub use civilians::{BuildTask, Civilian, CivilianType, parse_civilian_type};
#[cfg(test)]
pub use civilians::next_civilian_id;
pub use labor::{LaborPool, TemporaryPenalty, TierState, WorkerType};
pub use ledger::{
    CashEntry, CashFlow, CashSink, CashSource, FlowCategory, ResourceFlow, ResourceIn, ResourceOut,
    Stockpile,
};
pub use production::{ProductionChain, ProductionResult};
pub use trade::{
    Commodity, PlayerBuyOrder, PlayerSellOrder, TradeBid, TradeOffer, TradeTransaction,
    commodity_price,
};
pub use market::{MarketState, MarketTick, Trend};
pub use observability::BlockReason;
pub use transport::{FreightDemand, LogisticsState, TransportSystem};
