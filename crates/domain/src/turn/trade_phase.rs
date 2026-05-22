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
                    commodity: trade::Commodity::Resource(order.resource),
                    quantity: order.quantity,
                    price_per_unit: trade::base_price(order.resource),
                });
            }
        }
    }
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
                        commodity: trade::Commodity::Resource(order.resource),
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
                                .filter(|o| {
                                    o.commodity == trade::Commodity::Resource(*r)
                                        && o.seller != nation.id
                                })
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
    // into the SAME unified offer pool that `resolve_trades_with_preference`
    // consumes, priced at the fixed `minor_goods_buy_price`. In the same loop,
    // each AI GP (and the observer-mode human seat) also places BUY bids for
    // the 5 intermediate materials it is short of — sequenced after its
    // resource bids so the shared cargo capacity and treasury floor are
    // respected. Minors become bidders for the manufactured offers via the
    // `generate_minor_manufactured_bids` helper.
    {
        let buy_price = Money::dollars(game.game_data.game_config.minor_goods_buy_price);

        // Build (seller, commodity, quantity) surplus offers from every GP.
        // Deterministic ordering: by GP id, then commodity enum order.
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
            let canned_food_stockpile_target: u32 =
                crate::ai::lua_bridge::get_personality_config(game, reserve_personality)
                    .as_ref()
                    .and_then(|c| c.canned_food_stockpile_target)
                    .unwrap_or(20);
            let canned_food_reserve_total =
                immig_canned_food_reserve.saturating_add(canned_food_stockpile_target);

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

            let reserve_for_material = |m: MaterialType| -> u32 {
                match m {
                    MaterialType::Lumber => lumber_reserve.saturating_add(m_lumber_reserve),
                    MaterialType::Steel => steel_reserve.saturating_add(m_steel_reserve),
                    MaterialType::Fabric => m_fabric_reserve.saturating_add(fabric_chain_reserve),
                    MaterialType::CannedFood => canned_food_reserve_total,
                    MaterialType::Paper => paper_reserve,
                }
            };
            for &commodity in trade::ALL_MANUFACTURED {
                let (stock, reserve) = match commodity {
                    trade::ManufacturedCommodity::Material(m) => {
                        let stock = nation.economy.materials.get(&m).copied().unwrap_or(0);
                        (stock, reserve_for_material(m))
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
                    let unified = match commodity {
                        trade::ManufacturedCommodity::Material(m) => trade::Commodity::Material(m),
                        trade::ManufacturedCommodity::Goods(g) => trade::Commodity::Goods(g),
                    };
                    offers.push(trade::TradeOffer {
                        seller: *gp_id,
                        commodity: unified,
                        quantity: surplus,
                        price_per_unit: buy_price,
                    });
                }
            }

            // GP BUY bids for the 5 intermediate materials it is short of.
            // The human-controlled (non-observer) seat uses manual orders and
            // is skipped here — same condition as the resource-bid loop.
            if *gp_id == human_id && !game.observer_mode {
                continue;
            }
            // Cargo + treasury floor are SHARED with this GP's resource bids:
            // sequence material bids after the resource bids already pushed in
            // §2 so the combined projected spend respects the floor.
            let cargo_capacity = blockade_capacity
                .get(gp_id)
                .copied()
                .unwrap_or_else(|| nation.total_cargo_capacity(&game.game_data));
            let resource_bid_qty: u32 = all_bids
                .iter()
                .filter(|b| b.buyer == *gp_id)
                .map(|b| b.quantity)
                .sum();
            let remaining_cargo = cargo_capacity.saturating_sub(resource_bid_qty);
            // Estimate the resource bids' spend at base price — the price minor
            // offers actually clear at — rather than their 120%-of-base max, so
            // the shared budget isn't over-stated and material bids aren't
            // suppressed unnecessarily.
            let resource_bid_spend: i64 = all_bids
                .iter()
                .filter(|b| b.buyer == *gp_id)
                .map(|b| {
                    let unit = match b.commodity {
                        trade::Commodity::Resource(r) => trade::base_price(r),
                        _ => b.max_price_per_unit,
                    };
                    unit.as_dollars() * b.quantity as i64
                })
                .sum();
            let treasury_floor = {
                let p = crate::ai::common::get_personality(game, *gp_id);
                crate::ai::economy::trade_buy_treasury_floor(game, p)
            };
            let cash_available =
                nation.economy.treasury.as_dollars() - treasury_floor - resource_bid_spend;
            // gap = reserve − stock. Trade runs after production, so `stock`
            // already reflects this turn's mill + town output.
            let materials: Vec<(MaterialType, u32, u32)> = [
                MaterialType::Lumber,
                MaterialType::Steel,
                MaterialType::Fabric,
                MaterialType::Paper,
                MaterialType::CannedFood,
            ]
            .into_iter()
            .map(|m| {
                let stock = nation.economy.materials.get(&m).copied().unwrap_or(0);
                (m, reserve_for_material(m), stock)
            })
            .collect();
            all_bids.extend(material_buy_bids(
                *gp_id,
                &materials,
                remaining_cargo,
                cash_available,
                buy_price,
            ));
        }

        // Minors bid for the manufactured commodities currently on offer,
        // preserving the legacy per-(minor, commodity) skip roll.
        let minor_bid_seed = game.next_rng_u64();
        let minor_bids = generate_minor_manufactured_bids(game, &offers, minor_bid_seed);
        all_bids.extend(minor_bids);
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

    // Subsidies are a Great-Power-only lever.
    for gp_id in &gp_ids {
        if let Some(nation) = game.get_nation(*gp_id) {
            for (target_id, amount) in &nation.diplomacy.trade_subsidies {
                subsidies_map.insert((*gp_id, *target_id), *amount);
            }
        }
    }
    // Relationship scores must cover EVERY bidder — Great Powers and minor
    // nations alike — so contested offers resolve by diplomacy for all buyers,
    // not just GPs (otherwise minor bids fall back to score 0).
    for bid in &all_bids {
        for offer in &offers {
            if bid.buyer == offer.seller {
                continue;
            }
            let key = (bid.buyer, offer.seller);
            if relationship_scores.contains_key(&key) {
                continue;
            }
            if let Some(rel) = game.world.diplomacy.get_relation(bid.buyer, offer.seller) {
                relationship_scores.insert(key, rel.score);
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

    // 5. Apply transactions. Treasury delta + stockpile changes are uniform
    //    across resources, materials, and goods; cash-flow reporting is handled
    //    downstream by `finalize_cash_flow` reading `report.trade_transactions`
    //    (TradeExportRevenue for the seller, TradePurchase for the buyer).
    for txn in &transactions {
        // Buyer pays money and receives the commodity into the right stockpile.
        if let Some(buyer) = game.get_nation_mut(txn.buyer) {
            buyer.economy.treasury -= txn.total_cost;
            buyer.add_commodity(txn.commodity, txn.quantity);
        }
        // Seller earns money and loses stock. For resource trades the only GP
        // sellers are the human (via player_sell_orders); for material/goods
        // trades any GP can be the seller — so deduct from every GP seller.
        if let Some(seller) = game.get_nation_mut(txn.seller) {
            seller.economy.treasury += txn.total_cost;
            let deduct = match txn.commodity {
                trade::Commodity::Resource(_) => txn.seller == human_id,
                trade::Commodity::Material(_) | trade::Commodity::Goods(_) => {
                    seller.is_great_power()
                }
            };
            if deduct {
                seller.remove_commodity(txn.commodity, txn.quantity);
            }
            if matches!(
                txn.commodity,
                trade::Commodity::Material(_) | trade::Commodity::Goods(_)
            ) && seller.is_great_power()
            {
                seller.archives.goods_sales_revenue_dollars += txn.total_cost.as_dollars();
            }
        }
    }

    // 5c. Record trade history for each nation involved.
    for txn in &transactions {
        let label = txn.commodity.to_string();
        // Record for buyer (partner is seller)
        if let Some(buyer) = game.get_nation_mut(txn.buyer) {
            buyer.archives.trade_history.push(trade::TradeHistoryEntry {
                turn: current_turn,
                partner: txn.seller,
                resource: match txn.commodity {
                    trade::Commodity::Resource(r) => r,
                    _ => ResourceType::Timber, // sentinel; commodity_label is authoritative
                },
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
                    resource: match txn.commodity {
                        trade::Commodity::Resource(r) => r,
                        _ => ResourceType::Timber,
                    },
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
        std::collections::BTreeSet<trade::Commodity>,
    > = std::collections::BTreeMap::new();
    for txn in &transactions {
        trade_pairs
            .entry((txn.buyer, txn.seller))
            .or_default()
            .insert(txn.commodity);
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
        // Aggregate offers by (seller, commodity). Multiple offers from the
        // same seller for the same commodity collapse into one row so the UI
        // shows one line per supplier.
        type OfferKey = (NationId, trade::Commodity);
        let mut offer_rows: std::collections::BTreeMap<OfferKey, MarketOfferRecord> =
            std::collections::BTreeMap::new();
        for offer in &offers {
            let entry = offer_rows
                .entry((offer.seller, offer.commodity))
                .or_insert_with(|| MarketOfferRecord {
                    seller: offer.seller,
                    commodity: offer.commodity,
                    commodity_label: offer.commodity.to_string(),
                    offered: 0,
                    price_per_unit: offer.price_per_unit,
                    fills: Vec::new(),
                });
            entry.offered = entry.offered.saturating_add(offer.quantity);
        }
        for txn in transactions {
            // Aggregate fills per buyer per offer-row.
            let entry = offer_rows
                .entry((txn.seller, txn.commodity))
                .or_insert_with(|| MarketOfferRecord {
                    seller: txn.seller,
                    commodity: txn.commodity,
                    commodity_label: txn.commodity.to_string(),
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
        let all_rows: Vec<MarketOfferRecord> = offer_rows.into_values().collect();
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

    // 8b. Update persistent market state (#164): record per-commodity supply,
    //     demand, sold — now spanning resources, materials, and goods.
    {
        use trade::Commodity;
        let mut supply_map: std::collections::BTreeMap<Commodity, u32> =
            std::collections::BTreeMap::new();
        let mut demand_map: std::collections::BTreeMap<Commodity, u32> =
            std::collections::BTreeMap::new();
        let mut sold_map: std::collections::BTreeMap<Commodity, u32> =
            std::collections::BTreeMap::new();
        let mut price_sum_map: std::collections::BTreeMap<Commodity, (i64, u32)> =
            std::collections::BTreeMap::new(); // (total_price * qty, total_qty)

        for offer in &offers {
            *supply_map.entry(offer.commodity).or_insert(0) += offer.quantity;
        }
        for bid in &all_bids {
            *demand_map.entry(bid.commodity).or_insert(0) += bid.quantity;
        }
        for txn in transactions {
            *sold_map.entry(txn.commodity).or_insert(0) += txn.quantity;
            let (ps, pq) = price_sum_map.entry(txn.commodity).or_insert((0, 0));
            *ps += txn.price_per_unit.as_dollars() * txn.quantity as i64;
            *pq += txn.quantity;
        }

        // Union all commodities that appeared in offers or bids this turn
        let mut all_commodities: std::collections::BTreeSet<Commodity> =
            std::collections::BTreeSet::new();
        all_commodities.extend(supply_map.keys().copied());
        all_commodities.extend(demand_map.keys().copied());

        for commodity in all_commodities {
            let supply = supply_map.get(&commodity).copied().unwrap_or(0);
            let demand = demand_map.get(&commodity).copied().unwrap_or(0);
            let sold = sold_map.get(&commodity).copied().unwrap_or(0);
            let price = if let Some(&(ps, pq)) = price_sum_map.get(&commodity)
                && pq > 0
            {
                Money::dollars(ps / pq as i64)
            } else {
                match commodity {
                    Commodity::Resource(r) => trade::base_price(r),
                    Commodity::Material(_) | Commodity::Goods(_) => {
                        Money::dollars(game.game_data.game_config.minor_goods_buy_price)
                    }
                }
            };
            game.world.market_state.record_tick(
                commodity,
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

/// Compute a Great Power's material buy bids for the trade phase.
///
/// For each importable material (in the fixed deterministic order given by
/// `materials`), bid `reserve − stock` units — trade resolves after the
/// production phase, so `stock` already reflects this turn's mill + town
/// output. Bids are capped by the shared remaining cargo capacity and the
/// available cash budget; once either is exhausted no further bids are made.
///
/// `materials` holds `(material, reserve, stock)` triples. Pure (no
/// `GameState` access) so the gap/budget arithmetic is unit-testable.
fn material_buy_bids(
    buyer: NationId,
    materials: &[(MaterialType, u32, u32)],
    mut remaining_cargo: u32,
    mut cash_available: i64,
    buy_price: Money,
) -> Vec<trade::TradeBid> {
    let unit_price = buy_price.as_dollars().max(1);
    let mut bids = Vec::new();
    for &(m, reserve, stock) in materials {
        if remaining_cargo == 0 || cash_available <= 0 {
            break;
        }
        let gap = reserve.saturating_sub(stock);
        if gap == 0 {
            continue;
        }
        let affordable = (cash_available / unit_price).clamp(0, u32::MAX as i64) as u32;
        let qty = gap.min(remaining_cargo).min(affordable);
        if qty == 0 {
            continue;
        }
        bids.push(trade::TradeBid {
            buyer,
            commodity: trade::Commodity::Material(m),
            quantity: qty,
            max_price_per_unit: buy_price,
        });
        remaining_cargo -= qty;
        cash_available -= unit_price * qty as i64;
    }
    bids
}

/// Build buy bids for Minor Nations against the manufactured-commodity offers
/// (materials + goods) currently on the market.
///
/// This is a self-contained replacement for the legacy "skip-roll" resolution
/// loop: instead of resolving each offer directly to a minor, every eligible
/// minor places a `TradeBid` for the full offered quantity of each manufactured
/// commodity, and the unified `resolve_trades_with_preference` matcher decides
/// the winner by buyer↔seller relationship. The legacy per-(minor, commodity)
/// skip roll is preserved: a "skip" means the minor places no bid for that
/// commodity this turn. Keeping it self-contained makes the old behavior easy
/// to restore.
///
/// `buyer` is the minor's `NationId`, which preserves minor-vs-GP identity for
/// downstream code (recoverable via `Nation::is_great_power`).
fn generate_minor_manufactured_bids(
    game: &GameState,
    offers: &[trade::TradeOffer],
    seed: u64,
) -> Vec<trade::TradeBid> {
    let human_id = game.human_player_nation;
    let skip_chance = game.game_data.game_config.minor_goods_skip_chance.min(100);

    // The human GP can disable auto-trade with minors entirely; when off,
    // minors must not bid on the human's manufactured offers.
    let human_allows_minors = game
        .get_nation(human_id)
        .map(|n| n.economy.auto_trade_with_minors)
        .unwrap_or(true);

    // Eligible minor buyers: non-anarchic, non-integrated minor nations.
    let minor_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| {
            !n.is_great_power() && !n.diplomacy.is_in_anarchy && n.diplomacy.integrated_by.is_none()
        })
        .map(|n| n.id)
        .collect();

    // Only manufactured (material/goods) offers are bid on by minors.
    let manufactured: Vec<&trade::TradeOffer> = offers
        .iter()
        .filter(|o| {
            matches!(
                o.commodity,
                trade::Commodity::Material(_) | trade::Commodity::Goods(_)
            )
        })
        .collect();

    let mut bids = Vec::new();
    let mut rng_state = seed.max(1);
    // Deterministic iteration: minors (already in nation order) × offers (in
    // the order they were pushed — GP id, then commodity).
    for &mid in &minor_ids {
        for offer in &manufactured {
            if offer.seller == human_id && !human_allows_minors {
                continue;
            }
            // xorshift64 step for the deterministic per-(minor, commodity) skip.
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            let roll = (rng_state >> 32) as u32 % 100;
            if roll < skip_chance {
                continue; // minor skips this commodity this turn
            }
            bids.push(trade::TradeBid {
                buyer: mid,
                commodity: offer.commodity,
                quantity: offer.quantity,
                max_price_per_unit: offer.price_per_unit,
            });
        }
    }
    bids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplomacy::DiplomacyState;
    use crate::game_state::GameState;
    use crate::hex::HexCoord;
    use crate::map::tile::Tile;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};

    /// Minimal game: one Great Power (id 1, human seat) and one Minor (id 10).
    fn game_with_gp_and_minor(skip_chance: u32) -> GameState {
        let coord = HexCoord::new(0, 0);
        let mut hex_map = HexMap::new(10, 10);
        let tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord, tile);

        let province = Province::new(
            ProvinceId(1),
            "Homeland".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );

        let gp = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        let minor = Nation::new(
            NationId(10),
            "Smallville".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut game_data = crate::data::test_game_data();
        game_data.game_config.minor_goods_skip_chance = skip_chance;

        crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province],
        nations: vec![gp, minor],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: game_data,
        diplomacy: DiplomacyState::new(),
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        pending_landings: Vec::new(),
        history: Vec::new(),
        high_scores: Vec::new(),
        newspaper_archive: Vec::new(),
        battle_archive: Vec::new(),
        political_archive: Vec::new(),
        ai_debug: false,
        observer_mode: false,
        last_cash_flow: std::collections::HashMap::new(),
        last_resource_flow: std::collections::HashMap::new(),
        pending_ai_cash_spending: Vec::new(),
        pending_ai_cash_income: Vec::new(),
        next_unit_id: 6_000_000,}
    }

    #[test]
    fn minor_bids_for_each_manufactured_offer_when_never_skipping() {
        let game = game_with_gp_and_minor(0);
        let offers = vec![
            trade::TradeOffer {
                seller: NationId(1),
                commodity: trade::Commodity::Material(MaterialType::Steel),
                quantity: 4,
                price_per_unit: Money::dollars(150),
            },
            trade::TradeOffer {
                seller: NationId(1),
                commodity: trade::Commodity::Goods(GoodsType::Arms),
                quantity: 2,
                price_per_unit: Money::dollars(150),
            },
        ];
        let bids = generate_minor_manufactured_bids(&game, &offers, 12345);
        // skip_chance 0 ⇒ the lone minor bids on both manufactured offers,
        // for the full offered quantity, tagged with its own NationId.
        assert_eq!(bids.len(), 2);
        assert!(bids.iter().all(|b| b.buyer == NationId(10)));
        let steel = bids
            .iter()
            .find(|b| b.commodity == trade::Commodity::Material(MaterialType::Steel))
            .unwrap();
        assert_eq!(steel.quantity, 4);
        let arms = bids
            .iter()
            .find(|b| b.commodity == trade::Commodity::Goods(GoodsType::Arms))
            .unwrap();
        assert_eq!(arms.quantity, 2);
    }

    #[test]
    fn minor_bids_ignore_raw_resource_offers() {
        let game = game_with_gp_and_minor(0);
        // A raw-resource offer must never produce a minor manufactured bid.
        let offers = vec![trade::TradeOffer {
            seller: NationId(1),
            commodity: trade::Commodity::Resource(ResourceType::Coal),
            quantity: 5,
            price_per_unit: Money::dollars(75),
        }];
        let bids = generate_minor_manufactured_bids(&game, &offers, 99);
        assert!(bids.is_empty());
    }

    #[test]
    fn minor_bids_resolve_against_a_gp_manufactured_offer() {
        // End-to-end on the matcher: a minor bid generated by the helper
        // matches a GP's material offer in `resolve_trades`.
        let game = game_with_gp_and_minor(0);
        let offers = vec![trade::TradeOffer {
            seller: NationId(1),
            commodity: trade::Commodity::Material(MaterialType::Lumber),
            quantity: 3,
            price_per_unit: Money::dollars(150),
        }];
        let bids = generate_minor_manufactured_bids(&game, &offers, 7);
        let txns = trade::resolve_trades(&offers, &bids);
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].buyer, NationId(10));
        assert_eq!(txns[0].seller, NationId(1));
        assert_eq!(
            txns[0].commodity,
            trade::Commodity::Material(MaterialType::Lumber)
        );
        assert_eq!(txns[0].quantity, 3);
    }

    #[test]
    fn minor_does_not_bid_on_human_offers_when_auto_trade_disabled() {
        let mut game = game_with_gp_and_minor(0);
        // NationId(1) is the human seat; disabling auto_trade_with_minors
        // must stop minors bidding on its manufactured offers.
        game.world.nations[0].economy.auto_trade_with_minors = false;
        let offers = vec![trade::TradeOffer {
            seller: NationId(1),
            commodity: trade::Commodity::Material(MaterialType::Steel),
            quantity: 4,
            price_per_unit: Money::dollars(150),
        }];
        let bids = generate_minor_manufactured_bids(&game, &offers, 55);
        assert!(bids.is_empty());
    }

    // ── material_buy_bids ───────────────────────────────────────────

    const BUY_PRICE: Money = Money::dollars(150);

    #[test]
    fn material_buy_bids_no_bid_when_stock_meets_reserve() {
        // stock >= reserve for every material → gap is 0 → no bids.
        let materials = vec![
            (MaterialType::Lumber, 20, 20),
            (MaterialType::Steel, 20, 25),
            (MaterialType::Fabric, 10, 10),
        ];
        let bids = material_buy_bids(NationId(1), &materials, 999, 1_000_000, BUY_PRICE);
        assert!(bids.is_empty());
    }

    #[test]
    fn material_buy_bids_bids_exact_gap_when_unconstrained() {
        let materials = vec![
            (MaterialType::Steel, 20, 5),  // gap 15
            (MaterialType::Paper, 8, 8),   // gap 0 — skipped
            (MaterialType::Fabric, 12, 9), // gap 3
        ];
        let bids = material_buy_bids(NationId(1), &materials, 999, 1_000_000, BUY_PRICE);
        assert_eq!(bids.len(), 2);
        assert_eq!(bids[0].commodity, trade::Commodity::Material(MaterialType::Steel));
        assert_eq!(bids[0].quantity, 15);
        assert_eq!(bids[0].buyer, NationId(1));
        assert_eq!(bids[1].commodity, trade::Commodity::Material(MaterialType::Fabric));
        assert_eq!(bids[1].quantity, 3);
    }

    #[test]
    fn material_buy_bids_capped_by_remaining_cargo() {
        // First material's gap (15) exceeds the 10 units of cargo left, so it
        // takes all of it and nothing remains for the second.
        let materials = vec![
            (MaterialType::Steel, 20, 5),
            (MaterialType::Fabric, 12, 0),
        ];
        let bids = material_buy_bids(NationId(1), &materials, 10, 1_000_000, BUY_PRICE);
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0].quantity, 10);
    }

    #[test]
    fn material_buy_bids_capped_by_available_cash() {
        // Budget only affords 4 units at $150 apiece.
        let materials = vec![(MaterialType::Steel, 20, 5)];
        let bids = material_buy_bids(NationId(1), &materials, 999, 600, BUY_PRICE);
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0].quantity, 4);
    }

    #[test]
    fn material_buy_bids_no_bid_when_budget_nonpositive() {
        let materials = vec![(MaterialType::Steel, 20, 0)];
        let bids = material_buy_bids(NationId(1), &materials, 999, 0, BUY_PRICE);
        assert!(bids.is_empty());
    }

    #[test]
    fn minor_bids_resolve_deterministically_across_multiple_sellers() {
        // Two GP sellers offer the same commodity. Minor bids are commodity-keyed
        // (seller-agnostic by design); confirm resolution is deterministic, fills
        // from both sellers, and never over-allocates.
        let game = game_with_gp_and_minor(0); // skip_chance 0 → minor always bids
        let offers = vec![
            trade::TradeOffer {
                seller: NationId(11),
                commodity: trade::Commodity::Material(MaterialType::Steel),
                quantity: 4,
                price_per_unit: Money::dollars(150),
            },
            trade::TradeOffer {
                seller: NationId(12),
                commodity: trade::Commodity::Material(MaterialType::Steel),
                quantity: 3,
                price_per_unit: Money::dollars(150),
            },
        ];
        let bids = generate_minor_manufactured_bids(&game, &offers, 42);
        // The lone minor (id 10) bids once per offer when it never skips.
        assert_eq!(bids.len(), 2);
        assert!(bids.iter().all(|b| b.buyer == NationId(10)));

        // Same seed → identical bid set (determinism).
        let bids_again = generate_minor_manufactured_bids(&game, &offers, 42);
        assert_eq!(bids.len(), bids_again.len());

        // Resolution fills from both sellers, total exactly the offered 7 units.
        let rel = std::collections::HashMap::new();
        let subs = std::collections::HashMap::new();
        let txns = trade::resolve_trades_with_preference(&offers, &bids, &rel, &subs);
        let total: u32 = txns.iter().map(|t| t.quantity).sum();
        assert_eq!(total, 7);
        assert!(txns.iter().all(|t| t.buyer == NationId(10)));
    }
}
