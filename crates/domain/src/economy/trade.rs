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
        // Non-tradeable
        ResourceType::Grain => Money::dollars(0),
        ResourceType::Horses => Money::dollars(0),
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

/// Auto-generate trade offers from Minor Nations based on their resources.
/// Minor Nations sell their surplus resources at base price.
pub fn generate_minor_nation_offers(
    nations: &[Nation],
    provinces: &[Province],
    hex_map: &HexMap,
) -> Vec<TradeOffer> {
    let mut offers = Vec::new();

    for nation in nations {
        if nation.is_great_power() {
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
    fn non_tradeable_resources_have_zero_price() {
        assert_eq!(base_price(ResourceType::Grain), Money::dollars(0));
        assert_eq!(base_price(ResourceType::Horses), Money::dollars(0));
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
        hex_map.set_tile(
            coord_forest,
            Tile::with_province(TerrainType::ScrubForest, ProvinceId(20)),
        );
        hex_map.set_tile(
            coord_plantation,
            Tile::with_province(TerrainType::Plantation, ProvinceId(20)),
        );

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
        hex_map.set_tile(
            coord,
            Tile::with_province(TerrainType::ScrubForest, ProvinceId(1)),
        );

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
    fn generate_minor_nation_offers_skips_non_tradeable_grain() {
        let coord = HexCoord::new(0, 0);

        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            coord,
            Tile::with_province(TerrainType::Farm, ProvinceId(20)),
        );

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
            offers.is_empty(),
            "Grain is not tradeable and should not appear in offers"
        );
    }
}
