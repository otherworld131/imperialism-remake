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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TradeTransaction {
    pub buyer: NationId,
    pub seller: NationId,
    pub resource: ResourceType,
    pub quantity: u32,
    pub price_per_unit: Money,
    pub total_cost: Money,
}

/// A record of a past trade transaction, stored in a nation's trade history.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TradeHistoryEntry {
    pub turn: TurnNumber,
    pub partner: NationId,
    pub resource: ResourceType,
    pub quantity: u32,
    pub total_cost: Money,
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
        if nation.is_great_power() || nation.is_in_anarchy {
            continue;
        }

        // Calculate total resource production for this minor nation
        let mut production: std::collections::HashMap<ResourceType, u32> =
            std::collections::HashMap::new();

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

/// Generate trade bids for a nation, respecting consulate requirements and cargo capacity.
///
/// Rules:
/// - Only buy from Minor Nations where the nation has a trade consulate (check diplomacy).
/// - Total quantity of all bids cannot exceed cargo capacity (merchant ships).
/// - Prioritize buying resources the nation needs most (buy what they have least of).
/// - Set max_price at 120% of base_price (willing to pay a bit more).
pub fn generate_smart_bids(
    nation: &Nation,
    available_offers: &[TradeOffer],
    diplomacy: &DiplomacyState,
    max_cargo: u32,
) -> Vec<TradeBid> {
    if max_cargo == 0 {
        return Vec::new();
    }

    // Filter offers to only those from nations where we have a consulate
    let eligible_offers: Vec<&TradeOffer> = available_offers
        .iter()
        .filter(|offer| {
            // Check that a consulate exists between this nation and the seller
            if let Some(rel) = diplomacy.get_relation(nation.id, offer.seller) {
                rel.has_consulate
            } else {
                false
            }
        })
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

    // ── TradeHistoryEntry ──────────────────────────────────────

    #[test]
    fn trade_history_entry_stores_all_fields() {
        let entry = TradeHistoryEntry {
            turn: TurnNumber(5),
            partner: NationId(10),
            resource: ResourceType::Timber,
            quantity: 3,
            total_cost: Money::dollars(150),
        };
        assert_eq!(entry.turn, TurnNumber(5));
        assert_eq!(entry.partner, NationId(10));
        assert_eq!(entry.resource, ResourceType::Timber);
        assert_eq!(entry.quantity, 3);
        assert_eq!(entry.total_cost, Money::dollars(150));
    }

    #[test]
    fn trade_history_entry_serializes_and_deserializes() {
        let entry = TradeHistoryEntry {
            turn: TurnNumber(12),
            partner: NationId(5),
            resource: ResourceType::Coal,
            quantity: 7,
            total_cost: Money::dollars(525),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: TradeHistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.turn, entry.turn);
        assert_eq!(deserialized.partner, entry.partner);
        assert_eq!(deserialized.resource, entry.resource);
        assert_eq!(deserialized.quantity, entry.quantity);
        assert_eq!(deserialized.total_cost, entry.total_cost);
    }
}
