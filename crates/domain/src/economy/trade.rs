use crate::diplomacy::DiplomacyState;
use crate::map::{HexMap, Province};
use crate::nation::Nation;
use crate::types::*;

/// A trade offer: a nation wants to sell goods.
#[derive(Debug, Clone)]
pub struct TradeOffer {
    pub seller: NationId,
    pub resource: ResourceType,
    pub quantity: u32,
    pub price_per_unit: Money,
}

/// A trade bid: a nation wants to buy resources.
#[derive(Debug, Clone)]
pub struct TradeBid {
    pub buyer: NationId,
    pub resource: ResourceType,
    pub quantity: u32,
    pub max_price_per_unit: Money,
}

/// Result of a single trade transaction.
#[derive(Debug, Clone)]
pub struct TradeTransaction {
    pub buyer: NationId,
    pub seller: NationId,
    pub resource: ResourceType,
    pub quantity: u32,
    pub price_per_unit: Money,
    pub total_cost: Money,
}

/// A record of a past trade transaction, stored in a nation's trade history.
#[derive(Debug, Clone)]
pub struct TradeHistoryEntry {
    pub turn: TurnNumber,
    pub partner: NationId,
    pub resource: ResourceType,
    /// Human-readable label for the traded commodity. For resource trades this
    /// matches `resource` printed as a string. For material/goods auto-sales it
    /// holds the material or goods name (e.g. "Lumber", "Furniture").
    pub commodity_label: String,
    pub quantity: u32,
    pub total_cost: Money,
    /// Whether this nation was the buyer (true) or seller (false) in this transaction.
    pub bought: bool,
}

/// A unified commodity type covering resources, materials, and goods.
/// Used for player trade orders where any commodity can be sold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Commodity {
    Resource(ResourceType),
    Material(MaterialType),
    Goods(GoodsType),
}

impl std::fmt::Display for Commodity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Commodity::Resource(r) => write!(f, "{r:?}"),
            Commodity::Material(m) => write!(f, "{m}"),
            Commodity::Goods(g) => write!(f, "{g:?}"),
        }
    }
}

/// Price for a material using Lua-configurable GameConfig values.
pub fn material_price(material: MaterialType, cfg: &crate::data::GameConfig) -> Money {
    match material {
        MaterialType::Lumber => Money::dollars(cfg.lumber_price),
        MaterialType::Steel => Money::dollars(cfg.steel_price),
        MaterialType::Fabric => Money::dollars(cfg.fabric_price),
        MaterialType::Paper => Money::dollars(cfg.paper_price),
        MaterialType::Arms => Money::dollars(cfg.arms_price),
        MaterialType::CannedFood => Money::dollars(cfg.canned_food_price),
    }
}

/// Price for a finished good using Lua-configurable GameConfig values.
pub fn goods_price(goods: GoodsType, cfg: &crate::data::GameConfig) -> Money {
    match goods {
        GoodsType::Furniture => Money::dollars(cfg.furniture_price),
        GoodsType::Clothing => Money::dollars(cfg.clothing_price),
        GoodsType::Hardware => Money::dollars(cfg.hardware_price),
    }
}

/// Price for any commodity type using Lua-configurable GameConfig values.
pub fn commodity_price(commodity: Commodity, cfg: &crate::data::GameConfig) -> Money {
    match commodity {
        Commodity::Resource(r) => base_price(r),
        Commodity::Material(m) => material_price(m, cfg),
        Commodity::Goods(g) => goods_price(g, cfg),
    }
}

/// A player's order to sell a commodity on the world market.
#[derive(Debug, Clone)]
pub struct PlayerSellOrder {
    pub commodity: Commodity,
    pub quantity: u32,
}

/// A player's order to buy a resource from minor nations.
#[derive(Debug, Clone)]
pub struct PlayerBuyOrder {
    pub resource: ResourceType,
    pub quantity: u32,
    pub max_price_per_unit: Money,
}

/// Base prices for tradeable commodities.
pub fn base_price(resource: ResourceType) -> Money {
    match resource {
        ResourceType::Timber => Money::dollars(50),
        ResourceType::Coal => Money::dollars(75),
        ResourceType::Iron => Money::dollars(75),
        ResourceType::Cotton => Money::dollars(60),
        ResourceType::Wool => Money::dollars(60),
        ResourceType::Fruit => Money::dollars(40),
        ResourceType::Livestock => Money::dollars(40),
        ResourceType::Oil => Money::dollars(100),
        ResourceType::Gold => Money::dollars(500),
        ResourceType::Gems => Money::dollars(1000),
        ResourceType::Grain => Money::dollars(40),
        ResourceType::Horses => Money::dollars(60),
        ResourceType::Fish => Money::dollars(40),
    }
}

/// Apply subsidy to trade prices. Subsidized nations get better prices.
pub fn apply_subsidy(base_price: Money, subsidy: Money) -> Money {
    // Subsidy reduces the effective price the buyer pays
    let reduced = base_price.as_dollars() - subsidy.as_dollars();
    Money::dollars(reduced.max(1))
}

/// Adjusted price based on supply. More sellers = lower price.
pub fn market_price(resource: ResourceType, total_supply: u32) -> Money {
    let base = base_price(resource);
    // Price drops 5% per unit of supply above 10
    if total_supply > 10 {
        let discount = ((total_supply - 10) * 5).min(50) as i64; // max 50% discount
        Money::dollars(base.as_dollars() * (100 - discount) / 100)
    } else {
        base
    }
}

/// Resolve trade offers and bids, producing transactions.
/// Simple matching: pair each bid with the cheapest compatible offer.
pub fn resolve_trades(offers: &[TradeOffer], bids: &[TradeBid]) -> Vec<TradeTransaction> {
    let mut transactions = Vec::new();

    // Track remaining quantity for each offer
    let mut remaining: Vec<u32> = offers.iter().map(|o| o.quantity).collect();

    for bid in bids {
        let mut bid_remaining = bid.quantity;

        // Find matching offers: same resource, price within bid max, sorted by price (cheapest first)
        let mut matching_indices: Vec<usize> = offers
            .iter()
            .enumerate()
            .filter(|(i, o)| {
                o.resource == bid.resource
                    && o.price_per_unit <= bid.max_price_per_unit
                    && remaining[*i] > 0
                    && o.seller != bid.buyer
            })
            .map(|(i, _)| i)
            .collect();

        // Sort by price ascending (cheapest first)
        matching_indices.sort_by_key(|&i| offers[i].price_per_unit);

        for &offer_idx in &matching_indices {
            if bid_remaining == 0 {
                break;
            }

            let trade_qty = bid_remaining.min(remaining[offer_idx]);
            if trade_qty == 0 {
                continue;
            }

            let price = offers[offer_idx].price_per_unit;
            let total = price * trade_qty as i64;

            transactions.push(TradeTransaction {
                buyer: bid.buyer,
                seller: offers[offer_idx].seller,
                resource: bid.resource,
                quantity: trade_qty,
                price_per_unit: price,
                total_cost: total,
            });

            remaining[offer_idx] -= trade_qty;
            bid_remaining -= trade_qty;
        }
    }

    transactions
}

/// Resolve trades with a preference system.
///
/// When multiple GPs bid for the same MN's resources, the MN prefers the GP with
/// the highest effective relationship score. Subsidies boost the effective
/// relationship by +1 per $100 of subsidy.
///
/// `relationship_scores` maps `(buyer, seller)` to the base diplomatic score.
/// `subsidies` maps `(buyer, seller)` to the subsidy amount in Money.
pub fn resolve_trades_with_preference(
    offers: &[TradeOffer],
    bids: &[TradeBid],
    relationship_scores: &std::collections::HashMap<(NationId, NationId), i32>,
    subsidies: &std::collections::HashMap<(NationId, NationId), Money>,
) -> Vec<TradeTransaction> {
    let mut transactions = Vec::new();

    // Track remaining quantity for each offer
    let mut remaining: Vec<u32> = offers.iter().map(|o| o.quantity).collect();

    // Sort bids by effective relationship score (descending) so preferred buyers go first
    let mut sorted_bids: Vec<(usize, i64)> = bids
        .iter()
        .enumerate()
        .map(|(i, bid)| {
            // Calculate max effective score across all sellers this buyer might trade with
            let max_score = offers
                .iter()
                .filter(|o| o.resource == bid.resource && o.seller != bid.buyer)
                .map(|o| {
                    let base_score = relationship_scores
                        .get(&(bid.buyer, o.seller))
                        .copied()
                        .unwrap_or(0) as i64;
                    let subsidy_bonus = subsidies
                        .get(&(bid.buyer, o.seller))
                        .map(|s| s.as_dollars() / 100)
                        .unwrap_or(0);
                    base_score + subsidy_bonus
                })
                .max()
                .unwrap_or(0);
            (i, max_score)
        })
        .collect();

    // Sort by effective score descending (preferred buyers first)
    sorted_bids.sort_by(|a, b| b.1.cmp(&a.1));

    for (bid_idx, _) in &sorted_bids {
        let bid = &bids[*bid_idx];
        let mut bid_remaining = bid.quantity;

        // Find matching offers, sorted by price (cheapest first)
        let mut matching_indices: Vec<usize> = offers
            .iter()
            .enumerate()
            .filter(|(i, o)| {
                o.resource == bid.resource
                    && o.price_per_unit <= bid.max_price_per_unit
                    && remaining[*i] > 0
                    && o.seller != bid.buyer
            })
            .map(|(i, _)| i)
            .collect();

        matching_indices.sort_by_key(|&i| offers[i].price_per_unit);

        for &offer_idx in &matching_indices {
            if bid_remaining == 0 {
                break;
            }

            let trade_qty = bid_remaining.min(remaining[offer_idx]);
            if trade_qty == 0 {
                continue;
            }

            let price = offers[offer_idx].price_per_unit;
            let total = price * trade_qty as i64;

            transactions.push(TradeTransaction {
                buyer: bid.buyer,
                seller: offers[offer_idx].seller,
                resource: bid.resource,
                quantity: trade_qty,
                price_per_unit: price,
                total_cost: total,
            });

            remaining[offer_idx] -= trade_qty;
            bid_remaining -= trade_qty;
        }
    }

    transactions
}

/// Auto-generate trade offers from Minor Nations based on their resources.
/// Minor Nations sell their surplus resources at base price.
pub fn generate_minor_nation_offers(
    nations: &[Nation],
    provinces: &[Province],
    hex_map: &HexMap,
) -> Vec<TradeOffer> {
    let mut offers = Vec::new();

    for nation in nations {
        if nation.is_great_power() || nation.diplomacy.is_in_anarchy {
            continue;
        }

        // Calculate total resource production for this minor nation
        let mut production: std::collections::BTreeMap<ResourceType, u32> =
            std::collections::BTreeMap::new();

        for province in provinces {
            if province.owner != nation.id {
                continue;
            }
            for tile_coord in &province.tiles {
                if let Some(tile) = hex_map.get_tile(*tile_coord)
                    && let Some(yield_amount) = tile.calculate_yield()
                {
                    *production.entry(yield_amount.resource).or_insert(0) += yield_amount.quantity;
                }
            }
        }

        // Create offers for tradeable resources at base price
        for (resource, quantity) in production {
            if resource.is_tradeable() && quantity > 0 {
                let price = base_price(resource);
                if price != Money::ZERO {
                    offers.push(TradeOffer {
                        seller: nation.id,
                        resource,
                        quantity,
                        price_per_unit: price,
                    });
                }
            }
        }
    }

    offers
}

/// Auto-generate trade offers from Minor Nations with optional resource withholding.
///
/// `withhold_chance` is 0–100: each minor nation has this % chance to withhold
/// one randomly chosen resource offer for this turn. `seed` drives the PRNG so
/// results are deterministic for a given turn.
pub fn generate_minor_nation_offers_with_seed(
    nations: &[Nation],
    provinces: &[crate::map::Province],
    hex_map: &crate::map::HexMap,
    withhold_chance: u32,
    seed: u64,
) -> Vec<TradeOffer> {
    let mut offers = Vec::new();
    let mut rng_state = seed.max(1);

    for nation in nations {
        if nation.is_great_power()
            || nation.diplomacy.is_in_anarchy
            || nation.diplomacy.integrated_by.is_some()
        {
            continue;
        }

        let mut production: std::collections::BTreeMap<ResourceType, u32> =
            std::collections::BTreeMap::new();

        for province in provinces {
            if province.owner != nation.id {
                continue;
            }
            for tile_coord in &province.tiles {
                if let Some(tile) = hex_map.get_tile(*tile_coord)
                    && let Some(yield_amount) = tile.calculate_yield()
                {
                    *production.entry(yield_amount.resource).or_insert(0) += yield_amount.quantity;
                }
            }
        }

        // Decide which resource (if any) to withhold this turn
        let tradeable: Vec<ResourceType> = production
            .keys()
            .copied()
            .filter(|r| r.is_tradeable())
            .collect();

        let withheld = if !tradeable.is_empty() && withhold_chance > 0 {
            // xorshift64 step
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            let roll = (rng_state >> 32) as u32 % 100;
            if roll < withhold_chance {
                rng_state ^= rng_state << 13;
                rng_state ^= rng_state >> 7;
                rng_state ^= rng_state << 17;
                let idx = ((rng_state >> 32) as usize) % tradeable.len();
                Some(tradeable[idx])
            } else {
                None
            }
        } else {
            None
        };

        for (resource, quantity) in &production {
            if Some(*resource) == withheld {
                continue;
            }
            if resource.is_tradeable() && *quantity > 0 {
                let price = base_price(*resource);
                if price != Money::ZERO {
                    offers.push(TradeOffer {
                        seller: nation.id,
                        resource: *resource,
                        quantity: *quantity,
                        price_per_unit: price,
                    });
                }
            }
        }
    }

    offers
}

/// The manufactured commodity types that minor nations want to buy each turn.
/// Covers all Materials and Goods (everything that Great Powers produce in factories).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManufacturedCommodity {
    Material(crate::types::MaterialType),
    Goods(crate::types::GoodsType),
}

/// Generate buy bids from Minor Nations for one manufactured commodity each turn.
///
/// Each non-anarchic minor nation always wants to purchase exactly 1 unit of one
/// randomly chosen manufactured good (Material or GoodsType) per turn.  The
/// `price_per_unit` they are willing to pay is `buy_price`.  `seed` drives the
/// PRNG so results are deterministic for a given turn.
pub fn generate_minor_nation_goods_bids(
    nations: &[Nation],
    buy_price: Money,
    seed: u64,
) -> Vec<MinorGoodsBid> {
    use crate::types::{GoodsType, MaterialType};
    const ALL_MANUFACTURED: &[ManufacturedCommodity] = &[
        ManufacturedCommodity::Material(MaterialType::Lumber),
        ManufacturedCommodity::Material(MaterialType::Steel),
        ManufacturedCommodity::Material(MaterialType::Fabric),
        ManufacturedCommodity::Material(MaterialType::Paper),
        ManufacturedCommodity::Material(MaterialType::Arms),
        ManufacturedCommodity::Material(MaterialType::CannedFood),
        ManufacturedCommodity::Goods(GoodsType::Furniture),
        ManufacturedCommodity::Goods(GoodsType::Clothing),
        ManufacturedCommodity::Goods(GoodsType::Hardware),
    ];

    let mut bids = Vec::new();
    let mut rng_state = seed.max(1);

    for nation in nations {
        if nation.is_great_power()
            || nation.diplomacy.is_in_anarchy
            || nation.diplomacy.integrated_by.is_some()
        {
            continue;
        }

        // xorshift64 step — pick one manufactured commodity for this minor
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let idx = ((rng_state >> 32) as usize) % ALL_MANUFACTURED.len();
        let commodity = ALL_MANUFACTURED[idx];

        // Minor nations bid as a resource bid only for Resource-typed goods;
        // for Material/Goods we record the purchase as a goods-sale-style
        // event on the seller side.  Since TradeBid is resource-only we model
        // the manufactured-good purchase as a special one-unit resource phantom.
        // Instead, we add it to the bids using a dedicated field; for now we
        // record the desire as a minor-goods-bid separate from the resource pool.
        bids.push(MinorGoodsBid {
            buyer: nation.id,
            commodity,
            quantity: 1,
            price_per_unit: buy_price,
        });
    }

    bids
}

/// A bid from a minor nation to purchase one unit of a manufactured commodity.
#[derive(Debug, Clone)]
pub struct MinorGoodsBid {
    pub buyer: NationId,
    pub commodity: ManufacturedCommodity,
    pub quantity: u32,
    pub price_per_unit: Money,
}

/// Project per-resource consumption demand for one turn from a nation's
/// production-chain output targets. Used by need-based buy-side trade
/// (Trello card [3/6] — AI buy-side trade).
///
/// Inputs are read straight from `nation.economy.chain_targets` and the
/// existing Rust chain ratios:
///   * Lumber Mill: 2 Timber → 1 Lumber
///   * Steel Mill:  1 Coal + 1 Iron → 1 Steel
///   * Textile:     2 (Cotton ∪ Wool) → 1 Fabric
///   * Cannery:     1 Grain + 1 Fruit + 1 (Fish ∪ Livestock) → 1 CannedFood
///
/// Steps with target `u32::MAX` (the default "no cap") are clamped to the
/// building's current capacity so a fresh AI without targets still gets a
/// sane projection. Steps without a corresponding building contribute 0.
pub fn projected_resource_needs(nation: &Nation) -> std::collections::BTreeMap<ResourceType, u32> {
    use crate::economy::buildings::BuildingType;
    use std::collections::BTreeMap;

    let mut needs: BTreeMap<ResourceType, u32> = BTreeMap::new();
    let cap = |bt: BuildingType| -> u32 {
        nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == bt)
            .map(|b| b.capacity)
            .unwrap_or(0)
    };
    let resolve = |target: u32, capacity: u32| -> u32 {
        if capacity == 0 {
            return 0;
        }
        if target == u32::MAX {
            capacity
        } else {
            target.min(capacity)
        }
    };

    let t = &nation.economy.chain_targets;

    // Lumber Mill: 2 timber per lumber
    let lumber_units = resolve(t.timber_mill, cap(BuildingType::LumberMill));
    if lumber_units > 0 {
        *needs.entry(ResourceType::Timber).or_insert(0) += lumber_units * 2;
    }
    // Steel Mill: 1 coal + 1 iron per steel
    let steel_units = resolve(t.metal_mill, cap(BuildingType::SteelMill));
    if steel_units > 0 {
        *needs.entry(ResourceType::Coal).or_insert(0) += steel_units;
        *needs.entry(ResourceType::Iron).or_insert(0) += steel_units;
    }
    // Textile Mill: 2 cotton-or-wool per fabric — split 50/50 so we hedge.
    let fabric_units = resolve(t.textile_mill, cap(BuildingType::TextileMill));
    if fabric_units > 0 {
        let half = fabric_units; // 2 raw per fabric, half each side rounds up
        *needs.entry(ResourceType::Cotton).or_insert(0) += half;
        *needs.entry(ResourceType::Wool).or_insert(0) += half;
    }
    // Cannery: 1 grain + 1 fruit + 1 (fish|livestock) per canned-food unit.
    // The protein input is fungible (fish OR livestock); split so the totals
    // sum exactly to canned_units, not 2× canned_units.
    let canned_units = resolve(t.canned_food_factory, cap(BuildingType::FoodProcessing));
    if canned_units > 0 {
        *needs.entry(ResourceType::Grain).or_insert(0) += canned_units;
        *needs.entry(ResourceType::Fruit).or_insert(0) += canned_units;
        let fish = canned_units / 2;
        let livestock = canned_units - fish;
        *needs.entry(ResourceType::Fish).or_insert(0) += fish;
        *needs.entry(ResourceType::Livestock).or_insert(0) += livestock;
    }

    needs
}

/// Generate trade bids for an AI nation based on available offers and cargo capacity.
///
/// Rules:
/// - Buy from any nation except self (no consulate required).
/// - Total quantity of all bids cannot exceed cargo capacity (merchant ships).
/// - Prioritize buying resources the nation needs most (buy what they have least of).
/// - Set max_price at 120% of base_price (willing to pay a bit more).
pub fn generate_smart_bids(
    nation: &Nation,
    available_offers: &[TradeOffer],
    _diplomacy: &DiplomacyState,
    max_cargo: u32,
) -> Vec<TradeBid> {
    if max_cargo == 0 {
        return Vec::new();
    }

    // All offers from other nations are eligible (consulate not required for trade)
    let eligible_offers: Vec<&TradeOffer> = available_offers
        .iter()
        .filter(|offer| offer.seller != nation.id)
        .collect();

    if eligible_offers.is_empty() {
        return Vec::new();
    }

    // Collect unique tradeable resources from eligible offers
    let mut available_resources: Vec<ResourceType> = eligible_offers
        .iter()
        .map(|o| o.resource)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Sort by how little we have of each resource (ascending) — prioritize what we need most
    available_resources.sort_by_key(|r| (nation.resource_amount(*r), format!("{:?}", r)));

    let mut bids = Vec::new();
    let mut remaining_cargo = max_cargo;

    for resource in &available_resources {
        if remaining_cargo == 0 {
            break;
        }

        // Find total quantity available for this resource from eligible offers
        let total_available: u32 = eligible_offers
            .iter()
            .filter(|o| o.resource == *resource)
            .map(|o| o.quantity)
            .sum();

        if total_available == 0 {
            continue;
        }

        let bp = base_price(*resource);
        if bp == Money::ZERO {
            continue;
        }

        // Bid for min(available, remaining_cargo)
        let bid_qty = total_available.min(remaining_cargo);

        // Max price at 120% of base price
        let max_price = Money::dollars(bp.as_dollars() * 120 / 100);

        bids.push(TradeBid {
            buyer: nation.id,
            resource: *resource,
            quantity: bid_qty,
            max_price_per_unit: max_price,
        });

        remaining_cargo -= bid_qty;
    }

    bids
}

/// Generate **need-based** buy bids for an AI Great Power.
///
/// Replaces `generate_smart_bids` for AI driving by projecting per-resource
/// consumption from `chain_targets` (Trello card [2/6]) and bidding only on
/// resources the nation actually needs to keep its production lines fed.
///
/// Behavior summary:
/// 1. Compute target stockpile per resource = projected per-turn consumption ×
///    `buffer_turns`. Subtract current warehouse stock to get the gap.
/// 2. Skip resources where gap == 0 (no shortage → no bid).
/// 3. Bid up to `min(gap, available_offer_qty, remaining_cargo)`, ordered by
///    largest gap first so the most-starved chain gets fed even if cargo runs
///    out.
/// 4. Stop bidding when the projected total cost would push treasury below
///    `treasury_floor`. Each bid uses base_price for the cost projection.
/// 5. When `auto_trade_with_minors == false`, skip offers from minor nations.
///
/// This honors the AI's `auto_trade_with_minors` flag (Trello card [3/6]) and
/// gives the buy-side a cash guard so trade can never bankrupt the AI.
pub fn generate_need_based_bids(
    nation: &Nation,
    all_nations: &[Nation],
    available_offers: &[TradeOffer],
    max_cargo: u32,
    treasury_floor: Money,
    buffer_turns: u32,
) -> Vec<TradeBid> {
    if max_cargo == 0 || buffer_turns == 0 {
        return Vec::new();
    }

    // Resolve "is seller a minor nation" cheaply.
    let is_minor = |seller: NationId| -> bool {
        all_nations
            .iter()
            .find(|n| n.id == seller)
            .is_some_and(|n| !n.is_great_power())
    };

    // Filter offers per the auto_trade_with_minors policy and self-exclusion.
    let allow_minors = nation.economy.auto_trade_with_minors;
    let eligible_offers: Vec<&TradeOffer> = available_offers
        .iter()
        .filter(|o| o.seller != nation.id)
        .filter(|o| allow_minors || !is_minor(o.seller))
        .collect();

    if eligible_offers.is_empty() {
        return Vec::new();
    }

    let needs = projected_resource_needs(nation);
    if needs.is_empty() {
        return Vec::new();
    }

    // Compute per-resource gap = (per_turn_demand × buffer_turns) − current stock.
    // Drop resources with no gap.
    let mut gaps: Vec<(ResourceType, u32)> = needs
        .into_iter()
        .filter_map(|(r, per_turn)| {
            let target_stock = per_turn.saturating_mul(buffer_turns);
            let stock = nation.resource_amount(r);
            let gap = target_stock.saturating_sub(stock);
            if gap > 0 { Some((r, gap)) } else { None }
        })
        .collect();
    if gaps.is_empty() {
        return Vec::new();
    }
    // Order by largest gap first (deterministic tiebreak by resource debug name).
    gaps.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)))
    });

    let mut bids = Vec::new();
    let mut remaining_cargo = max_cargo;
    let mut projected_spend = Money::ZERO;
    let cash_available = nation.economy.treasury - treasury_floor;
    if cash_available <= Money::ZERO {
        return Vec::new();
    }

    for (resource, gap) in gaps {
        if remaining_cargo == 0 {
            break;
        }

        let total_available: u32 = eligible_offers
            .iter()
            .filter(|o| o.resource == resource)
            .map(|o| o.quantity)
            .sum();
        if total_available == 0 {
            continue;
        }

        let bp = base_price(resource);
        if bp == Money::ZERO {
            continue;
        }
        // Stay under the cash budget (use base price as a worst-case predictor;
        // actual fills are at offer price ≤ max_price_per_unit).
        let cash_left = cash_available - projected_spend;
        if cash_left <= Money::ZERO {
            break;
        }
        let cash_qty: u32 = (cash_left.as_dollars() / bp.as_dollars())
            .clamp(0, u32::MAX as i64) as u32;

        let bid_qty = gap.min(total_available).min(remaining_cargo).min(cash_qty);
        if bid_qty == 0 {
            continue;
        }

        let max_price = Money::dollars(bp.as_dollars() * 120 / 100);
        bids.push(TradeBid {
            buyer: nation.id,
            resource,
            quantity: bid_qty,
            max_price_per_unit: max_price,
        });
        remaining_cargo -= bid_qty;
        projected_spend += bp * bid_qty as i64;
    }

    bids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::HexCoord;
    use crate::map::tile::Tile;
    use crate::nation::NationColor;

    // ── base_price ──────────────────────────────────────────────

    #[test]
    fn base_price_returns_correct_values() {
        assert_eq!(base_price(ResourceType::Timber), Money::dollars(50));
        assert_eq!(base_price(ResourceType::Coal), Money::dollars(75));
        assert_eq!(base_price(ResourceType::Iron), Money::dollars(75));
        assert_eq!(base_price(ResourceType::Cotton), Money::dollars(60));
        assert_eq!(base_price(ResourceType::Wool), Money::dollars(60));
        assert_eq!(base_price(ResourceType::Fruit), Money::dollars(40));
        assert_eq!(base_price(ResourceType::Livestock), Money::dollars(40));
        assert_eq!(base_price(ResourceType::Oil), Money::dollars(100));
        assert_eq!(base_price(ResourceType::Gold), Money::dollars(500));
        assert_eq!(base_price(ResourceType::Gems), Money::dollars(1000));
    }

    #[test]
    fn all_resources_have_positive_price() {
        assert_eq!(base_price(ResourceType::Grain), Money::dollars(40));
        assert_eq!(base_price(ResourceType::Horses), Money::dollars(60));
    }

    // ── resolve_trades ──────────────────────────────────────────

    #[test]
    fn resolve_trades_matches_compatible_offers_and_bids() {
        let offers = vec![TradeOffer {
            seller: NationId(10),
            resource: ResourceType::Timber,
            quantity: 5,
            price_per_unit: Money::dollars(50),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            resource: ResourceType::Timber,
            quantity: 3,
            max_price_per_unit: Money::dollars(60),
        }];

        let txns = resolve_trades(&offers, &bids);
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].buyer, NationId(1));
        assert_eq!(txns[0].seller, NationId(10));
        assert_eq!(txns[0].resource, ResourceType::Timber);
        assert_eq!(txns[0].quantity, 3);
        assert_eq!(txns[0].price_per_unit, Money::dollars(50));
        assert_eq!(txns[0].total_cost, Money::dollars(150));
    }

    #[test]
    fn resolve_trades_respects_price_limits() {
        let offers = vec![TradeOffer {
            seller: NationId(10),
            resource: ResourceType::Iron,
            quantity: 5,
            price_per_unit: Money::dollars(100),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            resource: ResourceType::Iron,
            quantity: 3,
            max_price_per_unit: Money::dollars(50), // too low
        }];

        let txns = resolve_trades(&offers, &bids);
        assert!(
            txns.is_empty(),
            "No trade should occur when bid price is below offer price"
        );
    }

    #[test]
    fn resolve_trades_handles_partial_fills() {
        let offers = vec![TradeOffer {
            seller: NationId(10),
            resource: ResourceType::Coal,
            quantity: 2,
            price_per_unit: Money::dollars(75),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            resource: ResourceType::Coal,
            quantity: 5,
            max_price_per_unit: Money::dollars(100),
        }];

        let txns = resolve_trades(&offers, &bids);
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].quantity, 2); // only 2 available, bid wanted 5
        assert_eq!(txns[0].total_cost, Money::dollars(150));
    }

    #[test]
    fn resolve_trades_no_self_trade() {
        let offers = vec![TradeOffer {
            seller: NationId(1),
            resource: ResourceType::Timber,
            quantity: 5,
            price_per_unit: Money::dollars(50),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1), // same nation
            resource: ResourceType::Timber,
            quantity: 3,
            max_price_per_unit: Money::dollars(60),
        }];

        let txns = resolve_trades(&offers, &bids);
        assert!(txns.is_empty());
    }

    #[test]
    fn resolve_trades_multiple_offers_cheapest_first() {
        let offers = vec![
            TradeOffer {
                seller: NationId(10),
                resource: ResourceType::Timber,
                quantity: 2,
                price_per_unit: Money::dollars(80),
            },
            TradeOffer {
                seller: NationId(11),
                resource: ResourceType::Timber,
                quantity: 3,
                price_per_unit: Money::dollars(50),
            },
        ];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            resource: ResourceType::Timber,
            quantity: 4,
            max_price_per_unit: Money::dollars(100),
        }];

        let txns = resolve_trades(&offers, &bids);
        // Should buy from cheapest first: 3 from NationId(11) at $50, then 1 from NationId(10) at $80
        assert_eq!(txns.len(), 2);
        assert_eq!(txns[0].seller, NationId(11));
        assert_eq!(txns[0].quantity, 3);
        assert_eq!(txns[0].price_per_unit, Money::dollars(50));
        assert_eq!(txns[1].seller, NationId(10));
        assert_eq!(txns[1].quantity, 1);
        assert_eq!(txns[1].price_per_unit, Money::dollars(80));
    }

    #[test]
    fn resolve_trades_different_resources_no_match() {
        let offers = vec![TradeOffer {
            seller: NationId(10),
            resource: ResourceType::Coal,
            quantity: 5,
            price_per_unit: Money::dollars(75),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            resource: ResourceType::Iron, // different resource
            quantity: 3,
            max_price_per_unit: Money::dollars(100),
        }];

        let txns = resolve_trades(&offers, &bids);
        assert!(txns.is_empty());
    }

    #[test]
    fn resolve_trades_empty_inputs() {
        assert!(resolve_trades(&[], &[]).is_empty());
        assert!(
            resolve_trades(
                &[TradeOffer {
                    seller: NationId(10),
                    resource: ResourceType::Timber,
                    quantity: 5,
                    price_per_unit: Money::dollars(50),
                }],
                &[]
            )
            .is_empty()
        );
        assert!(
            resolve_trades(
                &[],
                &[TradeBid {
                    buyer: NationId(1),
                    resource: ResourceType::Timber,
                    quantity: 3,
                    max_price_per_unit: Money::dollars(60),
                }]
            )
            .is_empty()
        );
    }

    // ── generate_minor_nation_offers ─────────────────────────────

    #[test]
    fn generate_minor_nation_offers_creates_offers_for_minor_nations() {
        let coord_forest = HexCoord::new(0, 0);
        let coord_plantation = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        let mut forest_tile = Tile::with_province(TerrainType::Forest, ProvinceId(20));
        forest_tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord_forest, forest_tile);
        let mut cotton_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(20));
        cotton_tile.set_resource(ResourceType::Cotton);
        hex_map.set_tile(coord_plantation, cotton_tile);

        let province = Province::new(
            ProvinceId(20),
            "Minor Province".to_string(),
            NationId(10),
            coord_forest,
            vec![coord_forest, coord_plantation],
            3,
        );

        let minor_nation = Nation::new(
            NationId(10),
            "Bruhr".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        );

        let great_power = Nation::new(
            NationId(1),
            "France".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );

        let nations = vec![great_power, minor_nation];
        let provinces = vec![province];

        let offers = generate_minor_nation_offers(&nations, &provinces, &hex_map);

        // Should have offers for Timber and Cotton (both tradeable)
        assert!(!offers.is_empty());

        let timber_offers: Vec<_> = offers
            .iter()
            .filter(|o| o.resource == ResourceType::Timber)
            .collect();
        let cotton_offers: Vec<_> = offers
            .iter()
            .filter(|o| o.resource == ResourceType::Cotton)
            .collect();

        assert_eq!(timber_offers.len(), 1);
        assert_eq!(timber_offers[0].seller, NationId(10));
        assert_eq!(timber_offers[0].quantity, 1);
        assert_eq!(timber_offers[0].price_per_unit, Money::dollars(50));

        assert_eq!(cotton_offers.len(), 1);
        assert_eq!(cotton_offers[0].seller, NationId(10));
        assert_eq!(cotton_offers[0].quantity, 1);
        assert_eq!(cotton_offers[0].price_per_unit, Money::dollars(60));
    }

    #[test]
    fn generate_minor_nation_offers_excludes_great_powers() {
        let coord = HexCoord::new(0, 0);

        let mut hex_map = HexMap::new(10, 10);
        let mut tile = Tile::with_province(TerrainType::Forest, ProvinceId(1));
        tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord, tile);

        let province = Province::new(
            ProvinceId(1),
            "GP Province".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );

        let great_power = Nation::new(
            NationId(1),
            "France".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );

        let nations = vec![great_power];
        let provinces = vec![province];

        let offers = generate_minor_nation_offers(&nations, &provinces, &hex_map);
        assert!(
            offers.is_empty(),
            "Great Powers should not generate trade offers"
        );
    }

    #[test]
    fn generate_minor_nation_offers_includes_grain() {
        let coord = HexCoord::new(0, 0);

        let mut hex_map = HexMap::new(10, 10);
        let mut tile = Tile::with_province(TerrainType::Grassland, ProvinceId(20));
        tile.set_resource(ResourceType::Grain);
        hex_map.set_tile(coord, tile);

        let province = Province::new(
            ProvinceId(20),
            "Farmland".to_string(),
            NationId(10),
            coord,
            vec![coord],
            3,
        );

        let minor_nation = Nation::new(
            NationId(10),
            "Farmer".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        );

        let nations = vec![minor_nation];
        let provinces = vec![province];

        let offers = generate_minor_nation_offers(&nations, &provinces, &hex_map);
        assert!(
            !offers.is_empty(),
            "Grain is tradeable and should appear in offers"
        );
    }

    // ── apply_subsidy ───────────────────────────────────────────

    #[test]
    fn apply_subsidy_reduces_price() {
        let price = Money::dollars(100);
        let subsidy = Money::dollars(30);
        let result = apply_subsidy(price, subsidy);
        assert_eq!(result, Money::dollars(70));
    }

    #[test]
    fn apply_subsidy_does_not_go_below_one() {
        let price = Money::dollars(50);
        let subsidy = Money::dollars(100);
        let result = apply_subsidy(price, subsidy);
        assert_eq!(result, Money::dollars(1));
    }

    #[test]
    fn apply_subsidy_zero_subsidy_keeps_price() {
        let price = Money::dollars(75);
        let subsidy = Money::ZERO;
        let result = apply_subsidy(price, subsidy);
        assert_eq!(result, Money::dollars(75));
    }

    // ── market_price ────────────────────────────────────────────

    #[test]
    fn market_price_no_discount_at_low_supply() {
        // Supply <= 10: no discount
        assert_eq!(market_price(ResourceType::Timber, 5), Money::dollars(50));
        assert_eq!(market_price(ResourceType::Timber, 10), Money::dollars(50));
    }

    #[test]
    fn market_price_drops_with_high_supply() {
        // Supply 15: (15-10)*5 = 25% discount -> 50 * 75/100 = 37
        assert_eq!(market_price(ResourceType::Timber, 15), Money::dollars(37));
    }

    #[test]
    fn market_price_caps_discount_at_50_percent() {
        // Supply 30: (30-10)*5 = 100, capped at 50% -> 50 * 50/100 = 25
        assert_eq!(market_price(ResourceType::Timber, 30), Money::dollars(25));
        // Even higher supply: still 50%
        assert_eq!(market_price(ResourceType::Timber, 100), Money::dollars(25));
    }

    // ── resolve_trades_with_preference ──────────────────────────

    #[test]
    fn preference_gives_higher_relationship_priority() {
        let offers = vec![TradeOffer {
            seller: NationId(10),
            resource: ResourceType::Timber,
            quantity: 3,
            price_per_unit: Money::dollars(50),
        }];
        // Two buyers want the same resource, but only 3 available
        let bids = vec![
            TradeBid {
                buyer: NationId(1),
                resource: ResourceType::Timber,
                quantity: 3,
                max_price_per_unit: Money::dollars(60),
            },
            TradeBid {
                buyer: NationId(2),
                resource: ResourceType::Timber,
                quantity: 3,
                max_price_per_unit: Money::dollars(60),
            },
        ];

        let mut scores = std::collections::HashMap::new();
        scores.insert((NationId(1), NationId(10)), 10);
        scores.insert((NationId(2), NationId(10)), 50); // Nation 2 has higher relationship

        let subsidies = std::collections::HashMap::new();

        let txns = resolve_trades_with_preference(&offers, &bids, &scores, &subsidies);
        assert_eq!(txns.len(), 1);
        // Nation 2 should win because higher score
        assert_eq!(txns[0].buyer, NationId(2));
        assert_eq!(txns[0].quantity, 3);
    }

    #[test]
    fn subsidy_boosts_trade_preference() {
        let offers = vec![TradeOffer {
            seller: NationId(10),
            resource: ResourceType::Coal,
            quantity: 5,
            price_per_unit: Money::dollars(75),
        }];
        let bids = vec![
            TradeBid {
                buyer: NationId(1),
                resource: ResourceType::Coal,
                quantity: 5,
                max_price_per_unit: Money::dollars(100),
            },
            TradeBid {
                buyer: NationId(2),
                resource: ResourceType::Coal,
                quantity: 5,
                max_price_per_unit: Money::dollars(100),
            },
        ];

        let mut scores = std::collections::HashMap::new();
        scores.insert((NationId(1), NationId(10)), 10);
        scores.insert((NationId(2), NationId(10)), 5); // Nation 2 has lower base score

        let mut subsidies = std::collections::HashMap::new();
        // But Nation 2 gives $1000 subsidy -> +10 bonus, effective = 5+10 = 15
        subsidies.insert((NationId(2), NationId(10)), Money::dollars(1000));

        let txns = resolve_trades_with_preference(&offers, &bids, &scores, &subsidies);
        assert_eq!(txns.len(), 1);
        // Nation 2 wins: effective score 15 > Nation 1 score 10
        assert_eq!(txns[0].buyer, NationId(2));
    }

    // ── generate_minor_nation_offers_with_seed ─────────────────

    /// Build a minor nation with two tiles (Timber + Cotton) in a map/province.
    fn make_minor_with_resources() -> (
        Vec<crate::nation::Nation>,
        Vec<crate::map::Province>,
        HexMap,
    ) {
        use crate::map::Province;
        let coord_a = HexCoord::new(0, 0);
        let coord_b = HexCoord::new(1, 0);
        let mut hex_map = HexMap::new(10, 10);
        let mut tile_a = Tile::with_province(TerrainType::Forest, ProvinceId(20));
        tile_a.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord_a, tile_a);
        let mut tile_b = Tile::with_province(TerrainType::Grassland, ProvinceId(20));
        tile_b.set_resource(ResourceType::Cotton);
        hex_map.set_tile(coord_b, tile_b);
        let province = Province::new(
            ProvinceId(20),
            "Minor Province".to_string(),
            NationId(10),
            coord_a,
            vec![coord_a, coord_b],
            3,
        );
        let minor = Nation::new(
            NationId(10),
            "Bruhr".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        );
        (vec![minor], vec![province], hex_map)
    }

    #[test]
    fn withhold_chance_zero_never_withholds() {
        let (nations, provinces, hex_map) = make_minor_with_resources();
        let offers_normal = generate_minor_nation_offers(&nations, &provinces, &hex_map);
        let offers_seeded =
            generate_minor_nation_offers_with_seed(&nations, &provinces, &hex_map, 0, 42);
        // withhold_chance=0 must produce identical count as the unseeded version
        assert_eq!(offers_seeded.len(), offers_normal.len());
        assert_eq!(offers_seeded.len(), 2); // Timber + Cotton
    }

    #[test]
    fn withhold_chance_100_withholds_exactly_one_resource() {
        let (nations, provinces, hex_map) = make_minor_with_resources();
        let offers_full = generate_minor_nation_offers(&nations, &provinces, &hex_map);
        assert_eq!(offers_full.len(), 2, "setup: minor must have 2 resources");
        // At 100% chance, exactly one resource should be withheld
        let offers_withheld =
            generate_minor_nation_offers_with_seed(&nations, &provinces, &hex_map, 100, 99);
        assert_eq!(
            offers_withheld.len(),
            1,
            "100% chance must withhold exactly one of two resources"
        );
    }

    #[test]
    fn seeded_offers_are_deterministic() {
        let (nations, provinces, hex_map) = make_minor_with_resources();
        let a = generate_minor_nation_offers_with_seed(&nations, &provinces, &hex_map, 50, 12345);
        let b = generate_minor_nation_offers_with_seed(&nations, &provinces, &hex_map, 50, 12345);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.seller, y.seller);
            assert_eq!(x.resource, y.resource);
        }
    }

    #[test]
    fn different_seeds_can_withhold_different_resources() {
        let (nations, provinces, hex_map) = make_minor_with_resources();
        // Try many seeds; with 100% chance, each seed consistently withholds one specific resource.
        // Collect the withheld resource across seeds and verify they differ (not always same one).
        let full = generate_minor_nation_offers(&nations, &provinces, &hex_map);
        assert_eq!(full.len(), 2, "setup: minor must have Timber + Cotton");
        let withheld: Vec<ResourceType> = (1u64..=200)
            .filter_map(|seed| {
                let offers = generate_minor_nation_offers_with_seed(
                    &nations, &provinces, &hex_map, 100, seed,
                );
                // withheld = resource present in full but absent in offers
                full.iter()
                    .find(|o| !offers.iter().any(|x| x.resource == o.resource))
                    .map(|o| o.resource)
            })
            .collect();
        let has_timber = withheld.iter().any(|r| *r == ResourceType::Timber);
        let has_cotton = withheld.iter().any(|r| *r == ResourceType::Cotton);
        // With 200 seeds both resources should appear as withheld at some point
        assert!(
            has_timber && has_cotton,
            "different seeds should withhold different resources across 200 seeds"
        );
    }

    // ── generate_minor_nation_goods_bids ───────────────────────

    #[test]
    fn goods_bids_one_per_minor_nation() {
        let nations = vec![make_minor_nation(1)];
        let minor_count = nations
            .iter()
            .filter(|n| !n.is_great_power() && !n.diplomacy.is_in_anarchy)
            .count();
        let bids = generate_minor_nation_goods_bids(&nations, Money::dollars(150), 999);
        assert_eq!(bids.len(), minor_count);
    }

    #[test]
    fn goods_bids_use_specified_price() {
        let nations = vec![make_minor_nation(1)];
        let bids = generate_minor_nation_goods_bids(&nations, Money::dollars(200), 1);
        for bid in &bids {
            assert_eq!(bid.price_per_unit, Money::dollars(200));
            assert_eq!(bid.quantity, 1);
        }
    }

    #[test]
    fn goods_bids_are_deterministic() {
        let nations = vec![make_minor_nation(1)];
        let a = generate_minor_nation_goods_bids(&nations, Money::dollars(150), 77777);
        let b = generate_minor_nation_goods_bids(&nations, Money::dollars(150), 77777);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.buyer, y.buyer);
            assert_eq!(x.commodity, y.commodity);
        }
    }

    #[test]
    fn goods_bids_differ_with_different_seeds() {
        let nations = make_nations_with_many_minors();
        let a = generate_minor_nation_goods_bids(&nations, Money::dollars(150), 1);
        let b = generate_minor_nation_goods_bids(&nations, Money::dollars(150), 999999);
        // With enough minor nations, at least some commodities should differ
        let same = a
            .iter()
            .zip(b.iter())
            .filter(|(x, y)| x.commodity == y.commodity)
            .count();
        assert!(
            same < a.len(),
            "Expected at least some commodities to differ with different seeds"
        );
    }

    #[test]
    fn anarchic_minor_nations_excluded_from_bids() {
        let mut nations = vec![make_minor_nation(1)];
        for n in &mut nations {
            if !n.is_great_power() {
                n.diplomacy.is_in_anarchy = true;
                break;
            }
        }
        let bids = generate_minor_nation_goods_bids(&nations, Money::dollars(150), 1);
        assert_eq!(bids.len(), 0);
    }

    #[test]
    fn integrated_minor_nations_excluded_from_bids() {
        let mut nations = vec![make_minor_nation(1), make_minor_nation(2)];
        // Mark minor 1 as integrated (absorbed by another nation)
        nations[0].diplomacy.integrated_by = Some(NationId(99));
        let bids = generate_minor_nation_goods_bids(&nations, Money::dollars(150), 1);
        // Only minor 2 should bid
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0].buyer, NationId(2));
    }

    fn make_minor_nation(id: u32) -> crate::nation::Nation {
        use crate::nation::{Nation, NationColor};
        Nation::new(
            NationId(id),
            format!("Minor{id}"),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(id),
        )
    }

    fn make_nations_with_many_minors() -> Vec<crate::nation::Nation> {
        (1..=15).map(make_minor_nation).collect()
    }

    // ── TradeHistoryEntry ──────────────────────────────────────

    #[test]
    fn trade_history_entry_stores_all_fields() {
        let entry = TradeHistoryEntry {
            turn: TurnNumber(5),
            partner: NationId(10),
            resource: ResourceType::Timber,
            commodity_label: "Timber".to_string(),
            quantity: 3,
            total_cost: Money::dollars(150),
            bought: true,
        };
        assert_eq!(entry.turn, TurnNumber(5));
        assert_eq!(entry.partner, NationId(10));
        assert_eq!(entry.resource, ResourceType::Timber);
        assert_eq!(entry.quantity, 3);
        assert_eq!(entry.total_cost, Money::dollars(150));
    }

    // ── Need-based buy-side bids (Trello card [3/6]) ──────────────────

    fn make_gp(id: u32) -> crate::nation::Nation {
        use crate::nation::{Nation, NationColor};
        Nation::new(
            NationId(id),
            format!("GP{id}"),
            NationColor::Yellow,
            NationType::GreatPower,
            ProvinceId(id),
        )
    }

    #[test]
    fn need_based_buy_resource_with_deficit_and_cash() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut buyer = make_gp(2);
        buyer.economy.treasury = Money::dollars(20_000);
        // Owns a SteelMill capacity 2 ⇒ needs 2 coal + 2 iron per turn.
        // With 3 turns of buffer, target stock = 6 each. Stock is 0 ⇒ gap 6.
        buyer
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        buyer.economy.chain_targets.metal_mill = 2;

        let seller = make_gp(3);
        let nations = vec![buyer.clone(), seller.clone()];
        let offers = vec![
            TradeOffer {
                seller: NationId(3),
                resource: ResourceType::Coal,
                quantity: 10,
                price_per_unit: base_price(ResourceType::Coal),
            },
            TradeOffer {
                seller: NationId(3),
                resource: ResourceType::Iron,
                quantity: 10,
                price_per_unit: base_price(ResourceType::Iron),
            },
        ];

        let bids = generate_need_based_bids(
            &buyer,
            &nations,
            &offers,
            100,                   // ample cargo
            Money::dollars(5_000), // treasury floor
            3,                     // buffer turns
        );

        let coal_bid = bids.iter().find(|b| b.resource == ResourceType::Coal);
        let iron_bid = bids.iter().find(|b| b.resource == ResourceType::Iron);
        assert!(
            coal_bid.is_some(),
            "should bid for coal when SteelMill is starved"
        );
        assert!(
            iron_bid.is_some(),
            "should bid for iron when SteelMill is starved"
        );
        assert_eq!(coal_bid.unwrap().quantity, 6, "buffer 3 × 2 demand = 6");
        assert_eq!(iron_bid.unwrap().quantity, 6);
    }

    #[test]
    fn need_based_no_bid_when_no_deficit() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut buyer = make_gp(2);
        buyer.economy.treasury = Money::dollars(20_000);
        buyer
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        buyer.economy.chain_targets.metal_mill = 2;
        // Already stocked beyond buffer × demand
        buyer.add_resource(ResourceType::Coal, 100);
        buyer.add_resource(ResourceType::Iron, 100);

        let seller = make_gp(3);
        let nations = vec![buyer.clone(), seller.clone()];
        let offers = vec![TradeOffer {
            seller: NationId(3),
            resource: ResourceType::Coal,
            quantity: 10,
            price_per_unit: base_price(ResourceType::Coal),
        }];

        let bids =
            generate_need_based_bids(&buyer, &nations, &offers, 100, Money::dollars(5_000), 3);
        assert!(bids.is_empty(), "well-stocked AI must not bid");
    }

    #[test]
    fn need_based_respects_treasury_floor() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut buyer = make_gp(2);
        // Treasury sits exactly at the floor → cash_available is 0 → no bids.
        buyer.economy.treasury = Money::dollars(5_000);
        buyer
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        buyer.economy.chain_targets.metal_mill = 2;

        let seller = make_gp(3);
        let nations = vec![buyer.clone(), seller.clone()];
        let offers = vec![TradeOffer {
            seller: NationId(3),
            resource: ResourceType::Coal,
            quantity: 10,
            price_per_unit: base_price(ResourceType::Coal),
        }];

        let bids =
            generate_need_based_bids(&buyer, &nations, &offers, 100, Money::dollars(5_000), 3);
        assert!(bids.is_empty(), "must not spend below the treasury floor");
    }

    #[test]
    fn need_based_caps_bid_quantity_by_cash() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut buyer = make_gp(2);
        // Coal base price = $75. Above floor of $5k we have $300 → 4 coal max.
        buyer.economy.treasury = Money::dollars(5_300);
        buyer
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 5));
        buyer.economy.chain_targets.metal_mill = 5;

        let seller = make_gp(3);
        let nations = vec![buyer.clone(), seller.clone()];
        // Offer iron expensively so coal alone is bid.
        let offers = vec![TradeOffer {
            seller: NationId(3),
            resource: ResourceType::Coal,
            quantity: 100,
            price_per_unit: base_price(ResourceType::Coal),
        }];

        let bids =
            generate_need_based_bids(&buyer, &nations, &offers, 100, Money::dollars(5_000), 3);
        let coal = bids
            .iter()
            .find(|b| b.resource == ResourceType::Coal)
            .unwrap();
        // $300 cash budget / $75 = 4 max coal.
        assert!(
            coal.quantity <= 4,
            "cash floor must cap bid qty (got {})",
            coal.quantity
        );
    }

    #[test]
    fn need_based_skips_minors_when_auto_trade_off() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut buyer = make_gp(2);
        buyer.economy.treasury = Money::dollars(20_000);
        buyer
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        buyer.economy.chain_targets.metal_mill = 2;
        buyer.economy.auto_trade_with_minors = false;

        // Seller is a minor — should be filtered out when flag is off.
        let minor = make_minor_nation(3);
        let nations = vec![buyer.clone(), minor.clone()];
        let offers = vec![TradeOffer {
            seller: NationId(3),
            resource: ResourceType::Coal,
            quantity: 10,
            price_per_unit: base_price(ResourceType::Coal),
        }];

        let bids =
            generate_need_based_bids(&buyer, &nations, &offers, 100, Money::dollars(5_000), 3);
        assert!(
            bids.is_empty(),
            "auto_trade_with_minors=false must skip minor offers"
        );
    }

    #[test]
    fn need_based_includes_minors_when_auto_trade_on() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut buyer = make_gp(2);
        buyer.economy.treasury = Money::dollars(20_000);
        buyer
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        buyer.economy.chain_targets.metal_mill = 2;
        // Default is true; assert behavior explicitly.
        buyer.economy.auto_trade_with_minors = true;

        let minor = make_minor_nation(3);
        let nations = vec![buyer.clone(), minor.clone()];
        let offers = vec![TradeOffer {
            seller: NationId(3),
            resource: ResourceType::Coal,
            quantity: 10,
            price_per_unit: base_price(ResourceType::Coal),
        }];

        let bids =
            generate_need_based_bids(&buyer, &nations, &offers, 100, Money::dollars(5_000), 3);
        assert!(
            bids.iter().any(|b| b.resource == ResourceType::Coal),
            "auto_trade_with_minors=true must accept minor offers"
        );
    }

    #[test]
    fn projected_resource_needs_clamps_u32_max_target_to_capacity() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut nation = make_gp(2);
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 3));
        // u32::MAX is the default "no cap" sentinel — must be clamped to capacity.
        nation.economy.chain_targets.metal_mill = u32::MAX;

        let needs = projected_resource_needs(&nation);
        assert_eq!(needs.get(&ResourceType::Coal).copied().unwrap_or(0), 3);
        assert_eq!(needs.get(&ResourceType::Iron).copied().unwrap_or(0), 3);
    }

    #[test]
    fn projected_resource_needs_cannery_protein_sums_to_canned_units() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut nation = make_gp(2);
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 5));
        nation.economy.chain_targets.canned_food_factory = 5;

        let needs = projected_resource_needs(&nation);
        // Grain + fruit always one-for-one with canned units.
        assert_eq!(needs.get(&ResourceType::Grain).copied().unwrap_or(0), 5);
        assert_eq!(needs.get(&ResourceType::Fruit).copied().unwrap_or(0), 5);
        // Protein is fungible: fish + livestock must total canned_units, not double-count.
        let fish = needs.get(&ResourceType::Fish).copied().unwrap_or(0);
        let livestock = needs.get(&ResourceType::Livestock).copied().unwrap_or(0);
        assert_eq!(fish + livestock, 5, "protein split must sum to canned output");
    }

    #[test]
    fn need_based_zero_buffer_turns_yields_no_bids() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut buyer = make_gp(2);
        buyer.economy.treasury = Money::dollars(20_000);
        buyer
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        buyer.economy.chain_targets.metal_mill = 2;

        let seller = make_gp(3);
        let nations = vec![buyer.clone(), seller.clone()];
        let offers = vec![TradeOffer {
            seller: NationId(3),
            resource: ResourceType::Coal,
            quantity: 10,
            price_per_unit: base_price(ResourceType::Coal),
        }];

        let bids = generate_need_based_bids(
            &buyer,
            &nations,
            &offers,
            100,
            Money::dollars(5_000),
            0, // disable buy-side trade
        );
        assert!(bids.is_empty(), "buffer_turns=0 must short-circuit to no bids");
    }

    #[test]
    fn projected_resource_needs_uses_chain_targets() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut nation = make_gp(2);
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 4));
        nation.economy.chain_targets.timber_mill = 4;
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 3));
        nation.economy.chain_targets.metal_mill = 3;

        let needs = projected_resource_needs(&nation);
        // Lumber Mill 4 → 8 timber per turn.
        assert_eq!(needs.get(&ResourceType::Timber).copied().unwrap_or(0), 8);
        // Steel Mill 3 → 3 coal + 3 iron per turn.
        assert_eq!(needs.get(&ResourceType::Coal).copied().unwrap_or(0), 3);
        assert_eq!(needs.get(&ResourceType::Iron).copied().unwrap_or(0), 3);
    }

    #[test]
    fn trade_history_entry_stores_all_fields_clone() {
        let entry = TradeHistoryEntry {
            turn: TurnNumber(12),
            partner: NationId(5),
            resource: ResourceType::Coal,
            commodity_label: "Coal".to_string(),
            quantity: 7,
            total_cost: Money::dollars(525),
            bought: true,
        };
        let cloned = entry.clone();
        assert_eq!(cloned.turn, entry.turn);
        assert_eq!(cloned.partner, entry.partner);
        assert_eq!(cloned.resource, entry.resource);
        assert_eq!(cloned.quantity, entry.quantity);
        assert_eq!(cloned.total_cost, entry.total_cost);
    }
}
