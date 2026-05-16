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
    let minor_offer_seed = game.next_rng_u64();
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
            if human.resource_amount(order.resource) >= order.quantity && order.quantity > 0 {
                offers.push(trade::TradeOffer {
                    seller: human_id,
                    resource: order.resource,
                    quantity: order.quantity,
                    price_per_unit: trade::base_price(order.resource),
                });
            }
        }
    }
    // Extra market-archive rows from non-bid-pool flows (minor goods bids).
    let mut extra_market_rows: Vec<crate::game_state::MarketOfferRecord> = Vec::new();

    // 2. Generate bids: AI GPs use need-based auto-bids; the human-controlled
    //    GP uses manual buy orders. In observer mode the human seat is a
    //    viewpoint only — its nation is AI-controlled, so it must also use
    //    auto-bids (otherwise it never imports anything).
    let mut all_bids = Vec::new();

    for gp_id in &gp_ids {
        if *gp_id == human_id && !game.observer_mode {
            // Use player's manual buy orders instead of auto-generated bids.
            if let Some(human) = game.get_nation(*gp_id) {
                for order in &human.diplomacy.player_buy_orders {
                    if order.quantity == 0 {
                        continue;
                    }
                    all_bids.push(trade::TradeBid {
                        buyer: *gp_id,
                        resource: order.resource,
                        quantity: order.quantity,
                        max_price_per_unit: order.max_price_per_unit,
                    });
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
            //   * rank by import urgency (gap not coverable from own provinces)
            //     so resources without local supply bid first under cargo scarcity;
            //   * stop bidding when treasury would drop below the floor;
            //   * honor `auto_trade_with_minors` (skip minor offers when off).
            let personality = crate::ai::common::get_personality(game, *gp_id);
            let treasury_floor = crate::ai::economy::trade_buy_treasury_floor(game, personality);
            let buffer_turns = crate::ai::economy::trade_buy_buffer_turns(game, personality);
            // Per-turn own-province yield (local + remote, ungated by transport
            // — the buy-side is asking "what *can* my map supply", not "what
            // gets delivered this turn").
            let (local, remote) = crate::economy::current_collectable_resources(game, *gp_id);
            let mut own_yield: std::collections::BTreeMap<ResourceType, u32> =
                std::collections::BTreeMap::new();
            for (r, q) in local.into_iter().chain(remote.into_iter()) {
                *own_yield.entry(r).or_insert(0) += q;
            }
            let own_yield_vec: Vec<(ResourceType, u32)> = own_yield.into_iter().collect();
            let bids = trade::generate_need_based_bids(
                nation,
                &game.world.nations,
                &offers,
                &own_yield_vec,
                cargo_capacity,
                Money::dollars(treasury_floor),
                buffer_turns,
            );

            if game.ai_debug {
                let needs = trade::projected_resource_needs(nation);
                let total_offer_qty: u32 = offers.iter().map(|o| o.quantity).sum();
                let yield_for = |r: ResourceType| -> u32 {
                    own_yield_vec
                        .iter()
                        .find(|(rr, _)| *rr == r)
                        .map(|(_, q)| *q)
                        .unwrap_or(0)
                };
                let gap_summary: Vec<String> = needs
                    .iter()
                    .filter_map(|(r, per_turn)| {
                        let target = per_turn.saturating_mul(buffer_turns);
                        let stock = nation.resource_amount(*r);
                        let gap = target.saturating_sub(stock);
                        if gap == 0 {
                            None
                        } else {
                            let urgency = per_turn.saturating_sub(yield_for(*r));
                            let avail: u32 = offers
                                .iter()
                                .filter(|o| o.resource == *r && o.seller != nation.id)
                                .map(|o| o.quantity)
                                .sum();
                            Some(format!(
                                "{:?}(need={} stock={} gap={} urgency={} offer_qty={})",
                                r, per_turn, stock, gap, urgency, avail
                            ))
                        }
                    })
                    .collect();
                eprintln!(
                    "[TRADE:{}] turn={} cargo={} treasury=${} floor=${} buf={} \
                     offers_total={} bids={} | gaps: {}",
                    nation.name,
                    game.turn.0,
                    cargo_capacity,
                    nation.economy.treasury.as_dollars(),
                    treasury_floor,
                    buffer_turns,
                    total_offer_qty,
                    bids.len(),
                    if gap_summary.is_empty() {
                        "<none>".to_string()
                    } else {
                        gap_summary.join(", ")
                    }
                );
            }

            all_bids.extend(bids);
        }
    }

    // 2b. Each GP offers its manufactured-commodity surplus (stock - reserve)
    // to minor nations at the fixed `minor_goods_buy_price`. Minors are tried
    // in order of descending relationship with the seller; each minor has a
    // `minor_goods_skip_chance` chance of declining a given offer this turn.
    // If every minor skips, the offer goes unfilled and the surplus stays in
    // the GP's stockpile until next turn.
    {
        let mut rng_state = game.next_rng_u64().max(1);
        let buy_price = Money::dollars(game.game_data.game_config.minor_goods_buy_price);
        let skip_chance = game.game_data.game_config.minor_goods_skip_chance.min(100);

        // Eligible minor buyers: non-anarchic, non-integrated minor nations.
        let minor_ids: Vec<NationId> = game
            .world
            .nations
            .iter()
            .filter(|n| {
                !n.is_great_power()
                    && !n.diplomacy.is_in_anarchy
                    && n.diplomacy.integrated_by.is_none()
            })
            .map(|n| n.id)
            .collect();

        // Build (seller, commodity, quantity) offers from every GP's surplus.
        // Deterministic ordering: by GP id, then commodity enum order.
        let mut offers: Vec<trade::ManufacturedOffer> = Vec::new();
        for gp_id in &gp_ids {
            let personality = crate::ai::common::get_personality(game, *gp_id);
            // Human doesn't have a personality — use Balanced as the most
            // conservative preset for human reserves.
            let reserve_personality = if *gp_id == human_id {
                crate::ai::common::AiPersonality::Balanced
            } else {
                personality
            };
            let per_turn =
                crate::ai::economy::expansions_per_turn_target(game, reserve_personality);
            let buildings_factor =
                crate::ai::economy::expansion_reserve_buildings_factor(game, reserve_personality);
            let (lumber_reserve, steel_reserve) =
                crate::ai::economy::reserve_for_expansion(game, *gp_id, per_turn, buildings_factor);
            let (m_fabric_reserve, m_lumber_reserve, m_steel_reserve, _m_coal) =
                crate::ai::naval::merchant_navy_material_reserve(game, *gp_id);
            let arms_reserve_total: u32 = {
                let pending = game
                    .get_nation(*gp_id)
                    .map(|n| n.pending_recruits_arms_cost())
                    .unwrap_or(0);
                pending.saturating_add(crate::ai::economy::arms_sell_reserve(
                    game,
                    reserve_personality,
                ))
            };

            // The human player can disable auto-trade with minors entirely.
            if *gp_id == human_id {
                let allow = game
                    .get_nation(human_id)
                    .map(|n| n.economy.auto_trade_with_minors)
                    .unwrap_or(true);
                if !allow {
                    continue;
                }
            }

            let nation = match game.get_nation(*gp_id) {
                Some(n) => n,
                None => continue,
            };
            // Hold back the goods queued immigration this turn will consume.
            let cfg_immig = &game.game_data.game_config;
            let pending_immig = nation.economy.pending_immigration;
            let immig_canned_food_reserve =
                pending_immig.saturating_mul(cfg_immig.immigration_canned_food.max(0) as u32);
            let immig_clothing_reserve =
                pending_immig.saturating_mul(cfg_immig.immigration_clothing.max(0) as u32);
            let immig_furniture_reserve =
                pending_immig.saturating_mul(cfg_immig.immigration_furniture);

            // Physical-capacity floors: reserve 2× the factory's per-turn ceiling
            // so the AI keeps a one-turn buffer instead of liquidating finished
            // goods the moment production catches up. The "real demand"
            // (immigration / training) numbers above are tiny in the early game
            // and were getting drained immediately, blocking worker growth.
            let cap_of = |bt: crate::economy::BuildingType| -> u32 {
                nation
                    .economy
                    .buildings
                    .iter()
                    .find(|b| b.building_type == bt)
                    .map(|b| b.effective_capacity())
                    .unwrap_or(0)
            };
            let furniture_cap_floor = cap_of(crate::economy::BuildingType::FurnitureFactory) * 2;
            let clothing_cap_floor = cap_of(crate::economy::BuildingType::ClothingFactory) * 2;
            let armory_cap_floor = cap_of(crate::economy::BuildingType::Armory) * 2;
            let paper_cap_floor = cap_of(crate::economy::BuildingType::PaperFactory) * 2;

            // AI stockpile target for canned food: held back from sale on top
            // of immigration demand so the cannery isn't liquidated the moment
            // production catches up. Production-side aim lives in
            // `ai_set_production_targets`; without mirroring it here, the
            // surplus is offered to minors as soon as a single unit lands.
            let canned_food_stockpile_target: u32 = crate::ai::lua_bridge::get_personality_config(
                game,
                reserve_personality,
            )
            .as_ref()
            .and_then(|c| c.canned_food_stockpile_target)
            .unwrap_or(20);
            let canned_food_reserve_total = immig_canned_food_reserve
                .saturating_add(canned_food_stockpile_target);

            // Paper reserve: queued worker training + strategic floor for
            // tech research and emergency training, with a 2× factory-cap
            // floor so the chain has a one-turn buffer to back training.
            let pending_train_paper = nation
                .economy
                .pending_train_to_trained
                .saturating_mul(cfg_immig.train_to_trained_paper_cost)
                .saturating_add(
                    nation
                        .economy
                        .pending_train_to_expert
                        .saturating_mul(cfg_immig.train_to_expert_paper_cost),
                );
            let paper_reserve = pending_train_paper
                .saturating_add(cfg_immig.strategic_paper_reserve)
                .max(paper_cap_floor);

            // Chain-input reserve for Fabric: protect next turn's Clothing
            // Factory feed so we don't liquidate Fabric and stall the chain.
            // Sized to the planned ClothingFactory output × materials_per_good,
            // but clamped by the factory's physical capacity — a target of 8
            // with capacity 1 can never actually consume more than 2 fabric,
            // so reserving 16 would falsely flag stock as undersupplied.
            let clothing_cap = nation
                .economy
                .buildings
                .iter()
                .find(|b| b.building_type == crate::economy::BuildingType::ClothingFactory)
                .map(|b| b.effective_capacity())
                .unwrap_or(0);
            let fabric_chain_reserve = nation
                .economy
                .chain_targets
                .garment_factory
                .min(clothing_cap)
                .saturating_mul(cfg_immig.materials_per_good);

            for &commodity in trade::ALL_MANUFACTURED {
                let (stock, reserve) = match commodity {
                    trade::ManufacturedCommodity::Material(m) => {
                        let stock = nation.economy.materials.get(&m).copied().unwrap_or(0);
                        let reserve = match m {
                            MaterialType::Lumber => lumber_reserve.saturating_add(m_lumber_reserve),
                            MaterialType::Steel => steel_reserve.saturating_add(m_steel_reserve),
                            MaterialType::Fabric => {
                                m_fabric_reserve.saturating_add(fabric_chain_reserve)
                            }
                            MaterialType::CannedFood => canned_food_reserve_total,
                            MaterialType::Paper => paper_reserve,
                        };
                        (stock, reserve)
                    }
                    trade::ManufacturedCommodity::Goods(g) => {
                        let stock = nation.economy.goods.get(&g).copied().unwrap_or(0);
                        let reserve = match g {
                            GoodsType::Arms => arms_reserve_total.max(armory_cap_floor),
                            GoodsType::Clothing => immig_clothing_reserve.max(clothing_cap_floor),
                            GoodsType::Furniture => {
                                immig_furniture_reserve.max(furniture_cap_floor)
                            }
                            GoodsType::Hardware => 0,
                        };
                        (stock, reserve)
                    }
                };
                let surplus = stock.saturating_sub(reserve);
                if surplus > 0 {
                    offers.push(trade::ManufacturedOffer {
                        seller: *gp_id,
                        commodity,
                        quantity: surplus,
                        price_per_unit: buy_price,
                    });
                }
            }
        }

        // Resolve each offer: try minors in relationship-desc order, each
        // rolls a skip; first non-skipper takes the whole offer.
        for offer in &offers {
            // Sort minors by relationship score to the seller (desc). Ties
            // broken by NationId for stable determinism.
            let mut ordered_minors: Vec<(NationId, i32)> = minor_ids
                .iter()
                .map(|&mid| {
                    let score = game
                        .world
                        .diplomacy
                        .get_relation(offer.seller, mid)
                        .map(|r| r.score)
                        .unwrap_or(0);
                    (mid, score)
                })
                .collect();
            ordered_minors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.0.cmp(&b.0.0)));

            let mut taker: Option<NationId> = None;
            for (mid, _score) in &ordered_minors {
                // xorshift64 step for deterministic skip roll.
                rng_state ^= rng_state << 13;
                rng_state ^= rng_state >> 7;
                rng_state ^= rng_state << 17;
                let roll = (rng_state >> 32) as u32 % 100;
                if roll >= skip_chance {
                    taker = Some(*mid);
                    break;
                }
            }

            let buyer_id = match taker {
                Some(id) => id,
                None => continue, // every minor skipped — offer expires
            };

            let revenue = Money::dollars(offer.price_per_unit.as_dollars() * offer.quantity as i64);
            let commodity_label = match offer.commodity {
                trade::ManufacturedCommodity::Material(m) => format!("{m:?}"),
                trade::ManufacturedCommodity::Goods(g) => format!("{g:?}"),
            };

            if let Some(seller) = game.get_nation_mut(offer.seller) {
                seller.economy.treasury += revenue;
                match offer.commodity {
                    trade::ManufacturedCommodity::Material(m) => {
                        if let Some(s) = seller.economy.materials.get_mut(&m) {
                            *s = s.saturating_sub(offer.quantity);
                        }
                        if offer.seller == human_id {
                            report.stockpile_flows.auto_sold_materials.push((
                                offer.seller,
                                m,
                                offer.quantity,
                            ));
                        }
                    }
                    trade::ManufacturedCommodity::Goods(g) => {
                        if let Some(s) = seller.economy.goods.get_mut(&g) {
                            *s = s.saturating_sub(offer.quantity);
                        }
                        if offer.seller == human_id {
                            report.stockpile_flows.auto_sold_goods.push((
                                offer.seller,
                                g,
                                offer.quantity,
                            ));
                        }
                    }
                }
                seller.archives.goods_sales_revenue_dollars += revenue.as_dollars();
                seller
                    .archives
                    .trade_history
                    .push(trade::TradeHistoryEntry {
                        turn: current_turn,
                        partner: buyer_id,
                        resource: ResourceType::Timber, // sentinel; commodity_label carries the real name
                        commodity_label: commodity_label.clone(),
                        quantity: offer.quantity,
                        total_cost: revenue,
                        bought: false,
                    });
            }
            if let Some(buyer) = game.get_nation_mut(buyer_id) {
                buyer.archives.trade_history.push(trade::TradeHistoryEntry {
                    turn: current_turn,
                    partner: offer.seller,
                    resource: ResourceType::Timber,
                    commodity_label: commodity_label.clone(),
                    quantity: offer.quantity,
                    total_cost: revenue,
                    bought: true,
                });
            }
            // Cash-flow accounting differs for human vs AI:
            //   - Human's treasury delta is captured by goods_auto_sale_revenue
            //     (read by finalize_cash_flow).
            //   - AI GPs route through pending_ai_cash_income so the same
            //     finalize step counts the inflow without double-applying it.
            if offer.seller == human_id {
                report.goods_auto_sale_revenue.push((offer.seller, revenue));
            } else {
                game.transient
                    .pending_ai_cash_income
                    .push((offer.seller, revenue));
            }
            extra_market_rows.push(crate::game_state::MarketOfferRecord {
                seller: offer.seller,
                resource: ResourceType::Timber,
                commodity_label,
                offered: offer.quantity,
                price_per_unit: offer.price_per_unit,
                fills: vec![crate::game_state::MarketFillRecord {
                    buyer: buyer_id,
                    quantity: offer.quantity,
                    price_per_unit: offer.price_per_unit,
                }],
            });
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
    let trade_per_resource = game
        .game_data
        .game_config
        .trade_relation_improvement_per_resource;
    let trade_interval = game.game_data.game_config.trade_relation_turn_interval;
    let apply_trade_improvement = trade_interval > 0 && game.turn.0.is_multiple_of(trade_interval);
    if apply_trade_improvement {
        for ((buyer, seller), resources) in &trade_pairs {
            // Only improve relations if a trade consulate exists between the nations.
            if game.world.diplomacy.has_consulate(*buyer, *seller) {
                let improvement = (resources.len() as i32 * trade_per_resource).min(trade_cap);
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

    // 8a. Archive per-turn market activity for the "Historical Market" UI:
    //     every offer (seller × resource) plus the per-buyer fills it received.
    //     Bounded to the last MARKET_ARCHIVE_DEPTH turns to keep saves small.
    {
        use crate::game_state::{
            MARKET_ARCHIVE_DEPTH, MarketFillRecord, MarketOfferRecord, MarketTurnRecord,
        };
        // Aggregate offers by (seller, resource). Multiple offers from the
        // same minor for the same resource collapse into one row so the UI
        // shows one line per supplier.
        type OfferKey = (NationId, ResourceType);
        let mut offer_rows: std::collections::BTreeMap<OfferKey, MarketOfferRecord> =
            std::collections::BTreeMap::new();
        for offer in &offers {
            let entry = offer_rows
                .entry((offer.seller, offer.resource))
                .or_insert_with(|| MarketOfferRecord {
                    seller: offer.seller,
                    resource: offer.resource,
                    commodity_label: format!("{:?}", offer.resource),
                    offered: 0,
                    price_per_unit: offer.price_per_unit,
                    fills: Vec::new(),
                });
            entry.offered = entry.offered.saturating_add(offer.quantity);
        }
        for txn in transactions {
            // Aggregate fills per buyer per offer-row.
            let entry = offer_rows
                .entry((txn.seller, txn.resource))
                .or_insert_with(|| MarketOfferRecord {
                    seller: txn.seller,
                    resource: txn.resource,
                    commodity_label: format!("{:?}", txn.resource),
                    offered: 0,
                    price_per_unit: txn.price_per_unit,
                    fills: Vec::new(),
                });
            if let Some(fill) = entry.fills.iter_mut().find(|f| f.buyer == txn.buyer) {
                fill.quantity = fill.quantity.saturating_add(txn.quantity);
            } else {
                entry.fills.push(MarketFillRecord {
                    buyer: txn.buyer,
                    quantity: txn.quantity,
                    price_per_unit: txn.price_per_unit,
                });
            }
        }
        let mut all_rows: Vec<MarketOfferRecord> = offer_rows.into_values().collect();
        all_rows.extend(extra_market_rows);
        if !all_rows.is_empty() {
            let record = MarketTurnRecord { offers: all_rows };
            game.archive.market_archive.push((current_turn, record));
            // Bound to MARKET_ARCHIVE_DEPTH most recent turns.
            let len = game.archive.market_archive.len();
            if len > MARKET_ARCHIVE_DEPTH {
                game.archive
                    .market_archive
                    .drain(0..len - MARKET_ARCHIVE_DEPTH);
            }
        }
    }

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
