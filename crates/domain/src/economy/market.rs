//! Persistent market state with price history and trend analysis (Trello #164).
//!
//! `MarketState` promotes market pricing from a stateless helper computation
//! into a first-class persistent object. It records per-commodity supply,
//! demand, and realized prices each turn so the AI and UI can reason about
//! price trends rather than only current-turn snapshots.

use crate::economy::trade::Commodity;
use crate::types::*;
use std::collections::BTreeMap;
use std::collections::VecDeque;

/// Maximum number of turns retained per commodity in the price history.
const HISTORY_DEPTH: usize = 20;

/// A single turn's market observation for one commodity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarketTick {
    pub turn: TurnNumber,
    /// Effective market price this turn (may differ from base price due to supply pressure).
    pub price: Money,
    /// Total supply offered across all sellers.
    pub supply: u32,
    /// Total demand bid across all buyers.
    pub demand: u32,
    /// Demand that could not be fulfilled (demand – sold).
    pub unmet_demand: u32,
    /// Quantity actually sold.
    pub sold: u32,
}

/// Direction of recent price movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

impl std::fmt::Display for Trend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trend::Rising => write!(f, "↑"),
            Trend::Falling => write!(f, "↓"),
            Trend::Stable => write!(f, "→"),
        }
    }
}

/// Persistent market state: price history and trend analysis per commodity.
///
/// Updated at the end of each turn's execute phase from realized supply/demand.
/// The AI economy snapshot projects price trend and volatility from this state.
///
/// Separate maps per commodity tier (Resource/Material/Goods) mirror the pattern
/// in `NationEconomy` and sidestep the JSON restriction that map keys must be strings
/// (the `Commodity` enum serialises as an object, not a string).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MarketState {
    /// Most recently recorded price per resource (base price until first tick).
    #[serde(default)]
    pub resource_prices: BTreeMap<ResourceType, Money>,
    #[serde(default)]
    pub material_prices: BTreeMap<MaterialType, Money>,
    #[serde(default)]
    pub goods_prices: BTreeMap<GoodsType, Money>,
    /// Per-resource ring buffer of the last `HISTORY_DEPTH` ticks.
    #[serde(default)]
    pub resource_history: BTreeMap<ResourceType, VecDeque<MarketTick>>,
    #[serde(default)]
    pub material_history: BTreeMap<MaterialType, VecDeque<MarketTick>>,
    #[serde(default)]
    pub goods_history: BTreeMap<GoodsType, VecDeque<MarketTick>>,
}

impl MarketState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a market observation for `commodity` this turn.
    ///
    /// The history buffer is bounded to `HISTORY_DEPTH` ticks — oldest entries
    /// are dropped automatically so save sizes remain constant.
    pub fn record_tick(
        &mut self,
        commodity: Commodity,
        turn: TurnNumber,
        price: Money,
        supply: u32,
        demand: u32,
        sold: u32,
    ) {
        let unmet_demand = demand.saturating_sub(sold);
        let tick = MarketTick { turn, price, supply, demand, unmet_demand, sold };

        match commodity {
            Commodity::Resource(r) => {
                self.resource_prices.insert(r, price);
                let buf = self.resource_history.entry(r).or_default();
                if buf.len() == HISTORY_DEPTH { buf.pop_front(); }
                buf.push_back(tick);
            }
            Commodity::Material(m) => {
                self.material_prices.insert(m, price);
                let buf = self.material_history.entry(m).or_default();
                if buf.len() == HISTORY_DEPTH { buf.pop_front(); }
                buf.push_back(tick);
            }
            Commodity::Goods(g) => {
                self.goods_prices.insert(g, price);
                let buf = self.goods_history.entry(g).or_default();
                if buf.len() == HISTORY_DEPTH { buf.pop_front(); }
                buf.push_back(tick);
            }
        }
    }

    /// Most recently recorded price for `commodity`.
    ///
    /// Returns `Money::ZERO` if no ticks have been recorded yet.
    pub fn current_price(&self, commodity: Commodity) -> Money {
        match commodity {
            Commodity::Resource(r) => self.resource_prices.get(&r).copied().unwrap_or(Money::ZERO),
            Commodity::Material(m) => self.material_prices.get(&m).copied().unwrap_or(Money::ZERO),
            Commodity::Goods(g) => self.goods_prices.get(&g).copied().unwrap_or(Money::ZERO),
        }
    }

    fn history_buf(&self, commodity: Commodity) -> Option<&VecDeque<MarketTick>> {
        match commodity {
            Commodity::Resource(r) => self.resource_history.get(&r),
            Commodity::Material(m) => self.material_history.get(&m),
            Commodity::Goods(g) => self.goods_history.get(&g),
        }
    }

    /// Price trend over the last `window` turns for `commodity`.
    ///
    /// Compares the average price of the first half of the window against the
    /// second half. Returns `Stable` if there are fewer than 2 ticks recorded.
    ///
    /// `window` is clamped to the available history length.
    pub fn trend(&self, commodity: Commodity, window: usize) -> Trend {
        let Some(buf) = self.history_buf(commodity) else {
            return Trend::Stable;
        };
        if buf.len() < 2 {
            return Trend::Stable;
        }
        let window = window.min(buf.len());
        let slice: Vec<&MarketTick> = buf.iter().rev().take(window).collect();
        // slice[0] = newest, slice[window-1] = oldest within window
        let half = window / 2;
        if half == 0 {
            return Trend::Stable;
        }
        let recent_avg: f64 =
            slice[..half].iter().map(|t| t.price.as_dollars() as f64).sum::<f64>() / half as f64;
        let older_avg: f64 =
            slice[half..].iter().map(|t| t.price.as_dollars() as f64).sum::<f64>()
                / (window - half) as f64;
        // 5% threshold to avoid noise
        let ratio = if older_avg > 0.0 { recent_avg / older_avg } else { 1.0 };
        if ratio > 1.05 {
            Trend::Rising
        } else if ratio < 0.95 {
            Trend::Falling
        } else {
            Trend::Stable
        }
    }

    /// Price volatility (normalised standard deviation) over the last `window` turns.
    ///
    /// Returns 0.0 if fewer than 2 ticks are available.
    pub fn volatility(&self, commodity: Commodity, window: usize) -> f32 {
        let Some(buf) = self.history_buf(commodity) else {
            return 0.0;
        };
        let window = window.min(buf.len());
        if window < 2 {
            return 0.0;
        }
        let prices: Vec<f64> =
            buf.iter().rev().take(window).map(|t| t.price.as_dollars() as f64).collect();
        let mean = prices.iter().sum::<f64>() / prices.len() as f64;
        if mean == 0.0 {
            return 0.0;
        }
        let variance = prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / prices.len() as f64;
        (variance.sqrt() / mean) as f32
    }

    /// Iterator over all commodities with at least one recorded price.
    pub fn commodities_with_history(&self) -> impl Iterator<Item = Commodity> + '_ {
        let resources = self.resource_history.keys().map(|&r| Commodity::Resource(r));
        let materials = self.material_history.keys().map(|&m| Commodity::Material(m));
        let goods = self.goods_history.keys().map(|&g| Commodity::Goods(g));
        resources.chain(materials).chain(goods)
    }

    /// Iterator over all commodities with a recorded current price.
    pub fn commodities_with_price(&self) -> impl Iterator<Item = (Commodity, Money)> + '_ {
        let resources = self.resource_prices.iter().map(|(&r, &p)| (Commodity::Resource(r), p));
        let materials = self.material_prices.iter().map(|(&m, &p)| (Commodity::Material(m), p));
        let goods = self.goods_prices.iter().map(|(&g, &p)| (Commodity::Goods(g), p));
        resources.chain(materials).chain(goods)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ResourceType;

    fn coal() -> Commodity {
        Commodity::Resource(ResourceType::Coal)
    }

    #[test]
    fn record_tick_updates_price() {
        let mut ms = MarketState::new();
        ms.record_tick(coal(), TurnNumber::new(1), Money::dollars(75), 10, 8, 8);
        assert_eq!(ms.current_price(coal()), Money::dollars(75));
    }

    #[test]
    fn history_capped_at_depth() {
        let mut ms = MarketState::new();
        for i in 1..=(HISTORY_DEPTH + 5) as u32 {
            ms.record_tick(coal(), TurnNumber::new(i), Money::dollars(i as i64), 1, 1, 1);
        }
        assert_eq!(ms.resource_history[&ResourceType::Coal].len(), HISTORY_DEPTH);
        // Most recent price should be the last one recorded
        assert_eq!(ms.current_price(coal()), Money::dollars((HISTORY_DEPTH + 5) as i64));
    }

    #[test]
    fn trend_rising_when_prices_increase() {
        let mut ms = MarketState::new();
        // Older prices are low, recent prices are high
        for (i, price) in [50, 51, 52, 70, 75, 80].iter().enumerate() {
            ms.record_tick(coal(), TurnNumber::new(i as u32 + 1), Money::dollars(*price), 1, 1, 1);
        }
        assert_eq!(ms.trend(coal(), 6), Trend::Rising);
    }

    #[test]
    fn trend_falling_when_prices_decrease() {
        let mut ms = MarketState::new();
        for (i, price) in [80, 75, 70, 52, 51, 50].iter().enumerate() {
            ms.record_tick(coal(), TurnNumber::new(i as u32 + 1), Money::dollars(*price), 1, 1, 1);
        }
        assert_eq!(ms.trend(coal(), 6), Trend::Falling);
    }

    #[test]
    fn trend_stable_when_prices_flat() {
        let mut ms = MarketState::new();
        for i in 1..=6u32 {
            ms.record_tick(coal(), TurnNumber::new(i), Money::dollars(75), 1, 1, 1);
        }
        assert_eq!(ms.trend(coal(), 6), Trend::Stable);
    }

    #[test]
    fn trend_stable_with_no_history() {
        let ms = MarketState::new();
        assert_eq!(ms.trend(coal(), 5), Trend::Stable);
    }

    #[test]
    fn volatility_zero_with_flat_prices() {
        let mut ms = MarketState::new();
        for i in 1..=5u32 {
            ms.record_tick(coal(), TurnNumber::new(i), Money::dollars(75), 1, 1, 1);
        }
        assert!((ms.volatility(coal(), 5) as f64).abs() < 0.01);
    }

    #[test]
    fn volatility_nonzero_with_swings() {
        let mut ms = MarketState::new();
        for (i, price) in [50, 100, 50, 100, 50].iter().enumerate() {
            ms.record_tick(coal(), TurnNumber::new(i as u32 + 1), Money::dollars(*price), 1, 1, 1);
        }
        assert!(ms.volatility(coal(), 5) > 0.3);
    }

    #[test]
    fn market_tick_unmet_demand_computed_correctly() {
        let mut ms = MarketState::new();
        ms.record_tick(coal(), TurnNumber::new(1), Money::dollars(75), 5, 10, 5);
        let tick = &ms.resource_history[&ResourceType::Coal][0];
        assert_eq!(tick.unmet_demand, 5);
        assert_eq!(tick.supply, 5);
        assert_eq!(tick.demand, 10);
        assert_eq!(tick.sold, 5);
    }

    #[test]
    fn market_state_serializes_and_deserializes() {
        let mut ms = MarketState::new();
        ms.record_tick(coal(), TurnNumber::new(3), Money::dollars(80), 5, 7, 5);
        let json = serde_json::to_string(&ms).unwrap();
        let restored: MarketState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.current_price(coal()), Money::dollars(80));
        assert_eq!(restored.resource_history[&ResourceType::Coal].len(), 1);
    }
}
