use crate::diplomacy::DiplomacyState;
use crate::economy::market::MarketState;
use crate::map::{HexMap, Province};
use crate::nation::Nation;
use crate::types::*;

/// A trade offer: a nation wants to sell a commodity.
#[derive(Debug, Clone)]
pub struct TradeOffer {
    pub seller: NationId,
    pub commodity: Commodity,
    pub quantity: u32,
    pub price_per_unit: Money,
}

/// A trade bid: a nation wants to buy a commodity.
#[derive(Debug, Clone)]
pub struct TradeBid {
    pub buyer: NationId,
    pub commodity: Commodity,
    pub quantity: u32,
    pub max_price_per_unit: Money,
}

/// Result of a single trade transaction.
#[derive(Debug, Clone)]
pub struct TradeTransaction {
    pub buyer: NationId,
    pub seller: NationId,
    pub commodity: Commodity,
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

/// A player's order to sell a resource into the trade-offer pool.
///
/// The player's offer competes alongside Minor Nation offers; any GP (or
/// minor) with a matching bid can buy. No fallback "world market" exists —
/// unmatched offers are discarded at end-of-turn.
#[derive(Debug, Clone)]
pub struct PlayerSellOrder {
    pub resource: ResourceType,
    pub quantity: u32,
}

/// A trade the player accepted during the interactive end-turn trade
/// session (card #494). Executed verbatim — seller-pinned, at the offer's
/// listed price — before AI bids compete for the remaining pool.
#[derive(Debug, Clone)]
pub struct AcceptedTrade {
    pub seller: NationId,
    pub resource: ResourceType,
    pub quantity: u32,
    pub price_per_unit: Money,
}

/// The frozen trade state handed from `begin_turn` to `finish_turn`
/// (card #494): the offer pool built at the session pause plus whatever the
/// player accepted interactively. `interactive == false` means no session
/// ran (skip runs, batch, observer) and the human seat falls back to
/// wishlist auto-bids.
#[derive(Debug, Clone, Default)]
pub struct PreparedTradeSession {
    pub offers: Vec<TradeOffer>,
    pub accepted: Vec<AcceptedTrade>,
    pub interactive: bool,
}

/// Apply subsidy to trade prices. Subsidized nations get better prices.
pub fn apply_subsidy(base_price: Money, subsidy: Money) -> Money {
    // Subsidy reduces the effective price the buyer pays
    let reduced = base_price.as_dollars() - subsidy.as_dollars();
    Money::dollars(reduced.max(1))
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
                o.commodity == bid.commodity
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
                commodity: bid.commodity,
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

/// Resolve trades with a seller-preference system.
///
/// Resolution is **offer-centric**: each seller's offer is allocated to the
/// bidders that want it, ranked by that seller's effective relationship with
/// each buyer — so the diplomatic preference applies to the *actual* seller
/// being filled, not a cross-seller surrogate. Subsidies boost the effective
/// relationship by +1 per $100 of subsidy.
///
/// Offers are processed cheapest-first so buyers are filled from the cheapest
/// available supply globally; within one offer, the highest-relationship
/// (then lowest-`NationId`) buyer is served first. A buyer never trades with
/// itself, and an offer only matches a bid whose `max_price` covers it.
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

    // Track remaining quantity wanted for each bid.
    let mut bid_remaining: Vec<u32> = bids.iter().map(|b| b.quantity).collect();

    // Effective relationship score the given seller has toward the given bid's
    // buyer: base diplomatic score + subsidy bonus (+1 per $100).
    let effective_score = |bid: &TradeBid, seller: NationId| -> i64 {
        let base = relationship_scores
            .get(&(bid.buyer, seller))
            .copied()
            .unwrap_or(0) as i64;
        let subsidy_bonus = subsidies
            .get(&(bid.buyer, seller))
            .map(|s| s.as_dollars() / 100)
            .unwrap_or(0);
        base + subsidy_bonus
    };

    // Process offers cheapest-first (ties broken by seller id for determinism)
    // so buyers are filled from the cheapest supply available.
    let mut offer_order: Vec<usize> = (0..offers.len()).collect();
    offer_order.sort_by(|&a, &b| {
        offers[a]
            .price_per_unit
            .cmp(&offers[b].price_per_unit)
            .then(offers[a].seller.0.cmp(&offers[b].seller.0))
    });

    for &offer_idx in &offer_order {
        let offer = &offers[offer_idx];
        let mut offer_remaining = offer.quantity;
        if offer_remaining == 0 {
            continue;
        }

        // Bidders that still want this commodity at an acceptable price.
        let mut candidates: Vec<usize> = (0..bids.len())
            .filter(|&i| {
                bids[i].commodity == offer.commodity
                    && offer.price_per_unit <= bids[i].max_price_per_unit
                    && bids[i].buyer != offer.seller
                    && bid_remaining[i] > 0
            })
            .collect();

        // The seller serves its highest-relationship buyer first; ties broken
        // by lowest buyer NationId for deterministic resolution.
        candidates.sort_by(|&a, &b| {
            effective_score(&bids[b], offer.seller)
                .cmp(&effective_score(&bids[a], offer.seller))
                .then(bids[a].buyer.0.cmp(&bids[b].buyer.0))
        });

        for &bid_idx in &candidates {
            if offer_remaining == 0 {
                break;
            }
            let trade_qty = offer_remaining.min(bid_remaining[bid_idx]);
            if trade_qty == 0 {
                continue;
            }
            let price = offer.price_per_unit;
            transactions.push(TradeTransaction {
                buyer: bids[bid_idx].buyer,
                seller: offer.seller,
                commodity: offer.commodity,
                quantity: trade_qty,
                price_per_unit: price,
                total_cost: price * trade_qty as i64,
            });
            offer_remaining -= trade_qty;
            bid_remaining[bid_idx] -= trade_qty;
        }
    }

    transactions
}

/// Auto-generate trade offers from Minor Nations based on their resources.
/// Minor Nations sell their surplus resources at base price. Minors that have
/// been incorporated into a great power's empire stop offering — their
/// resources now belong to the overlord (the minor still consumes goods, but
/// nothing flows from it to the world market).
pub fn generate_minor_nation_offers(
    nations: &[Nation],
    provinces: &[Province],
    hex_map: &HexMap,
    market: &MarketState,
) -> Vec<TradeOffer> {
    let mut offers = Vec::new();

    for nation in nations {
        if nation.is_great_power()
            || nation.diplomacy.is_in_anarchy
            || nation.diplomacy.integrated_by.is_some()
        {
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
                // Trello #464: minors offer level-1 yields regardless of
                // prospect/improvement state.
                if let Some(tile) = hex_map.get_tile(*tile_coord) {
                    for yield_amount in tile.calculate_minor_offer_yields() {
                        *production.entry(yield_amount.resource).or_insert(0) +=
                            yield_amount.quantity;
                    }
                }
            }
        }

        // Create offers for tradeable resources at current market price.
        // Minors hoard Gold and Gems (monetary resources) — those are
        // strategic wealth they keep for their own treasury conversion, not
        // commodities they put up for sale to Great Powers.
        for (resource, quantity) in production {
            if resource.is_tradeable() && !resource.is_monetary() && quantity > 0 {
                let price = market.current_price(Commodity::Resource(resource));
                if price != Money::ZERO {
                    offers.push(TradeOffer {
                        seller: nation.id,
                        commodity: Commodity::Resource(resource),
                        quantity,
                        price_per_unit: price,
                    });
                }
            }
        }
    }

    offers
}

/// Auto-generate trade offers from Minor Nations with random-subset selection
/// of which resource types are offered each turn.
///
/// `withhold_chance > 0` switches the minor to "random subset" mode: each turn
/// the minor picks K ∈ [1, N] resource types uniformly at random from the N
/// tradeable types it produces, and offers the **full turn yield** of each
/// chosen type (one offer per type). The remaining types are withheld until a
/// future turn. This mirrors the original Imperialism behavior where minor
/// nations rotate which goods hit the world market, instead of dumping every
/// commodity every turn.
///
/// `withhold_chance == 0` keeps the legacy "offer everything" behavior, used
/// by older tests and by callers that want a stable offer pool. `seed` drives
/// the xorshift64 PRNG so results are deterministic per turn.
pub fn generate_minor_nation_offers_with_seed(
    nations: &[Nation],
    provinces: &[crate::map::Province],
    hex_map: &crate::map::HexMap,
    withhold_chance: u32,
    seed: u64,
    market: &MarketState,
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
                // Trello #464: minor nations advertise their resources at
                // level-1 yields even when the deposit is unprospected /
                // unimproved. This is what supplies the world market with
                // coal/iron from minors that never develop their own tiles.
                if let Some(tile) = hex_map.get_tile(*tile_coord) {
                    for yield_amount in tile.calculate_minor_offer_yields() {
                        *production.entry(yield_amount.resource).or_insert(0) +=
                            yield_amount.quantity;
                    }
                }
            }
        }

        // Per-(minor, resource) skip roll: for each tradeable resource the
        // minor produces, roll d100; if the roll is below `skip_chance` the
        // minor withholds *that specific resource* this turn. Independent
        // rolls per resource — symmetric with the per-(minor, commodity)
        // skip on the buy side (`minor_goods_skip_chance`). Resources are
        // visited in deterministic sorted order so the seed → skip mapping
        // is stable across runs and platforms.
        //
        // Monetary resources (Gold, Gems) are excluded: minors hoard them
        // for their own treasury conversion rather than putting them on the
        // world market. They never enter the sell-side offer pool.
        let mut tradeable: Vec<(ResourceType, u32)> = production
            .iter()
            .filter(|(r, _)| r.is_tradeable() && !r.is_monetary())
            .map(|(r, q)| (*r, *q))
            .collect();
        tradeable.sort_by_key(|(r, _)| format!("{r:?}"));

        for (resource, quantity) in &tradeable {
            // Advance RNG and consume one roll per (minor, resource) pair —
            // this happens unconditionally so the roll sequence doesn't
            // change based on availability or price (cleaner determinism).
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            let roll = (rng_state >> 32) as u32 % 100;
            if roll < withhold_chance {
                continue;
            }
            if *quantity == 0 {
                continue;
            }
            let price = market.current_price(Commodity::Resource(*resource));
            if price != Money::ZERO {
                offers.push(TradeOffer {
                    seller: nation.id,
                    commodity: Commodity::Resource(*resource),
                    quantity: *quantity,
                    price_per_unit: price,
                });
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

/// The full set of manufactured commodities Great Powers can produce and offer
/// for sale to minor nations each turn.
pub const ALL_MANUFACTURED: &[ManufacturedCommodity] = {
    use crate::types::{GoodsType, MaterialType};
    &[
        ManufacturedCommodity::Material(MaterialType::Lumber),
        ManufacturedCommodity::Material(MaterialType::Steel),
        ManufacturedCommodity::Material(MaterialType::Fabric),
        ManufacturedCommodity::Material(MaterialType::Paper),
        ManufacturedCommodity::Material(MaterialType::CannedFood),
        ManufacturedCommodity::Goods(GoodsType::Furniture),
        ManufacturedCommodity::Goods(GoodsType::Clothing),
        ManufacturedCommodity::Goods(GoodsType::Hardware),
        ManufacturedCommodity::Goods(GoodsType::Arms),
    ]
};

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

    // Direct worker meals (Imperialism-1 ratio): grain = ⌈w/2⌉,
    // meat = ⌊w/4⌋, fruit = w − grain − meat. Without this, the AI sells food
    // the workforce will starve on. Protein is fungible — mirror runtime
    // draw order (livestock first, then fish) for the per-resource floor.
    let workers = nation.economy.labor.total_workers();
    if workers > 0 {
        let (grain_need, fruit_need, meat_need) =
            crate::economy::labor::worker_food_demand(workers);
        *needs.entry(ResourceType::Grain).or_insert(0) += grain_need;
        *needs.entry(ResourceType::Fruit).or_insert(0) += fruit_need;
        let livestock_held = nation.resource_amount(ResourceType::Livestock);
        let livestock_need = meat_need.min(livestock_held);
        let fish_need = meat_need - livestock_need;
        *needs.entry(ResourceType::Livestock).or_insert(0) += livestock_need;
        *needs.entry(ResourceType::Fish).or_insert(0) += fish_need;
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
    market: &MarketState,
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

    // Collect unique tradeable resources from eligible offers. `generate_smart_bids`
    // only ever bids on raw resources, so non-resource offers are skipped here.
    let mut available_resources: Vec<ResourceType> = eligible_offers
        .iter()
        .filter_map(|o| match o.commodity {
            Commodity::Resource(r) => Some(r),
            _ => None,
        })
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
            .filter(|o| o.commodity == Commodity::Resource(*resource))
            .map(|o| o.quantity)
            .sum();

        if total_available == 0 {
            continue;
        }

        let bp = market.current_price(Commodity::Resource(*resource));
        if bp == Money::ZERO {
            continue;
        }

        // Bid for min(available, remaining_cargo)
        let bid_qty = total_available.min(remaining_cargo);

        // Max price at 120% of current market price
        let max_price = Money::dollars(bp.as_dollars() * 120 / 100);

        bids.push(TradeBid {
            buyer: nation.id,
            commodity: Commodity::Resource(*resource),
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
///    *import urgency* — per-turn shortfall the nation can't cover from its
///    own provinces — so resources without local supply (e.g. coal/iron in a
///    nation that doesn't natively produce them) win cargo first when capacity
///    is scarce. Largest absolute gap is the secondary tiebreak.
/// 4. Stop bidding when the projected total cost would push treasury below
///    `treasury_floor`. Each bid uses base_price for the cost projection.
/// 5. When `auto_trade_with_minors == false`, skip offers from minor nations.
///
/// `own_yield` is the nation's per-resource own-province supply per turn
/// (local + remote = `current_collectable_resources`). Pass an empty slice
/// from tests; the priority then degenerates to "largest gap first" which
/// matches the legacy ordering.
#[allow(clippy::too_many_arguments)]
pub fn generate_need_based_bids(
    nation: &Nation,
    all_nations: &[Nation],
    available_offers: &[TradeOffer],
    own_yield: &[(ResourceType, u32)],
    max_cargo: u32,
    treasury_floor: Money,
    buffer_turns: u32,
    market: &MarketState,
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

    let yield_for = |r: ResourceType| -> u32 {
        own_yield
            .iter()
            .find(|(rr, _)| *rr == r)
            .map(|(_, q)| *q)
            .unwrap_or(0)
    };

    // Compute per-resource gap = (per_turn_demand × buffer_turns) − current stock.
    // Also compute import_urgency = per_turn_demand − own_yield (clamped at 0):
    // resources the nation can't source domestically rank ahead of resources
    // it produces locally, even if the local-production resource has a bigger
    // absolute warehouse shortfall.
    let mut gaps: Vec<(ResourceType, u32, u32)> = needs
        .into_iter()
        .filter_map(|(r, per_turn)| {
            let target_stock = per_turn.saturating_mul(buffer_turns);
            let stock = nation.resource_amount(r);
            let gap = target_stock.saturating_sub(stock);
            if gap == 0 {
                return None;
            }
            let urgency = per_turn.saturating_sub(yield_for(r));
            Some((r, gap, urgency))
        })
        .collect();
    if gaps.is_empty() {
        return Vec::new();
    }
    // Order by largest import urgency first, then largest gap, then resource
    // name for determinism.
    gaps.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)))
    });

    let cash_available = nation.economy.treasury - treasury_floor;
    if cash_available <= Money::ZERO {
        return Vec::new();
    }

    // Pre-compute per-resource offer-pool quantity once.
    let total_available_for = |resource: ResourceType| -> u32 {
        eligible_offers
            .iter()
            .filter(|o| o.commodity == Commodity::Resource(resource))
            .map(|o| o.quantity)
            .sum()
    };

    let mut bid_qty_for: std::collections::BTreeMap<ResourceType, u32> =
        std::collections::BTreeMap::new();
    let mut remaining_cargo = max_cargo;
    let mut projected_spend = Money::ZERO;

    let cash_qty_left = |projected: Money, bp: Money| -> u32 {
        let cash_left = cash_available - projected;
        if cash_left <= Money::ZERO {
            return 0;
        }
        (cash_left.as_dollars() / bp.as_dollars()).clamp(0, u32::MAX as i64) as u32
    };

    // Floor pass: allocate at least 1 unit per gap-positive resource in
    // urgency order (subject to availability/cargo/cash). This guarantees
    // every critical chain input gets *some* import even when one resource's
    // gap is huge — without this, a single high-gap resource would consume
    // all cargo and starve coal/iron-only-abroad nations from importing
    // those at all.
    for (resource, _gap, _urgency) in &gaps {
        if remaining_cargo == 0 {
            break;
        }
        let avail = total_available_for(*resource);
        if avail == 0 {
            continue;
        }
        let bp = market.current_price(Commodity::Resource(*resource));
        if bp == Money::ZERO {
            continue;
        }
        let cq = cash_qty_left(projected_spend, bp);
        if cq == 0 {
            continue;
        }
        let unit = 1u32.min(avail).min(remaining_cargo).min(cq);
        if unit == 0 {
            continue;
        }
        *bid_qty_for.entry(*resource).or_insert(0) += unit;
        remaining_cargo -= unit;
        projected_spend += bp * unit as i64;
    }

    // Fill pass: walk priority order again, top each resource up toward its
    // full gap. Highest-urgency resources fill first; once one is at its
    // cap (gap, availability, cargo, or cash), move to the next.
    for (resource, gap, _urgency) in &gaps {
        if remaining_cargo == 0 {
            break;
        }
        let already = bid_qty_for.get(resource).copied().unwrap_or(0);
        let avail = total_available_for(*resource);
        let bp = market.current_price(Commodity::Resource(*resource));
        if bp == Money::ZERO {
            continue;
        }
        let room_gap = gap.saturating_sub(already);
        let room_avail = avail.saturating_sub(already);
        let cq = cash_qty_left(projected_spend, bp);
        let extra = room_gap.min(room_avail).min(remaining_cargo).min(cq);
        if extra == 0 {
            continue;
        }
        *bid_qty_for.entry(*resource).or_insert(0) += extra;
        remaining_cargo -= extra;
        projected_spend += bp * extra as i64;
    }

    // Emit bids in priority order so the trade matcher sees the most-urgent
    // resources first (matters when offers are matched FIFO).
    let mut bids = Vec::new();
    for (resource, _gap, _urgency) in &gaps {
        let qty = bid_qty_for.get(resource).copied().unwrap_or(0);
        if qty == 0 {
            continue;
        }
        let bp = market.current_price(Commodity::Resource(*resource));
        let max_price = Money::dollars(bp.as_dollars() * 120 / 100);
        bids.push(TradeBid {
            buyer: nation.id,
            commodity: Commodity::Resource(*resource),
            quantity: qty,
            max_price_per_unit: max_price,
        });
    }

    bids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::HexCoord;
    use crate::map::tile::Tile;
    use crate::nation::NationColor;

    /// Test fixture: a MarketState seeded from the default GameConfig so
    /// every commodity starts at its tier base. Equivalent to "fresh game".
    fn test_market() -> MarketState {
        MarketState::with_config(&crate::data::GameConfig::default())
    }

    /// Test fixture: the tier base price for a Resource under default config.
    fn r_base() -> Money {
        Money::dollars(crate::data::GameConfig::default().market_resource_base_price)
    }

    // ── tier base price ─────────────────────────────────────────

    #[test]
    fn tier_base_price_uniform_within_tier() {
        let cfg = crate::data::GameConfig::default();
        let r_base = Money::dollars(cfg.market_resource_base_price);
        let m_base = Money::dollars(cfg.market_material_base_price);
        let g_base = Money::dollars(cfg.market_goods_base_price);
        // Industrial resources and raw food sit at the resource tier.
        for r in [
            ResourceType::Coal,
            ResourceType::Gems,
            ResourceType::Grain,
            ResourceType::Fruit,
            ResourceType::Livestock,
            ResourceType::Fish,
        ] {
            assert_eq!(
                MarketState::tier_base_price(Commodity::Resource(r), &cfg),
                r_base,
                "{:?} should price at the resource tier",
                r
            );
        }
        // Processed materials (including Canned Food) PLUS Horses sit at
        // the material tier. Horses is a raw `ResourceType` but commands a
        // material-tier price.
        assert_eq!(
            MarketState::tier_base_price(Commodity::Resource(ResourceType::Horses), &cfg),
            m_base,
            "Horses should price at the material tier"
        );
        for m in [MaterialType::Steel, MaterialType::CannedFood] {
            assert_eq!(
                MarketState::tier_base_price(Commodity::Material(m), &cfg),
                m_base,
                "{:?} should price at the material tier",
                m
            );
        }
        // Finished goods sit at the goods tier.
        assert_eq!(
            MarketState::tier_base_price(Commodity::Goods(GoodsType::Arms), &cfg),
            g_base
        );
    }

    // ── resolve_trades ──────────────────────────────────────────

    #[test]
    fn resolve_trades_matches_compatible_offers_and_bids() {
        let offers = vec![TradeOffer {
            seller: NationId(10),
            commodity: Commodity::Resource(ResourceType::Timber),
            quantity: 5,
            price_per_unit: Money::dollars(50),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            commodity: Commodity::Resource(ResourceType::Timber),
            quantity: 3,
            max_price_per_unit: Money::dollars(60),
        }];

        let txns = resolve_trades(&offers, &bids);
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].buyer, NationId(1));
        assert_eq!(txns[0].seller, NationId(10));
        assert_eq!(txns[0].commodity, Commodity::Resource(ResourceType::Timber));
        assert_eq!(txns[0].quantity, 3);
        assert_eq!(txns[0].price_per_unit, Money::dollars(50));
        assert_eq!(txns[0].total_cost, Money::dollars(150));
    }

    #[test]
    fn resolve_trades_respects_price_limits() {
        let offers = vec![TradeOffer {
            seller: NationId(10),
            commodity: Commodity::Resource(ResourceType::Iron),
            quantity: 5,
            price_per_unit: Money::dollars(100),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            commodity: Commodity::Resource(ResourceType::Iron),
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
            commodity: Commodity::Resource(ResourceType::Coal),
            quantity: 2,
            price_per_unit: Money::dollars(75),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            commodity: Commodity::Resource(ResourceType::Coal),
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
            commodity: Commodity::Resource(ResourceType::Timber),
            quantity: 5,
            price_per_unit: Money::dollars(50),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1), // same nation
            commodity: Commodity::Resource(ResourceType::Timber),
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
                commodity: Commodity::Resource(ResourceType::Timber),
                quantity: 2,
                price_per_unit: Money::dollars(80),
            },
            TradeOffer {
                seller: NationId(11),
                commodity: Commodity::Resource(ResourceType::Timber),
                quantity: 3,
                price_per_unit: Money::dollars(50),
            },
        ];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            commodity: Commodity::Resource(ResourceType::Timber),
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
            commodity: Commodity::Resource(ResourceType::Coal),
            quantity: 5,
            price_per_unit: Money::dollars(75),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            commodity: Commodity::Resource(ResourceType::Iron), // different resource
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
                    commodity: Commodity::Resource(ResourceType::Timber),
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
                    commodity: Commodity::Resource(ResourceType::Timber),
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

        let offers = generate_minor_nation_offers(&nations, &provinces, &hex_map, &test_market());

        // Should have offers for Timber and Cotton (both tradeable)
        assert!(!offers.is_empty());

        let timber_offers: Vec<_> = offers
            .iter()
            .filter(|o| o.commodity == Commodity::Resource(ResourceType::Timber))
            .collect();
        let cotton_offers: Vec<_> = offers
            .iter()
            .filter(|o| o.commodity == Commodity::Resource(ResourceType::Cotton))
            .collect();

        // Card #464: minors offer at level-1 yield. For surface resources
        // (Timber, Cotton) on unimproved tiles that's 1 + 1 = 2. All
        // resources price at the resource-tier base under the dynamic-pricing
        // model (no per-commodity static prices anymore).
        assert_eq!(timber_offers.len(), 1);
        assert_eq!(timber_offers[0].seller, NationId(10));
        assert_eq!(timber_offers[0].quantity, 2);
        assert_eq!(timber_offers[0].price_per_unit, r_base());

        assert_eq!(cotton_offers.len(), 1);
        assert_eq!(cotton_offers[0].seller, NationId(10));
        assert_eq!(cotton_offers[0].quantity, 2);
        assert_eq!(cotton_offers[0].price_per_unit, r_base());
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

        let offers = generate_minor_nation_offers(&nations, &provinces, &hex_map, &test_market());
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

        let offers = generate_minor_nation_offers(&nations, &provinces, &hex_map, &test_market());
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

    // ── resolve_trades_with_preference ──────────────────────────

    #[test]
    fn preference_gives_higher_relationship_priority() {
        let offers = vec![TradeOffer {
            seller: NationId(10),
            commodity: Commodity::Resource(ResourceType::Timber),
            quantity: 3,
            price_per_unit: Money::dollars(50),
        }];
        // Two buyers want the same resource, but only 3 available
        let bids = vec![
            TradeBid {
                buyer: NationId(1),
                commodity: Commodity::Resource(ResourceType::Timber),
                quantity: 3,
                max_price_per_unit: Money::dollars(60),
            },
            TradeBid {
                buyer: NationId(2),
                commodity: Commodity::Resource(ResourceType::Timber),
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
            commodity: Commodity::Resource(ResourceType::Coal),
            quantity: 5,
            price_per_unit: Money::dollars(75),
        }];
        let bids = vec![
            TradeBid {
                buyer: NationId(1),
                commodity: Commodity::Resource(ResourceType::Coal),
                quantity: 5,
                max_price_per_unit: Money::dollars(100),
            },
            TradeBid {
                buyer: NationId(2),
                commodity: Commodity::Resource(ResourceType::Coal),
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

    #[test]
    fn preference_is_seller_specific_with_multiple_sellers() {
        // Two sellers, two buyers. Seller 10 favors buyer 1; seller 11 favors
        // buyer 2. Each buyer wants exactly one offer's worth. The matcher must
        // route each offer to that seller's preferred buyer — a cross-seller
        // "max score" surrogate would mis-route here.
        let offers = vec![
            TradeOffer {
                seller: NationId(10),
                commodity: Commodity::Resource(ResourceType::Timber),
                quantity: 3,
                price_per_unit: Money::dollars(50),
            },
            TradeOffer {
                seller: NationId(11),
                commodity: Commodity::Resource(ResourceType::Timber),
                quantity: 3,
                price_per_unit: Money::dollars(50),
            },
        ];
        let bids = vec![
            TradeBid {
                buyer: NationId(1),
                commodity: Commodity::Resource(ResourceType::Timber),
                quantity: 3,
                max_price_per_unit: Money::dollars(60),
            },
            TradeBid {
                buyer: NationId(2),
                commodity: Commodity::Resource(ResourceType::Timber),
                quantity: 3,
                max_price_per_unit: Money::dollars(60),
            },
        ];
        let mut scores = std::collections::HashMap::new();
        scores.insert((NationId(1), NationId(10)), 40); // buyer 1 favored by seller 10
        scores.insert((NationId(2), NationId(11)), 50); // buyer 2 favored by seller 11
        let subsidies = std::collections::HashMap::new();

        let txns = resolve_trades_with_preference(&offers, &bids, &scores, &subsidies);
        assert_eq!(txns.len(), 2);
        let from_10 = txns.iter().find(|t| t.seller == NationId(10)).unwrap();
        assert_eq!(from_10.buyer, NationId(1));
        let from_11 = txns.iter().find(|t| t.seller == NationId(11)).unwrap();
        assert_eq!(from_11.buyer, NationId(2));
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
    fn minor_offers_include_undiscovered_coal_at_level_1_card_464() {
        use crate::map::Province;
        let coord = HexCoord::new(0, 0);
        let mut hex_map = HexMap::new(10, 10);
        // Hills + Coal but unprospected, level 0 — calculate_yield says None.
        let mut tile = Tile::with_province(TerrainType::Hills, ProvinceId(20));
        tile.set_resource(ResourceType::Coal);
        hex_map.set_tile(coord, tile);
        let province = Province::new(
            ProvinceId(20),
            "Coal Country".to_string(),
            NationId(10),
            coord,
            vec![coord],
            3,
        );
        let minor = Nation::new(
            NationId(10),
            "Coalia".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        );

        let offers = generate_minor_nation_offers(&[minor], &[province], &hex_map, &test_market());
        let coal_offer = offers
            .iter()
            .find(|o| o.commodity == Commodity::Resource(ResourceType::Coal));
        assert!(
            coal_offer.is_some(),
            "minor nation must offer undiscovered Coal at level-1 yield (Trello #464)"
        );
        // Level-1 Coal yield = 2; price at base $75.
        assert_eq!(coal_offer.unwrap().quantity, 2);
        assert_eq!(coal_offer.unwrap().price_per_unit, r_base());
    }

    #[test]
    fn minors_never_offer_gold_or_gems() {
        use crate::map::Province;
        // A minor with Gold and Gems tiles must produce zero offers — these
        // are monetary resources the minor hoards for its own treasury,
        // never the world market. Test both generator entrypoints.
        let coord_gold = HexCoord::new(0, 0);
        let coord_gems = HexCoord::new(1, 0);
        let mut hex_map = HexMap::new(10, 10);
        let mut tile_gold = Tile::with_province(TerrainType::Hills, ProvinceId(20));
        tile_gold.set_resource(ResourceType::Gold);
        hex_map.set_tile(coord_gold, tile_gold);
        let mut tile_gems = Tile::with_province(TerrainType::Hills, ProvinceId(20));
        tile_gems.set_resource(ResourceType::Gems);
        hex_map.set_tile(coord_gems, tile_gems);
        let province = Province::new(
            ProvinceId(20),
            "Treasure Coast".to_string(),
            NationId(10),
            coord_gold,
            vec![coord_gold, coord_gems],
            3,
        );
        let minor = Nation::new(
            NationId(10),
            "Auria".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        );
        let nations = vec![minor];
        let provinces = vec![province];

        let offers = generate_minor_nation_offers(&nations, &provinces, &hex_map, &test_market());
        assert!(
            !offers
                .iter()
                .any(|o| matches!(o.commodity, Commodity::Resource(r) if r.is_monetary())),
            "legacy generator must not offer monetary resources from minors; got {:?}",
            offers.iter().map(|o| o.commodity).collect::<Vec<_>>()
        );

        for seed in 1u64..=50 {
            let offers = generate_minor_nation_offers_with_seed(
                &nations,
                &provinces,
                &hex_map,
                0, // never skip — every roll passes
                seed,
                &test_market(),
            );
            assert!(
                !offers
                    .iter()
                    .any(|o| matches!(o.commodity, Commodity::Resource(r) if r.is_monetary())),
                "seeded generator must not offer monetary resources from minors (seed {seed})"
            );
        }
    }

    #[test]
    fn withhold_chance_zero_never_withholds() {
        let (nations, provinces, hex_map) = make_minor_with_resources();
        let offers_normal =
            generate_minor_nation_offers(&nations, &provinces, &hex_map, &test_market());
        let offers_seeded = generate_minor_nation_offers_with_seed(
            &nations,
            &provinces,
            &hex_map,
            0,
            42,
            &test_market(),
        );
        // withhold_chance=0 must produce identical count as the unseeded version
        assert_eq!(offers_seeded.len(), offers_normal.len());
        assert_eq!(offers_seeded.len(), 2); // Timber + Cotton
    }

    #[test]
    fn skip_chance_50_yields_variable_subset_of_resources() {
        // At 50% skip per (minor, resource), across many seeds the offer
        // count must vary — sometimes nothing, sometimes some, sometimes
        // everything. We assert ≥ 2 distinct counts appear, which is the
        // load-bearing claim (per-roll, not always-on or always-off).
        let (nations, provinces, hex_map) = make_minor_with_resources();
        let offers_full =
            generate_minor_nation_offers(&nations, &provinces, &hex_map, &test_market());
        assert_eq!(offers_full.len(), 2, "setup: minor must have 2 resources");
        let mut saw = [false; 3]; // counts 0, 1, 2
        for seed in 1..200u64 {
            let n = generate_minor_nation_offers_with_seed(
                &nations,
                &provinces,
                &hex_map,
                50,
                seed,
                &test_market(),
            )
            .len();
            assert!(n <= 2, "offer count must not exceed N=2, got {n}");
            saw[n] = true;
        }
        let distinct: usize = saw.iter().filter(|x| **x).count();
        assert!(
            distinct >= 2,
            "across 200 seeds at 50% skip we must observe at least 2 distinct counts; saw {saw:?}"
        );
    }

    #[test]
    fn skip_chance_preserves_full_quantity_when_resource_is_offered() {
        // Whatever resources survive the per-roll skip, each offer carries
        // the full turn yield for that resource (no quantity splitting).
        let (nations, provinces, hex_map) = make_minor_with_resources();
        let full = generate_minor_nation_offers(&nations, &provinces, &hex_map, &test_market());
        let by_resource: std::collections::HashMap<_, _> =
            full.iter().map(|o| (o.commodity, o.quantity)).collect();

        for seed in 1..20u64 {
            let offers = generate_minor_nation_offers_with_seed(
                &nations,
                &provinces,
                &hex_map,
                50,
                seed,
                &test_market(),
            );
            for o in &offers {
                let expected = by_resource[&o.commodity];
                assert_eq!(
                    o.quantity, expected,
                    "minor must offer full turn yield for {:?}, got {} (expected {})",
                    o.commodity, o.quantity, expected
                );
            }
        }
    }

    #[test]
    fn seeded_offers_are_deterministic() {
        let (nations, provinces, hex_map) = make_minor_with_resources();
        let a = generate_minor_nation_offers_with_seed(
            &nations,
            &provinces,
            &hex_map,
            50,
            12345,
            &test_market(),
        );
        let b = generate_minor_nation_offers_with_seed(
            &nations,
            &provinces,
            &hex_map,
            50,
            12345,
            &test_market(),
        );
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.seller, y.seller);
            assert_eq!(x.commodity, y.commodity);
        }
    }

    #[test]
    fn skip_chance_50_can_withhold_each_resource_across_seeds() {
        // Across enough seeds at 50% skip, each individual resource should
        // sometimes be withheld and sometimes present. Confirms the per-roll
        // model is truly per-resource and not coupled across them.
        let (nations, provinces, hex_map) = make_minor_with_resources();
        let full = generate_minor_nation_offers(&nations, &provinces, &hex_map, &test_market());
        assert_eq!(full.len(), 2, "setup: minor must have Timber + Cotton");
        let mut timber_withheld = false;
        let mut cotton_withheld = false;
        let mut timber_offered = false;
        let mut cotton_offered = false;
        for seed in 1u64..=200 {
            let offers = generate_minor_nation_offers_with_seed(
                &nations,
                &provinces,
                &hex_map,
                50,
                seed,
                &test_market(),
            );
            let has_timber = offers
                .iter()
                .any(|o| o.commodity == Commodity::Resource(ResourceType::Timber));
            let has_cotton = offers
                .iter()
                .any(|o| o.commodity == Commodity::Resource(ResourceType::Cotton));
            if has_timber {
                timber_offered = true;
            } else {
                timber_withheld = true;
            }
            if has_cotton {
                cotton_offered = true;
            } else {
                cotton_withheld = true;
            }
        }
        assert!(
            timber_withheld && timber_offered && cotton_withheld && cotton_offered,
            "each resource must be both withheld and offered across 200 seeds at 50% skip"
        );
    }

    #[test]
    fn skip_chance_100_withholds_everything() {
        let (nations, provinces, hex_map) = make_minor_with_resources();
        let offers = generate_minor_nation_offers_with_seed(
            &nations,
            &provinces,
            &hex_map,
            100,
            42,
            &test_market(),
        );
        assert!(
            offers.is_empty(),
            "100% skip must withhold every resource — got {} offer(s)",
            offers.len()
        );
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
                commodity: Commodity::Resource(ResourceType::Coal),
                quantity: 10,
                price_per_unit: r_base(),
            },
            TradeOffer {
                seller: NationId(3),
                commodity: Commodity::Resource(ResourceType::Iron),
                quantity: 10,
                price_per_unit: r_base(),
            },
        ];

        let bids = generate_need_based_bids(
            &buyer,
            &nations,
            &offers,
            &[],                   // no own yield (test scenario)
            100,                   // ample cargo
            Money::dollars(5_000), // treasury floor
            3,                     // buffer turns
            &test_market(),
        );

        let coal_bid = bids
            .iter()
            .find(|b| b.commodity == Commodity::Resource(ResourceType::Coal));
        let iron_bid = bids
            .iter()
            .find(|b| b.commodity == Commodity::Resource(ResourceType::Iron));
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
            commodity: Commodity::Resource(ResourceType::Coal),
            quantity: 10,
            price_per_unit: r_base(),
        }];

        let bids = generate_need_based_bids(
            &buyer,
            &nations,
            &offers,
            &[],
            100,
            Money::dollars(5_000),
            3,
            &test_market(),
        );
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
            commodity: Commodity::Resource(ResourceType::Coal),
            quantity: 10,
            price_per_unit: r_base(),
        }];

        let bids = generate_need_based_bids(
            &buyer,
            &nations,
            &offers,
            &[],
            100,
            Money::dollars(5_000),
            3,
            &test_market(),
        );
        assert!(bids.is_empty(), "must not spend below the treasury floor");
    }

    #[test]
    fn need_based_caps_bid_quantity_by_cash() {
        use crate::economy::buildings::{Building, BuildingType};
        let mut buyer = make_gp(2);
        // Resource tier base = $60. Above floor of $5k we have $300 → 5 max.
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
            commodity: Commodity::Resource(ResourceType::Coal),
            quantity: 100,
            price_per_unit: r_base(),
        }];

        let bids = generate_need_based_bids(
            &buyer,
            &nations,
            &offers,
            &[],
            100,
            Money::dollars(5_000),
            3,
            &test_market(),
        );
        let coal = bids
            .iter()
            .find(|b| b.commodity == Commodity::Resource(ResourceType::Coal))
            .unwrap();
        // $300 cash budget / $60 (resource tier base) = 5 max coal.
        assert!(
            coal.quantity <= 5,
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
            commodity: Commodity::Resource(ResourceType::Coal),
            quantity: 10,
            price_per_unit: r_base(),
        }];

        let bids = generate_need_based_bids(
            &buyer,
            &nations,
            &offers,
            &[],
            100,
            Money::dollars(5_000),
            3,
            &test_market(),
        );
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
            commodity: Commodity::Resource(ResourceType::Coal),
            quantity: 10,
            price_per_unit: r_base(),
        }];

        let bids = generate_need_based_bids(
            &buyer,
            &nations,
            &offers,
            &[],
            100,
            Money::dollars(5_000),
            3,
            &test_market(),
        );
        assert!(
            bids.iter()
                .any(|b| b.commodity == Commodity::Resource(ResourceType::Coal)),
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
        assert_eq!(
            fish + livestock,
            5,
            "protein split must sum to canned output"
        );
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
            commodity: Commodity::Resource(ResourceType::Coal),
            quantity: 10,
            price_per_unit: r_base(),
        }];

        let bids = generate_need_based_bids(
            &buyer,
            &nations,
            &offers,
            &[],
            100,
            Money::dollars(5_000),
            0, // disable buy-side trade
            &test_market(),
        );
        assert!(
            bids.is_empty(),
            "buffer_turns=0 must short-circuit to no bids"
        );
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

    // ── Commodity-based trade resolution ───────────────────────────

    #[test]
    fn resolve_trades_matches_material_commodity() {
        // A material offer matched by a material bid resolves like any other
        // commodity — the matcher keys purely on `Commodity` equality.
        let offers = vec![TradeOffer {
            seller: NationId(10),
            commodity: Commodity::Material(MaterialType::Steel),
            quantity: 5,
            price_per_unit: Money::dollars(150),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            commodity: Commodity::Material(MaterialType::Steel),
            quantity: 3,
            max_price_per_unit: Money::dollars(150),
        }];

        let txns = resolve_trades(&offers, &bids);
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].commodity, Commodity::Material(MaterialType::Steel));
        assert_eq!(txns[0].quantity, 3);
        assert_eq!(txns[0].total_cost, Money::dollars(450));
    }

    #[test]
    fn resolve_trades_does_not_cross_match_resource_and_material() {
        // A Steel *resource*-shaped bid must never pull a Steel *material*
        // offer — `Commodity` discriminants differ.
        let offers = vec![TradeOffer {
            seller: NationId(10),
            commodity: Commodity::Material(MaterialType::Lumber),
            quantity: 5,
            price_per_unit: Money::dollars(150),
        }];
        let bids = vec![TradeBid {
            buyer: NationId(1),
            commodity: Commodity::Resource(ResourceType::Timber),
            quantity: 3,
            max_price_per_unit: Money::dollars(150),
        }];
        assert!(resolve_trades(&offers, &bids).is_empty());
    }
}
