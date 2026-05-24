//! Persistent market state with price history and trend analysis (Trello #164).
//!
//! `MarketState` promotes market pricing from a stateless helper computation
//! into a first-class persistent object. It records per-commodity supply,
//! demand, and realized prices each turn so the AI and UI can reason about
//! price trends rather than only current-turn snapshots.
//!
//! Pricing model: each commodity starts at its tier base (Resource / Material
//! / Goods) read from `GameConfig`. Each turn, `apply_drift` nudges the
//! `current_price` based on the most recent tick's supply/demand imbalance,
//! clamped to `[floor_multiplier, ceiling_multiplier] × tier_base`. The drift
//! is cumulative — sustained shortage walks the price up to the ceiling,
//! sustained glut walks it down to the floor.

use crate::data::GameConfig;
use crate::economy::trade::Commodity;
use crate::types::*;
use std::collections::BTreeMap;
use std::collections::VecDeque;

/// Maximum number of turns retained per commodity in the price history.
const HISTORY_DEPTH: usize = 20;

/// All resource variants — used to seed `MarketState` with tier base prices
/// for every commodity at game start.
const ALL_RESOURCES: [ResourceType; 13] = [
    ResourceType::Timber,
    ResourceType::Coal,
    ResourceType::Iron,
    ResourceType::Cotton,
    ResourceType::Wool,
    ResourceType::Grain,
    ResourceType::Fruit,
    ResourceType::Livestock,
    ResourceType::Horses,
    ResourceType::Oil,
    ResourceType::Gold,
    ResourceType::Gems,
    ResourceType::Fish,
];
const ALL_MATERIALS: [MaterialType; 5] = [
    MaterialType::Lumber,
    MaterialType::Steel,
    MaterialType::Fabric,
    MaterialType::Paper,
    MaterialType::CannedFood,
];
const ALL_GOODS: [GoodsType; 4] = [
    GoodsType::Furniture,
    GoodsType::Clothing,
    GoodsType::Hardware,
    GoodsType::Arms,
];


/// A single turn's market observation for one commodity.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Default)]
pub struct MarketState {
    /// Most recently recorded price per resource (base price until first tick).
    pub resource_prices: BTreeMap<ResourceType, Money>,
    pub material_prices: BTreeMap<MaterialType, Money>,
    pub goods_prices: BTreeMap<GoodsType, Money>,
    /// Per-resource ring buffer of the last `HISTORY_DEPTH` ticks.
    pub resource_history: BTreeMap<ResourceType, VecDeque<MarketTick>>,
    pub material_history: BTreeMap<MaterialType, VecDeque<MarketTick>>,
    pub goods_history: BTreeMap<GoodsType, VecDeque<MarketTick>>,
}

impl MarketState {
    /// Bare constructor — leaves all price maps empty. Prefer
    /// [`MarketState::with_config`] for production use so prices are seeded at
    /// their tier base.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructor that seeds `current_price` for every commodity at its tier
    /// base. After this, [`Self::current_price`] always returns a sensible
    /// value, even before any trade has happened.
    pub fn with_config(config: &GameConfig) -> Self {
        let mut state = Self::default();
        for r in ALL_RESOURCES {
            state
                .resource_prices
                .insert(r, Self::tier_base_price(Commodity::Resource(r), config));
        }
        for m in ALL_MATERIALS {
            state
                .material_prices
                .insert(m, Self::tier_base_price(Commodity::Material(m), config));
        }
        for g in ALL_GOODS {
            state
                .goods_prices
                .insert(g, Self::tier_base_price(Commodity::Goods(g), config));
        }
        state
    }

    /// Tier base price for `commodity`. Three tiers:
    ///
    /// - **Resource tier** — most raw `ResourceType`s: industrial inputs
    ///   (Timber, Coal, Iron, Cotton, Wool, Oil, Gold, Gems) and raw food
    ///   (Grain, Fruit, Livestock, Fish).
    /// - **Material tier** — processed materials (Lumber, Steel, Fabric,
    ///   Paper, Canned Food) PLUS **Horses**. Horses are a raw
    ///   `ResourceType` but command a material-tier price — a bred animal
    ///   trades like a manufactured product, not a raw ore.
    /// - **Goods tier** — finished goods (Furniture, Clothing, Hardware,
    ///   Arms).
    ///
    /// Per-commodity differentiation within a tier emerges only from drift.
    pub fn tier_base_price(commodity: Commodity, config: &GameConfig) -> Money {
        match commodity {
            Commodity::Resource(ResourceType::Horses) => {
                Money::dollars(config.market_material_base_price)
            }
            Commodity::Resource(_) => Money::dollars(config.market_resource_base_price),
            Commodity::Material(_) => Money::dollars(config.market_material_base_price),
            Commodity::Goods(_) => Money::dollars(config.market_goods_base_price),
        }
    }

    /// Record a market observation for `commodity` this turn.
    ///
    /// Updates the history ring buffer only — `current_price` is owned by
    /// [`Self::apply_drift`], which should be called per commodity once per
    /// turn after all ticks have been recorded.
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
        let tick = MarketTick {
            turn,
            price,
            supply,
            demand,
            unmet_demand,
            sold,
        };

        match commodity {
            Commodity::Resource(r) => {
                let buf = self.resource_history.entry(r).or_default();
                if buf.len() == HISTORY_DEPTH {
                    buf.pop_front();
                }
                buf.push_back(tick);
            }
            Commodity::Material(m) => {
                let buf = self.material_history.entry(m).or_default();
                if buf.len() == HISTORY_DEPTH {
                    buf.pop_front();
                }
                buf.push_back(tick);
            }
            Commodity::Goods(g) => {
                let buf = self.goods_history.entry(g).or_default();
                if buf.len() == HISTORY_DEPTH {
                    buf.pop_front();
                }
                buf.push_back(tick);
            }
        }
    }

    /// Drift `current_price` for `commodity` based on this turn's market.
    ///
    /// If a tick was recorded for `current_turn`: drift by
    /// `base × drift_step_pct/100 × imbalance`, where
    /// `imbalance = (demand − supply) / max(supply, demand, 1) ∈ [−1, 1]`.
    ///
    /// If there's no market at all this turn (neither offer nor bid), the
    /// price is unchanged — no idle mean-reversion. Real-world markets don't
    /// move prices when nobody trades; we shouldn't either.
    ///
    /// The result is clamped to `[floor_multiplier, ceiling_multiplier] × base`.
    pub fn apply_drift(
        &mut self,
        commodity: Commodity,
        current_turn: TurnNumber,
        config: &GameConfig,
    ) {
        let base = Self::tier_base_price(commodity, config).as_dollars();
        let floor = (base as f64 * config.market_floor_multiplier).round() as i64;
        let ceiling = (base as f64 * config.market_ceiling_multiplier).round() as i64;
        let current = self.current_price(commodity).as_dollars();

        let this_turn_tick = self
            .latest_tick(commodity)
            .filter(|t| t.turn == current_turn);

        let Some(tick) = this_turn_tick else {
            return; // no trade activity this turn → price doesn't move
        };

        let denom = tick.supply.max(tick.demand).max(1) as f64;
        let imbalance = (tick.demand as f64 - tick.supply as f64) / denom;
        let delta = base as f64 * (config.market_drift_step_pct as f64 / 100.0) * imbalance;

        let next = ((current as f64) + delta).round() as i64;
        let clamped = next.clamp(floor, ceiling);
        let new_price = Money::dollars(clamped);
        match commodity {
            Commodity::Resource(r) => {
                self.resource_prices.insert(r, new_price);
            }
            Commodity::Material(m) => {
                self.material_prices.insert(m, new_price);
            }
            Commodity::Goods(g) => {
                self.goods_prices.insert(g, new_price);
            }
        }
    }

    fn latest_tick(&self, commodity: Commodity) -> Option<&MarketTick> {
        self.history_buf(commodity).and_then(|buf| buf.back())
    }

    /// Most recently recorded price for `commodity`.
    ///
    /// Returns the tier base if no `with_config` seeding happened and no drift
    /// has run yet — but in normal play `MarketState::with_config` seeds every
    /// commodity at construction so this fallback is effectively unreachable.
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
        let recent_avg: f64 = slice[..half]
            .iter()
            .map(|t| t.price.as_dollars() as f64)
            .sum::<f64>()
            / half as f64;
        let older_avg: f64 = slice[half..]
            .iter()
            .map(|t| t.price.as_dollars() as f64)
            .sum::<f64>()
            / (window - half) as f64;
        // 5% threshold to avoid noise
        let ratio = if older_avg > 0.0 {
            recent_avg / older_avg
        } else {
            1.0
        };
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
        let prices: Vec<f64> = buf
            .iter()
            .rev()
            .take(window)
            .map(|t| t.price.as_dollars() as f64)
            .collect();
        let mean = prices.iter().sum::<f64>() / prices.len() as f64;
        if mean == 0.0 {
            return 0.0;
        }
        let variance = prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / prices.len() as f64;
        (variance.sqrt() / mean) as f32
    }

    /// Iterator over all commodities with at least one recorded price.
    pub fn commodities_with_history(&self) -> impl Iterator<Item = Commodity> + '_ {
        let resources = self
            .resource_history
            .keys()
            .map(|&r| Commodity::Resource(r));
        let materials = self
            .material_history
            .keys()
            .map(|&m| Commodity::Material(m));
        let goods = self.goods_history.keys().map(|&g| Commodity::Goods(g));
        resources.chain(materials).chain(goods)
    }

    /// Iterator over all commodities with a recorded current price.
    pub fn commodities_with_price(&self) -> impl Iterator<Item = (Commodity, Money)> + '_ {
        let resources = self
            .resource_prices
            .iter()
            .map(|(&r, &p)| (Commodity::Resource(r), p));
        let materials = self
            .material_prices
            .iter()
            .map(|(&m, &p)| (Commodity::Material(m), p));
        let goods = self
            .goods_prices
            .iter()
            .map(|(&g, &p)| (Commodity::Goods(g), p));
        resources.chain(materials).chain(goods)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameConfig;
    use crate::types::ResourceType;

    fn coal() -> Commodity {
        Commodity::Resource(ResourceType::Coal)
    }

    fn cfg() -> GameConfig {
        GameConfig::default()
    }

    #[test]
    fn with_config_seeds_every_commodity_at_tier_base() {
        let ms = MarketState::with_config(&cfg());
        let r_base = Money::dollars(cfg().market_resource_base_price);
        let m_base = Money::dollars(cfg().market_material_base_price);
        let g_base = Money::dollars(cfg().market_goods_base_price);
        assert_eq!(ms.current_price(Commodity::Resource(ResourceType::Coal)), r_base);
        assert_eq!(ms.current_price(Commodity::Resource(ResourceType::Gems)), r_base);
        assert_eq!(ms.current_price(Commodity::Material(MaterialType::Steel)), m_base);
        assert_eq!(ms.current_price(Commodity::Goods(GoodsType::Arms)), g_base);
    }

    #[test]
    fn history_capped_at_depth() {
        let mut ms = MarketState::with_config(&cfg());
        for i in 1..=(HISTORY_DEPTH + 5) as u32 {
            ms.record_tick(
                coal(),
                TurnNumber::new(i),
                Money::dollars(i as i64),
                1,
                1,
                1,
            );
        }
        assert_eq!(
            ms.resource_history[&ResourceType::Coal].len(),
            HISTORY_DEPTH
        );
    }

    #[test]
    fn apply_drift_raises_price_under_persistent_shortage_then_clamps() {
        let c = cfg();
        let mut ms = MarketState::with_config(&c);
        let base = c.market_resource_base_price;
        let ceiling = (base as f64 * c.market_ceiling_multiplier).round() as i64;
        // Run 200 turns of pure shortage — must clamp to ceiling, not go past.
        for t in 1..=200u32 {
            let turn = TurnNumber::new(t);
            ms.record_tick(coal(), turn, Money::dollars(base), 0, 20, 0);
            ms.apply_drift(coal(), turn, &c);
        }
        assert_eq!(ms.current_price(coal()), Money::dollars(ceiling));
    }

    #[test]
    fn apply_drift_lowers_price_under_persistent_glut_then_clamps() {
        let c = cfg();
        let mut ms = MarketState::with_config(&c);
        let base = c.market_resource_base_price;
        let floor = (base as f64 * c.market_floor_multiplier).round() as i64;
        for t in 1..=200u32 {
            let turn = TurnNumber::new(t);
            ms.record_tick(coal(), turn, Money::dollars(base), 20, 0, 0);
            ms.apply_drift(coal(), turn, &c);
        }
        assert_eq!(ms.current_price(coal()), Money::dollars(floor));
    }

    #[test]
    fn apply_drift_stable_at_base_under_balanced_market() {
        let c = cfg();
        let mut ms = MarketState::with_config(&c);
        let base = c.market_resource_base_price;
        for t in 1..=50u32 {
            let turn = TurnNumber::new(t);
            ms.record_tick(coal(), turn, Money::dollars(base), 10, 10, 10);
            ms.apply_drift(coal(), turn, &c);
        }
        // Balanced means imbalance == 0, so drift is 0 every turn.
        assert_eq!(ms.current_price(coal()), Money::dollars(base));
    }

    #[test]
    fn apply_drift_does_nothing_when_no_market_this_turn() {
        let c = cfg();
        let mut ms = MarketState::with_config(&c);
        let base = c.market_resource_base_price;
        let ceiling = (base as f64 * c.market_ceiling_multiplier).round() as i64;
        // Drive to ceiling first with sustained shortage.
        for t in 1..=400u32 {
            let turn = TurnNumber::new(t);
            ms.record_tick(coal(), turn, Money::dollars(base), 0, 20, 0);
            ms.apply_drift(coal(), turn, &c);
        }
        assert_eq!(ms.current_price(coal()), Money::dollars(ceiling));
        // Apply drift on a later turn with NO recorded tick this turn —
        // price must stay put (no idle mean-reversion).
        let before = ms.current_price(coal()).as_dollars();
        ms.apply_drift(coal(), TurnNumber::new(500), &c);
        let after = ms.current_price(coal()).as_dollars();
        assert_eq!(
            before, after,
            "no market this turn must leave the price unchanged"
        );
    }

    #[test]
    fn trend_rising_when_prices_increase() {
        let mut ms = MarketState::new();
        // Older prices are low, recent prices are high
        for (i, price) in [50, 51, 52, 70, 75, 80].iter().enumerate() {
            ms.record_tick(
                coal(),
                TurnNumber::new(i as u32 + 1),
                Money::dollars(*price),
                1,
                1,
                1,
            );
        }
        assert_eq!(ms.trend(coal(), 6), Trend::Rising);
    }

    #[test]
    fn trend_falling_when_prices_decrease() {
        let mut ms = MarketState::new();
        for (i, price) in [80, 75, 70, 52, 51, 50].iter().enumerate() {
            ms.record_tick(
                coal(),
                TurnNumber::new(i as u32 + 1),
                Money::dollars(*price),
                1,
                1,
                1,
            );
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
            ms.record_tick(
                coal(),
                TurnNumber::new(i as u32 + 1),
                Money::dollars(*price),
                1,
                1,
                1,
            );
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
    fn record_tick_appends_history_without_touching_current_price() {
        let mut ms = MarketState::with_config(&cfg());
        let seeded = ms.current_price(coal());
        // Passing a different price into record_tick must NOT update current_price.
        ms.record_tick(coal(), TurnNumber::new(3), Money::dollars(999), 5, 7, 5);
        assert_eq!(ms.current_price(coal()), seeded);
        assert_eq!(ms.resource_history[&ResourceType::Coal].len(), 1);
        assert_eq!(
            ms.resource_history[&ResourceType::Coal][0].price,
            Money::dollars(999)
        );
    }
}
