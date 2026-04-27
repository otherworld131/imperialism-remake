use crate::economy::trade::{self};
use crate::game_state::GameState;
use crate::turn::processor::TurnReport;
use crate::types::*;

/// Resolve a trade session: generate offers from Minor Nations, handle player
/// sell/buy orders, use smart bids for AI GPs, resolve trades, and apply results.
pub(super) fn resolve_trade_session(
    game: &mut GameState,
    report: &mut TurnReport,
    blockade_capacity: &std::collections::HashMap<NationId, u32>,
) {
    let human_id = game.human_player_nation;
    let cfg = game.game_data.game_config.clone();

    // 0. Deduct subsidy costs from Great Powers (skip anarchic nations)
    let gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    for gp_id in &gp_ids {
        let subsidies: Vec<(NationId, Money)> = game
            .get_nation(*gp_id)
            .map(|n| n.diplomacy.trade_subsidies.iter().map(|(k, v)| (*k, *v)).collect())
            .unwrap_or_default();
        for (target_id, cost) in subsidies {
            if cost != Money::ZERO {
                if let Some(nation) = game.get_nation_mut(*gp_id) {
                    nation.economy.treasury -= cost;
                }
                report.subsidy_costs.push((*gp_id, target_id, cost));
            }
        }
    }

    // 1. Generate offers from Minor Nations
    let mut offers =
        trade::generate_minor_nation_offers(&game.world.nations, &game.world.provinces, &game.world.hex_map);

    // 1b. Add human player's resource sell offers to the pool
    if let Some(human) = game.get_nation(human_id) {
        let sell_orders: Vec<trade::PlayerSellOrder> = human.diplomacy.player_sell_orders.clone();
        for order in &sell_orders {
            if let trade::Commodity::Resource(r) = order.commodity
                && human.resource_amount(r) >= order.quantity
                && order.quantity > 0
            {
                offers.push(trade::TradeOffer {
                    seller: human_id,
                    resource: r,
                    quantity: order.quantity,
                    price_per_unit: trade::base_price(r),
                });
            }
        }
    }

    // 1c. Auto-sell player's material/goods sell orders (world market demand)
    let current_turn = game.turn;
    let mut player_goods_revenue = Money::ZERO;
    if let Some(human) = game.get_nation(human_id) {
        let sell_orders: Vec<trade::PlayerSellOrder> = human.diplomacy.player_sell_orders.clone();
        let mut goods_sold: Vec<(trade::Commodity, u32, Money)> = Vec::new();

        for order in &sell_orders {
            match order.commodity {
                trade::Commodity::Material(m) => {
                    let stock = human.economy.materials.get(&m).copied().unwrap_or(0);
                    let qty = order.quantity.min(stock);
                    if qty > 0 {
                        let price = trade::material_price(m, &cfg);
                        let revenue = Money::dollars(price.as_dollars() * qty as i64);
                        player_goods_revenue += revenue;
                        goods_sold.push((order.commodity, qty, revenue));
                    }
                }
                trade::Commodity::Goods(g) => {
                    let stock = human.economy.goods.get(&g).copied().unwrap_or(0);
                    let qty = order.quantity.min(stock);
                    if qty > 0 {
                        let price = trade::goods_price(g, &cfg);
                        let revenue = Money::dollars(price.as_dollars() * qty as i64);
                        player_goods_revenue += revenue;
                        goods_sold.push((order.commodity, qty, revenue));
                    }
                }
                trade::Commodity::Resource(_) => {} // handled in 1b via offer pool
            }
        }
        // Apply material/goods sales
        if let Some(human) = game.get_nation_mut(human_id) {
            human.economy.treasury += player_goods_revenue;
            human.archives.goods_sales_revenue_dollars += player_goods_revenue.as_dollars();
            if player_goods_revenue != Money::ZERO {
                report
                    .goods_auto_sale_revenue
                    .push((human_id, player_goods_revenue));
            }
            for (commodity, qty, _revenue) in &goods_sold {
                match commodity {
                    trade::Commodity::Material(m) => {
                        if let Some(stock) = human.economy.materials.get_mut(m) {
                            *stock = stock.saturating_sub(*qty);
                        }
                    }
                    trade::Commodity::Goods(g) => {
                        if let Some(stock) = human.economy.goods.get_mut(g) {
                            *stock = stock.saturating_sub(*qty);
                        }
                    }
                    trade::Commodity::Resource(_) => {}
                }
            }
        }
    }

    // 2. Generate bids: AI GPs use smart bids, human player uses manual buy orders
    let mut all_bids = Vec::new();

    for gp_id in &gp_ids {
        if *gp_id == human_id {
            // Use player's manual buy orders instead of auto-generated bids
            if let Some(human) = game.get_nation(*gp_id) {
                for order in &human.diplomacy.player_buy_orders {
                    if order.quantity > 0 {
                        all_bids.push(trade::TradeBid {
                            buyer: *gp_id,
                            resource: order.resource,
                            quantity: order.quantity,
                            max_price_per_unit: order.max_price_per_unit,
                        });
                    }
                }
            }
            continue;
        }
        if let Some(nation) = game.get_nation(*gp_id) {
            // Use blockade-adjusted cargo capacity instead of raw capacity
            let cargo_capacity = blockade_capacity
                .get(gp_id)
                .copied()
                .unwrap_or_else(|| nation.total_cargo_capacity());
            let bids = trade::generate_smart_bids(nation, &offers, &game.world.diplomacy, cargo_capacity);
            all_bids.extend(bids);
        }
    }

    if offers.is_empty() && all_bids.is_empty() {
        // Clear player orders and return
        if let Some(human) = game.get_nation_mut(human_id) {
            human.diplomacy.player_sell_orders.clear();
            human.diplomacy.player_buy_orders.clear();
        }
        return;
    }

    // 3. Build relationship scores and subsidies maps for preference-based resolution
    let mut relationship_scores: std::collections::HashMap<(NationId, NationId), i32> =
        std::collections::HashMap::new();
    let mut subsidies_map: std::collections::HashMap<(NationId, NationId), Money> =
        std::collections::HashMap::new();

    for gp_id in &gp_ids {
        if let Some(nation) = game.get_nation(*gp_id) {
            // Collect subsidies
            for (target_id, amount) in &nation.diplomacy.trade_subsidies {
                subsidies_map.insert((*gp_id, *target_id), *amount);
            }
        }
        // Collect relationship scores
        for offer in &offers {
            if let Some(rel) = game.world.diplomacy.get_relation(*gp_id, offer.seller) {
                relationship_scores.insert((*gp_id, offer.seller), rel.score);
            }
        }
    }

    // 4. Resolve trades with preference system
    let transactions = trade::resolve_trades_with_preference(
        &offers,
        &all_bids,
        &relationship_scores,
        &subsidies_map,
    );

    // 5. Apply transactions
    for txn in &transactions {
        // Buyer pays money and receives resources
        if let Some(buyer) = game.get_nation_mut(txn.buyer) {
            buyer.economy.treasury -= txn.total_cost;
            buyer.add_resource(txn.resource, txn.quantity);
        }
        if let Some(seller) = game.get_nation_mut(txn.seller) {
            seller.economy.treasury += txn.total_cost;
        }
    }

    // 5b. Deduct sold resources from player (GP sellers lose warehouse stock)
    for txn in &transactions {
        if txn.seller == human_id
            && let Some(seller) = game.get_nation_mut(human_id)
        {
            seller.remove_resource(txn.resource, txn.quantity);
        }
    }

    // 5c. Record trade history for each nation involved
    for txn in &transactions {
        // Record for buyer (partner is seller)
        if let Some(buyer) = game.get_nation_mut(txn.buyer) {
            buyer.archives.trade_history.push(trade::TradeHistoryEntry {
                turn: current_turn,
                partner: txn.seller,
                resource: txn.resource,
                quantity: txn.quantity,
                total_cost: txn.total_cost,
                bought: true,
            });
        }
        // Record for seller (partner is buyer)
        if let Some(seller) = game.get_nation_mut(txn.seller) {
            seller.archives.trade_history.push(trade::TradeHistoryEntry {
                turn: current_turn,
                partner: txn.buyer,
                resource: txn.resource,
                quantity: txn.quantity,
                total_cost: txn.total_cost,
                bought: false,
            });
        }
    }

    // 6. Diplomatic impact: +1 score per distinct commodity type traded per partner pair
    let mut trade_pairs: std::collections::BTreeMap<
        (NationId, NationId),
        std::collections::BTreeSet<ResourceType>,
    > = std::collections::BTreeMap::new();
    for txn in &transactions {
        trade_pairs
            .entry((txn.buyer, txn.seller))
            .or_default()
            .insert(txn.resource);
    }
    // Cap + interval sourced from scripts/config/game.lua — unscaled improvement
    // let minors voluntarily join an empire in only ~5 years of passive trade.
    let trade_cap = game.game_data.game_config.trade_relation_improvement_cap;
    let trade_interval = game.game_data.game_config.trade_relation_turn_interval;
    let apply_trade_improvement = trade_interval > 0 && game.turn.0.is_multiple_of(trade_interval);
    if apply_trade_improvement {
        for ((buyer, seller), resources) in &trade_pairs {
            // Only improve relations if a trade consulate exists between the nations.
            if game.world.diplomacy.has_consulate(*buyer, *seller) {
                let improvement = (resources.len() as i32).min(trade_cap);
                let rel = game.world.diplomacy.ensure_relation(*buyer, *seller);
                rel.improve_score(improvement);
                report.trade_diplomacy.push((*buyer, *seller, improvement));
            }
        }
    }

    // 7. Record trade balance per nation
    let mut spent: std::collections::HashMap<NationId, Money> = std::collections::HashMap::new();
    let mut earned: std::collections::HashMap<NationId, Money> = std::collections::HashMap::new();
    for txn in &transactions {
        *spent.entry(txn.buyer).or_insert(Money::ZERO) += txn.total_cost;
        *earned.entry(txn.seller).or_insert(Money::ZERO) += txn.total_cost;
    }
    // Include player's auto-sold materials/goods revenue
    if player_goods_revenue != Money::ZERO {
        *earned.entry(human_id).or_insert(Money::ZERO) += player_goods_revenue;
    }
    let all_ids: std::collections::HashSet<NationId> =
        spent.keys().chain(earned.keys()).copied().collect();
    for nid in all_ids {
        report.trade_balance.push((
            nid,
            *spent.get(&nid).unwrap_or(&Money::ZERO),
            *earned.get(&nid).unwrap_or(&Money::ZERO),
        ));
    }

    // 8. Record in report
    report.trade_transactions = transactions;
    let transactions = &report.trade_transactions;

    // 8b. Update persistent market state (#164): record per-resource supply, demand, sold.
    {
        let mut supply_map: std::collections::BTreeMap<ResourceType, u32> =
            std::collections::BTreeMap::new();
        let mut demand_map: std::collections::BTreeMap<ResourceType, u32> =
            std::collections::BTreeMap::new();
        let mut sold_map: std::collections::BTreeMap<ResourceType, u32> =
            std::collections::BTreeMap::new();
        let mut price_sum_map: std::collections::BTreeMap<ResourceType, (i64, u32)> =
            std::collections::BTreeMap::new(); // (total_price * qty, total_qty)

        for offer in &offers {
            *supply_map.entry(offer.resource).or_insert(0) += offer.quantity;
        }
        for bid in &all_bids {
            *demand_map.entry(bid.resource).or_insert(0) += bid.quantity;
        }
        for txn in transactions {
            *sold_map.entry(txn.resource).or_insert(0) += txn.quantity;
            let (ps, pq) = price_sum_map.entry(txn.resource).or_insert((0, 0));
            *ps += txn.price_per_unit.as_dollars() * txn.quantity as i64;
            *pq += txn.quantity;
        }

        // Union all resources that appeared in offers or bids this turn
        let mut all_resources: std::collections::BTreeSet<ResourceType> =
            std::collections::BTreeSet::new();
        all_resources.extend(supply_map.keys().copied());
        all_resources.extend(demand_map.keys().copied());

        for resource in all_resources {
            let supply = supply_map.get(&resource).copied().unwrap_or(0);
            let demand = demand_map.get(&resource).copied().unwrap_or(0);
            let sold = sold_map.get(&resource).copied().unwrap_or(0);
            let price = if let Some(&(ps, pq)) = price_sum_map.get(&resource)
                && pq > 0
            {
                Money::dollars(ps / pq as i64)
            } else {
                crate::economy::trade::base_price(resource)
            };
            game.world.market_state.record_tick(
                crate::economy::trade::Commodity::Resource(resource),
                current_turn,
                price,
                supply,
                demand,
                sold,
            );
        }
    }

    // 9. Clear player trade orders for next turn
    if let Some(human) = game.get_nation_mut(human_id) {
        human.diplomacy.player_sell_orders.clear();
        human.diplomacy.player_buy_orders.clear();
    }
}
