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
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    for gp_id in &gp_ids {
        let subsidies: Vec<(NationId, Money)> = game
            .get_nation(*gp_id)
            .map(|n| {
                n.diplomacy
                    .trade_subsidies
                    .iter()
                    .map(|(k, v)| (*k, *v))
                    .collect()
            })
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

    let current_turn = game.turn;

    // 1. Generate offers from Minor Nations (with optional random withholding)
    let minor_offer_seed =
        (current_turn.0 as u64).wrapping_mul(0x9e3779b97f4a7c15) ^ 0x6c62272e07bb0142;
    let withhold_chance = game
        .game_data
        .game_config
        .minor_resource_withhold_chance
        .min(100);
    let mut offers = trade::generate_minor_nation_offers_with_seed(
        &game.world.nations,
        &game.world.provinces,
        &game.world.hex_map,
        withhold_chance,
        minor_offer_seed,
    );

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
            for (commodity, qty, revenue) in &goods_sold {
                let commodity_label = match commodity {
                    trade::Commodity::Material(m) => format!("{m}"),
                    trade::Commodity::Goods(g) => format!("{g:?}"),
                    trade::Commodity::Resource(r) => format!("{r:?}"),
                };
                match commodity {
                    trade::Commodity::Material(m) => {
                        if let Some(stock) = human.economy.materials.get_mut(m) {
                            *stock = stock.saturating_sub(*qty);
                        }
                        report
                            .stockpile_flows
                            .auto_sold_materials
                            .push((human_id, *m, *qty));
                    }
                    trade::Commodity::Goods(g) => {
                        if let Some(stock) = human.economy.goods.get_mut(g) {
                            *stock = stock.saturating_sub(*qty);
                        }
                        report
                            .stockpile_flows
                            .auto_sold_goods
                            .push((human_id, *g, *qty));
                    }
                    trade::Commodity::Resource(_) => {}
                }
                // Record auto-sale in trade history with world-market sentinel partner (NationId(0))
                if *qty > 0 {
                    human.archives.trade_history.push(trade::TradeHistoryEntry {
                        turn: current_turn,
                        partner: NationId(0),
                        resource: ResourceType::Timber, // sentinel; commodity_label carries the real name
                        commodity_label,
                        quantity: *qty,
                        total_cost: *revenue,
                        bought: false,
                    });
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
                .unwrap_or_else(|| nation.total_cargo_capacity(&game.game_data));
            // Need-based bids drive AI buying (Trello card [3/6]):
            //   * project chain consumption × buffer_turns to compute gaps;
            //   * stop bidding when treasury would drop below the floor;
            //   * honor `auto_trade_with_minors` (skip minor offers when off).
            let personality = crate::ai::common::get_personality(game, *gp_id);
            let treasury_floor = crate::ai::economy::trade_buy_treasury_floor(game, personality);
            let buffer_turns = crate::ai::economy::trade_buy_buffer_turns(game, personality);
            let bids = trade::generate_need_based_bids(
                nation,
                &game.world.nations,
                &offers,
                cargo_capacity,
                Money::dollars(treasury_floor),
                buffer_turns,
            );
            all_bids.extend(bids);
        }
    }

    // 2b. Generate minor nation goods bids and resolve them against GP stockpiles.
    // Each non-anarchic minor nation always wants to buy 1 unit of one manufactured
    // commodity (Material or Goods) per turn, chosen randomly but deterministically.
    {
        let minor_bid_seed =
            (current_turn.0 as u64).wrapping_mul(0xbf58476d1ce4e5b9) ^ 0x94d049bb133111eb;
        let buy_price = Money::dollars(game.game_data.game_config.minor_goods_buy_price);
        let minor_bids =
            trade::generate_minor_nation_goods_bids(&game.world.nations, buy_price, minor_bid_seed);

        for bid in &minor_bids {
            // Try to fill from human player first, then AI GPs
            let mut filled = false;

            // Check human player stock. Apply the same expansion reserve we
            // give AI GPs so auto-trade with minors doesn't drain lumber/steel
            // that the player needs for industrial expansion. The player can
            // still sell manually via the trade screen, and they can disable
            // this auto-trade entirely via `auto_trade_with_minors`.
            let (human_lumber_reserve, human_steel_reserve) = {
                // The human doesn't have a personality, so use the Balanced
                // tunables — they're the most conservative across all four
                // personality presets.
                let per_turn = crate::ai::economy::expansions_per_turn_target(
                    game,
                    crate::ai::common::AiPersonality::Balanced,
                );
                let buildings_factor = crate::ai::economy::expansion_reserve_buildings_factor(
                    game,
                    crate::ai::common::AiPersonality::Balanced,
                );
                crate::ai::economy::reserve_for_expansion(
                    game,
                    human_id,
                    per_turn,
                    buildings_factor,
                )
            };
            let has_stock = match bid.commodity {
                trade::ManufacturedCommodity::Material(m) => game
                    .get_nation(human_id)
                    .map(|n| {
                        let stock = n.economy.materials.get(&m).copied().unwrap_or(0);
                        let reserve = match m {
                            MaterialType::Lumber => human_lumber_reserve,
                            MaterialType::Steel => human_steel_reserve,
                            _ => 0,
                        };
                        stock.saturating_sub(reserve) >= bid.quantity
                    })
                    .unwrap_or(false),
                trade::ManufacturedCommodity::Goods(g) => game
                    .get_nation(human_id)
                    .map(|n| n.economy.goods.get(&g).copied().unwrap_or(0) >= bid.quantity)
                    .unwrap_or(false),
            };

            let player_allows_auto_trade = game
                .get_nation(human_id)
                .map(|n| n.economy.auto_trade_with_minors)
                .unwrap_or(true);

            if has_stock && player_allows_auto_trade {
                let revenue = Money::dollars(buy_price.as_dollars() * bid.quantity as i64);
                let commodity_label = match bid.commodity {
                    trade::ManufacturedCommodity::Material(m) => format!("{m}"),
                    trade::ManufacturedCommodity::Goods(g) => format!("{g:?}"),
                };
                if let Some(seller) = game.get_nation_mut(human_id) {
                    seller.economy.treasury += revenue;
                    match bid.commodity {
                        trade::ManufacturedCommodity::Material(m) => {
                            if let Some(s) = seller.economy.materials.get_mut(&m) {
                                *s = s.saturating_sub(bid.quantity);
                            }
                            report.stockpile_flows.auto_sold_materials.push((
                                human_id,
                                m,
                                bid.quantity,
                            ));
                        }
                        trade::ManufacturedCommodity::Goods(g) => {
                            if let Some(s) = seller.economy.goods.get_mut(&g) {
                                *s = s.saturating_sub(bid.quantity);
                            }
                            report.stockpile_flows.auto_sold_goods.push((
                                human_id,
                                g,
                                bid.quantity,
                            ));
                        }
                    }
                    seller.archives.goods_sales_revenue_dollars += revenue.as_dollars();
                    // Record in trade history: player sold manufactured goods to minor nation
                    seller
                        .archives
                        .trade_history
                        .push(trade::TradeHistoryEntry {
                            turn: current_turn,
                            partner: bid.buyer,
                            resource: ResourceType::Timber, // sentinel; commodity_label carries the real name
                            commodity_label: commodity_label.clone(),
                            quantity: bid.quantity,
                            total_cost: revenue,
                            bought: false,
                        });
                }
                // Record buyer-side entry for the minor nation (bought=true)
                if let Some(buyer) = game.get_nation_mut(bid.buyer) {
                    buyer.archives.trade_history.push(trade::TradeHistoryEntry {
                        turn: current_turn,
                        partner: human_id,
                        resource: ResourceType::Timber, // sentinel; commodity_label carries the real name
                        commodity_label: commodity_label.clone(),
                        quantity: bid.quantity,
                        total_cost: revenue,
                        bought: true,
                    });
                }
                // Minor nation buyers don't have tracked cash flows — payment
                // represents abstracted demand, not a real treasury deduction.
                // Record the revenue so cash-flow reconciliation accounts for it.
                report.goods_auto_sale_revenue.push((human_id, revenue));
                filled = true;
            }

            if !filled {
                // Try AI GP sellers (skip human, already checked).
                // AI GPs hold back lumber+steel for industrial expansion — see
                // `crate::ai::economy::reserve_for_expansion`. The auto-bid
                // resolver must respect that reserve, otherwise minor nations
                // can drain the entire steel/lumber stockpile every turn and
                // the AI never has materials available for `expand_building`
                // to pay for the next mill/factory tier.
                for gp_id in &gp_ids {
                    if *gp_id == human_id {
                        continue;
                    }
                    let (lumber_reserve, steel_reserve) = {
                        let personality = crate::ai::common::get_personality(game, *gp_id);
                        let per_turn =
                            crate::ai::economy::expansions_per_turn_target(game, personality);
                        let buildings_factor =
                            crate::ai::economy::expansion_reserve_buildings_factor(
                                game,
                                personality,
                            );
                        crate::ai::economy::reserve_for_expansion(
                            game,
                            *gp_id,
                            per_turn,
                            buildings_factor,
                        )
                    };
                    let gp_has_stock = match bid.commodity {
                        trade::ManufacturedCommodity::Material(m) => game
                            .get_nation(*gp_id)
                            .map(|n| {
                                let stock = n.economy.materials.get(&m).copied().unwrap_or(0);
                                let reserve = match m {
                                    MaterialType::Lumber => lumber_reserve,
                                    MaterialType::Steel => steel_reserve,
                                    _ => 0,
                                };
                                stock.saturating_sub(reserve) >= bid.quantity
                            })
                            .unwrap_or(false),
                        trade::ManufacturedCommodity::Goods(g) => game
                            .get_nation(*gp_id)
                            .map(|n| n.economy.goods.get(&g).copied().unwrap_or(0) >= bid.quantity)
                            .unwrap_or(false),
                    };
                    if gp_has_stock {
                        let revenue = Money::dollars(buy_price.as_dollars() * bid.quantity as i64);
                        let commodity_label = match bid.commodity {
                            trade::ManufacturedCommodity::Material(m) => format!("{m}"),
                            trade::ManufacturedCommodity::Goods(g) => format!("{g:?}"),
                        };
                        if let Some(seller) = game.get_nation_mut(*gp_id) {
                            seller.economy.treasury += revenue;
                            match bid.commodity {
                                trade::ManufacturedCommodity::Material(m) => {
                                    if let Some(s) = seller.economy.materials.get_mut(&m) {
                                        *s = s.saturating_sub(bid.quantity);
                                    }
                                }
                                trade::ManufacturedCommodity::Goods(g) => {
                                    if let Some(s) = seller.economy.goods.get_mut(&g) {
                                        *s = s.saturating_sub(bid.quantity);
                                    }
                                }
                            }
                            seller.archives.goods_sales_revenue_dollars += revenue.as_dollars();
                            // Record in trade history: AI GP sold manufactured goods to minor nation
                            seller
                                .archives
                                .trade_history
                                .push(trade::TradeHistoryEntry {
                                    turn: current_turn,
                                    partner: bid.buyer,
                                    resource: ResourceType::Timber, // sentinel; commodity_label carries the real name
                                    commodity_label: commodity_label.clone(),
                                    quantity: bid.quantity,
                                    total_cost: revenue,
                                    bought: false,
                                });
                        }
                        // Record buyer-side entry for the minor nation (bought=true)
                        if let Some(buyer) = game.get_nation_mut(bid.buyer) {
                            buyer.archives.trade_history.push(trade::TradeHistoryEntry {
                                turn: current_turn,
                                partner: *gp_id,
                                resource: ResourceType::Timber, // sentinel; commodity_label carries the real name
                                commodity_label,
                                quantity: bid.quantity,
                                total_cost: revenue,
                                bought: true,
                            });
                        }
                        // Minor nation buyers don't have tracked cash flows.
                        // Route through pending_ai_cash_income so finalize_cash_flow picks it up.
                        game.transient
                            .pending_ai_cash_income
                            .push((*gp_id, revenue));
                        break;
                    }
                }
            }
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
        let label = format!("{:?}", txn.resource);
        // Record for buyer (partner is seller)
        if let Some(buyer) = game.get_nation_mut(txn.buyer) {
            buyer.archives.trade_history.push(trade::TradeHistoryEntry {
                turn: current_turn,
                partner: txn.seller,
                resource: txn.resource,
                commodity_label: label.clone(),
                quantity: txn.quantity,
                total_cost: txn.total_cost,
                bought: true,
            });
        }
        // Record for seller (partner is buyer)
        if let Some(seller) = game.get_nation_mut(txn.seller) {
            seller
                .archives
                .trade_history
                .push(trade::TradeHistoryEntry {
                    turn: current_turn,
                    partner: txn.buyer,
                    resource: txn.resource,
                    commodity_label: label.clone(),
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
