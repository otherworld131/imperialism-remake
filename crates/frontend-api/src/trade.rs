//! Trade screen: market data query plus subsidy, auto-trade, and player
//! buy/sell order commands. Bodies moved verbatim from `wasm-bridge`.

use crate::ApiError;
use crate::parse::parse_resource_type;
use domain::economy::trade::Commodity;
use domain::game_state::GameState;
use domain::types::*;

/// Query trade data for a nation.
pub fn get_trade_data(game: &GameState, nation_id: u32) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    // Market prices
    let all_resources = [
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
    ];
    let market_prices: Vec<serde_json::Value> = all_resources
        .iter()
        .map(|&r| {
            let commodity = Commodity::Resource(r);
            let bp = game.world.market_state.current_price(commodity);
            let trend = game.world.market_state.trend(commodity, 5);
            let stock = nation.resource_amount(r);
            serde_json::json!({
                "resource": format!("{:?}", r),
                "base_price": bp.as_dollars(),
                "trend": trend.to_string(),
                "stock": stock,
            })
        })
        .collect();

    // Trade history (last 10 turns), newest first
    let history_min_turn = game.turn.0.saturating_sub(9);
    let history: Vec<serde_json::Value> = nation
        .archives
        .trade_history
        .iter()
        .rev()
        .filter(|entry| entry.turn.0 >= history_min_turn)
        .map(|entry| {
            let partner_nation = game.get_nation(entry.partner);
            let partner_name = partner_nation.map(|n| n.name.as_str()).unwrap_or("Unknown");
            let partner_is_great_power =
                partner_nation.map(|n| n.is_great_power()).unwrap_or(false);
            // Use commodity_label when set (manufactured goods), fall back to resource name
            let commodity = if entry.commodity_label.is_empty() {
                format!("{:?}", entry.resource)
            } else {
                entry.commodity_label.clone()
            };
            serde_json::json!({
                "turn": entry.turn.0,
                "partner_name": partner_name,
                "partner_id": entry.partner.0,
                "partner_is_great_power": partner_is_great_power,
                "resource": commodity,
                "quantity": entry.quantity,
                "total_cost": entry.total_cost.as_dollars(),
                "bought": entry.bought,
            })
        })
        .collect();

    // Subsidies
    let subsidies: Vec<serde_json::Value> = nation
        .diplomacy
        .trade_subsidies
        .iter()
        .map(|(&target_nid, &amount)| {
            let target_name = game
                .get_nation(target_nid)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let has_consulate = game
                .world
                .diplomacy
                .get_relation(nid, target_nid)
                .map(|r| r.has_consulate)
                .unwrap_or(false);
            serde_json::json!({
                "nation_id": target_nid.0,
                "nation_name": target_name,
                "amount": amount.as_dollars(),
                "has_consulate": has_consulate,
            })
        })
        .collect();

    // Trade balance from history. All sales (resource trades, auto-sell goods,
    // minor-nation goods bids) are recorded as TradeHistoryEntry with bought=false.
    let mut total_bought: i64 = 0;
    let mut total_sold: i64 = 0;
    for entry in &nation.archives.trade_history {
        if entry.bought {
            total_bought += entry.total_cost.as_dollars();
        } else {
            total_sold += entry.total_cost.as_dollars();
        }
    }

    // Cargo capacity from merchant fleet
    let total_cargo: u32 = nation
        .military
        .merchant_fleet
        .iter()
        .map(|s| game.game_data.ship_stats(s.ship_type).cargo)
        .sum();

    // Minor nations with consulates
    let minor_nations: Vec<serde_json::Value> = game
        .world
        .nations
        .iter()
        .filter(|n| n.nation_type == NationType::MinorNation && n.id != nid)
        .map(|n| {
            let rel = game.world.diplomacy.get_relation(nid, n.id);
            let has_consulate = rel.map(|r| r.has_consulate).unwrap_or(false);
            let has_embassy = rel.map(|r| r.has_embassy).unwrap_or(false);
            // Collect resources available in minor nation's provinces
            let mut mn_resources = Vec::new();
            for &pid in &n.province_ids {
                if let Some(prov) = game.get_province(pid) {
                    for &coord in &prov.tiles {
                        if let Some(tile) = game.world.hex_map.get_tile(coord)
                            && tile.has_visible_resource()
                            && let Some(r) = tile.resource_deposit()
                        {
                            let rs = format!("{:?}", r);
                            if !mn_resources.contains(&rs) {
                                mn_resources.push(rs);
                            }
                        }
                    }
                }
            }
            serde_json::json!({
                "nation_id": n.id.0,
                "name": n.name,
                "has_consulate": has_consulate,
                "has_embassy": has_embassy,
                "resources": mn_resources,
            })
        })
        .collect();

    // Player sell orders (resources only)
    let player_sell_orders: Vec<serde_json::Value> = nation
        .diplomacy
        .player_sell_orders
        .iter()
        .map(|o| {
            let name = format!("{:?}", o.resource);
            serde_json::json!({
                "commodity_type": "resource",
                "commodity_name": name,
                "quantity": o.quantity,
                "price": game
                    .world
                    .market_state
                    .current_price(Commodity::Resource(o.resource))
                    .as_dollars(),
            })
        })
        .collect();

    // Player buy orders (resources only)
    let player_buy_orders: Vec<serde_json::Value> = nation
        .diplomacy
        .player_buy_orders
        .iter()
        .map(|o| {
            let name = format!("{:?}", o.resource);
            serde_json::json!({
                "commodity_type": "resource",
                "commodity_name": name.clone(),
                "resource": name,
                "quantity": o.quantity,
                "max_price": o.max_price_per_unit.as_dollars(),
            })
        })
        .collect();

    // Available offers from minor nations — use the same seeded withholding path as trade resolution
    let minor_offer_seed =
        (game.turn.0 as u64).wrapping_mul(0x9e3779b97f4a7c15) ^ 0x6c62272e07bb0142;
    let withhold_chance = game
        .game_data
        .game_config
        .minor_resource_skip_chance
        .min(100);
    let mut available_offers: Vec<serde_json::Value> =
        domain::economy::trade::generate_minor_nation_offers_with_seed(
            &game.world.nations,
            &game.world.provinces,
            &game.world.hex_map,
            withhold_chance,
            minor_offer_seed,
            &game.world.market_state,
        )
        .iter()
        .map(|o| {
            let seller_name = game
                .get_nation(o.seller)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            serde_json::json!({
                "seller_id": o.seller.0,
                "seller_name": seller_name,
                "resource": o.commodity.to_string(),
                "quantity": o.quantity,
                "price": o.price_per_unit.as_dollars(),
                "is_great_power": false,
            })
        })
        .collect();

    // Add surplus offers from other Great Powers
    for gp in &game.world.nations {
        if gp.id == nid || !gp.is_great_power() {
            continue;
        }
        for (&resource, &qty) in &gp.economy.warehouse {
            if qty > 3 {
                let surplus = qty - 3;
                let price = game
                    .world
                    .market_state
                    .current_price(Commodity::Resource(resource));
                available_offers.push(serde_json::json!({
                    "seller_id": gp.id.0,
                    "seller_name": gp.name,
                    "resource": format!("{:?}", resource),
                    "quantity": surplus,
                    "price": price.as_dollars(),
                    "is_great_power": true,
                }));
            }
        }
    }

    // Sellable items: resources, materials, goods with stock > 0
    let sellable_resources: Vec<serde_json::Value> = all_resources
        .iter()
        .filter_map(|&r| {
            let stock = nation.resource_amount(r);
            if stock > 0 {
                Some(serde_json::json!({
                    "name": format!("{:?}", r),
                    "stock": stock,
                    "price": game
                        .world
                        .market_state
                        .current_price(Commodity::Resource(r))
                        .as_dollars(),
                }))
            } else {
                None
            }
        })
        .collect();

    // Remaining cargo after current orders
    let orders_qty: u32 = nation
        .diplomacy
        .player_sell_orders
        .iter()
        .map(|o| o.quantity)
        .chain(
            nation
                .diplomacy
                .player_buy_orders
                .iter()
                .map(|o| o.quantity),
        )
        .sum();
    let remaining_cargo = total_cargo.saturating_sub(orders_qty);

    // Per-turn market activity archive — feeds the "Historical Market" tab.
    // Newest turn first so the UI sidebar can show latest at the top.
    let market_archive: Vec<serde_json::Value> = game
        .archive
        .market_archive
        .iter()
        .rev()
        .map(|(turn, record)| {
            let offers_json: Vec<serde_json::Value> = record
                .offers
                .iter()
                .map(|row| {
                    let seller_nation = game.get_nation(row.seller);
                    let seller_name = seller_nation
                        .map(|n| n.name.as_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    let seller_is_great_power =
                        seller_nation.map(|n| n.is_great_power()).unwrap_or(false);
                    let fills_json: Vec<serde_json::Value> = row
                        .fills
                        .iter()
                        .map(|fill| {
                            let buyer_nation = game.get_nation(fill.buyer);
                            let buyer_name = buyer_nation
                                .map(|n| n.name.as_str())
                                .unwrap_or("Unknown")
                                .to_string();
                            let buyer_is_great_power =
                                buyer_nation.map(|n| n.is_great_power()).unwrap_or(false);
                            serde_json::json!({
                                "buyer_id": fill.buyer.0,
                                "buyer_name": buyer_name,
                                "buyer_is_great_power": buyer_is_great_power,
                                "quantity": fill.quantity,
                                "price_per_unit": fill.price_per_unit.as_dollars(),
                            })
                        })
                        .collect();
                    let sold: u32 = row.fills.iter().map(|f| f.quantity).sum();
                    let commodity_label = if row.commodity_label.is_empty() {
                        row.commodity.to_string()
                    } else {
                        row.commodity_label.clone()
                    };
                    serde_json::json!({
                        "seller_id": row.seller.0,
                        "seller_name": seller_name,
                        "seller_is_great_power": seller_is_great_power,
                        "resource": commodity_label,
                        "offered": row.offered,
                        "sold": sold,
                        "price_per_unit": row.price_per_unit.as_dollars(),
                        "fills": fills_json,
                    })
                })
                .collect();
            serde_json::json!({
                "turn": turn.0,
                "offers": offers_json,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "market_prices": market_prices,
        "trade_history": history,
        "market_archive": market_archive,
        "subsidies": subsidies,
        "trade_balance": {
            "total_bought": total_bought,
            "total_sold": total_sold,
            "net": total_sold - total_bought,
        },
        "total_cargo": total_cargo,
        "remaining_cargo": remaining_cargo,
        "minor_nations": minor_nations,
        "treasury": nation.economy.treasury.as_dollars(),
        "player_sell_orders": player_sell_orders,
        "player_buy_orders": player_buy_orders,
        "available_offers": available_offers,
        "sellable_resources": sellable_resources,
        "auto_trade_with_minors": nation.economy.auto_trade_with_minors,
    }))
}

/// Toggle automatic minor-nation goods purchases for the player.
pub fn set_auto_trade_with_minors(
    game: &mut GameState,
    nation_id: u32,
    enabled: bool,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    if let Some(nation) = game.get_nation_mut(nid) {
        nation.economy.auto_trade_with_minors = enabled;
    }
    Ok(())
}

/// Set trade subsidy for a minor nation.
pub fn set_trade_subsidy(
    game: &mut GameState,
    nation_id: u32,
    target_nation_id: u32,
    amount: i64,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target_nid = NationId(target_nation_id);

    // Validate target nation exists
    if game.get_nation(target_nid).is_none() {
        return Err(ApiError::raw("{\"error\":\"target nation not found\"}"));
    }
    if game
        .get_nation(target_nid)
        .map(|n| n.nation_type != NationType::MinorNation)
        .unwrap_or(true)
    {
        return Err(ApiError::raw(
            "{\"error\":\"subsidies can only be set for minor nations\"}",
        ));
    }

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    if amount <= 0 {
        nation.diplomacy.trade_subsidies.remove(&target_nid);
    } else {
        nation
            .diplomacy
            .trade_subsidies
            .insert(target_nid, Money::dollars(amount));
    }

    Ok(())
}

/// Set a player sell order for a resource. Materials and goods are not
/// tradeable via player orders — the world market was removed.
pub fn set_player_sell_order(
    game: &mut GameState,
    nation_id: u32,
    commodity_type: &str,
    commodity_name: &str,
    quantity: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);

    if commodity_type != "resource" {
        return Err(ApiError::raw(
            r#"{"error":"only resources can be sold (world market removed)"}"#,
        ));
    }
    let resource = match parse_resource_type(commodity_name) {
        Some(r) => r,
        None => return Err(ApiError::raw(r#"{"error":"invalid resource"}"#)),
    };

    let total_cargo: u32 = match game.get_nation(nid) {
        Some(n) => n.total_cargo_capacity(&game.game_data),
        None => return Err(ApiError::raw(r#"{"error":"nation not found"}"#)),
    };

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw(r#"{"error":"nation not found"}"#)),
    };

    let available = nation.resource_amount(resource);
    if quantity > available {
        return Err(ApiError::raw(r#"{"error":"insufficient stock"}"#));
    }

    let other_orders: u32 = nation
        .diplomacy
        .player_sell_orders
        .iter()
        .filter(|o| o.resource != resource)
        .map(|o| o.quantity)
        .chain(
            nation
                .diplomacy
                .player_buy_orders
                .iter()
                .map(|o| o.quantity),
        )
        .sum();
    if other_orders + quantity > total_cargo {
        return Err(ApiError::raw(r#"{"error":"exceeds cargo capacity"}"#));
    }

    nation
        .diplomacy
        .player_sell_orders
        .retain(|o| o.resource != resource);
    if quantity > 0 {
        nation
            .diplomacy
            .player_sell_orders
            .push(domain::economy::trade::PlayerSellOrder { resource, quantity });
    }

    Ok(())
}

/// Set a player buy order for a resource. Filled from the offer pool of
/// Minor Nation offers and GP surplus.
pub fn set_player_buy_order(
    game: &mut GameState,
    nation_id: u32,
    commodity_type: &str,
    commodity_name: &str,
    quantity: u32,
    max_price: i64,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);

    if commodity_type != "resource" {
        return Err(ApiError::raw(
            r#"{"error":"only resources can be bought (world market removed)"}"#,
        ));
    }
    let resource = match parse_resource_type(commodity_name) {
        Some(r) => r,
        None => return Err(ApiError::raw(r#"{"error":"invalid resource"}"#)),
    };

    let total_cargo: u32 = match game.get_nation(nid) {
        Some(n) => n.total_cargo_capacity(&game.game_data),
        None => return Err(ApiError::raw(r#"{"error":"nation not found"}"#)),
    };

    // Snapshot the market price before taking the mutable nation borrow so the
    // immutable read of `game.world.market_state` doesn't conflict.
    let default_max_price = Money::dollars(
        game.world
            .market_state
            .current_price(Commodity::Resource(resource))
            .as_dollars()
            * 120
            / 100,
    );

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw(r#"{"error":"nation not found"}"#)),
    };

    let other_orders: u32 = nation
        .diplomacy
        .player_sell_orders
        .iter()
        .map(|o| o.quantity)
        .chain(
            nation
                .diplomacy
                .player_buy_orders
                .iter()
                .filter(|o| o.resource != resource)
                .map(|o| o.quantity),
        )
        .sum();
    if other_orders + quantity > total_cargo {
        return Err(ApiError::raw(r#"{"error":"exceeds cargo capacity"}"#));
    }

    let max_price_per_unit = if max_price > 0 {
        Money::dollars(max_price)
    } else {
        default_max_price
    };

    nation
        .diplomacy
        .player_buy_orders
        .retain(|o| o.resource != resource);
    if quantity > 0 {
        nation
            .diplomacy
            .player_buy_orders
            .push(domain::economy::trade::PlayerBuyOrder {
                resource,
                quantity,
                max_price_per_unit,
            });
    }

    Ok(())
}
