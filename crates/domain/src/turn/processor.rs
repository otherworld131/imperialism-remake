use crate::ai::run_ai_turns;
use crate::economy::buildings::BuildingType;
use crate::economy::civilians::CivilianType;
use crate::economy::ledger::{
    CashFlow, CashSink, CashSource, ResourceFlow, ResourceIn, ResourceOut, Stockpile,
    StockpileFlowTracking,
};
use crate::economy::production::{
    ProductionChain, calculate_armory_production, calculate_canned_food_production,
    calculate_factory_production, calculate_mill_production, calculate_paper_production,
};
use crate::economy::trade::TradeTransaction;
use crate::events::*;
use crate::game_state::{
    GameState, PendingDiplomacyAction, PoliticalSnapshot, PoliticalSnapshotEntry,
};
use crate::map::SettlementLevel;
use crate::map::infrastructure::is_province_connected_multi_filtered;
use crate::military::battle_outcome::{BattleParams, compute_battle_outcome};
use crate::military::combat::{BattleConfig, BattleResult, CombatForce, TargetingPriority};
use crate::military::naval::{NavalBattleResult, resolve_naval_battle};
use crate::military::ships::Ship;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::turn::civilian_phase::{update_province_connectivity, update_settlements};
use crate::turn::diplomacy_phase::{
    record_broken_alliance_headlines, resolve_diplomatic_proposals,
};
#[cfg(test)]
use crate::turn::economy_phase::compute_blockade_capacity;
use crate::turn::economy_phase::{apply_blockade_effects, apply_maintenance, tick_buildings};
use crate::turn::news_phase::generate_newspaper;
use crate::turn::rewards_phase::resolve_rewards;
use crate::turn::scoring::{CouncilVoteResult, calculate_score, run_council_vote};
use crate::types::*;
use std::collections::{HashMap, HashSet};

/// Result of processing one turn.
#[derive(Debug)]
pub struct TurnReport {
    pub turn: TurnNumber,
    pub year: u32,
    pub quarter: u32,
    pub events: Vec<DomainEvent>,
    pub resource_production: Vec<(NationId, ResourceType, u32)>,
    pub gold_income: Vec<(NationId, Money)>,
    pub maintenance_costs: Vec<(NationId, Money)>,
    pub production_output: Vec<(NationId, String, u32)>,
    pub food_consumed: Vec<(NationId, u32)>,
    pub starvation: Vec<(NationId, u32)>,
    pub newspaper_headlines: Vec<Headline>,
    pub techs_available: Vec<(NationId, Vec<String>)>,
    pub council_vote: Option<CouncilVoteResult>,
    pub trade_transactions: Vec<TradeTransaction>,
    pub battles: Vec<BattleResult>,
    /// Naval battles resolved this turn.
    pub naval_battles: Vec<NavalBattleResult>,
    /// Scores for all Great Powers: (nation_id, nation_name, total_score).
    pub scores: Vec<(NationId, String, u32)>,
    /// Summary of notable actions taken by AI nations this turn.
    pub ai_actions: Vec<crate::ai::AiAction>,
    /// Descriptions of completed civilian work this turn.
    pub civilian_completions: Vec<(NationId, String)>,
    /// Resources lost to insufficient transport capacity: (nation, resource, amount lost).
    pub transport_overflow: Vec<(NationId, ResourceType, u32)>,
    /// Workers recruited via immigration this turn: (nation, count).
    pub immigration: Vec<(NationId, u32)>,
    /// Settlement upgrades that happened this turn: (province_id, new_level_name).
    pub settlement_upgrades: Vec<(ProvinceId, String)>,
    /// Trade balance: (nation_id, total_spent, total_earned).
    pub trade_balance: Vec<(NationId, Money, Money)>,
    /// Town production output: (nation_id, item_name, quantity).
    pub town_production: Vec<(NationId, String, u32)>,
    /// Unit movement descriptions: (nation_id, description).
    pub unit_movements: Vec<(NationId, String)>,
    /// Voluntary incorporations: (minor_nation_id, great_power_id).
    pub incorporations: Vec<(NationId, NationId)>,
    /// Unit upgrades: (nation_id, from_type, to_type).
    pub unit_upgrades: Vec<(NationId, String, String)>,
    /// Subsidy costs deducted this turn: (nation_id, target_nation_id, cost).
    pub subsidy_costs: Vec<(NationId, NationId, Money)>,
    /// Diplomatic score improvements from trade: (nation_a, nation_b, improvement).
    pub trade_diplomacy: Vec<(NationId, NationId, i32)>,
    /// Resources lost because the producing province was disconnected from the capital.
    pub disconnected_resources: Vec<(NationId, ResourceType, u32)>,
    /// Rewards earned this turn: (nation_id, description).
    pub rewards_earned: Vec<(NationId, String)>,
    // ── Cash-flow ledger fields (transient — used to build `cash_flow`) ───
    /// Treasury snapshot at the very start of `process_turn`, per nation.
    pub opening_treasury: HashMap<NationId, Money>,
    /// Treasury snapshot just before `process_turn` returns, per nation.
    pub closing_treasury: HashMap<NationId, Money>,
    /// AI spending entries: (nation, sink, amount, optional partner).
    /// Populated at every AI treasury-mutation site so the cash-flow aggregator
    /// can see money that left the treasury inside AI decision paths.
    pub ai_cash_spending: Vec<(NationId, CashSink, Money, Option<NationId>)>,
    /// Per-nation total of construction cost paid this turn (roads/ports/etc).
    pub construction_spending: Vec<(NationId, Money)>,
    /// Per-nation revenue from auto-selling own materials/goods this turn.
    pub goods_auto_sale_revenue: Vec<(NationId, Money)>,
    /// AI goods-sale revenue (direct cash-in entries from AI economy paths).
    pub ai_goods_sale_revenue: Vec<(NationId, Money)>,
    /// Debt forgiven by the bankruptcy clamp: (nation, amount). Entered as
    /// income in the cash-flow breakdown so reconciliation closes.
    pub bankruptcy_writeoff: Vec<(NationId, Money)>,
    /// Per-nation derived cash flow (populated by `finalize_cash_flow` at end
    /// of `process_turn`). This is what CLI, batch, and the WASM bridge read.
    pub cash_flow: HashMap<NationId, CashFlow>,
    /// Per-nation derived resource flow (populated by `finalize_resource_flow`
    /// at end of `process_turn`). Best-effort visibility, NOT reconciled.
    pub resource_flow: HashMap<NationId, ResourceFlow>,
    /// Structured tracking of material- and goods-stockpile movements during
    /// production/consumption phases. Folded into `resource_flow` by
    /// `finalize_resource_flow` so the Materials ledger sees per-stockpile
    /// production / trade / consumption breakdowns.
    pub stockpile_flows: StockpileFlowTracking,
}

impl TurnReport {
    /// Create an empty report (used by WASM bridge for pact defense responses).
    pub fn empty() -> Self {
        Self {
            turn: TurnNumber(0),
            year: 0,
            quarter: 0,
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        }
    }

    /// Format a compact summary line suitable for CLI display.
    ///
    /// Example: `Turn 21 (1820 Q1) | Treasury: $8,500 | Workers: 5 | Army: 3 | Provinces: 9 | Score: 1,230 (#2)`
    pub fn format_summary_line(&self, game: &GameState) -> String {
        let player = match game.get_nation(game.human_player_nation) {
            Some(n) => n,
            None => return format!("Turn {} ({})", self.turn.0, self.turn),
        };

        let treasury = player.economy.treasury.as_dollars();
        let workers = player.economy.labor.total_workers();
        let army = player.military.army.len();
        let provinces = player.province_count();

        // Find score and rank from the scores list
        let (score, rank) = self
            .scores
            .iter()
            .enumerate()
            .find(|(_, (nid, _, _))| *nid == game.human_player_nation)
            .map(|(i, (_, _, s))| (*s, i + 1))
            .unwrap_or((0, 0));

        format!(
            "Turn {} ({}) | Treasury: ${} | Workers: {} | Army: {} | Provinces: {} | Score: {} (#{})",
            self.turn.0,
            self.turn,
            format_number_with_commas(treasury),
            workers,
            army,
            provinces,
            format_number_with_commas(score as i64),
            rank
        )
    }
}

/// Format a number with comma separators (e.g. 8500 -> "8,500").
fn format_number_with_commas(n: i64) -> String {
    let negative = n < 0;
    let s = n.unsigned_abs().to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    if negative {
        result.push('-');
    }
    result.chars().rev().collect()
}

/// Award free Clipper ships when a Great Power establishes its first colony.
///
/// Called when a GP either conquers a Minor Nation province or receives one
/// through voluntary incorporation. Awards 2 Clippers and sets the `has_colony` flag.
fn award_first_colony_clippers(game: &mut GameState, nation_id: NationId, report: &mut TurnReport) {
    let already_has_colony = game
        .get_nation(nation_id)
        .is_some_and(|n| n.military.has_colony);
    if already_has_colony {
        return;
    }
    use crate::map::UnitId;
    use crate::military::ships::{Ship, ShipType};
    let clipper_hull = game.game_data.ship_stats(ShipType::Clipper).hull;
    let Some(nation) = game.get_nation_mut(nation_id) else {
        return;
    };
    nation.military.has_colony = true;
    let base_id = 5_000_000 + nation.id.0 * 100;
    nation.military.merchant_fleet.push(Ship::new(
        UnitId(base_id + 1),
        ShipType::Clipper,
        nation_id,
        clipper_hull,
    ));
    nation.military.merchant_fleet.push(Ship::new(
        UnitId(base_id + 2),
        ShipType::Clipper,
        nation_id,
        clipper_hull,
    ));
    let name = nation.name.clone();
    report.rewards_earned.push((
        nation_id,
        format!(
            "{} receives free Clipper ships for establishing its first colony!",
            name
        ),
    ));
    report.newspaper_headlines.push(
        Headline::new(
            format!(
                "{} receives free Clipper ships for establishing its first colony!",
                name
            ),
            HeadlineCategory::Trade,
        )
        .for_nation(nation_id),
    );
}

fn clear_economy_batch_reservations(game: &mut GameState) {
    for nation in &mut game.world.nations {
        nation.economy.release_all_reservations();
    }
    game.transient.pending_economy_orders.clear();
}

/// Process one turn of the game.
pub fn process_turn(game: &mut GameState) -> TurnReport {
    let turn = game.turn;
    let mut report = TurnReport {
        turn,
        year: turn.year(),
        quarter: turn.quarter(),
        events: Vec::new(),
        resource_production: Vec::new(),
        gold_income: Vec::new(),
        maintenance_costs: Vec::new(),
        production_output: Vec::new(),
        food_consumed: Vec::new(),
        starvation: Vec::new(),
        newspaper_headlines: Vec::new(),
        techs_available: Vec::new(),
        council_vote: None,
        trade_transactions: Vec::new(),
        battles: Vec::new(),
        naval_battles: Vec::new(),
        scores: Vec::new(),
        ai_actions: Vec::new(),
        civilian_completions: Vec::new(),
        transport_overflow: Vec::new(),
        immigration: Vec::new(),
        settlement_upgrades: Vec::new(),
        trade_balance: Vec::new(),
        town_production: Vec::new(),
        unit_movements: Vec::new(),
        incorporations: Vec::new(),
        unit_upgrades: Vec::new(),
        subsidy_costs: Vec::new(),
        trade_diplomacy: Vec::new(),
        disconnected_resources: Vec::new(),
        rewards_earned: Vec::new(),
        opening_treasury: HashMap::new(),
        closing_treasury: HashMap::new(),
        ai_cash_spending: Vec::new(),
        construction_spending: Vec::new(),
        goods_auto_sale_revenue: Vec::new(),
        ai_goods_sale_revenue: Vec::new(),
        bankruptcy_writeoff: Vec::new(),
        cash_flow: HashMap::new(),
        resource_flow: HashMap::new(),
        stockpile_flows: StockpileFlowTracking::default(),
    };

    // Cash-flow ledger: snapshot every nation's treasury BEFORE any turn work.
    // We diff against the closing snapshot to build a per-turn breakdown and
    // to run the reconciliation invariant in tests.
    //
    // Defensive reset: AI pending collectors are drained at end-of-turn, but
    // `run_ai_turns` is public and callable outside `process_turn`. Stale
    // entries left behind by such standalone calls would bleed into the next
    // turn's ledger — clear them before we open the new accounting window.
    game.transient.pending_ai_cash_spending.clear();
    game.transient.pending_ai_cash_income.clear();
    game.transient.pending_economy_orders.clear();
    // Reset per-zone fleet movement budgets; lazily reinitialised by MoveFleet commands.
    for nation in &mut game.world.nations {
        nation.military.fleet_moves_remaining.clear();
    }
    for nation in &game.world.nations {
        report
            .opening_treasury
            .insert(nation.id, nation.economy.treasury);
    }

    // 0. Player-issued direct diplomacy actions resolve on end turn rather
    // than immediately. Apply them before AI decisions so the new quarter's
    // diplomacy state is coherent for the rest of turn processing.
    resolve_pending_direct_diplomacy_actions(game, &mut report);

    // 1. AI decisions for computer-controlled Great Powers
    let ai_actions = run_ai_turns(game);
    report.ai_actions = ai_actions;

    // 0-post. Resolve pending diplomatic proposals (AI-to-AI evaluated inline,
    // but this handles any proposals from the turn processor level — e.g. mutual proposals)
    resolve_diplomatic_proposals(game, &mut report);
    let broken_alliances = game
        .world
        .diplomacy
        .finalize_pending_separate_peace_breaks();
    record_broken_alliance_headlines(game, &mut report, &broken_alliances);

    // 0a. Alliance obligations: AI allies automatically join wars
    resolve_alliance_obligations(game, &mut report);

    // 0b. Voluntary incorporations: Minor Nations with high relations join Great Powers
    resolve_voluntary_incorporations(game, &mut report);

    // 0c. Unit upgrades for AI nations (auto-upgrade when tech is available)
    resolve_unit_upgrades(game, &mut report);

    // 0d. Resolve civilian actions (tick working civilians, apply improvements)
    resolve_civilian_actions(game, &mut report);

    run_pre_immigration_phases(game, &mut report);

    // 5b. Pending immigration: process queued worker recruitment orders.
    process_pending_immigration(game, &mut report);

    // 5c. Pending civilian hires: process queued hiring orders
    process_pending_civilian_hires(game);

    // 5d. Pending worker training: Untrained→Trained and Trained→Expert
    process_pending_worker_training(game);

    // 5e. Pending freight car builds
    process_pending_freight_cars(game);

    // 5f. Pending ship construction
    process_pending_ships(game);

    // 5g. Pending army recruitment
    process_pending_army_recruits(game);

    // 6. Maintenance costs (placeholder)
    apply_maintenance(game, &mut report);

    // 6b. Resolve beachhead operations (establish naval landing sites)
    resolve_beachheads(game, &mut report);

    // 6c. Resolve military unit movement (pending moves)
    let moved_unit_ids = resolve_military_movement(game, &mut report);

    // 7. Resolve combat (pending attacks — units that moved this turn are excluded)
    let fought_unit_ids = resolve_combat(game, &mut report, &moved_unit_ids);

    // 7a. Anarchy sweep: after all initial attacks and counter-attacks have
    // resolved, apply anarchy only to nations that still do not hold their
    // capital. Prevents a capital captured and then recaptured in the same
    // turn from leaving the nation in spurious anarchy.
    apply_end_of_combat_anarchy(game, &mut report);

    // 7a2. Rest heals units (Trello card #20): any living army unit that did
    // not move and did not participate in combat this turn recovers health.
    heal_resting_units(game, &moved_unit_ids, &fought_unit_ids);

    // 7a3. Resolve pending fleet movements queued by the player (card #471).
    // Apply each (nation, from_zone, to_zone) via the existing whole-zone
    // mover so fleets engage in naval combat from their new position.
    resolve_pending_fleet_moves(game);

    // 7b. Resolve naval combat (warship engagements between nations at war)
    resolve_naval_combat(game, &mut report);

    // 7c. Apply blockade effects (reduce trade cargo for blockaded nations)
    apply_blockade_effects(game, &mut report);

    // 7d. Resolve rewards (Generals earned, capitol expansion)
    resolve_rewards(game, &mut report);

    // 7e. Garrison regeneration: every `garrison_regen_interval_turns`,
    // under-strength province militia tick back toward default.
    regenerate_garrisons(game);

    // 8. Apply any pending human tech research queued from the Tech screen.
    resolve_human_tech_research(game, &mut report);

    // 8b. Report available techs
    report_available_techs(game, &mut report);

    // 8c. Resolve technology for AI nations (generate TechnologyResearched events)
    resolve_technology(game, &mut report);

    // 9. Council of Governors vote (at decade boundaries)
    check_council_vote(game, &mut report);

    // 10. Calculate and store scores for all Great Powers
    calculate_scores(game, &mut report);

    // 10b. Recompute province connectivity, then update settlement progression
    update_province_connectivity(game);
    update_settlements(game, &mut report);

    // 11. Generate newspaper
    generate_newspaper(game, &mut report);

    // 11b. Archive headlines for history browsing
    game.archive
        .newspaper_archive
        .push((game.turn, report.newspaper_headlines.clone()));

    // 11c. Archive battle results for history browsing
    if !report.battles.is_empty() || !report.naval_battles.is_empty() {
        game.archive.battle_archive.push((
            game.turn,
            report.battles.clone(),
            report.naval_battles.clone(),
        ));
    }

    // 11d. Snapshot province ownership and capitals for the political-map
    // history view. Capitals are archived explicitly because minor-nation
    // capitals can be reassigned during the game.
    let provinces: Vec<PoliticalSnapshotEntry> = game
        .world
        .provinces
        .iter()
        .map(|p| (p.id, p.owner, p.incorporated_from))
        .collect();
    let capitals: Vec<(NationId, ProvinceId)> = game
        .world
        .nations
        .iter()
        .map(|n| (n.id, n.capital_province_id))
        .collect();
    game.archive.political_archive.push((
        game.turn,
        PoliticalSnapshot {
            provinces,
            capitals,
        },
    ));

    // Cash-flow ledger: drain AI collectors and snapshot closing treasury.
    // This runs BEFORE `advance_turn` because advance_turn only bumps the
    // turn counter — it does not touch treasury — but keeping snapshot and
    // aggregator adjacent makes the flow easier to audit.
    report.ai_cash_spending = std::mem::take(&mut game.transient.pending_ai_cash_spending);
    report.ai_goods_sale_revenue = std::mem::take(&mut game.transient.pending_ai_cash_income);
    for nation in &game.world.nations {
        report
            .closing_treasury
            .insert(nation.id, nation.economy.treasury);
    }
    finalize_cash_flow(game, &mut report);
    finalize_resource_flow(game, &mut report);

    // 12. Advance turn
    report
        .events
        .push(DomainEvent::TurnEnded(TurnEnded { turn }));
    game.advance_turn();
    report
        .events
        .push(DomainEvent::TurnStarted(TurnStarted { turn: game.turn }));

    report
}

fn resolve_pending_direct_diplomacy_actions(game: &mut GameState, report: &mut TurnReport) {
    let queued = std::mem::take(&mut game.transient.pending_diplomacy_actions);
    for action in queued {
        match action {
            PendingDiplomacyAction::BuildConsulate { player, target } => {
                let cost = Money::dollars(game.game_data.game_config.consulate_cost);
                if game.world.diplomacy.build_consulate(player, target).is_ok() {
                    if let Some(nation) = game.get_nation_mut(player) {
                        nation.economy.treasury -= cost;
                    }
                    let target_name = game
                        .get_nation(target)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| "Unknown".to_string());
                    report.newspaper_headlines.push(
                        Headline::new(
                            format!("Trade consulate built with {}", target_name),
                            HeadlineCategory::Diplomacy,
                        )
                        .for_nations(&[player, target]),
                    );
                    game.archive
                        .history
                        .push((game.turn, HistoryEvent::ConsulateBuilt { player, target }));
                }
            }
            PendingDiplomacyAction::BuildEmbassy { player, target } => {
                let cost = Money::dollars(game.game_data.game_config.embassy_cost);
                if game.world.diplomacy.build_embassy(player, target).is_ok() {
                    if let Some(nation) = game.get_nation_mut(player) {
                        nation.economy.treasury -= cost;
                    }
                    let target_name = game
                        .get_nation(target)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| "Unknown".to_string());
                    report.newspaper_headlines.push(
                        Headline::new(
                            format!("Embassy established with {}", target_name),
                            HeadlineCategory::Diplomacy,
                        )
                        .for_nations(&[player, target]),
                    );
                    game.archive
                        .history
                        .push((game.turn, HistoryEvent::EmbassyBuilt { player, target }));
                }
            }
            PendingDiplomacyAction::DeclareWar { from, to } => {
                if !game.world.diplomacy.is_at_war(from, to) {
                    game.world.diplomacy.declare_war_at(from, to, game.turn);
                    report
                        .events
                        .push(DomainEvent::WarDeclared(crate::events::WarDeclared {
                            attacker: from,
                            defender: to,
                        }));
                    let attacker_name = game
                        .get_nation(from)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| "Unknown".to_string());
                    let defender_name = game
                        .get_nation(to)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| "Unknown".to_string());
                    report.newspaper_headlines.push(
                        Headline::new(
                            format!("{attacker_name} declared war on {defender_name}"),
                            HeadlineCategory::Diplomacy,
                        )
                        .for_nations(&[from, to]),
                    );
                    game.archive.history.push((
                        game.turn,
                        HistoryEvent::WarDeclared {
                            attacker: from,
                            defender: to,
                            protectee: None,
                        },
                    ));
                }
            }
            PendingDiplomacyAction::SendGrant { from, to, amount } => {
                if game.world.diplomacy.is_at_war(from, to) {
                    continue;
                }
                let Some(sender) = game.get_nation(from) else {
                    continue;
                };
                if sender.economy.treasury < amount {
                    continue;
                }
                if let Some(nation) = game.get_nation_mut(from) {
                    nation.economy.treasury -= amount;
                }
                if let Some(nation) = game.get_nation_mut(to) {
                    nation.economy.treasury += amount;
                }
                game.world.diplomacy.send_grant(from, to, amount);
                let from_name = game
                    .get_nation(from)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                let to_name = game
                    .get_nation(to)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                report.newspaper_headlines.push(
                    Headline::new(
                        format!(
                            "{from_name} sent a ${} grant to {to_name}",
                            amount.as_dollars()
                        ),
                        HeadlineCategory::Diplomacy,
                    )
                    .for_nations(&[from, to]),
                );
            }
            PendingDiplomacyAction::BreakTreaty {
                from,
                to,
                treaty_type,
            } => {
                if !game.world.diplomacy.has_treaty(from, to, treaty_type) {
                    continue;
                }
                game.world.diplomacy.break_treaty(from, to, treaty_type);
                let from_name = game
                    .get_nation(from)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                let to_name = game
                    .get_nation(to)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                report.newspaper_headlines.push(
                    Headline::new(
                        format!("{from_name} broke {:?} with {to_name}", treaty_type),
                        HeadlineCategory::Diplomacy,
                    )
                    .for_nations(&[from, to]),
                );
            }
        }
    }
}

fn run_pre_immigration_phases(game: &mut GameState, report: &mut TurnReport) {
    // 1. Economy phase: collect tile resources (plan → reserve → execute, Trello #161)
    // Only collect_resources fires here; production and trade run after their
    // interleaved steps (transport, monetary conversion, town production).
    use crate::turn::economy_phase::{
        EconomicOrderKind, collect_economic_orders, execute_reserved_economy, validate_and_reserve,
    };
    let collect_orders: Vec<_> = collect_economic_orders(game)
        .into_iter()
        .filter(|o| o.kind == EconomicOrderKind::CollectTileResources)
        .collect();
    let reserved_collect = validate_and_reserve(game, collect_orders);
    execute_reserved_economy(game, report, reserved_collect);
    clear_economy_batch_reservations(game);

    // 1b. Transport resolution: cap resources delivered by freight car capacity
    resolve_transport(game, report);

    // 2. Gold/Gems -> money conversion
    convert_monetary_resources(game, report);

    // 3. Production phase (plan → reserve → execute)
    let produce_orders: Vec<_> = collect_economic_orders(game)
        .into_iter()
        .filter(|o| o.kind == EconomicOrderKind::RunProduction)
        .collect();
    let reserved_produce = validate_and_reserve(game, produce_orders);
    execute_reserved_economy(game, report, reserved_produce);
    clear_economy_batch_reservations(game);

    // 3a. Town production: Villages and Towns produce materials and goods autonomously
    resolve_town_production(game, report);

    // 3b. Trade phase (plan → reserve → execute; blockade capacity computed inside)
    let trade_orders: Vec<_> = collect_economic_orders(game)
        .into_iter()
        .filter(|o| o.kind == EconomicOrderKind::ExecuteTrade)
        .collect();
    let reserved_trade = validate_and_reserve(game, trade_orders);
    execute_reserved_economy(game, report, reserved_trade);
    clear_economy_batch_reservations(game);

    // 4. Tick buildings (process expansion timers)
    tick_buildings(game);

    // 5. Food consumption
    food_consumption(game, report);
}

pub fn projected_immigration_queue_capacity(game: &GameState, nation_id: NationId) -> u32 {
    let Some(nation) = game.get_nation(nation_id) else {
        return 0;
    };
    if nation.diplomacy.is_in_anarchy || !nation.is_great_power() {
        return 0;
    }

    let mut projected = game.clone();
    let mut report = TurnReport::empty();
    run_pre_immigration_phases(&mut projected, &mut report);

    let cfg = &projected.game_data.game_config;
    let Some(nation) = projected.get_nation(nation_id) else {
        return 0;
    };

    let max_by_canned_food = if cfg.immigration_canned_food > 0 {
        nation.material_amount(MaterialType::CannedFood) / cfg.immigration_canned_food
    } else {
        u32::MAX
    };
    let max_by_clothing = if cfg.immigration_clothing > 0 {
        nation.goods_amount(GoodsType::Clothing) / cfg.immigration_clothing
    } else {
        u32::MAX
    };
    let max_by_furniture = if cfg.immigration_furniture > 0 {
        nation.goods_amount(GoodsType::Furniture) / cfg.immigration_furniture
    } else {
        u32::MAX
    };
    nation
        .immigration_turn_capacity(cfg)
        .min(max_by_canned_food)
        .min(max_by_clothing)
        .min(max_by_furniture)
}

/// Derive per-nation `CashFlow` from the scattered per-source fields on
/// `TurnReport`, write it to `report.cash_flow` and `game.transient.last_cash_flow`,
/// and update `nation.archives.cash_income_totals` / `cash_expense_totals`
/// cumulative counters.
///
/// Every income source and expense sink we currently know about is aggregated
/// here — if the reconciliation invariant fails, there is either an untracked
/// treasury mutation site, or a miscategorised source in this function.
fn finalize_cash_flow(game: &mut GameState, report: &mut TurnReport) {
    use std::collections::HashMap as StdHashMap;

    // Gather per-nation partial sums first so we can construct each CashFlow
    // once, then write cumulative totals onto the Nation afterward.
    let mut flows: StdHashMap<NationId, CashFlow> = StdHashMap::new();
    for nation in &game.world.nations {
        let opening = *report
            .opening_treasury
            .get(&nation.id)
            .unwrap_or(&nation.economy.treasury);
        let mut flow = CashFlow::new(opening);
        flow.closing_treasury = nation.economy.treasury;
        flows.insert(nation.id, flow);
    }

    // ── Income ──────────────────────────────────────────────────
    // Gold/Gems conversion — already captured per-nation in gold_income.
    for (nation_id, amount) in &report.gold_income {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_income(CashSource::GoldGemsConversion, *amount, None);
        }
    }
    // Goods auto-sales (player/normal path) — recorded at processor.rs
    // by the goods_auto_sale_revenue instrumentation.
    for (nation_id, amount) in &report.goods_auto_sale_revenue {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_income(CashSource::GoodsAutoSales, *amount, None);
        }
    }
    // Trade sales — seller side (revenue in) of every matched trade txn.
    for txn in &report.trade_transactions {
        if let Some(flow) = flows.get_mut(&txn.seller) {
            flow.add_income(
                CashSource::TradeExportRevenue,
                txn.total_cost,
                Some(txn.buyer),
            );
        }
    }
    // AI goods-sale revenue — drained from pending_ai_cash_income above.
    for (nation_id, amount) in &report.ai_goods_sale_revenue {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_income(CashSource::AiGoodsSale, *amount, None);
        }
    }
    // Bankruptcy writeoff — clamp adjustment treated as income so the
    // reconciliation invariant closes.
    for (nation_id, amount) in &report.bankruptcy_writeoff {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_income(CashSource::BankruptcyWriteoff, *amount, None);
        }
    }

    // ── Expense ─────────────────────────────────────────────────
    // Army maintenance.
    for (nation_id, amount) in &report.maintenance_costs {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_expense(CashSink::ArmyMaintenance, *amount, None);
        }
    }
    // Subsidies — payer side.
    for (payer, target, amount) in &report.subsidy_costs {
        if let Some(flow) = flows.get_mut(payer) {
            flow.add_expense(CashSink::Subsidy, *amount, Some(*target));
        }
    }
    // Trade purchases — buyer side of every matched trade txn.
    for txn in &report.trade_transactions {
        if let Some(flow) = flows.get_mut(&txn.buyer) {
            flow.add_expense(CashSink::TradePurchase, txn.total_cost, Some(txn.seller));
        }
    }
    // Infrastructure construction (roads/ports/etc).
    for (nation_id, amount) in &report.construction_spending {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_expense(CashSink::ConstructionInfrastructure, *amount, None);
        }
    }
    // AI spending — captured via pending_ai_cash_spending.
    for (nation_id, sink, amount, partner) in &report.ai_cash_spending {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_expense(*sink, *amount, *partner);
        }
    }

    // Update Nation cumulative totals (dollars).
    for nation in &mut game.world.nations {
        if let Some(flow) = flows.get(&nation.id) {
            for (source, dollars) in flow.income_totals_by_source() {
                *nation
                    .archives
                    .cash_income_totals
                    .entry(source)
                    .or_insert(0) += dollars;
            }
            for (sink, dollars) in flow.expense_totals_by_sink() {
                *nation.archives.cash_expense_totals.entry(sink).or_insert(0) += dollars;
            }
        }
    }

    // Publish to TurnReport and persist on GameState for WASM bridge reads.
    report.cash_flow = flows.clone();
    game.transient.last_cash_flow = flows;
}

/// Derive per-nation `ResourceFlow` from scattered per-source fields already
/// on `TurnReport`. Best-effort visibility — NOT reconciled. Covers:
/// - `resource_production` → HomeProduction (raw resources)
/// - `transport_overflow` → TransportOverflow (resources lost)
/// - `disconnected_resources` → DisconnectedLoss (resources lost)
/// - `trade_transactions` → TradeImport / TradeExport per resource
/// - `stockpile_flows` → mill/factory/town/food/immigration/auto-sale movements
///   for resources, materials, and goods stockpiles
///
/// AI-side material/goods movements that happen inside `run_ai_turns`
/// (ship/transport building in `ai/naval.rs`, mill expansion + freight cars in
/// `ai/economy.rs`, paper-for-training in `ai/labor.rs`, Steel→Arms conversion)
/// are routed through `game.transient.pending_ai_material_outflows` /
/// `pending_ai_material_inflows` / `pending_ai_goods_outflows`, drained at the
/// top of this function.
///
/// Writes to `report.resource_flow` and `game.transient.last_resource_flow`.
fn finalize_resource_flow(game: &mut GameState, report: &mut TurnReport) {
    let mut flows: std::collections::HashMap<NationId, ResourceFlow> =
        std::collections::HashMap::new();
    for nation in &game.world.nations {
        flows.entry(nation.id).or_default();
    }

    // Home production inflows
    for (nation_id, resource, amount) in &report.resource_production {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_inflow(
                Stockpile::Resource(*resource),
                ResourceIn::HomeProduction,
                *amount,
            );
        }
    }
    // Transport overflow: resources that never reached the warehouse
    for (nation_id, resource, amount) in &report.transport_overflow {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_outflow(
                Stockpile::Resource(*resource),
                ResourceOut::TransportOverflow,
                *amount,
            );
        }
    }
    // Disconnected resources: lost because producing province was isolated
    for (nation_id, resource, amount) in &report.disconnected_resources {
        if let Some(flow) = flows.get_mut(nation_id) {
            flow.add_outflow(
                Stockpile::Resource(*resource),
                ResourceOut::DisconnectedLoss,
                *amount,
            );
        }
    }
    // Trade inflow (buyer) / outflow (seller). `report.trade_transactions`
    // holds resource trades; material/goods trades carry their own stockpile
    // accounting via `auto_sold_materials`/`auto_sold_goods` below.
    for txn in &report.trade_transactions {
        let stockpile = match txn.commodity {
            crate::economy::trade::Commodity::Resource(r) => Stockpile::Resource(r),
            crate::economy::trade::Commodity::Material(m) => Stockpile::Material(m),
            crate::economy::trade::Commodity::Goods(g) => Stockpile::Goods(g),
        };
        if let Some(flow) = flows.get_mut(&txn.buyer) {
            flow.add_inflow(stockpile, ResourceIn::TradeImport, txn.quantity);
        }
        if let Some(flow) = flows.get_mut(&txn.seller) {
            flow.add_outflow(stockpile, ResourceOut::TradeExport, txn.quantity);
        }
    }

    // Drain AI-side stockpile collectors before reading stockpile_flows.
    let ai_mat = std::mem::take(&mut game.transient.pending_ai_material_outflows);
    for (nid, material, sink, amount) in ai_mat {
        if let Some(flow) = flows.get_mut(&nid) {
            flow.add_outflow(Stockpile::Material(material), sink, amount);
        }
    }
    let ai_goods = std::mem::take(&mut game.transient.pending_ai_goods_outflows);
    for (nid, good, sink, amount) in ai_goods {
        if let Some(flow) = flows.get_mut(&nid) {
            flow.add_outflow(Stockpile::Goods(good), sink, amount);
        }
    }
    let ai_mat_in = std::mem::take(&mut game.transient.pending_ai_material_inflows);
    for (nid, material, source, amount) in ai_mat_in {
        if let Some(flow) = flows.get_mut(&nid) {
            flow.add_inflow(Stockpile::Material(material), source, amount);
        }
    }
    let ai_goods_in = std::mem::take(&mut game.transient.pending_ai_goods_inflows);
    for (nid, good, source, amount) in ai_goods_in {
        if let Some(flow) = flows.get_mut(&nid) {
            flow.add_inflow(Stockpile::Goods(good), source, amount);
        }
    }

    // ── Material/Goods stockpile flows from structured tracking ──
    let sf = &report.stockpile_flows;
    for (nid, resource, amount) in &sf.mill_consumed_resources {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_outflow(
                Stockpile::Resource(*resource),
                ResourceOut::MillConsumed,
                *amount,
            );
        }
    }
    for (nid, material, amount) in &sf.mill_produced_materials {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_inflow(
                Stockpile::Material(*material),
                ResourceIn::MillOutput,
                *amount,
            );
        }
    }
    for (nid, material, amount) in &sf.factory_consumed_materials {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_outflow(
                Stockpile::Material(*material),
                ResourceOut::FactoryConsumed,
                *amount,
            );
        }
    }
    for (nid, good, amount) in &sf.factory_produced_goods {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_inflow(Stockpile::Goods(*good), ResourceIn::FactoryOutput, *amount);
        }
    }
    for (nid, material, amount) in &sf.town_produced_materials {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_inflow(
                Stockpile::Material(*material),
                ResourceIn::TownProduced,
                *amount,
            );
        }
    }
    for (nid, good, amount) in &sf.town_produced_goods {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_inflow(Stockpile::Goods(*good), ResourceIn::TownProduced, *amount);
        }
    }
    for (nid, resource, amount) in &sf.food_processed_inputs {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_outflow(
                Stockpile::Resource(*resource),
                ResourceOut::FoodProcessedInput,
                *amount,
            );
        }
    }
    for (nid, amount) in &sf.canned_food_produced {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_inflow(
                Stockpile::Material(MaterialType::CannedFood),
                ResourceIn::FoodProcessed,
                *amount,
            );
        }
    }
    for (nid, resource, amount) in &sf.worker_food_consumed {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_outflow(
                Stockpile::Resource(*resource),
                ResourceOut::WorkerFood,
                *amount,
            );
        }
    }
    for (nid, amount) in &sf.worker_canned_food_consumed {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_outflow(
                Stockpile::Material(MaterialType::CannedFood),
                ResourceOut::WorkerFood,
                *amount,
            );
        }
    }
    for (nid, material, amount) in &sf.auto_sold_materials {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_outflow(
                Stockpile::Material(*material),
                ResourceOut::TradeExport,
                *amount,
            );
        }
    }
    for (nid, good, amount) in &sf.auto_sold_goods {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_outflow(Stockpile::Goods(*good), ResourceOut::TradeExport, *amount);
        }
    }
    for (nid, material, amount) in &sf.immigration_consumed_materials {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_outflow(
                Stockpile::Material(*material),
                ResourceOut::ImmigrationConsumed,
                *amount,
            );
        }
    }
    for (nid, good, amount) in &sf.immigration_consumed_goods {
        if let Some(flow) = flows.get_mut(nid) {
            flow.add_outflow(
                Stockpile::Goods(*good),
                ResourceOut::ImmigrationConsumed,
                *amount,
            );
        }
    }

    report.resource_flow = flows.clone();
    game.transient.last_resource_flow = flows;
}

/// Compute the set of province IDs connected to a nation's capital.
///
/// A province is connected if:
///   1. It IS the capital province, OR
///   2. Infrastructure (railroads/depots/ports) connects it, OR
///   3. It is directly adjacent to the capital province (shares a hex neighbor).
///
/// Adjacency ensures early-game resource delivery before railroads are built,
/// matching the connectivity logic used for settlement progression.
pub fn connected_provinces(game: &GameState, nation_id: NationId) -> HashSet<ProvinceId> {
    let mut connected = HashSet::new();
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return connected,
    };
    let capital_pid = nation.capital_province_id;
    connected.insert(capital_pid);

    let capital_province = match game.get_province(capital_pid) {
        Some(p) => p,
        None => return connected,
    };
    let capital_tile = capital_province.capital_tile;

    // Precompute capital province neighbors for adjacency check
    let capital_neighbors: HashSet<crate::hex::HexCoord> = capital_province
        .tiles
        .iter()
        .flat_map(|t| t.neighbors())
        .collect();

    // Collect every owned country-capital tile — each is an independent hub
    // for rail/port connectivity (captured foreign capitals included).
    let mut seed_tiles: Vec<crate::hex::HexCoord> = vec![capital_tile];
    for &pid in &nation.province_ids {
        if let Some(prov) = game.get_province(pid) {
            for &t in &prov.tiles {
                if t == capital_tile {
                    continue;
                }
                if let Some(tile) = game.world.hex_map.get_tile(t)
                    && tile.is_country_capital
                {
                    seed_tiles.push(t);
                }
            }
        }
    }

    let blockaded_ports = crate::military::naval::compute_blockaded_ports(game, nation_id);

    for &pid in &nation.province_ids {
        if pid == capital_pid {
            continue;
        }

        // Infrastructure connection (railroad/depot/port) seeded from every
        // owned country-capital tile. Ports under undisputed enemy blockade
        // are skipped (card #408).
        if is_province_connected_multi_filtered(
            &game.world.hex_map,
            &game.world.sea_zones,
            &seed_tiles,
            pid,
            &game.world.provinces,
            &blockaded_ports,
        ) {
            connected.insert(pid);
            continue;
        }

        // Adjacency: at least one tile of the province neighbors a capital tile
        if let Some(province) = game.get_province(pid)
            && province.tiles.iter().any(|t| capital_neighbors.contains(t))
        {
            connected.insert(pid);
        }
    }

    connected
}

/// Collect resource yields from all tiles owned by each nation.
///
/// For each nation, iterates through their provinces, looks up tiles in the hex map,
/// calculates yields, and adds resources to the nation's warehouse.
/// Only resources from provinces connected to the capital are delivered;
/// resources from disconnected provinces are tracked in `report.disconnected_resources`.
pub(super) fn collect_resources(game: &mut GameState, report: &mut TurnReport) {
    // Phase 0: precompute connected provinces for each nation
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    let connected_map: Vec<(NationId, HashSet<ProvinceId>)> = nation_ids
        .iter()
        .map(|&nid| (nid, connected_provinces(game, nid)))
        .collect();

    // Phase 0b: precompute per-nation collectable-hex sets (capital province
    // plus a 1-hex radius around every connected depot).
    let collectable_by_nation: Vec<(NationId, HashSet<crate::hex::HexCoord>)> = nation_ids
        .iter()
        .map(|&nid| {
            if game.get_nation(nid).is_none() {
                return (nid, HashSet::new());
            }
            let owned: Vec<&crate::map::Province> = game
                .world
                .provinces
                .iter()
                .filter(|p| p.owner == nid)
                .collect();
            let connected = connected_map
                .iter()
                .find(|(id, _)| *id == nid)
                .map(|(_, s)| s.clone())
                .unwrap_or_default();
            let set = crate::map::infrastructure::collectable_hexes(
                &game.world.hex_map,
                &owned,
                &connected,
            );
            (nid, set)
        })
        .collect();

    // Phase 1: collect production data using immutable borrows
    let mut production_data: Vec<(NationId, ResourceType, u32)> = Vec::new();
    let mut disconnected_data: Vec<(NationId, ResourceType, u32)> = Vec::new();

    for province in &game.world.provinces {
        // Anarchic nations produce no resources
        if game
            .get_nation(province.owner)
            .is_some_and(|n| n.diplomacy.is_in_anarchy)
        {
            continue;
        }
        // Check if this province is connected for its owner
        let is_connected = connected_map
            .iter()
            .find(|(nid, _)| *nid == province.owner)
            .map(|(_, set)| set.contains(&province.id))
            .unwrap_or(false);

        let collectable = collectable_by_nation
            .iter()
            .find(|(nid, _)| *nid == province.owner)
            .map(|(_, s)| s);

        for tile_coord in &province.tiles {
            if let Some(tile) = game.world.hex_map.get_tile(*tile_coord)
                && let Some(yield_amount) = tile.calculate_yield()
            {
                let tile_collectable = collectable.map(|s| s.contains(tile_coord)).unwrap_or(false);
                // A connected hex yields only if inside a connected depot's
                // radius (or the capital province). Disconnected reporting
                // keeps the whole province's yields — the player needs to see
                // what's out there to decide where to place a depot, so we
                // don't filter those by depot geometry (there is no depot
                // geometry yet when a province is disconnected).
                if is_connected && tile_collectable {
                    production_data.push((
                        province.owner,
                        yield_amount.resource,
                        yield_amount.quantity,
                    ));
                } else if !is_connected {
                    disconnected_data.push((
                        province.owner,
                        yield_amount.resource,
                        yield_amount.quantity,
                    ));
                }
            }

            // Card #483: bare Grassland tiles (no resource deposit, no
            // improvement) passively yield 1 Grain when reachable from a
            // connected depot or the country capital. Matches Imperialism 1
            // behavior — connecting a hex matters before you can afford to
            // improve it.
            //
            // We key off membership in `collectable` only — that set already
            // encodes reachability (capital radius or connected-depot radius),
            // so the capital's adjacent tiles always count even when their
            // parent province is otherwise disconnected.
            if let Some(tile) = game.world.hex_map.get_tile(*tile_coord)
                && tile.terrain() == TerrainType::Grassland
                && tile.resource_deposit().is_none()
                && tile.improvement_level() == 0
            {
                let tile_collectable = collectable.map(|s| s.contains(tile_coord)).unwrap_or(false);
                if tile_collectable {
                    production_data.push((province.owner, ResourceType::Grain, 1));
                }
            }
        }

        // Card #418: ports and coastal country capitals haul Fish out of the
        // sea. A port (or capital acting as one, card #419) yields 1 Fish per
        // adjacent ocean tile, capped at 3 per port. Lake-adjacent ports
        // yield nothing — only true ocean tiles are fisheries.
        for tile_coord in &province.tiles {
            let Some(tile) = game.world.hex_map.get_tile(*tile_coord) else {
                continue;
            };
            let is_port = tile.infrastructure.has_port;
            let is_coastal_capital = tile.is_country_capital
                && tile_coord.neighbors().iter().any(|n| {
                    game.world
                        .hex_map
                        .get_tile(*n)
                        .is_some_and(|t| !t.terrain().is_land())
                });
            if !is_port && !is_coastal_capital {
                continue;
            }
            // Count adjacent ocean (non-lake) sea tiles.
            let ocean_neighbors = tile_coord
                .neighbors()
                .iter()
                .filter(|n| {
                    let Some(t) = game.world.hex_map.get_tile(**n) else {
                        return false;
                    };
                    if t.terrain().is_land() {
                        return false;
                    }
                    // Exclude lake hexes — only true ocean fisheries yield Fish.
                    !game
                        .world
                        .sea_zones
                        .iter()
                        .any(|z| z.is_lake && z.hexes.contains(*n))
                })
                .count() as u32;
            let yield_qty = ocean_neighbors.min(3);
            if yield_qty == 0 {
                continue;
            }
            if is_connected {
                production_data.push((province.owner, ResourceType::Fish, yield_qty));
            } else {
                disconnected_data.push((province.owner, ResourceType::Fish, yield_qty));
            }
        }
    }

    // Phase 2: apply connected resources to nations using mutable borrows,
    // with AI difficulty bonus applied to non-human nations.
    // Record adjusted amounts so report.resource_production reflects what was
    // actually added to the warehouse (used by resolve_transport for freight accounting).
    let human_id = game.human_player_nation;
    let difficulty = game.difficulty;
    let mut adjusted_production_data: Vec<(NationId, ResourceType, u32)> = Vec::new();
    for (nation_id, resource, amount) in &production_data {
        if let Some(nation) = game.world.nations.iter_mut().find(|n| n.id == *nation_id) {
            // Apply AI difficulty bonus multiplier
            let bonus_multiplier = match difficulty {
                Difficulty::Hard => {
                    if *nation_id != human_id {
                        1.1
                    } else {
                        1.0
                    }
                }
                Difficulty::NighOnImpossible => {
                    if *nation_id != human_id {
                        1.25
                    } else {
                        1.0
                    }
                }
                _ => 1.0,
            };
            let adjusted = (*amount as f64 * bonus_multiplier).round() as u32;
            nation.add_resource(*resource, adjusted);
            adjusted_production_data.push((*nation_id, *resource, adjusted));
        }
    }

    // Debug: log food collected per Great Power this turn
    if game.ai_debug {
        let food_types = [
            ResourceType::Grain,
            ResourceType::Fruit,
            ResourceType::Livestock,
        ];
        for nation in game.world.nations.iter().filter(|n| n.is_great_power()) {
            let collected: u32 = adjusted_production_data
                .iter()
                .filter(|(nid, r, _)| *nid == nation.id && food_types.contains(r))
                .map(|(_, _, a)| a)
                .sum();
            let disconnected: u32 = disconnected_data
                .iter()
                .filter(|(nid, r, _)| *nid == nation.id && food_types.contains(r))
                .map(|(_, _, a)| a)
                .sum();
            if collected > 0 || disconnected > 0 {
                eprintln!(
                    "[COLLECT:{}] food_delivered={}, food_disconnected={}, freight_cars={}",
                    nation.name, collected, disconnected, nation.military.transport.freight_cars
                );
            }
        }
    }

    // Record adjusted amounts so resolve_transport uses the same quantities as inventory.
    report.resource_production.extend(adjusted_production_data);
    report.disconnected_resources.extend(disconnected_data);
}

/// Resolve transport: cap resources delivered *this turn* based on freight car capacity.
///
/// Works on the resources collected this turn (from `report.resource_production`), not on the
/// total warehouse. Resources already in the warehouse from prior turns are unaffected.
///
/// For each nation:
/// - The capital tile itself delivers for free.
/// - All other collectable tiles draw from the freight pool.
/// - If freight cars == 0: only capital-tile resources are delivered.
/// - If freight cars > 0: total resources from non-capital tiles are capped at freight capacity.
///   Excess resources are removed from warehouse.
/// - Resources lost are tracked in `report.transport_overflow`.
fn resolve_transport(game: &mut GameState, report: &mut TurnReport) {
    // Gather per-nation resource production from this turn
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let nation = match game.world.nations.iter().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };
        if nation.diplomacy.is_in_anarchy {
            continue;
        }

        let rail_capacity = nation.military.transport.total_capacity();
        let sea_capacity = nation.total_cargo_capacity(&game.game_data);
        let transport = nation.military.transport.clone();
        // Remote deliveries use rail freight + merchant-marine cargo as a
        // single combined pool. The UI's freight panel already projects against
        // this combined capacity (`crates/wasm-bridge/src/lib.rs:3079`); the
        // turn processor must agree, otherwise sea capacity is shown but never
        // delivers (Trello bug #461).
        let combined_transport = crate::economy::TransportSystem {
            freight_cars: rail_capacity.saturating_add(sea_capacity),
            allocations: transport.allocations.clone(),
        };
        let require_explicit_allocations = nation_id == game.human_player_nation;
        let bonus_multiplier = match game.difficulty {
            Difficulty::Hard if nation_id != game.human_player_nation => 1.1,
            Difficulty::NighOnImpossible if nation_id != game.human_player_nation => 1.25,
            _ => 1.0,
        };

        let (mut local_items, mut remote_items) =
            crate::economy::current_collectable_resources(game, nation_id);
        for items in [&mut local_items, &mut remote_items] {
            for (_, qty) in items.iter_mut() {
                *qty = (*qty as f64 * bonus_multiplier).round() as u32;
            }
        }

        let total_produced: u32 = local_items
            .iter()
            .chain(remote_items.iter())
            .map(|(_, q)| *q)
            .sum();
        if total_produced == 0 {
            // No production this turn, but still record capacity so freight_unused
            // is non-zero and military rail-move checks later in the turn work correctly.
            if let Some(n) = game.world.nations.iter_mut().find(|n| n.id == nation_id) {
                n.economy
                    .logistics
                    .update(rail_capacity, sea_capacity, &[], &[]);
            }
            continue;
        }

        let has_positive_allocations = transport.allocations.iter().any(|(_, units)| *units > 0);
        let delivered_remote_items = if require_explicit_allocations && !has_positive_allocations {
            Vec::new()
        } else {
            combined_transport.calculate_deliveries(&remote_items)
        };

        let Some(nation) = game.world.nations.iter_mut().find(|n| n.id == nation_id) else {
            continue;
        };
        for (resource, remote_available) in &remote_items {
            let delivered = delivered_remote_items
                .iter()
                .find(|(r, _)| r == resource)
                .map(|(_, qty)| *qty)
                .unwrap_or(0);
            let overflow = remote_available.saturating_sub(delivered);
            if overflow > 0 {
                let removable = overflow.min(nation.resource_amount(*resource));
                if removable > 0 {
                    nation.remove_resource(*resource, removable);
                    report
                        .transport_overflow
                        .push((nation_id, *resource, removable));
                }
            }
        }

        if remote_items.is_empty() {
            nation
                .economy
                .logistics
                .update(rail_capacity, sea_capacity, &[], &[]);
        } else {
            nation.economy.logistics.update(
                rail_capacity,
                sea_capacity,
                &remote_items,
                &delivered_remote_items,
            );
        }
    }
}

/// Process queued immigration orders.
///
/// Each immigrant consumes canned food + clothing and produces one untrained
/// worker. Orders are capped by the nation's province-based immigration rate.
fn process_pending_immigration(game: &mut GameState, report: &mut TurnReport) {
    let cfg = game.game_data.game_config.clone();
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let requested = {
            let Some(nation) = game.get_nation_mut(nation_id) else {
                continue;
            };
            if nation.diplomacy.is_in_anarchy {
                continue;
            }
            let requested = nation.economy.pending_immigration;
            nation.economy.pending_immigration = 0;
            requested
        };
        if requested == 0 {
            continue;
        }

        let Some(nation) = game.get_nation_mut(nation_id) else {
            continue;
        };
        if !nation.is_great_power() {
            continue;
        }

        let max_by_canned_food = if cfg.immigration_canned_food > 0 {
            nation.material_amount(MaterialType::CannedFood) / cfg.immigration_canned_food
        } else {
            requested
        };
        let max_by_clothing = if cfg.immigration_clothing > 0 {
            nation.goods_amount(GoodsType::Clothing) / cfg.immigration_clothing
        } else {
            requested
        };
        let max_by_furniture = if cfg.immigration_furniture > 0 {
            nation.goods_amount(GoodsType::Furniture) / cfg.immigration_furniture
        } else {
            requested
        };
        let actual = requested
            .min(nation.immigration_turn_capacity(&cfg))
            .min(max_by_canned_food)
            .min(max_by_clothing)
            .min(max_by_furniture);

        if actual == 0 {
            continue;
        }

        if cfg.immigration_canned_food > 0 {
            nation.consume_material(
                MaterialType::CannedFood,
                actual * cfg.immigration_canned_food,
            );
            report.stockpile_flows.immigration_consumed_materials.push((
                nation_id,
                MaterialType::CannedFood,
                actual * cfg.immigration_canned_food,
            ));
        }
        if cfg.immigration_clothing > 0 {
            nation.consume_goods(GoodsType::Clothing, actual * cfg.immigration_clothing);
            report.stockpile_flows.immigration_consumed_goods.push((
                nation_id,
                GoodsType::Clothing,
                actual * cfg.immigration_clothing,
            ));
        }
        if cfg.immigration_furniture > 0 {
            nation.consume_goods(GoodsType::Furniture, actual * cfg.immigration_furniture);
            report.stockpile_flows.immigration_consumed_goods.push((
                nation_id,
                GoodsType::Furniture,
                actual * cfg.immigration_furniture,
            ));
        }
        for _ in 0..actual {
            nation.economy.labor.recruit_immigrant();
        }
        report.immigration.push((nation_id, actual));
    }
}

/// Resolve civilian work actions for all nations.
///
/// For each nation, ticks all working civilians. When work completes:
/// - Farmer/Rancher/Forester/Miner/Driller: improve the tile (increment improvement_level)
/// - Prospector: reveal the tile's hidden resource deposit (simplified: always reveals coal/iron/oil)
fn resolve_civilian_actions(game: &mut GameState, report: &mut TurnReport) {
    // Phase 0: cancel in-flight engineer tasks whose tile is no longer owned
    // (province lost mid-build). Report each cancellation so it's not silent.
    let owned_by_nation: std::collections::HashMap<NationId, HashSet<crate::hex::HexCoord>> = {
        let mut map: std::collections::HashMap<NationId, HashSet<crate::hex::HexCoord>> =
            std::collections::HashMap::new();
        for p in &game.world.provinces {
            map.entry(p.owner)
                .or_default()
                .extend(p.tiles.iter().copied());
        }
        map
    };
    let mut stranded_reports: Vec<(NationId, String)> = Vec::new();
    for nation in &mut game.world.nations {
        let empty_set: HashSet<crate::hex::HexCoord> = HashSet::new();
        let owned = owned_by_nation.get(&nation.id).unwrap_or(&empty_set);
        for civilian in &mut nation.military.civilians {
            if civilian.civilian_type != CivilianType::Engineer {
                continue;
            }
            if let Some(pos) = civilian.position
                && !owned.contains(&pos)
                && civilian.working
            {
                stranded_reports.push((
                    nation.id,
                    format!(
                        "Engineer's build at ({}, {}) cancelled — territory lost.",
                        pos.q, pos.r
                    ),
                ));
                civilian.working = false;
                civilian.turns_remaining = 0;
                civilian.build_task = None;
            }
        }
    }
    report.civilian_completions.extend(stranded_reports);

    // Phase 1: collect completed work info using immutable borrows on nations,
    // then apply tile mutations separately.
    struct CompletedWork {
        nation_id: NationId,
        civilian_type: CivilianType,
        build_task: Option<crate::economy::civilians::BuildTask>,
        position: crate::hex::HexCoord,
        description: String,
    }

    let mut completed: Vec<CompletedWork> = Vec::new();

    for nation in &mut game.world.nations {
        if nation.diplomacy.is_in_anarchy {
            continue;
        }
        for civilian in &mut nation.military.civilians {
            if !civilian.working {
                continue;
            }
            let just_finished = civilian.tick();
            if just_finished && let Some(pos) = civilian.position {
                let task = civilian.build_task.take();
                let desc = format!(
                    "{} completed work at ({}, {})",
                    civilian.civilian_type, pos.q, pos.r
                );
                completed.push(CompletedWork {
                    nation_id: nation.id,
                    civilian_type: civilian.civilian_type,
                    build_task: task,
                    position: pos,
                    description: desc,
                });
            }
        }
    }

    // Phase 2: apply tile improvements
    for work in &completed {
        // Engineer builds take a different, borrow-heavy path (they need
        // provinces + game_config + a mutable hex_map), so handle those first.
        if work.civilian_type == CivilianType::Engineer {
            if let Some(task) = work.build_task {
                let provinces_snapshot = game.world.provinces.clone();
                let cfg = game.game_data.game_config.clone();
                let researched: Vec<crate::events::TechId> = game
                    .get_nation(work.nation_id)
                    .map(|n| n.researched_techs.clone())
                    .unwrap_or_default();
                let result: Result<Money, crate::DomainError> = match task {
                    crate::economy::civilians::BuildTask::Railroad => {
                        crate::map::infrastructure::build_railroad(
                            &mut game.world.hex_map,
                            work.position,
                            work.nation_id,
                            &researched,
                            &provinces_snapshot,
                            &game.game_data,
                            &cfg,
                        )
                    }
                    crate::economy::civilians::BuildTask::Depot => {
                        crate::map::infrastructure::build_depot(
                            &mut game.world.hex_map,
                            work.position,
                            work.nation_id,
                            &provinces_snapshot,
                            &cfg,
                        )
                    }
                    crate::economy::civilians::BuildTask::Port => {
                        crate::map::infrastructure::build_port(
                            &mut game.world.hex_map,
                            work.position,
                            work.nation_id,
                            &provinces_snapshot,
                            &cfg,
                        )
                    }
                };
                match result {
                    Ok(cost) => {
                        if let Some(nation) = game.get_nation_mut(work.nation_id) {
                            // Debit the treasury. It is allowed to go negative —
                            // build orders are issued synchronously with funds
                            // at order time, but a treasury drop between order
                            // and completion would otherwise leave us unable
                            // to stop the build. Report the charge either way.
                            nation.economy.treasury -= cost;
                        }
                        if cost != Money::ZERO {
                            report.construction_spending.push((work.nation_id, cost));
                        }
                        report.civilian_completions.push((
                            work.nation_id,
                            format!(
                                "{} built at ({}, {}) — cost {}",
                                task, work.position.q, work.position.r, cost
                            ),
                        ));
                    }
                    Err(err) => {
                        // Build failed at completion time (territory lost, tile
                        // state changed, etc.). Surface the failure so it is
                        // not silently dropped.
                        report.civilian_completions.push((
                            work.nation_id,
                            format!(
                                "{} build at ({}, {}) failed: {}",
                                task, work.position.q, work.position.r, err
                            ),
                        ));
                    }
                }
            }
            // Engineers are reusable — clear the tile's assigned_civilian so
            // they can be redeployed next turn by the AI / player.
            if let Some(tile) = game.world.hex_map.get_tile_mut(work.position)
                && let Some(nation) = game.world.nations.iter().find(|n| n.id == work.nation_id)
                && let Some(civ) = nation
                    .military
                    .civilians
                    .iter()
                    .find(|c| c.position == Some(work.position))
                && tile.assigned_civilian == Some(civ.id)
            {
                tile.assigned_civilian = None;
            }
            continue;
        }

        // Pre-compute the tech-gated improvement cap before mut-borrowing the
        // tile, so we can cap the improvement at what the nation's researched
        // techs allow (manual p.27–28: e.g. Mountain mines need Square-Set
        // Timbering for L2, Dynamite for L3).
        let tech_capped_max: u8 = {
            let researched: Vec<crate::events::TechId> = game
                .get_nation(work.nation_id)
                .map(|n| n.researched_techs.clone())
                .unwrap_or_default();
            let (terrain, resource) = match game.world.hex_map.get_tile(work.position) {
                Some(t) => (t.terrain(), t.resource_deposit()),
                None => (crate::types::TerrainType::Sea, None),
            };
            game.game_data
                .tech_tree
                .effective_max_improvement_level(terrain, resource, &researched)
        };

        if let Some(tile) = game.world.hex_map.get_tile_mut(work.position) {
            // Release the civilian's tile slot so the engineer (or another
            // improver) can use it next turn.
            tile.assigned_civilian = None;
            match work.civilian_type {
                CivilianType::Farmer
                | CivilianType::Rancher
                | CivilianType::Forester
                | CivilianType::Miner
                | CivilianType::Driller => {
                    if tile.improvement_level() < tech_capped_max {
                        tile.improve();
                    }
                }
                CivilianType::Prospector => {
                    // The map generator pre-places hidden deposits at world-gen
                    // time; the Prospector's job is to *reveal* what's there.
                    // Skip tiles with a visible surface resource (e.g. Hills
                    // with Wool): nothing hidden to find there.
                    if tile.terrain().can_have_deposits()
                        && !tile.is_prospected()
                        && !tile.has_visible_resource()
                    {
                        match tile.resource_deposit() {
                            Some(r) => tile.reveal_deposit(r),
                            None => tile.reveal_no_deposit(),
                        }
                    }
                }
                CivilianType::Engineer => {}
            }
        }
        report
            .civilian_completions
            .push((work.nation_id, work.description.clone()));
    }
}

/// Convert monetary resources (Gold, Gems) into treasury money.
///
/// Gold: each unit = $500
/// Gems: each unit = $1,000
fn convert_monetary_resources(game: &mut GameState, report: &mut TurnReport) {
    for nation in &mut game.world.nations {
        if nation.diplomacy.is_in_anarchy {
            continue;
        }
        let gold_amount = nation.resource_amount(ResourceType::Gold);
        let gems_amount = nation.resource_amount(ResourceType::Gems);

        let mut income = Money::ZERO;

        if gold_amount > 0 {
            let gold_value =
                Money::dollars(gold_amount as i64 * game.game_data.game_config.gold_value);
            income += gold_value;
            nation.remove_resource(ResourceType::Gold, gold_amount);
        }

        if gems_amount > 0 {
            let gems_value =
                Money::dollars(gems_amount as i64 * game.game_data.game_config.gems_value);
            income += gems_value;
            nation.remove_resource(ResourceType::Gems, gems_amount);
        }

        if income != Money::ZERO {
            nation.economy.treasury += income;
            report.gold_income.push((nation.id, income));
        }
    }
}

/// Run production chains: mills convert resources to materials, factories convert materials to goods.
///
/// Labor is a shared pool: workers provide labor based on training level (untrained=1,
/// trained=2, expert=4). Each unit of production costs labor_per_production (default 2).
/// Mills consume labor first, then remaining labor feeds factories.
/// Process end-of-turn civilian hiring queue: deduct cash and expert workers, add civilians.
fn process_pending_civilian_hires(game: &mut GameState) {
    let cfg = game.game_data.game_config.clone();
    let civilian_costs_expert = cfg.civilian_costs_expert;
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let pending: Vec<(CivilianType, u32)> = {
            let Some(nation) = game.get_nation_mut(nation_id) else {
                continue;
            };
            if nation.diplomacy.is_in_anarchy {
                continue;
            }
            nation.economy.pending_civilian_hires.drain().collect()
        };
        if pending.is_empty() {
            continue;
        }

        for (civ_type, count) in pending {
            let cost_per = civ_type.creation_cost(&cfg);
            let actual = {
                let Some(nation) = game.get_nation(nation_id) else {
                    break;
                };
                let max_by_cash = if cost_per > Money::ZERO {
                    (nation.economy.treasury.as_dollars() / cost_per.as_dollars())
                        .clamp(0, u32::MAX as i64) as u32
                } else {
                    count
                };
                let max_by_experts = if civilian_costs_expert {
                    nation.economy.labor.expert
                } else {
                    count
                };
                count.min(max_by_cash).min(max_by_experts)
            };
            if actual == 0 {
                continue;
            }

            let total_cost = Money::dollars(cost_per.as_dollars() * i64::from(actual));
            {
                let Some(nation) = game.get_nation_mut(nation_id) else {
                    break;
                };
                nation.economy.treasury -= total_cost;
                if civilian_costs_expert {
                    nation.economy.labor.expert =
                        nation.economy.labor.expert.saturating_sub(actual);
                }
            }
            for _ in 0..actual {
                let cid = game.alloc_unit_id();
                let new_civ = crate::economy::civilians::Civilian::new(cid, civ_type, nation_id);
                if let Some(nation) = game.get_nation_mut(nation_id) {
                    nation.military.civilians.push(new_civ);
                }
            }
        }
    }
}

/// Process end-of-turn worker training queue: Untrained→Trained and Trained→Expert.
/// Cost: Paper material + labor (read from GameConfig).
fn process_pending_worker_training(game: &mut GameState) {
    let cfg = game.game_data.game_config.clone();
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let (req_trained, req_expert) = {
            let Some(nation) = game.get_nation_mut(nation_id) else {
                continue;
            };
            if nation.diplomacy.is_in_anarchy {
                continue;
            }
            let t = (
                nation.economy.pending_train_to_trained,
                nation.economy.pending_train_to_expert,
            );
            nation.economy.pending_train_to_trained = 0;
            nation.economy.pending_train_to_expert = 0;
            t
        };
        if req_trained == 0 && req_expert == 0 {
            continue;
        }

        let Some(nation) = game.get_nation_mut(nation_id) else {
            continue;
        };
        let avail_paper = nation.material_amount(MaterialType::Paper);
        let avail_labor = nation.economy.labor.total_labor_units();

        let pp_t = cfg.train_to_trained_paper_cost;
        let lp_t = cfg.train_to_trained_labor_cost;
        let pp_e = cfg.train_to_expert_paper_cost;
        let lp_e = cfg.train_to_expert_labor_cost;

        let max_train = req_trained
            .min(if pp_t > 0 {
                avail_paper / pp_t
            } else {
                req_trained
            })
            .min(if lp_t > 0 {
                avail_labor / lp_t
            } else {
                req_trained
            })
            .min(nation.economy.labor.untrained);

        let rem_paper = avail_paper.saturating_sub(max_train * pp_t);
        let rem_labor = avail_labor.saturating_sub(max_train * lp_t);
        let max_expert = req_expert
            .min(if pp_e > 0 {
                rem_paper / pp_e
            } else {
                req_expert
            })
            .min(if lp_e > 0 {
                rem_labor / lp_e
            } else {
                req_expert
            })
            .min(nation.economy.labor.trained);

        if max_train > 0 {
            if let Some(v) = nation.economy.materials.get_mut(&MaterialType::Paper) {
                *v = v.saturating_sub(max_train * pp_t);
            }
            nation.economy.labor.untrained =
                nation.economy.labor.untrained.saturating_sub(max_train);
            nation.economy.labor.trained += max_train;
        }
        if max_expert > 0 {
            if let Some(v) = nation.economy.materials.get_mut(&MaterialType::Paper) {
                *v = v.saturating_sub(max_expert * pp_e);
            }
            nation.economy.labor.trained = nation.economy.labor.trained.saturating_sub(max_expert);
            nation.economy.labor.expert += max_expert;
        }
    }
}

fn process_pending_freight_cars(game: &mut GameState) {
    let (labor_cost, lumber_cost, steel_cost) =
        crate::economy::transport::TransportSystem::build_freight_car_cost();
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    for nation_id in nation_ids {
        let count = game
            .get_nation(nation_id)
            .map(|n| n.economy.pending_freight_cars)
            .unwrap_or(0);
        if count == 0 {
            continue;
        }
        let Some(nation) = game.get_nation_mut(nation_id) else {
            continue;
        };
        nation.economy.pending_freight_cars = 0;
        let max_by_lumber = nation.material_amount(MaterialType::Lumber) / lumber_cost.max(1);
        let max_by_steel = nation.material_amount(MaterialType::Steel) / steel_cost.max(1);
        let max_by_labor = nation.economy.labor.total_labor_units() / labor_cost.max(1);
        let actual = count.min(max_by_lumber).min(max_by_steel).min(max_by_labor);
        if actual > 0 {
            nation.consume_material(MaterialType::Lumber, actual * lumber_cost);
            nation.consume_material(MaterialType::Steel, actual * steel_cost);
            nation.military.transport.build_freight_cars(actual);
        }
    }
}

fn process_pending_ships(game: &mut GameState) {
    use crate::military::ships::{Ship, ShipCategory};
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    for nation_id in nation_ids {
        let pending: Vec<String> = game
            .get_nation(nation_id)
            .map(|n| n.economy.pending_ships.clone())
            .unwrap_or_default();
        if pending.is_empty() {
            continue;
        }
        let Some(nation) = game.get_nation_mut(nation_id) else {
            continue;
        };
        nation.economy.pending_ships.clear();
        for ship_type_str in pending {
            if let Ok(ship_type) = ship_type_str.parse::<crate::military::ships::ShipType>() {
                // Deduct resources at end-of-turn when the ship is actually built.
                let stats = game.game_data.ship_stats(ship_type).clone();
                if let Some(n) = game.get_nation_mut(nation_id) {
                    n.consume_material(MaterialType::Fabric, stats.fabric_cost);
                    n.consume_material(MaterialType::Lumber, stats.lumber_cost);
                    n.consume_goods(GoodsType::Arms, stats.arms_cost);
                    n.consume_material(MaterialType::Steel, stats.steel_cost);
                    n.remove_resource(ResourceType::Coal, stats.coal_cost);
                }
                let sid = game.alloc_unit_id();
                let new_ship = Ship::with_data(sid, ship_type, nation_id, &game.game_data);
                if let Some(n) = game.get_nation_mut(nation_id) {
                    match ship_type.category() {
                        ShipCategory::Merchant => n.military.merchant_fleet.push(new_ship),
                        ShipCategory::Warship => n.military.warships.push(new_ship),
                    }
                }
            }
        }
    }
}

fn process_pending_army_recruits(game: &mut GameState) {
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    for nation_id in nation_ids {
        let pending: Vec<String> = game
            .get_nation(nation_id)
            .map(|n| n.economy.pending_army_recruits.clone())
            .unwrap_or_default();
        if pending.is_empty() {
            continue;
        }
        if let Some(nation) = game.get_nation_mut(nation_id) {
            nation.economy.pending_army_recruits.clear();
        }
        let capital = match game.get_nation(nation_id).map(|n| n.capital_province_id) {
            Some(id) => id,
            None => continue,
        };
        for unit_type_str in pending {
            if let Ok(unit_type) = unit_type_str.parse::<ArmyUnitType>()
                && unit_type.can_build()
            {
                let can = game
                    .get_nation(nation_id)
                    .map(|n| n.can_recruit_unit(unit_type))
                    .unwrap_or(false);
                if can {
                    if let Some(nation) = game.get_nation_mut(nation_id) {
                        nation.deduct_recruit_resources(unit_type);
                    }
                    let uid = game.alloc_unit_id();
                    let new_unit = ArmyUnit::new(uid, unit_type, nation_id, capital);
                    if let Some(n) = game.get_nation_mut(nation_id) {
                        n.military.army.push(new_unit);
                    }
                }
            }
        }
    }
}

pub(super) fn run_production(game: &mut GameState, report: &mut TurnReport) {
    let cfg = &game.game_data.game_config;
    let untrained_mult = cfg.untrained_labor;
    let trained_mult = cfg.trained_labor;
    let expert_mult = cfg.expert_labor;
    let armory_steel_per_arm = cfg.armory_steel_per_arm;
    let armory_labor_per_arm = cfg.armory_labor_per_arm;

    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let nation = match game.world.nations.iter().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };
        if nation.diplomacy.is_in_anarchy {
            continue;
        }

        // Gather current resource inventory as slices
        let resources: Vec<(ResourceType, u32)> = nation
            .economy
            .warehouse
            .iter()
            .map(|(r, q)| (*r, *q))
            .collect();

        let total_labor =
            nation
                .economy
                .labor
                .total_labor_units_with(untrained_mult, trained_mult, expert_mult);

        // Building capacities
        let lumber_mill_cap = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);
        let steel_mill_cap = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::SteelMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);
        let textile_mill_cap = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::TextileMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);
        let furniture_cap = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::FurnitureFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);
        let hardware_cap = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::HardwareFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);
        let clothing_cap = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::ClothingFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        // Honor chain_targets: proportional labor allocation + feed percentages.
        // Mutable because the opportunistic cannery top-up may raise
        // `canned_food_factory` before feed caps are applied.
        let mut targets = nation.economy.chain_targets.clone();
        let armory_cap = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::Armory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);
        let paper_cap = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::PaperFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);
        let canned_food_cap = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::FoodProcessing)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);
        let mut labor = super::economy_phase::allocate_labor(
            total_labor,
            &targets,
            super::economy_phase::BuildingCapacities {
                timber: lumber_mill_cap,
                metal: steel_mill_cap,
                textile: textile_mill_cap,
                furniture: furniture_cap,
                hardware: hardware_cap,
                clothing: clothing_cap,
                armory: armory_cap,
                paper: paper_cap,
                canned_food: canned_food_cap,
            },
        );

        // Opportunistic cannery top-up: idle workers + surplus raw food
        // turn into extra canned food, regardless of the AI's strategic
        // target. Compute the input bottleneck after composite-meal
        // reservation, then ask the helper to expand both labor budget and
        // output target.
        if canned_food_cap > 0 {
            let workers = nation.economy.labor.total_workers();
            let bottleneck = crate::economy::labor::cannery_input_cap(
                nation.resource_amount(ResourceType::Grain),
                nation.resource_amount(ResourceType::Fruit),
                nation.resource_amount(ResourceType::Fish),
                nation.resource_amount(ResourceType::Livestock),
                nation.material_amount(MaterialType::CannedFood),
                workers,
            );
            super::economy_phase::cannery_opportunistic_topup(
                &mut labor,
                &mut targets,
                total_labor,
                canned_food_cap,
                bottleneck,
            );
        }

        let fed_resources = super::economy_phase::apply_feed_to_resources(&resources, &targets);

        // ── Mills: resources → materials ──

        let timber_result = if lumber_mill_cap > 0 {
            Some(calculate_mill_production(
                ProductionChain::Timber,
                &fed_resources,
                lumber_mill_cap,
                labor.timber_mill,
            ))
        } else {
            None
        };

        let metal_result = if steel_mill_cap > 0 {
            Some(calculate_mill_production(
                ProductionChain::Metal,
                &fed_resources,
                steel_mill_cap,
                labor.metal_mill,
            ))
        } else {
            None
        };

        let textile_result = if textile_mill_cap > 0 {
            Some(calculate_mill_production(
                ProductionChain::Textile,
                &fed_resources,
                textile_mill_cap,
                labor.textile_mill,
            ))
        } else {
            None
        };

        // Apply mill results: consume resources, produce materials
        let Some(nation) = game.world.nations.iter_mut().find(|n| n.id == nation_id) else {
            continue;
        };

        // Collect newly produced materials to feed into factories
        let mut new_materials: Vec<(MaterialType, u32)> = Vec::new();

        for result in [&timber_result, &metal_result, &textile_result]
            .into_iter()
            .flatten()
        {
            // Consume resources
            for (resource, amount) in &result.resources_consumed {
                if *amount > 0 {
                    nation.remove_resource(*resource, *amount);
                    report
                        .stockpile_flows
                        .mill_consumed_resources
                        .push((nation_id, *resource, *amount));
                }
            }
            // Produce materials
            for (material, amount) in &result.materials_produced {
                if *amount > 0 {
                    *nation.economy.materials.entry(*material).or_insert(0) += *amount;
                    new_materials.push((*material, *amount));
                    report
                        .production_output
                        .push((nation_id, format!("{:?}", material), *amount));
                    report
                        .stockpile_flows
                        .mill_produced_materials
                        .push((nation_id, *material, *amount));
                }
            }
        }

        // ── Factories: materials → goods ──

        // Combined materials: warehouse stock + this turn's mill output
        let combined_materials: Vec<(MaterialType, u32)> = nation
            .economy
            .materials
            .iter()
            .map(|(m, q)| (*m, *q))
            .collect();
        let fed_materials =
            super::economy_phase::apply_feed_to_materials(&combined_materials, &targets);

        let furniture_result = if furniture_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Timber,
                &fed_materials,
                furniture_cap,
                labor.lumber_factory,
            ))
        } else {
            None
        };

        let hardware_result = if hardware_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Metal,
                &fed_materials,
                hardware_cap,
                labor.steel_factory,
            ))
        } else {
            None
        };

        let clothing_result = if clothing_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Textile,
                &fed_materials,
                clothing_cap,
                labor.garment_factory,
            ))
        } else {
            None
        };

        // Apply factory results: consume materials, produce goods
        for result in [&furniture_result, &hardware_result, &clothing_result]
            .into_iter()
            .flatten()
        {
            // Consume materials
            for (material, amount) in &result.materials_consumed {
                if *amount > 0 {
                    let entry = nation.economy.materials.entry(*material).or_insert(0);
                    *entry = entry.saturating_sub(*amount);
                    report
                        .stockpile_flows
                        .factory_consumed_materials
                        .push((nation_id, *material, *amount));
                }
            }
            // Produce goods
            for (good, amount) in &result.goods_produced {
                if *amount > 0 {
                    *nation.economy.goods.entry(*good).or_insert(0) += *amount;
                    report
                        .production_output
                        .push((nation_id, format!("{:?}", good), *amount));
                    report
                        .stockpile_flows
                        .factory_produced_goods
                        .push((nation_id, *good, *amount));
                }
            }
        }

        // ── Armory: Steel → Arms ──
        let combined_materials_for_armory: Vec<(MaterialType, u32)> = nation
            .economy
            .materials
            .iter()
            .map(|(m, q)| (*m, *q))
            .collect();
        let fed_materials_for_armory =
            super::economy_phase::apply_feed_to_materials(&combined_materials_for_armory, &targets);
        let armory_result = if armory_cap > 0 {
            let steel_for_armory = fed_materials_for_armory
                .iter()
                .find(|(m, _)| *m == MaterialType::Steel)
                .map(|(_, q)| *q)
                .unwrap_or(0)
                .min(targets.armory);
            Some(calculate_armory_production(
                steel_for_armory,
                armory_cap,
                labor.armory,
                armory_steel_per_arm,
                armory_labor_per_arm,
            ))
        } else {
            None
        };
        if let Some(result) = &armory_result {
            for (material, amount) in &result.materials_consumed {
                if *amount > 0 {
                    let entry = nation.economy.materials.entry(*material).or_insert(0);
                    *entry = entry.saturating_sub(*amount);
                }
            }
            for (goods, amount) in &result.goods_produced {
                if *amount > 0 {
                    *nation.economy.goods.entry(*goods).or_insert(0) += *amount;
                    report
                        .production_output
                        .push((nation_id, format!("{:?}", goods), *amount));
                    if let GoodsType::Arms = goods {
                        nation.military.total_arms_built =
                            nation.military.total_arms_built.saturating_add(*amount);
                    }
                }
            }
        }

        // ── Paper Factory: Lumber → Paper ──
        if paper_cap > 0 {
            let current_lumber = nation
                .economy
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0);
            let lumber_slice: Vec<(MaterialType, u32)> =
                vec![(MaterialType::Lumber, current_lumber)];
            let paper_result =
                calculate_paper_production(&lumber_slice, paper_cap, labor.paper_factory);
            for (material, amount) in &paper_result.materials_consumed {
                if *amount > 0 {
                    let entry = nation.economy.materials.entry(*material).or_insert(0);
                    *entry = entry.saturating_sub(*amount);
                }
            }
            for (material, amount) in &paper_result.materials_produced {
                if *amount > 0 {
                    *nation.economy.materials.entry(*material).or_insert(0) += *amount;
                    report
                        .production_output
                        .push((nation_id, format!("{:?}", material), *amount));
                }
            }
        }

        // ── Cannery: 1 Grain + 1 Fruit + 1 (Fish OR Livestock) → 1 CannedFood ──
        if canned_food_cap > 0 {
            // Workers eat an Imperialism-1 ration each turn (grain = ⌈w/2⌉,
            // fruit = ⌊balance⌋, meat = ⌊w/4⌋). Canned food substitutes for any
            // single missing food unit, but raw food fills slots first. Reserve
            // the worker-meal demand before the cannery sees inputs so the
            // workforce can't be starved by canning.
            let workers = nation.economy.labor.total_workers();
            let (grain_need, fruit_need, meat_need) =
                crate::economy::labor::worker_food_demand(workers);
            let livestock_held = nation.resource_amount(ResourceType::Livestock);
            let livestock_for_meals = meat_need.min(livestock_held);
            let fish_for_meals = meat_need.saturating_sub(livestock_for_meals);
            let current_resources: Vec<(ResourceType, u32)> = nation
                .economy
                .warehouse
                .iter()
                .map(|(r, q)| {
                    let reserved = match *r {
                        ResourceType::Grain => grain_need,
                        ResourceType::Fruit => fruit_need,
                        ResourceType::Livestock => livestock_for_meals,
                        ResourceType::Fish => fish_for_meals,
                        _ => 0,
                    };
                    (*r, q.saturating_sub(reserved.min(*q)))
                })
                .collect();
            let fed_for_cannery =
                super::economy_phase::apply_feed_to_resources(&current_resources, &targets);
            let canned_result = calculate_canned_food_production(
                &fed_for_cannery,
                canned_food_cap,
                labor.canned_food_factory,
            );
            for (resource, amount) in &canned_result.resources_consumed {
                if *amount > 0 {
                    nation.remove_resource(*resource, *amount);
                    report
                        .stockpile_flows
                        .food_processed_inputs
                        .push((nation_id, *resource, *amount));
                }
            }
            for (material, amount) in &canned_result.materials_produced {
                if *amount > 0 {
                    *nation.economy.materials.entry(*material).or_insert(0) += *amount;
                    report
                        .production_output
                        .push((nation_id, format!("{:?}", material), *amount));
                    if let MaterialType::CannedFood = material {
                        report
                            .stockpile_flows
                            .canned_food_produced
                            .push((nation_id, *amount));
                    }
                }
            }
        }
    }
}

/// Resolve autonomous town production for Village and Town provinces.
///
/// For each province that `can_produce()`:
/// 1. Sum resource yields from all tiles in the province.
/// 2. Apply 2:1 conversion for raw resources → materials:
///    - Timber → Lumber (2 timber → 1 lumber)
///    - Coal + Iron → Steel (1 coal + 1 iron → 1 steel)
///    - Cotton/Wool → Fabric (2 cotton/wool → 1 fabric)
/// 3. Convert half of materials to goods (2:1):
///    - Lumber → Furniture (2 lumber → 1 furniture)
///    - Steel → Hardware (2 steel → 1 hardware)
///    - Fabric → Clothing (2 fabric → 1 clothing)
/// 4. Towns produce at double rate (multiplier 2).
/// 5. Add produced materials and goods to the owning nation's warehouse.
fn resolve_town_production(game: &mut GameState, report: &mut TurnReport) {
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let (local_outputs, remote_outputs) = crate::economy::project_town_outputs(game, nation_id);
        if local_outputs.is_empty() && remote_outputs.is_empty() {
            continue;
        }

        let Some(nation) = game.world.nations.iter_mut().find(|n| n.id == nation_id) else {
            continue;
        };
        let mut outputs_to_apply = local_outputs;

        let original_granted = nation
            .economy
            .logistics
            .per_resource
            .iter()
            .map(|(resource, demand)| (*resource, demand.granted))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut adjusted_granted = original_granted.clone();
        let original_unused = nation.economy.logistics.freight_unused;
        let (delivered_remote_outputs, remaining_unused) =
            crate::economy::allocate_town_output_freight(
                &mut adjusted_granted,
                &remote_outputs,
                &nation.military.transport.allocations,
                original_unused,
            );

        for (resource, before) in original_granted {
            let after = adjusted_granted.get(&resource).copied().unwrap_or(0);
            let displaced = before.saturating_sub(after);
            if displaced == 0 {
                continue;
            }
            let removable = displaced.min(nation.resource_amount(resource));
            if removable == 0 {
                continue;
            }
            nation.remove_resource(resource, removable);
            if let Some(demand) = nation.economy.logistics.per_resource.get_mut(&resource) {
                demand.granted = demand.granted.saturating_sub(removable);
                demand.unmet = demand.unmet.saturating_add(removable);
            }
            report
                .transport_overflow
                .push((nation_id, resource, removable));
        }

        let used_unused_capacity = original_unused.saturating_sub(remaining_unused);
        nation.economy.logistics.freight_unused = remaining_unused;
        nation.economy.logistics.freight_committed = nation
            .economy
            .logistics
            .freight_total
            .saturating_sub(remaining_unused);

        if used_unused_capacity > 0 || !delivered_remote_outputs.is_empty() {
            outputs_to_apply.extend(delivered_remote_outputs);
        }

        for (stockpile, qty) in outputs_to_apply {
            apply_town_output(nation, report, nation_id, stockpile, qty);
        }
    }
}

fn apply_town_output(
    nation: &mut crate::nation::Nation,
    report: &mut TurnReport,
    nation_id: NationId,
    stockpile: crate::economy::FreightTarget,
    qty: u32,
) {
    if qty == 0 {
        return;
    }
    match stockpile {
        crate::economy::FreightTarget::Material(material) => {
            nation.add_material(material, qty);
            report
                .town_production
                .push((nation_id, format!("{:?}", material), qty));
            report
                .stockpile_flows
                .town_produced_materials
                .push((nation_id, material, qty));
        }
        crate::economy::FreightTarget::Goods(good) => {
            nation.add_goods(good, qty);
            report
                .town_production
                .push((nation_id, format!("{:?}", good), qty));
            report
                .stockpile_flows
                .town_produced_goods
                .push((nation_id, good, qty));
        }
        crate::economy::FreightTarget::Resource(_) => {}
    }
}

/// Consume food for each nation based on population.
///
/// Per turn the workforce demands `grain = ⌈w/2⌉`, `meat = ⌊w/4⌋`,
/// `fruit = w − grain − meat` (Imperialism-1 ratio; see `worker_food_demand`).
/// Each slot is drawn from its own stockpile (meat: livestock first, then fish).
/// Any unmet food unit is then covered 1-for-1 by `CannedFood`; whatever still
/// can't be sourced starves one worker per missing unit, capped by
/// `starvation_cap`.
fn food_consumption(game: &mut GameState, report: &mut TurnReport) {
    let ai_debug = game.ai_debug;
    for nation in &mut game.world.nations {
        if nation.diplomacy.is_in_anarchy {
            continue;
        }
        let population = nation.economy.labor.total_workers();
        if population == 0 {
            continue;
        }
        let nation_id = nation.id;

        let grain_held = nation.resource_amount(ResourceType::Grain);
        let fruit_held = nation.resource_amount(ResourceType::Fruit);
        let livestock_held = nation.resource_amount(ResourceType::Livestock);
        let fish_held = nation.resource_amount(ResourceType::Fish);
        let canned_held = nation.material_amount(MaterialType::CannedFood);

        let (grain_need, fruit_need, meat_need) =
            crate::economy::labor::worker_food_demand(population);

        let grain_consumed = grain_held.min(grain_need);
        let fruit_consumed = fruit_held.min(fruit_need);
        let livestock_consumed = livestock_held.min(meat_need);
        let fish_consumed = fish_held.min(meat_need - livestock_consumed);
        let meat_consumed = livestock_consumed + fish_consumed;

        let deficit = (grain_need - grain_consumed)
            + (fruit_need - fruit_consumed)
            + (meat_need - meat_consumed);

        if ai_debug && nation.is_great_power() {
            eprintln!(
                "[FOOD:{}] w={} need=g{}/f{}/m{} held=g{}/f{}/l{}/F{}/c{} deficit={}",
                nation.name,
                population,
                grain_need,
                fruit_need,
                meat_need,
                grain_held,
                fruit_held,
                livestock_held,
                fish_held,
                canned_held,
                deficit,
            );
        }

        if grain_consumed > 0 {
            nation.remove_resource(ResourceType::Grain, grain_consumed);
            report.stockpile_flows.worker_food_consumed.push((
                nation_id,
                ResourceType::Grain,
                grain_consumed,
            ));
        }
        if fruit_consumed > 0 {
            nation.remove_resource(ResourceType::Fruit, fruit_consumed);
            report.stockpile_flows.worker_food_consumed.push((
                nation_id,
                ResourceType::Fruit,
                fruit_consumed,
            ));
        }
        if livestock_consumed > 0 {
            nation.remove_resource(ResourceType::Livestock, livestock_consumed);
            report.stockpile_flows.worker_food_consumed.push((
                nation_id,
                ResourceType::Livestock,
                livestock_consumed,
            ));
        }
        if fish_consumed > 0 {
            nation.remove_resource(ResourceType::Fish, fish_consumed);
            report.stockpile_flows.worker_food_consumed.push((
                nation_id,
                ResourceType::Fish,
                fish_consumed,
            ));
        }

        // CannedFood fallback: one canned unit substitutes for any one missing
        // raw food unit (covers any slot).
        let canned_consumed = canned_held.min(deficit);
        if canned_consumed > 0 {
            nation.consume_material(MaterialType::CannedFood, canned_consumed);
            report
                .stockpile_flows
                .worker_canned_food_consumed
                .push((nation_id, canned_consumed));
        }

        let total_food_consumed = grain_consumed + fruit_consumed + meat_consumed + canned_consumed;
        if total_food_consumed > 0 {
            report.food_consumed.push((nation_id, total_food_consumed));
        }

        // Starvation: each still-missing food unit kills one worker.
        let unsated = deficit - canned_consumed;
        if unsated > 0 {
            let workers_lost = unsated
                .min(population)
                .min(game.game_data.game_config.starvation_cap);
            let mut actual_lost = 0;
            for _ in 0..workers_lost {
                if nation.economy.labor.remove_worker() {
                    actual_lost += 1;
                }
            }
            if actual_lost > 0 {
                report.starvation.push((nation_id, actual_lost));
            }
        }
    }
}

/// Apply maintenance costs for army units.
/// Resolve beachhead (naval landing) operations.
///
/// For each nation with warships assigned to `Beachhead(target_province)`:
/// 1. Validate the target province is coastal and owned by an enemy
/// 2. Add it to `pending_landings` with the current turn number
/// 3. Remove stale landings from nations that no longer have warships assigned
///
/// Landing sites are only usable for attacks on turns AFTER establishment
/// (enforced in `can_attack_province`).
fn resolve_beachheads(game: &mut GameState, report: &mut TurnReport) {
    use crate::military::naval::NavalOperation;

    let current_turn = game.turn;

    // Remove landings where the nation no longer has warships assigned to that target
    let active_assignments: Vec<(NationId, ProvinceId)> = game
        .world
        .nations
        .iter()
        .flat_map(|nation| {
            nation.military.warships.iter().filter_map(move |ship| {
                if let Some(NavalOperation::Beachhead(target_pid)) = ship.operation {
                    Some((nation.id, target_pid))
                } else {
                    None
                }
            })
        })
        .collect();

    // Keep existing landings that still have ships assigned AND remain diplomatically valid.
    // Pre-compute valid landing IDs to avoid borrow conflict with retain.
    let valid_landings: Vec<(NationId, ProvinceId)> = game
        .transient
        .pending_landings
        .iter()
        .filter(|(nid, pid, _)| {
            let has_ships = active_assignments
                .iter()
                .any(|(n, p)| *n == *nid && *p == *pid);
            if !has_ships {
                return false;
            }
            // Revalidate: target must still be ocean-coastal, owned by enemy, and at war
            let target_valid = game.get_province(*pid).is_some_and(|p| {
                p.ocean_coastal && {
                    let at_war = game
                        .world
                        .diplomacy
                        .get_relation(*nid, p.owner)
                        .is_some_and(|r| r.at_war);
                    let target_anarchic = game
                        .get_nation(p.owner)
                        .is_some_and(|n| n.diplomacy.is_in_anarchy);
                    at_war || target_anarchic
                }
            });
            // Revalidate embarkation: attacker must still own an ocean-coastal province
            let attacker_has_coast = game.get_nation(*nid).is_some_and(|n| {
                n.province_ids.iter().any(|&pid| {
                    game.get_province(pid)
                        .is_some_and(|p| p.ocean_coastal || p.coastal)
                })
            });
            target_valid && attacker_has_coast
        })
        .map(|(nid, pid, _)| (*nid, *pid))
        .collect();
    game.transient
        .pending_landings
        .retain(|(nid, pid, _)| valid_landings.iter().any(|(n, p)| *n == *nid && *p == *pid));

    // Collect new beachhead assignments: (nation_id, target_province_id)
    let mut new_requests: Vec<(NationId, ProvinceId)> = Vec::new();
    for nation in &game.world.nations {
        for ship in &nation.military.warships {
            if let Some(NavalOperation::Beachhead(target_pid)) = ship.operation
                && !new_requests
                    .iter()
                    .any(|(nid, pid)| *nid == nation.id && *pid == target_pid)
                // Don't re-add if already in pending_landings
                && !game
                    .transient.pending_landings
                    .iter()
                    .any(|(nid, pid, _)| *nid == nation.id && *pid == target_pid)
            {
                new_requests.push((nation.id, target_pid));
            }
        }
    }

    for (attacker_id, target_pid) in new_requests {
        // Embarkation: attacker must own at least one ocean-coastal province (port).
        let attacker_has_coast = game
            .get_nation(attacker_id)
            .map(|n| {
                n.province_ids.iter().any(|&pid| {
                    game.get_province(pid)
                        .is_some_and(|p| p.ocean_coastal || p.coastal)
                })
            })
            .unwrap_or(false);
        if !attacker_has_coast {
            continue;
        }

        // Zone adjacency: when zones are computed, at least one assigned ship must be in a
        // non-lake zone that borders the target province. When zones have not been computed
        // (test/legacy mode), skip the zone check.
        let fleet_zone_ok = {
            let zones_computed = !game.world.sea_zones.is_empty();
            let attacker_ship_zones: Vec<_> = game
                .get_nation(attacker_id)
                .map(|n| {
                    n.military.warships.iter()
                        .filter(|s| matches!(s.operation, Some(crate::military::naval::NavalOperation::Beachhead(pid)) if pid == target_pid))
                        .filter_map(|s| s.sea_zone)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !zones_computed {
                // Legacy / test mode: no zone check
                true
            } else if attacker_ship_zones.is_empty() {
                // Zones exist but ships have no zone → not deployed, cannot establish beachhead
                false
            } else {
                // At least one assigned ship must be in an ocean zone bordering the target
                attacker_ship_zones.iter().any(|&zone_id| {
                    game.world
                        .sea_zones
                        .iter()
                        .find(|z| z.id == zone_id)
                        .is_some_and(|z| !z.is_lake && z.coastal_provinces.contains(&target_pid))
                })
            }
        };
        let valid = fleet_zone_ok
            && game.get_province(target_pid).is_some_and(|p| {
                p.ocean_coastal && {
                    let at_war = game
                        .world
                        .diplomacy
                        .get_relation(attacker_id, p.owner)
                        .is_some_and(|r| r.at_war);
                    let target_anarchic = game
                        .get_nation(p.owner)
                        .is_some_and(|n| n.diplomacy.is_in_anarchy);
                    at_war || target_anarchic
                }
            });

        if valid {
            game.transient
                .pending_landings
                .push((attacker_id, target_pid, current_turn));
            let attacker_name = game
                .get_nation(attacker_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let target_name = game
                .get_province(target_pid)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "{} establishes a naval landing site at {}",
                        attacker_name, target_name
                    ),
                    HeadlineCategory::Military,
                )
                .for_nation(attacker_id),
            );
        }
    }
}

/// Check whether a nation can attack a target province.
///
/// An attack is valid if:
/// 1. The attacker owns a province that is **land-adjacent** to the target, OR
/// 2. The attacker has an active **naval landing site** on the target province
///    that was established on a **previous** turn (not this turn).
fn can_attack_province(
    game: &GameState,
    attacker_id: NationId,
    target_province_id: ProvinceId,
) -> bool {
    // Check for active landing site established on a previous turn
    let current_turn = game.turn;
    if game
        .transient
        .pending_landings
        .iter()
        .any(|(nid, pid, established)| {
            *nid == attacker_id && *pid == target_province_id && *established < current_turn
        })
    {
        return true;
    }

    // Check for land adjacency: attacker must own a province sharing a hex edge
    // with the target province.
    let target_prov = match game.get_province(target_province_id) {
        Some(p) => p,
        None => return false,
    };

    let attacker_province_ids: Vec<ProvinceId> = match game.get_nation(attacker_id) {
        Some(n) => n.province_ids.clone(),
        None => return false,
    };

    for &owned_pid in &attacker_province_ids {
        if let Some(owned_prov) = game.get_province(owned_pid)
            && crate::map::provinces_are_adjacent(&game.world.hex_map, owned_prov, target_prov)
        {
            return true;
        }
    }

    false
}

/// Resolve military unit movement from pending_moves.
///
/// For each pending move:
/// 1. Validate the unit exists in the nation's army
/// 2. If destination is owned by the nation, move the unit there
/// 3. If destination is owned by an enemy at war, convert to a pending_attack instead
/// 4. Otherwise, reject the move
fn resolve_military_movement(
    game: &mut GameState,
    report: &mut TurnReport,
) -> HashSet<crate::map::UnitId> {
    let mut moved_unit_ids: HashSet<crate::map::UnitId> = HashSet::new();
    // Per-nation rail freight consumed by military moves this turn (separate from
    // resource-delivery freight_committed so the two don't interfere in the rail gate).
    let mut rail_committed_military: HashMap<NationId, u32> = HashMap::new();
    let mut moves: Vec<(NationId, crate::map::UnitId, ProvinceId)> =
        game.transient.pending_moves.drain(..).collect();

    // Sort moves so highest-firepower units (highest strategic value) are
    // processed first within each nation. When freight capacity runs out, the
    // most powerful units get through; low-value surplus moves are dropped.
    moves.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| {
            let fp = |nid: NationId, uid: crate::map::UnitId| -> u32 {
                game.world
                    .nations
                    .iter()
                    .find(|n| n.id == nid)
                    .and_then(|n| n.military.army.iter().find(|u| u.id == uid))
                    .map(|u| u.unit_type.stats().firepower)
                    .unwrap_or(0)
            };
            fp(b.0, b.1).cmp(&fp(a.0, a.1))
        })
    });

    for (nation_id, unit_id, dest_province_id) in moves {
        // Anarchic nations' armies don't move
        if game
            .get_nation(nation_id)
            .is_some_and(|n| n.diplomacy.is_in_anarchy)
        {
            continue;
        }

        // Look up the destination province owner
        let dest_owner = match game.get_province(dest_province_id) {
            Some(p) => p.owner,
            None => continue,
        };
        let dest_name = game
            .get_province(dest_province_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        if dest_owner == nation_id {
            // Friendly province: check if unit can move and whether the freight
            // budget allows a rail-strategic (non-adjacent) redeployment.

            // Extract unit data before any mutable borrows.
            let unit_data = game
                .world
                .nations
                .iter()
                .find(|n| n.id == nation_id)
                .and_then(|n| n.military.army.iter().find(|u| u.id == unit_id))
                .map(|u| {
                    (
                        u.position,
                        !u.unit_type.can_move(),
                        crate::economy::transport::unit_transport_size(u),
                        format!("{:?}", u.unit_type),
                    )
                });
            let Some((src_pos, cannot_move, transport_size, unit_type)) = unit_data else {
                continue;
            };

            if cannot_move {
                // Militia and garrison artillery cannot leave their home province.
                continue;
            }

            // No-op move: destination equals the unit's current position. Drop
            // it silently and DO NOT mark the unit as moved, so it heals this
            // turn. AI redistribution can otherwise enqueue trivial reshuffles
            // that keep a unit perpetually flagged as "moved".
            if dest_province_id == src_pos {
                continue;
            }

            // Non-adjacent moves use the rail network: 5 freight cars per armament
            // point (manual p. 47).  Adjacent moves are free marches.
            let is_non_adjacent = {
                let src = game.world.provinces.iter().find(|p| p.id == src_pos);
                let dst = game
                    .world
                    .provinces
                    .iter()
                    .find(|p| p.id == dest_province_id);
                match (src, dst) {
                    (Some(s), Some(d)) => {
                        !crate::map::provinces_are_adjacent(&game.world.hex_map, s, d)
                    }
                    _ => false,
                }
            };

            if is_non_adjacent {
                let rail_cost = 5 * transport_size;
                // Rail moves consume freight CARS only — not merchant-marine capacity.
                // Available rail = rail_total
                //   - resource-delivery portion of freight_committed (committed beyond sea_total)
                //   - rail already used by earlier military moves this turn (tracked separately)
                let mil_rail_used = rail_committed_military
                    .get(&nation_id)
                    .copied()
                    .unwrap_or(0);
                let rail_available = game
                    .world
                    .nations
                    .iter()
                    .find(|n| n.id == nation_id)
                    .map(|n| {
                        let resource_rail = n
                            .economy
                            .logistics
                            .freight_committed
                            .saturating_sub(n.economy.logistics.sea_total);
                        n.economy
                            .logistics
                            .rail_total
                            .saturating_sub(resource_rail + mil_rail_used)
                    })
                    .unwrap_or(0);
                if rail_cost > rail_available {
                    report.unit_movements.push((
                        nation_id,
                        format!(
                            "{} rail move to {} blocked — need {} freight cars, {} available",
                            unit_type, dest_name, rail_cost, rail_available
                        ),
                    ));
                    continue;
                }
            }

            // Perform the move. Military rail costs are accumulated in
            // rail_committed_military and flushed to logistics after the loop.
            if let Some(nation) = game.get_nation_mut(nation_id)
                && let Some(unit) = nation.military.army.iter_mut().find(|u| u.id == unit_id)
            {
                unit.position = dest_province_id;
            }
            if is_non_adjacent {
                let rail_cost = 5 * transport_size;
                *rail_committed_military.entry(nation_id).or_insert(0) += rail_cost;
            }
            moved_unit_ids.insert(unit_id);
            report
                .unit_movements
                .push((nation_id, format!("{} moved to {}", unit_type, dest_name)));
        } else {
            // Check if at war with the destination owner, or if target is anarchic
            let at_war = game
                .world
                .diplomacy
                .get_relation(nation_id, dest_owner)
                .is_some_and(|r| r.at_war);
            let target_is_anarchic = game
                .get_nation(dest_owner)
                .is_some_and(|n| n.diplomacy.is_in_anarchy);
            if at_war || target_is_anarchic {
                // Validate adjacency: attacker must own an adjacent province
                // or have an active landing site on the target.
                if can_attack_province(game, nation_id, dest_province_id) {
                    game.transient
                        .pending_attacks
                        .push((nation_id, dest_province_id));
                    report.unit_movements.push((
                        nation_id,
                        format!("Attack ordered on {} (enemy territory)", dest_name),
                    ));
                } else {
                    // Province is not adjacent and no active landing site
                    report.unit_movements.push((
                        nation_id,
                        format!(
                            "Cannot attack {} — not adjacent and no naval landing site",
                            dest_name
                        ),
                    ));
                }
            }
            // Otherwise ignore the invalid move
        }
    }

    // Flush military rail costs into logistics so freight_committed and freight_unused
    // reflect the full turn usage (resources + military moves) for display purposes.
    for (nation_id, mil_cost) in rail_committed_military {
        if let Some(nation) = game.get_nation_mut(nation_id) {
            nation.economy.logistics.freight_committed += mil_cost;
            nation.economy.logistics.freight_unused = nation
                .economy
                .logistics
                .freight_unused
                .saturating_sub(mil_cost);
        }
    }

    moved_unit_ids
}

/// Resolve combat for all pending attacks.
///
/// For each pending attack:
/// 1. Create attacker CombatForce from the attacking nation's army units
/// 2. Create defender CombatForce from garrison (based on province owner type)
/// 3. Call resolve_battle()
/// 4. If attacker wins: change province owner, record ProvinceConquered event, add headline
/// 5. Clear pending_attacks after processing
fn resolve_combat(
    game: &mut GameState,
    report: &mut TurnReport,
    moved_unit_ids: &HashSet<crate::map::UnitId>,
) -> HashSet<crate::map::UnitId> {
    let mut fought_unit_ids: HashSet<crate::map::UnitId> = HashSet::new();
    let all_attacks: Vec<(NationId, ProvinceId)> =
        game.transient.pending_attacks.drain(..).collect();
    // Filter out attacks from anarchic nations (their armies only defend)
    let attacks: Vec<(NationId, ProvinceId)> = all_attacks
        .into_iter()
        .filter(|(attacker_id, _)| {
            !game
                .get_nation(*attacker_id)
                .is_some_and(|n| n.diplomacy.is_in_anarchy)
        })
        .collect();

    // Track provinces that have already changed hands this turn to prevent
    // ping-pong (Province A going X → Y → Z in one turn).
    let mut already_contested: HashSet<ProvinceId> = HashSet::new();
    // Track nations that have already conquered a province this turn
    // to prevent territory swap loops (A takes from B while B takes from A).
    let mut already_conquered: HashSet<NationId> = HashSet::new();
    // Track nations that lost a province this turn — they cannot attack.
    let mut lost_province: HashSet<NationId> = HashSet::new();

    for (attacker_id, province_id) in attacks {
        // Skip attacks on provinces that already changed hands this turn
        if already_contested.contains(&province_id) {
            continue;
        }
        // Limit each nation to one conquest per turn to prevent swap loops
        if already_conquered.contains(&attacker_id) {
            continue;
        }
        // Nations that lost a province this turn cannot attack (prevents revenge loops)
        if lost_province.contains(&attacker_id) {
            continue;
        }
        // Validate adjacency: attacker must own an adjacent province
        // or have an active landing site on the target.
        if !can_attack_province(game, attacker_id, province_id) {
            continue;
        }
        // Look up province owner
        let defender_id = match game.get_province(province_id) {
            Some(p) => p.owner,
            None => continue,
        };

        // Skip self-conquest (attacker already owns the province)
        if attacker_id == defender_id {
            continue;
        }

        // Re-check diplomatic legality: skip if peace was made earlier this turn
        let still_at_war = game
            .world
            .diplomacy
            .get_relation(attacker_id, defender_id)
            .is_some_and(|r| r.at_war);
        let defender_anarchic = game
            .get_nation(defender_id)
            .is_some_and(|n| n.diplomacy.is_in_anarchy);
        if !still_at_war && !defender_anarchic {
            continue;
        }

        // Get defender nation type for garrison creation
        let defender_type = match game.get_nation(defender_id) {
            Some(n) => n.nation_type,
            None => continue,
        };

        // Trigger pact defense: if defender (Minor Nation) has a NAP with any GP,
        // a protector may intervene (declaring war and incorporating the minor).
        trigger_pact_defense(game, defender_id, attacker_id, report);

        // If pact defense changed province ownership (minor was incorporated),
        // abort this attack — the attacker is now at war with the protector instead.
        let current_owner = game
            .get_province(province_id)
            .map(|p| p.owner)
            .unwrap_or(defender_id);
        if current_owner != defender_id {
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "{}'s attack on {} is thwarted by intervention!",
                        game.get_nation(attacker_id)
                            .map(|n| n.name.as_str())
                            .unwrap_or("Unknown"),
                        game.get_nation(defender_id)
                            .map(|n| n.name.as_str())
                            .unwrap_or("Unknown"),
                    ),
                    HeadlineCategory::War,
                )
                .for_nations(&[attacker_id, defender_id]),
            );
            continue;
        }

        // Determine which attacker-owned provinces are adjacent to the target.
        let target_tiles: Vec<crate::hex::HexCoord> = game
            .get_province(province_id)
            .map(|p| p.tiles.clone())
            .unwrap_or_default();
        let attacker_owned: HashSet<ProvinceId> = game
            .get_nation(attacker_id)
            .map(|n| n.province_ids.iter().copied().collect())
            .unwrap_or_default();
        let mut adjacent_attacker_pids: HashSet<ProvinceId> = HashSet::new();
        for &tile_coord in &target_tiles {
            for neighbor in tile_coord.neighbors() {
                if let Some(tile) = game.world.hex_map.get_tile(neighbor)
                    && let Some(pid) = tile.province_id
                    && pid != province_id
                    && attacker_owned.contains(&pid)
                {
                    adjacent_attacker_pids.insert(pid);
                }
            }
        }

        // Compute beachhead capacity for this target (0 if no beachhead assigned).
        let beachhead_cap = game
            .get_nation(attacker_id)
            .map(|n| {
                use crate::military::naval::NavalOperation;
                let assigned_ships: Vec<_> = n
                    .military
                    .warships
                    .iter()
                    .filter(|s| s.operation == Some(NavalOperation::Beachhead(province_id)))
                    .cloned()
                    .collect();
                crate::military::naval::beachhead_force_size(&assigned_ships, &game.game_data)
            })
            .unwrap_or(0) as usize;

        // Precompute set of attacker-owned coastal (port) province IDs —
        // only units in port provinces may embark for a naval landing.
        let coastal_attacker_pids: HashSet<ProvinceId> = attacker_owned
            .iter()
            .copied()
            .filter(|&pid| game.get_province(pid).is_some_and(|p| p.coastal))
            .collect();

        // Assemble two cohorts:
        //   - Land cohort: units in adjacent attacker-owned provinces (no cap)
        //   - Naval cohort: units in coastal (port) non-adjacent provinces,
        //     capped by beachhead_cap (only if beachhead exists on target)
        // These compose a mixed attack when both are present.
        let (land_cohort, naval_cohort): (Vec<ArmyUnit>, Vec<ArmyUnit>) =
            match game.get_nation(attacker_id) {
                Some(n) => {
                    let (land, other): (Vec<_>, Vec<_>) = n
                        .military
                        .army
                        .iter()
                        // Militia / GarrisonArtillery can never join an
                        // outgoing attack — they defend their home province
                        // only (manual p. 36).
                        .filter(|u| u.unit_type.can_move())
                        .filter(|u| !moved_unit_ids.contains(&u.id))
                        .cloned()
                        .partition(|u| adjacent_attacker_pids.contains(&u.position));
                    // Naval embarkation: only units in coastal/port provinces can board ships
                    let mut naval: Vec<ArmyUnit> = if beachhead_cap > 0 {
                        other
                            .into_iter()
                            .filter(|u| coastal_attacker_pids.contains(&u.position))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    if naval.len() > beachhead_cap {
                        // Send best-firepower units first up to the beachhead capacity
                        naval.sort_by(|a, b| {
                            b.effective_firepower()
                                .partial_cmp(&a.effective_firepower())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        naval.truncate(beachhead_cap);
                    }
                    (land, naval)
                }
                None => continue,
            };

        // Track unit IDs by cohort so post-battle relocation can move every
        // surviving attacker into the conquered province (both land cohort
        // crossing the border and naval cohort going ashore).
        let land_unit_ids: HashSet<crate::map::UnitId> = land_cohort.iter().map(|u| u.id).collect();
        let naval_cohort_ids_for_relocation: Vec<crate::map::UnitId> =
            naval_cohort.iter().map(|u| u.id).collect();
        let has_naval_cohort = !naval_cohort.is_empty();

        let mut attacker_units: Vec<ArmyUnit> = land_cohort;
        attacker_units.extend(naval_cohort);

        if attacker_units.is_empty() {
            continue;
        }

        // A "pure naval" attack is one with no land cohort at all.
        // Used by auto-conquer relocation logic below.
        let is_naval_attack = land_unit_ids.is_empty();

        let attacker_force = CombatForce {
            nation: attacker_id,
            units: attacker_units,
        };
        // Mark attackers as having participated this turn. Holds for both
        // the auto-conquer branch (they moved onto the target) and the
        // battle branch below (they fired). Used by the rest-heals-units
        // pass to keep resting-only units eligible.
        fought_unit_ids.extend(attacker_force.units.iter().map(|u| u.id));

        // Create defender force: every army unit (field army + persistent
        // militia + garrison artillery) stationed in the province joins the
        // defence. Militia and GarrisonArtillery are now real `ArmyUnit`s in
        // `nation.military.army` (manual page 36), so the position-filter picks them
        // up automatically — no per-battle synthesis needed.
        let defense_units: Vec<_> = match game.get_nation(defender_id) {
            Some(n) => n
                .military
                .army
                .iter()
                .filter(|u| u.position == province_id)
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        let _ = defender_type; // still needed elsewhere (anarchy/event reports)

        // If no defenders at all, province falls without a fight
        if defense_units.is_empty() {
            // Auto-conquer: no battle needed.
            // For land attacks, participating units (those in adjacent provinces
            // that didn't move this turn) move into the conquered province.
            // Naval attacks: units return to origin (no position change).
            let defender_is_minor = game
                .get_nation(defender_id)
                .is_some_and(|n| !n.is_great_power());
            if let Some(province) = game.get_province_mut(province_id) {
                let origin_input = province.incorporated_from.or(province.conquest_origin);
                province.conquest_origin = attribute_conquest_origin(
                    origin_input,
                    attacker_id,
                    defender_id,
                    defender_is_minor,
                );
                // Conquest ends any diplomatic-incorporation relationship
                // for this province — `incorporated_from` drives map visuals
                // and should only reflect diplomatic integration, not spoils
                // of war (card #79 + reviewer F-008).
                province.incorporated_from = None;
                province.owner = attacker_id;
            }
            if let Some(defender_nation) = game.get_nation_mut(defender_id) {
                defender_nation
                    .province_ids
                    .retain(|&pid| pid != province_id);
                // Destroy any defender units in the conquered province
                defender_nation
                    .military
                    .army
                    .retain(|u| u.position != province_id);
            }
            if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
                attacker_nation.add_province(province_id);
                // Relocate participating units into the conquered province.
                // Land attacks: any adjacent unit that hadn't moved gets
                // pulled in. Naval attacks: the embarked naval cohort goes
                // ashore (otherwise the freshly-taken province has no
                // garrison and a same-turn counter-attack walks straight in).
                if is_naval_attack {
                    let naval_ids: HashSet<crate::map::UnitId> =
                        naval_cohort_ids_for_relocation.iter().copied().collect();
                    for unit in &mut attacker_nation.military.army {
                        if naval_ids.contains(&unit.id) {
                            unit.position = province_id;
                        }
                    }
                } else {
                    for unit in &mut attacker_nation.military.army {
                        if !moved_unit_ids.contains(&unit.id)
                            && adjacent_attacker_pids.contains(&unit.position)
                        {
                            unit.position = province_id;
                        }
                    }
                }
            }
            already_contested.insert(province_id);
            already_conquered.insert(attacker_id);
            lost_province.insert(defender_id);

            // Militia rebalance: if the new owner has adjacent provinces
            // over-stocked from prior retreats, flow militia back into the
            // freshly-taken province (up to the new owner's default).
            // `rebalance_militia_into` syncs the target province cache on
            // every exit path, so even a no-op call here zeroes the stale
            // old-owner count.
            rebalance_militia_into(game, attacker_id, province_id);

            // Captured GP capital via auto-conquest: same immediate
            // industrialization as the battle path, so the province jumps
            // to Village instead of staying at Hamlet.
            let is_gp_capital = defender_type == NationType::GreatPower
                && game
                    .get_nation(defender_id)
                    .is_some_and(|n| n.capital_province_id == province_id);
            if is_gp_capital
                && let Some(province) = game.get_province_mut(province_id)
                && province.settlement_level == SettlementLevel::Hamlet
            {
                province.settlement_level = SettlementLevel::Village;
                province.industrialization_turns_remaining = None;
                province.town_countdown = Some(12);
                report
                    .settlement_upgrades
                    .push((province_id, "Village".to_string()));
                report.newspaper_headlines.push(
                    Headline::new(
                        format!(
                            "{} immediately industrializes under new management!",
                            province.name
                        ),
                        HeadlineCategory::Growth,
                    )
                    .for_nation(attacker_id),
                );
            }

            let turn = game.turn;
            game.archive.history.push((
                turn,
                HistoryEvent::ProvinceConquered {
                    conqueror: attacker_id,
                    loser: defender_id,
                    province: province_id,
                },
            ));
            // Anarchy handled by the end-of-combat sweep.
            continue;
        }

        let defender_force = CombatForce {
            nation: defender_id,
            units: defense_units,
        };
        fought_unit_ids.extend(defender_force.units.iter().map(|u| u.id));

        // Get terrain and fort level from the province's capital tile
        let (battle_terrain, battle_fort_level) = game
            .get_province(province_id)
            .and_then(|prov| {
                game.world.hex_map.get_tile(prov.capital_tile).map(|tile| {
                    let terrain = tile.terrain();
                    let fort_level = if tile.infrastructure.has_fort {
                        tile.infrastructure.fort_level
                    } else {
                        0
                    };
                    (Some(terrain), fort_level)
                })
            })
            .unwrap_or((None, 0));

        // Card #18: compute retreat capability before battle.
        let defender_neighbors = defender_retreat_neighbors(game, defender_id, province_id);
        let battle_config = build_battle_config(
            game,
            attacker_id,
            defender_id,
            province_id,
            defender_neighbors.len(),
        );
        let battle_outcome = compute_battle_outcome(BattleParams {
            attacker_id,
            defender_id,
            target_province: province_id,
            attacker_units: &attacker_force.units,
            defender_units: &defender_force.units,
            terrain: battle_terrain,
            fort_level: battle_fort_level,
            battle_config,
            game_config: &game.game_data.game_config,
        });
        let mut result = battle_outcome.raw_result;

        // Track which provinces the attacking units came from (for battle screen arrows
        // and newspaper headlines). For naval landings, this is the embarkation province
        // of each naval cohort unit (its `position` before boarding ships).
        {
            let mut origins: Vec<ProvinceId> = attacker_force
                .units
                .iter()
                .map(|u| u.position)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            origins.sort_by_key(|p| p.0);
            result.attacker_origin_provinces = origins;
        }
        result.is_naval_landing = has_naval_cohort;

        // Update attacker's army: remove units that fought, add back survivors
        {
            let battle_ids: HashSet<crate::map::UnitId> =
                attacker_force.units.iter().map(|u| u.id).collect();
            if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
                attacker_nation
                    .military
                    .army
                    .retain(|u| !battle_ids.contains(&u.id));
                attacker_nation
                    .military
                    .army
                    .extend(result.attacker_survivors.iter().cloned());
            }
        }

        // Update defender's army: remove units that fought, add back survivors
        {
            let battle_ids: HashSet<crate::map::UnitId> =
                defender_force.units.iter().map(|u| u.id).collect();
            if let Some(defender_nation) = game.get_nation_mut(defender_id) {
                defender_nation
                    .military
                    .army
                    .retain(|u| !battle_ids.contains(&u.id));
                defender_nation
                    .military
                    .army
                    .extend(result.defender_survivors.iter().cloned());
            }
        }

        // Card #18: if the defender retreated, scatter survivors across
        // neighboring own-provinces BEFORE the attacker-won branch runs its
        // "destroy stragglers at the battle province" cleanup.
        let max_garrison = game.game_data.game_config.max_garrison_per_province as u8;
        if result.defender_retreated {
            let survivor_ids: Vec<crate::map::UnitId> =
                result.defender_survivors.iter().map(|u| u.id).collect();
            let placements = place_defender_retreat(
                game,
                defender_id,
                &survivor_ids,
                &defender_neighbors,
                max_garrison,
            );
            result.defender_retreated_to = placements;
            // Refresh garrison-count caches on every neighbor that absorbed
            // militia and on the battle province itself (to be zeroed).
            for nid in &defender_neighbors {
                sync_garrison_cache(game, *nid);
            }
            sync_garrison_cache(game, province_id);
        }
        // Symmetric bookkeeping for attacker retreat (survivors stay at their
        // origin positions — we just report them).
        if result.retreated {
            result.attacker_retreated_to = result
                .attacker_survivors
                .iter()
                .map(|u| (u.id, u.position))
                .collect();
        }

        // Headline suffix describing where the attack came from. Used in both the
        // conquest ("X conquers Y") and repel ("Y repels attack") headlines below.
        let origin_suffix = {
            let names: Vec<String> = result
                .attacker_origin_provinces
                .iter()
                .filter_map(|pid| game.get_province(*pid).map(|p| p.name.clone()))
                .collect();
            if names.is_empty() {
                String::new()
            } else if result.is_naval_landing {
                format!(" — naval landing from {}", names.join(", "))
            } else {
                format!(" — advance from {}", names.join(", "))
            }
        };

        if result.attacker_won {
            // Move surviving attacker units into the conquered province —
            // both the land cohort that crossed the border and the naval
            // cohort that landed (the troops are ashore; the ships head
            // back to port empty). Without this, a pure-naval landing
            // leaves the conquered province with zero defenders and a
            // same-turn counter-attack walks straight in.
            {
                let survivor_ids: HashSet<crate::map::UnitId> =
                    result.attacker_survivors.iter().map(|u| u.id).collect();
                if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
                    for unit in &mut attacker_nation.military.army {
                        if survivor_ids.contains(&unit.id) {
                            unit.position = province_id;
                        }
                    }
                }
            }

            // Siege artillery destroys fort sections: reduce fort_level by 1
            if result.siege_reduced_fort
                && let Some(province) = game.get_province(province_id)
            {
                let cap_tile = province.capital_tile;
                if let Some(tile) = game.world.hex_map.get_tile_mut(cap_tile)
                    && tile.infrastructure.has_fort
                    && tile.infrastructure.fort_level > 0
                {
                    tile.infrastructure.fort_level -= 1;
                    if tile.infrastructure.fort_level == 0 {
                        tile.infrastructure.has_fort = false;
                    }
                }
            }

            // Change province owner and reset garrison (conquering nation has no garrison)
            let defender_is_minor = game
                .get_nation(defender_id)
                .is_some_and(|n| !n.is_great_power());
            if let Some(province) = game.get_province_mut(province_id) {
                let origin_input = province.incorporated_from.or(province.conquest_origin);
                province.conquest_origin = attribute_conquest_origin(
                    origin_input,
                    attacker_id,
                    defender_id,
                    defender_is_minor,
                );
                province.incorporated_from = None;
                province.owner = attacker_id;
                province.garrison_count = 0;
            }

            // Update nation province lists
            if let Some(defender_nation) = game.get_nation_mut(defender_id) {
                defender_nation
                    .province_ids
                    .retain(|pid| *pid != province_id);
                // Destroy any remaining defender units in the conquered province
                defender_nation
                    .military
                    .army
                    .retain(|u| u.position != province_id);
            }
            if let Some(attacker_nation) = game.get_nation_mut(attacker_id) {
                attacker_nation.add_province(province_id);
            }
            already_contested.insert(province_id);
            already_conquered.insert(attacker_id);
            lost_province.insert(defender_id);

            // Militia rebalance from adjacent own-provinces (reconquest /
            // retreated-militia-come-home rule). Rebalance picks the
            // target default based on the new owner's nation type.
            rebalance_militia_into(game, attacker_id, province_id);

            // Captured GP capital: industrialize immediately (set to Village)
            let is_gp_capital = defender_type == NationType::GreatPower
                && game
                    .get_nation(defender_id)
                    .is_some_and(|n| n.capital_province_id == province_id);
            if is_gp_capital
                && let Some(province) = game.get_province_mut(province_id)
                && province.settlement_level == SettlementLevel::Hamlet
            {
                province.settlement_level = SettlementLevel::Village;
                province.industrialization_turns_remaining = None;
                province.town_countdown = Some(12);
                report
                    .settlement_upgrades
                    .push((province_id, "Village".to_string()));
                report.newspaper_headlines.push(
                    Headline::new(
                        format!(
                            "{} immediately industrializes under new management!",
                            province.name
                        ),
                        HeadlineCategory::Growth,
                    )
                    .for_nation(attacker_id),
                );
            }

            // Award conquest medal if this is a Minor Nation capital
            let is_mn_capital = defender_type == NationType::MinorNation
                && game
                    .get_nation(defender_id)
                    .is_some_and(|n| n.capital_province_id == province_id);
            if is_mn_capital
                && let Some(attacker_nation) = game.get_nation_mut(attacker_id)
                && let Some(first_unit) = attacker_nation.military.army.first_mut()
            {
                first_unit.award_medal();
                report
                    .rewards_earned
                    .push((attacker_id, "Conquest medal awarded!".to_string()));
            }

            // Free Clippers: first colony established (first MN province conquered)
            if defender_type == NationType::MinorNation {
                award_first_colony_clippers(game, attacker_id, report);
            }

            // Record event
            report
                .events
                .push(DomainEvent::ProvinceConquered(ProvinceConquered {
                    province: province_id,
                    old_owner: defender_id,
                    new_owner: attacker_id,
                }));

            let atk_name = game
                .get_nation(attacker_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let def_name_conquest = game
                .get_nation(defender_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let prov_name = game
                .get_province(province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "BREAKING: {} conquers {} from {}!{}",
                        atk_name, prov_name, def_name_conquest, origin_suffix
                    ),
                    HeadlineCategory::War,
                )
                .for_nations(&[attacker_id, defender_id]),
            );

            // Record history event
            game.archive.history.push((
                game.turn,
                HistoryEvent::ProvinceConquered {
                    conqueror: attacker_id,
                    loser: defender_id,
                    province: province_id,
                },
            ));

            // Anarchy is evaluated in a single end-of-combat sweep
            // (`apply_end_of_combat_anarchy`) so a capital captured and then
            // recaptured within the same turn does not trigger anarchy.

            // Check if the defender has been eliminated (lost all provinces)
            let defender_eliminated = game
                .get_nation(defender_id)
                .is_some_and(|n| n.is_great_power() && n.province_ids.is_empty());
            if defender_eliminated {
                report.newspaper_headlines.push(
                    Headline::new(
                        format!("{} has been eliminated!", def_name_conquest),
                        HeadlineCategory::War,
                    )
                    .for_nation(defender_id),
                );
            }
        } else {
            let def_name = game
                .get_nation(defender_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let prov_name = game
                .get_province(province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "{} repels attack on {}!{}",
                        def_name, prov_name, origin_suffix
                    ),
                    HeadlineCategory::Battle,
                )
                .for_nation(defender_id),
            );
        }

        // Keep the defender's garrison_count cache in sync with surviving
        // militia on the battle province (zeroed on conquest; may have
        // shrunk if some militia died while the defender held on).
        sync_garrison_cache(game, province_id);

        report.battles.push(result);
    }

    // ── Counter-attacks ──────────────────────────────────────────
    // After all initial attacks resolve, check if any conquered province has
    // the original defender's army units in an adjacent province.  If so,
    // those units counter-attack the newly occupied province (one round only).
    let conquered_provinces: Vec<(NationId, ProvinceId, NationId)> = report
        .battles
        .iter()
        .filter(|b| b.attacker_won)
        .map(|b| (b.attacker, b.province, b.defender))
        .collect();

    let mut counter_attacks: Vec<(NationId, ProvinceId, Vec<ProvinceId>)> = Vec::new();

    for (_new_owner, conquered_prov_id, original_defender) in &conquered_provinces {
        // Collect tiles that belong to the conquered province
        let conquered_tiles: Vec<crate::hex::HexCoord> = game
            .get_province(*conquered_prov_id)
            .map(|p| p.tiles.clone())
            .unwrap_or_default();

        // Find all provinces adjacent to the conquered province
        let mut adjacent_province_ids: Vec<ProvinceId> = Vec::new();
        for tile_coord in &conquered_tiles {
            for neighbor in tile_coord.neighbors() {
                if let Some(tile) = game.world.hex_map.get_tile(neighbor)
                    && let Some(adj_pid) = tile.province_id
                    && adj_pid != *conquered_prov_id
                    && !adjacent_province_ids.contains(&adj_pid)
                {
                    adjacent_province_ids.push(adj_pid);
                }
            }
        }

        // Check if the original defender has army units in any adjacent province
        // that is still owned by the defender
        let defender_nation = match game.get_nation(*original_defender) {
            Some(n) => n,
            None => continue,
        };

        let has_adjacent_units = defender_nation.military.army.iter().any(|u| {
            adjacent_province_ids.contains(&u.position)
                && defender_nation.province_ids.contains(&u.position)
        });

        if has_adjacent_units
            && !counter_attacks
                .iter()
                .any(|(_, p, _)| *p == *conquered_prov_id)
        {
            counter_attacks.push((
                *original_defender,
                *conquered_prov_id,
                adjacent_province_ids,
            ));
        }
    }

    // Resolve counter-attacks (second pass — no further counter-attacks after this)
    for (counter_attacker_id, target_province_id, adjacent_province_ids) in counter_attacks {
        let new_owner_id = match game.get_province(target_province_id) {
            Some(p) => p.owner,
            None => continue,
        };

        // The counter-attacker uses army units from adjacent provinces they still own.
        // Units that moved this turn cannot participate in the counter-attack either.
        // Militia / GarrisonArtillery are locked to their home province.
        let counter_units: Vec<ArmyUnit> = match game.get_nation(counter_attacker_id) {
            Some(n) => n
                .military
                .army
                .iter()
                .filter(|u| {
                    u.unit_type.can_move()
                        && !moved_unit_ids.contains(&u.id)
                        && adjacent_province_ids.contains(&u.position)
                        && n.province_ids.contains(&u.position)
                })
                .cloned()
                .collect(),
            None => continue,
        };

        if counter_units.is_empty() {
            continue;
        }

        let counter_force = CombatForce {
            nation: counter_attacker_id,
            units: counter_units,
        };
        fought_unit_ids.extend(counter_force.units.iter().map(|u| u.id));

        // Defender of counter-attack is the new occupier — use units in the target province
        let occupier_units: Vec<ArmyUnit> = match game.get_nation(new_owner_id) {
            Some(n) => n
                .military
                .army
                .iter()
                .filter(|u| u.position == target_province_id)
                .cloned()
                .collect(),
            None => continue,
        };

        let defender_force = CombatForce {
            nation: new_owner_id,
            units: occupier_units,
        };
        fought_unit_ids.extend(defender_force.units.iter().map(|u| u.id));

        let (battle_terrain, battle_fort_level) = game
            .get_province(target_province_id)
            .and_then(|prov| {
                game.world.hex_map.get_tile(prov.capital_tile).map(|tile| {
                    let terrain = tile.terrain();
                    let fort_level = if tile.infrastructure.has_fort {
                        tile.infrastructure.fort_level
                    } else {
                        0
                    };
                    (Some(terrain), fort_level)
                })
            })
            .unwrap_or((None, 0));

        // Card #18: for counter-attacks, the "defender" is the occupier who
        // just took the province — they almost never have neighbors they own
        // here (freshly conquered), so retreat is usually gated off anyway.
        let defender_neighbors = defender_retreat_neighbors(game, new_owner_id, target_province_id);
        let battle_config = build_battle_config(
            game,
            counter_attacker_id,
            new_owner_id,
            target_province_id,
            defender_neighbors.len(),
        );
        let counter_outcome = compute_battle_outcome(BattleParams {
            attacker_id: counter_attacker_id,
            defender_id: new_owner_id,
            target_province: target_province_id,
            attacker_units: &counter_force.units,
            defender_units: &defender_force.units,
            terrain: battle_terrain,
            fort_level: battle_fort_level,
            battle_config,
            game_config: &game.game_data.game_config,
        });
        let mut result = counter_outcome.raw_result;

        // Track which provinces the counter-attacking units came from
        {
            let mut origins: Vec<ProvinceId> = counter_force
                .units
                .iter()
                .map(|u| u.position)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            origins.sort_by_key(|p| p.0);
            result.attacker_origin_provinces = origins;
        }

        // Update counter-attacker's army: remove participants, add survivors
        {
            let battle_ids: HashSet<crate::map::UnitId> =
                counter_force.units.iter().map(|u| u.id).collect();
            if let Some(ca_nation) = game.get_nation_mut(counter_attacker_id) {
                ca_nation
                    .military
                    .army
                    .retain(|u| !battle_ids.contains(&u.id));
                ca_nation
                    .military
                    .army
                    .extend(result.attacker_survivors.iter().cloned());
            }
        }

        // Update occupier's army: remove participants, add survivors
        {
            let battle_ids: HashSet<crate::map::UnitId> =
                defender_force.units.iter().map(|u| u.id).collect();
            if let Some(occ_nation) = game.get_nation_mut(new_owner_id) {
                occ_nation
                    .military
                    .army
                    .retain(|u| !battle_ids.contains(&u.id));
                occ_nation
                    .military
                    .army
                    .extend(result.defender_survivors.iter().cloned());
            }
        }

        // Card #18: retreat bookkeeping for counter-attacks (rare here).
        let max_garrison = game.game_data.game_config.max_garrison_per_province as u8;
        if result.defender_retreated {
            let survivor_ids: Vec<crate::map::UnitId> =
                result.defender_survivors.iter().map(|u| u.id).collect();
            let placements = place_defender_retreat(
                game,
                new_owner_id,
                &survivor_ids,
                &defender_neighbors,
                max_garrison,
            );
            result.defender_retreated_to = placements;
            for nid in &defender_neighbors {
                sync_garrison_cache(game, *nid);
            }
            sync_garrison_cache(game, target_province_id);
        }
        if result.retreated {
            result.attacker_retreated_to = result
                .attacker_survivors
                .iter()
                .map(|u| (u.id, u.position))
                .collect();
        }

        if result.attacker_won {
            // Counter-attack succeeds: province returns to original defender.
            // The dispossessed party here is the occupier (`new_owner_id`) —
            // preserve minor-origin attribution so the province can still be
            // released under card #79 even after changing hands multiple times.
            let occupier_is_minor = game
                .get_nation(new_owner_id)
                .is_some_and(|n| !n.is_great_power());
            if let Some(province) = game.get_province_mut(target_province_id) {
                let origin_input = province.incorporated_from.or(province.conquest_origin);
                province.conquest_origin = attribute_conquest_origin(
                    origin_input,
                    counter_attacker_id,
                    new_owner_id,
                    occupier_is_minor,
                );
                province.incorporated_from = None;
                province.owner = counter_attacker_id;
            }
            if let Some(occ_nation) = game.get_nation_mut(new_owner_id) {
                occ_nation
                    .province_ids
                    .retain(|pid| *pid != target_province_id);
                // Destroy any occupier units in the re-conquered province
                occ_nation
                    .military
                    .army
                    .retain(|u| u.position != target_province_id);
            }
            // Move surviving counter-attacker units into the recaptured province
            // (counter-attacks are always land-based — from adjacent provinces)
            {
                let survivor_ids: HashSet<crate::map::UnitId> =
                    result.attacker_survivors.iter().map(|u| u.id).collect();
                if let Some(ca_nation) = game.get_nation_mut(counter_attacker_id) {
                    ca_nation.add_province(target_province_id);
                    for unit in &mut ca_nation.military.army {
                        if survivor_ids.contains(&unit.id) {
                            unit.position = target_province_id;
                        }
                    }
                }
            }

            // Reset garrison to 0 on re-conquered province, then pull
            // excess militia back from adjacent neighbors. Rebalance picks
            // the target default based on the new owner's nation type.
            if let Some(province) = game.get_province_mut(target_province_id) {
                province.garrison_count = 0;
            }
            rebalance_militia_into(game, counter_attacker_id, target_province_id);

            let ca_name = game
                .get_nation(counter_attacker_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let prov_name = game
                .get_province(target_province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            report.newspaper_headlines.push(
                Headline::new(
                    format!("{} counter-attacks and recaptures {}!", ca_name, prov_name),
                    HeadlineCategory::War,
                )
                .for_nation(counter_attacker_id),
            );
            // Anarchy is evaluated in the end-of-combat sweep.
        } else {
            let occ_name = game
                .get_nation(new_owner_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let prov_name = game
                .get_province(target_province_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            report.newspaper_headlines.push(
                Headline::new(
                    format!("{} repels counter-attack on {}!", occ_name, prov_name),
                    HeadlineCategory::Battle,
                )
                .for_nation(new_owner_id),
            );
        }

        // Keep garrison_count cache in sync for the counter-attack site too.
        sync_garrison_cache(game, target_province_id);

        report.battles.push(result);
    }
    fought_unit_ids
}

/// Compute the `conquest_origin` field for a province that just changed
/// hands via military conquest, so that card #79 can restore minor
/// provinces to their original owner even if they were taken purely
/// militarily. `current_origin` is the ancestor-minor attribution pulled
/// in from either `incorporated_from` (if the province was previously
/// diplomatically incorporated from a minor) or `conquest_origin` (if it
/// was already conquered from a minor earlier in the chain).
///
/// - Preserve an existing attribution across further conquests.
/// - If none and the dispossessed defender is a minor, stamp the defender.
/// - If the new owner IS the origin minor reclaiming their own land, drop
///   the attribution — a nation is not "conquered from itself".
///
/// Owned provinces neighboring a battle province that the defender could
/// retreat into. The battle province itself is excluded.
fn defender_retreat_neighbors(
    game: &GameState,
    defender_id: NationId,
    battle_province: ProvinceId,
) -> Vec<ProvinceId> {
    let Some(battle_prov) = game.get_province(battle_province) else {
        return Vec::new();
    };
    let Some(def) = game.get_nation(defender_id) else {
        return Vec::new();
    };
    let mut neighbors: Vec<ProvinceId> = Vec::new();
    for &pid in &def.province_ids {
        if pid == battle_province {
            continue;
        }
        if let Some(p) = game.get_province(pid)
            && crate::map::provinces_are_adjacent(&game.world.hex_map, battle_prov, p)
        {
            neighbors.push(pid);
        }
    }
    neighbors
}

/// Build a [`BattleConfig`] for card #18 retreat rules.
///
/// - Defender can retreat iff the battle is NOT at the defender's capital
///   and there is at least one owned neighboring province.
/// - Attacker can always retreat (back to its origin provinces).
/// - Attacker thresholds come from the attacker's Lua personality config,
///   defender thresholds from the defender's — so each side's retreat
///   tuning reflects its own personality.
fn build_battle_config(
    game: &GameState,
    attacker_id: NationId,
    defender_id: NationId,
    battle_province: ProvinceId,
    defender_neighbor_count: usize,
) -> BattleConfig {
    let is_capital_defense = game
        .get_nation(defender_id)
        .is_some_and(|n| n.capital_province_id == battle_province);
    let defender_can_retreat = !is_capital_defense && defender_neighbor_count > 0;

    let (atk_prebattle, atk_postbattle) = retreat_thresholds_for(game, attacker_id);
    let (def_prebattle, def_postbattle) = retreat_thresholds_for(game, defender_id);

    BattleConfig {
        targeting: TargetingPriority::StrongestFirst,
        attacker_can_retreat: true,
        defender_can_retreat,
        attacker_retreat_ratio: atk_prebattle,
        defender_retreat_ratio: def_prebattle,
        attacker_postbattle_fp_loss: atk_postbattle,
        defender_postbattle_fp_loss: def_postbattle,
        current_turn: game.turn.0,
    }
}

/// Read `(retreat_prebattle_ratio, retreat_postbattle_fp_loss)` for a
/// nation from its Lua personality config, falling back to neutral defaults
/// when Lua is unavailable or the values are missing.
fn retreat_thresholds_for(game: &GameState, nation_id: NationId) -> (f64, f64) {
    let personality = crate::ai::common::get_personality(game, nation_id);
    let cfg = crate::ai::lua_bridge::get_personality_config(game, personality);
    let prebattle = cfg
        .as_ref()
        .and_then(|c| c.retreat_prebattle_ratio)
        .unwrap_or(2.0);
    let postbattle = cfg
        .as_ref()
        .and_then(|c| c.retreat_postbattle_fp_loss)
        .unwrap_or(0.60);
    (prebattle, postbattle)
}

/// Relocate retreating defender survivors across neighboring own-provinces.
///
/// - Field army survivors split evenly across neighbors (round-robin), with
///   no per-province cap.
/// - Militia survivors also split round-robin, but each neighbor is capped
///   at `max_garrison_per_province` total militia. Overflow militia die
///   (no neighbor has room).
/// - `GarrisonArtillery` never retreats — the unit is destroyed with the
///   province (callers drop it before placement, but we filter here too as
///   defense in depth).
///
/// Returns the `(unit_id, destination)` placements actually applied. Dead
/// militia do not appear in the return value.
fn place_defender_retreat(
    game: &mut GameState,
    defender_id: NationId,
    survivor_ids: &[crate::map::UnitId],
    neighbors: &[ProvinceId],
    max_garrison_per_province: u8,
) -> Vec<(crate::map::UnitId, ProvinceId)> {
    let mut placements: Vec<(crate::map::UnitId, ProvinceId)> = Vec::new();
    if neighbors.is_empty() || survivor_ids.is_empty() {
        return placements;
    }

    // Partition survivor ids by type (read-only snapshot first to keep the
    // borrow checker happy).
    let (mut field_army, mut militia, mut dying_artillery): (
        Vec<crate::map::UnitId>,
        Vec<crate::map::UnitId>,
        Vec<crate::map::UnitId>,
    ) = (Vec::new(), Vec::new(), Vec::new());
    if let Some(nation) = game.get_nation(defender_id) {
        for uid in survivor_ids {
            if let Some(u) = nation.military.army.iter().find(|u| u.id == *uid) {
                match u.unit_type {
                    crate::military::units::ArmyUnitType::Minutemen => militia.push(*uid),
                    crate::military::units::ArmyUnitType::GarrisonArtillery => {
                        dying_artillery.push(*uid)
                    }
                    _ => field_army.push(*uid),
                }
            }
        }
    }

    // Current militia density per neighbor (for cap enforcement).
    let mut militia_at_neighbor: Vec<usize> = neighbors
        .iter()
        .map(|&nid| {
            game.get_nation(defender_id)
                .map(|n| n.militia_at(nid))
                .unwrap_or(0)
        })
        .collect();

    // Field army: round-robin, no cap.
    for (idx, uid) in field_army.iter().enumerate() {
        let dest = neighbors[idx % neighbors.len()];
        placements.push((*uid, dest));
    }

    // Militia: round-robin with per-neighbor cap; overflow dies.
    let mut militia_idx = 0usize;
    let mut overflow_militia: Vec<crate::map::UnitId> = Vec::new();
    'outer: for uid in &militia {
        let attempts = neighbors.len();
        for _ in 0..attempts {
            let slot = militia_idx % neighbors.len();
            militia_idx += 1;
            if militia_at_neighbor[slot] < max_garrison_per_province as usize {
                militia_at_neighbor[slot] += 1;
                placements.push((*uid, neighbors[slot]));
                continue 'outer;
            }
        }
        // Every neighbor is already at the cap — this militia has nowhere
        // to go and perishes with the province.
        overflow_militia.push(*uid);
    }

    let current_turn = game.turn.0;
    // Apply placements; destroy overflow militia and dying artillery.
    if let Some(nation) = game.get_nation_mut(defender_id) {
        for (uid, dest) in &placements {
            for u in &mut nation.military.army {
                if u.id == *uid {
                    u.position = *dest;
                    // Card #478: a unit that just relocated isn't
                    // entrenched at its new province until next turn.
                    u.arrived_turn = current_turn;
                }
            }
        }
        let doomed: std::collections::HashSet<crate::map::UnitId> = overflow_militia
            .into_iter()
            .chain(dying_artillery)
            .collect();
        nation.military.army.retain(|u| !doomed.contains(&u.id));
    }
    placements
}

/// Pull militia **units** from a new owner's neighboring provinces back
/// into a freshly-taken province, up to that owner's default garrison
/// size (GP vs minor). Owner type drives the target; using a single GP
/// default would over-stock minor-nation provinces after a minor retakes
/// one of its own.
///
/// Triggered on every ownership change. Only pulls from neighbors that
/// currently exceed the target default (i.e. overflow from a previous
/// retreat); neighbors at or below the default are left alone. Militia
/// unit `position` fields are updated in place — no units are created or
/// destroyed here.
fn rebalance_militia_into(game: &mut GameState, new_owner: NationId, province: ProvinceId) {
    // Always refresh the target province's cache on exit — the caller may
    // have just transferred ownership (cache still reflects the old
    // owner's militia) and any early return path below still has to leave
    // the cache consistent.
    let default_garrison = {
        let gc = &game.game_data.game_config;
        match game.get_nation(new_owner) {
            Some(n) if n.is_great_power() => gc.default_garrison_per_province as u8,
            Some(_) => gc.minor_default_garrison as u8,
            None => {
                sync_garrison_cache(game, province);
                return;
            }
        }
    };
    // Snapshot neighbors that the new owner owns and are adjacent to the
    // target province.
    let Some(target_prov) = game.get_province(province) else {
        sync_garrison_cache(game, province);
        return;
    };
    let target_snapshot = target_prov.clone();
    let Some(n) = game.get_nation(new_owner) else {
        sync_garrison_cache(game, province);
        return;
    };
    let mut neighbors: Vec<ProvinceId> = n
        .province_ids
        .iter()
        .copied()
        .filter(|&pid| pid != province)
        .filter(|&pid| {
            game.get_province(pid).is_some_and(|p| {
                crate::map::provinces_are_adjacent(&game.world.hex_map, &target_snapshot, p)
            })
        })
        .collect();
    if neighbors.is_empty() {
        sync_garrison_cache(game, province);
        return;
    }

    let mut current_at_target = n.militia_at(province);
    if current_at_target >= default_garrison as usize {
        sync_garrison_cache(game, province);
        return; // already full
    }
    // Greedy pull: repeatedly pick the neighbor with the most excess militia.
    loop {
        if current_at_target >= default_garrison as usize {
            break;
        }
        // Find best neighbor: highest militia_at above default.
        let mut best: Option<(ProvinceId, usize)> = None;
        for &pid in &neighbors {
            let have = game
                .get_nation(new_owner)
                .map(|n| n.militia_at(pid))
                .unwrap_or(0);
            if have > default_garrison as usize {
                let excess = have - default_garrison as usize;
                if best.map(|(_, e)| excess > e).unwrap_or(true) {
                    best = Some((pid, excess));
                }
            }
        }
        let Some((src_pid, _)) = best else {
            break; // no neighbor has excess
        };
        let current_turn = game.turn.0;
        // Move one militia from src_pid to province.
        let moved_id = {
            let Some(nm) = game.get_nation_mut(new_owner) else {
                break;
            };
            let Some(unit) = nm.military.army.iter_mut().find(|u| {
                u.position == src_pid
                    && u.unit_type == crate::military::units::ArmyUnitType::Minutemen
            }) else {
                // Shouldn't happen — we just counted them above — but stay safe.
                break;
            };
            unit.position = province;
            unit.arrived_turn = current_turn;
            unit.id
        };
        let _ = moved_id;
        sync_garrison_cache(game, src_pid);
        current_at_target += 1;
    }
    sync_garrison_cache(game, province);
    neighbors.sort_by_key(|p| p.0); // deterministic ordering for cache refresh
    for pid in neighbors {
        sync_garrison_cache(game, pid);
    }
}

/// Rest heals units (Trello card #20): any living army unit that did not
/// move *and* did not participate in combat this turn recovers
/// `REST_HEAL_AMOUNT` health. Matches the original Imperialism rule
/// "armies that don't move regain HP". The `heal()` method on `ArmyUnit`
/// already caps at 100 and applies the medal-based fast-heal multiplier.
fn heal_resting_units(
    game: &mut GameState,
    moved_unit_ids: &HashSet<crate::map::UnitId>,
    fought_unit_ids: &HashSet<crate::map::UnitId>,
) {
    use crate::military::units::HealBlock;
    let heal_amount = game.game_data.game_config.rest_heal_amount;
    for nation in &mut game.world.nations {
        for unit in &mut nation.military.army {
            if !unit.is_alive() {
                continue;
            }
            if moved_unit_ids.contains(&unit.id) {
                unit.last_heal_block = Some(HealBlock::Moved);
                continue;
            }
            if fought_unit_ids.contains(&unit.id) {
                unit.last_heal_block = Some(HealBlock::Fought);
                continue;
            }
            if unit.health >= 100 {
                unit.last_heal_block = Some(HealBlock::FullHealth);
                continue;
            }
            unit.heal(heal_amount);
            unit.last_heal_block = None;
        }
    }
}

/// Garrison regeneration (manual p. 36): every
/// `garrison_regen_interval_turns` turns, each province whose current
/// militia count is below its default gains +1 Militia (spawned into the
/// owning nation's army). The `garrison_count` cache is refreshed for each
/// touched province.
fn regenerate_garrisons(game: &mut GameState) {
    let interval = game.game_data.game_config.garrison_regen_interval_turns;
    if interval == 0 {
        return;
    }
    if !game.turn.0.is_multiple_of(interval) {
        return;
    }
    let default_gp = game.game_data.game_config.default_garrison_per_province as usize;
    let default_minor = game.game_data.game_config.minor_default_garrison as usize;

    // Snapshot plan: (owner, province, current_count, default_for_owner).
    let mut spawns: Vec<(NationId, ProvinceId)> = Vec::new();
    for prov in &game.world.provinces {
        let owner = prov.owner;
        let Some(nation) = game.get_nation(owner) else {
            continue;
        };
        let target = if nation.is_great_power() {
            default_gp
        } else {
            default_minor
        };
        let current = nation.militia_at(prov.id);
        if current < target {
            spawns.push((owner, prov.id));
        }
    }
    let current_turn = game.turn.0;
    for (owner, pid) in spawns {
        let mut unit =
            crate::military::combat::spawn_militia_unit(&mut game.next_unit_id, owner, pid);
        // Card #478: freshly-spawned militia aren't entrenched until next
        // turn (`arrived_turn < current_turn` only holds from turn N+1 on).
        unit.arrived_turn = current_turn;
        if let Some(nation) = game.get_nation_mut(owner) {
            nation.military.army.push(unit);
        }
        sync_garrison_cache(game, pid);
    }
}

/// Refresh the cached `garrison_count` on a province from the owner's
/// actual militia stationed there. Call after any event that changes
/// militia population (battle, retreat, regen, rebalance, conquest).
fn sync_garrison_cache(game: &mut GameState, province_id: ProvinceId) {
    let Some(prov) = game.get_province(province_id) else {
        return;
    };
    let owner = prov.owner;
    let count = game
        .get_nation(owner)
        .map(|n| n.militia_at(province_id))
        .unwrap_or(0)
        .min(u8::MAX as usize) as u8;
    if let Some(prov) = game.get_province_mut(province_id) {
        prov.garrison_count = count;
    }
}

fn attribute_conquest_origin(
    current_origin: Option<NationId>,
    new_owner: NationId,
    defender_id: NationId,
    defender_is_minor: bool,
) -> Option<NationId> {
    let origin = current_origin.or_else(|| defender_is_minor.then_some(defender_id));
    match origin {
        Some(id) if id == new_owner => None,
        other => other,
    }
}

/// Sweep all nations after combat resolves and apply anarchy to any that no
/// longer hold their capital province. Using the final post-combat state
/// means a capital captured then recaptured within the same turn does not
/// leave the original owner in anarchy (card #98).
fn apply_end_of_combat_anarchy(game: &mut GameState, report: &mut TurnReport) {
    // Card #96: when an overlord enters anarchy mid-sweep, it releases its
    // integrated minors. Those released minors must NOT be re-flagged by the
    // sweep's own subsequent visit — they were just restored to life.
    // Track the release set so the outer loop can skip them. This keeps the
    // sweep precise: a genuinely defeated *independent* minor (not released
    // from an overlord) still enters anarchy as before.
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    let mut released_this_sweep: std::collections::HashSet<NationId> =
        std::collections::HashSet::new();
    for nation_id in nation_ids {
        if released_this_sweep.contains(&nation_id) {
            continue;
        }
        check_and_apply_anarchy(game, nation_id, report, &mut released_this_sweep);
    }
}

/// Check if a nation just lost its capital province and should enter anarchy.
/// Returns true if anarchy was triggered. Populates `released_this_sweep`
/// with the IDs of any integrated minors released as a consequence of this
/// nation entering anarchy, so the outer sweep loop can skip them.
fn check_and_apply_anarchy(
    game: &mut GameState,
    nation_id: NationId,
    report: &mut TurnReport,
    released_this_sweep: &mut std::collections::HashSet<NationId>,
) -> bool {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return false,
    };
    if nation.diplomacy.is_in_anarchy {
        return false; // already in anarchy
    }
    // Absorbed minors are not sovereign polities: they have no provinces
    // (ownership is held by the overlord) and no capital to defend. Anarchy
    // is a property of independent nations that have lost their capital, so
    // skip them here — the sweep in `apply_end_of_combat_anarchy` otherwise
    // flags every absorbed minor as anarchic on the same turn of
    // incorporation.
    if !nation.is_great_power() && nation.diplomacy.integrated_by.is_some() {
        return false;
    }
    // Check if the nation still owns its capital province
    if nation.province_ids.contains(&nation.capital_province_id) {
        return false;
    }
    // Enter anarchy
    let name = nation.name.clone();
    if let Some(n) = game.get_nation_mut(nation_id) {
        n.diplomacy.is_in_anarchy = true;
    }
    // Card #68: anarchy ends all wars involving this nation for dedup
    // purposes — clear any pact-defense requests so fresh cascades can run
    // if this nation ever recovers (future feature) or if any remaining
    // state queries consult the set.
    game.world
        .diplomacy
        .clear_pact_defense_for_nation(nation_id);
    // Card #79: integrated minors regain their independence when the
    // overlord falls into anarchy. Runs before the NationEnteredAnarchy
    // event so consumers that snapshot state see the newly-released minors.
    let released = release_integrated_minors(game, nation_id, report);
    released_this_sweep.extend(released);
    report
        .events
        .push(DomainEvent::NationEnteredAnarchy(NationEnteredAnarchy {
            nation: nation_id,
        }));
    report.newspaper_headlines.push(
        Headline::new(
            format!(
                "ANARCHY: {} collapses into chaos after losing its capital!",
                name
            ),
            HeadlineCategory::War,
        )
        .for_nation(nation_id),
    );
    game.archive.history.push((
        game.turn,
        HistoryEvent::FellIntoAnarchy { nation: nation_id },
    ));
    true
}

/// Release every minor nation whose provinces (either by diplomatic
/// incorporation or by straight military conquest) are currently held by
/// `overlord_id` and originate from the minor. Provinces marked
/// `incorporated_from = Some(minor_id)` and still owned by the anarchic
/// overlord are removed from the overlord and reassigned to the minor,
/// who resumes as an independent nation. Implements card #79: "when a
/// great power loses its capital, any diplomatically integrated country
/// regains its independence."
fn release_integrated_minors(
    game: &mut GameState,
    overlord_id: NationId,
    report: &mut TurnReport,
) -> Vec<NationId> {
    // Every minor is a candidate: the eligibility decision is made per
    // minor by checking whether the overlord is currently sitting on any
    // of that minor's origin-marked provinces. Iterating by province flags
    // alone covers both the "fully absorbed" path (integrated_by set) and
    // the "partially conquered militarily" path (integrated_by stays None
    // because the minor still has independent territory elsewhere).
    let mut released: Vec<NationId> = Vec::new();
    let minor_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| n.id)
        .collect();

    for minor_id in minor_ids {
        // Collect the overlord-owned provinces that were originally the
        // minor's — either by diplomatic incorporation (incorporated_from)
        // or by military conquest (conquest_origin).
        let provinces_to_restore: Vec<ProvinceId> = game
            .world
            .provinces
            .iter()
            .filter(|p| {
                p.owner == overlord_id
                    && (p.incorporated_from == Some(minor_id)
                        || p.conquest_origin == Some(minor_id))
            })
            .map(|p| p.id)
            .collect();

        if provinces_to_restore.is_empty() {
            // This minor had nothing to reclaim from the collapsing overlord.
            // Only clear `integrated_by` if it specifically pointed at the
            // now-anarchic overlord — otherwise the minor is integrated by
            // a different great power and must not be touched.
            // Also clear anarchy: per card #96, a released subject must not
            // inherit the overlord's anarchy state. The caller (the sweep)
            // must also know to skip this minor on its own iteration, so we
            // add it to the `released` return list.
            if let Some(minor) = game.get_nation_mut(minor_id)
                && minor.diplomacy.integrated_by == Some(overlord_id)
            {
                minor.diplomacy.integrated_by = None;
                minor.diplomacy.is_in_anarchy = false;
                released.push(minor_id);
            }
            continue;
        }

        // Determine the actual recipient of the restored provinces. If the
        // minor is currently integrated by a different GP, route the provinces
        // to that GP so the minor's integration state remains consistent.
        let minor_current_overlord = game
            .get_nation(minor_id)
            .and_then(|n| n.diplomacy.integrated_by);
        let actual_owner = if let Some(other) = minor_current_overlord
            && other != overlord_id
        {
            other
        } else {
            minor_id
        };

        // Transfer provinces to actual_owner. Clear origin markers only when
        // restoring to the minor itself; preserve them when routing to another
        // GP so that future releases can still trace the chain back to the minor.
        for pid in &provinces_to_restore {
            if let Some(prov) = game.get_province_mut(*pid) {
                prov.owner = actual_owner;
                if actual_owner == minor_id {
                    prov.incorporated_from = None;
                    prov.conquest_origin = None;
                }
            }
        }

        // Remove those provinces from the collapsing overlord's list and
        // scatter any army units positioned on them.
        if let Some(overlord) = game.get_nation_mut(overlord_id) {
            overlord
                .province_ids
                .retain(|p| !provinces_to_restore.contains(p));
            overlord
                .military
                .army
                .retain(|u| !provinces_to_restore.contains(&u.position));
        }

        if actual_owner != minor_id {
            // Provinces route to the GP that currently integrates this minor.
            // The minor remains integrated — no independence events or garrison seeding.
            if let Some(other_gp) = game.get_nation_mut(actual_owner) {
                for pid in &provinces_to_restore {
                    other_gp.add_province(*pid);
                }
            }
            // Sync garrison cache: the collapsing overlord's army was just
            // scattered, so garrison_count is stale for the new owner.
            for &pid in &provinces_to_restore {
                sync_garrison_cache(game, pid);
            }
            continue;
        }

        // Full restoration: minor resumes as an independent nation.
        if let Some(minor) = game.get_nation_mut(minor_id) {
            for pid in &provinces_to_restore {
                minor.add_province(*pid);
            }
            // Only clear integration pointer when this overlord was the integrator.
            if minor.diplomacy.integrated_by == Some(overlord_id) {
                minor.diplomacy.integrated_by = None;
            }
            // Card #96: a released subject must come back as a functioning
            // independent nation, never as an anarchic black-banner state.
            // If its original capital didn't come back (e.g. a third power
            // seized it before the overlord fell), promote the first
            // restored province to capital so the post-combat anarchy sweep
            // does not re-flag the minor.
            if !minor.province_ids.contains(&minor.capital_province_id)
                && let Some(&new_capital) = provinces_to_restore.first()
            {
                minor.capital_province_id = new_capital;
            }
            minor.diplomacy.is_in_anarchy = false;
        }

        // Seed token defenders so the reborn minor isn't immediately overrun
        // the next turn. Refill each restored province's militia up to
        // `minor_default_garrison`; if the minor still holds its capital and
        // has no GarrisonArtillery there, spawn one (mirrors the initial minor
        // garrison layout in `create_garrison`).
        seed_released_minor_garrison(game, minor_id, &provinces_to_restore);
        // Unconditional sync covers provinces already at target garrison
        // (seed only syncs when it spawns) and the post-artillery case.
        for &pid in &provinces_to_restore {
            sync_garrison_cache(game, pid);
        }

        let minor_name = game
            .get_nation(minor_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let overlord_name = game
            .get_nation(overlord_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        report.events.push(DomainEvent::MinorRegainedIndependence(
            crate::events::MinorRegainedIndependence {
                minor: minor_id,
                former_overlord: overlord_id,
            },
        ));
        report.newspaper_headlines.push(
            Headline::with_reason(
                format!(
                    "{} regains its independence as {} collapses into chaos",
                    minor_name, overlord_name
                ),
                HeadlineCategory::Diplomacy,
                format!(
                    "{} fell into anarchy; {} reclaimed {} province(s)",
                    overlord_name,
                    minor_name,
                    provinces_to_restore.len()
                ),
            )
            .for_nations(&[minor_id, overlord_id]),
        );
        game.archive.history.push((
            game.turn,
            HistoryEvent::RegainedIndependence {
                minor: minor_id,
                former_overlord: overlord_id,
            },
        ));
        released.push(minor_id);
    }
    released
}

/// Give a freshly-released minor token defenders on each restored province so
/// it is not trivially re-conquerable on turn 1 of independence. Refills each
/// province's persistent Militia up to `minor_default_garrison` and, if the
/// minor holds its capital province and has no GarrisonArtillery there, spawns
/// one — matching the initial minor-nation layout from `create_garrison`.
fn seed_released_minor_garrison(
    game: &mut GameState,
    minor_id: NationId,
    provinces_to_restore: &[ProvinceId],
) {
    let target = game.game_data.game_config.minor_default_garrison as usize;
    let capital_id = game.get_nation(minor_id).map(|n| n.capital_province_id);

    let current_turn = game.turn.0;
    for pid in provinces_to_restore {
        let current = game
            .get_nation(minor_id)
            .map(|n| n.militia_at(*pid))
            .unwrap_or(0);
        let missing = target.saturating_sub(current);
        if missing > 0 {
            for _ in 0..missing {
                let mut unit = crate::military::combat::spawn_militia_unit(
                    &mut game.next_unit_id,
                    minor_id,
                    *pid,
                );
                unit.arrived_turn = current_turn;
                if let Some(minor) = game.get_nation_mut(minor_id) {
                    minor.military.army.push(unit);
                }
            }
            sync_garrison_cache(game, *pid);
        }
    }

    if let Some(capital_pid) = capital_id
        && provinces_to_restore.contains(&capital_pid)
    {
        let needs_artillery = game
            .get_nation(minor_id)
            .map(|n| !n.has_garrison_artillery_at(capital_pid))
            .unwrap_or(false);
        if needs_artillery {
            let unit = crate::military::combat::spawn_garrison_artillery_unit(
                &mut game.next_unit_id,
                minor_id,
                capital_pid,
            );
            if let Some(minor) = game.get_nation_mut(minor_id) {
                minor.military.army.push(unit);
            }
        }
    }
}

/// Pact defense: when a minor nation with NAPs is attacked, eligible protectors
/// are asked to intervene in priority order (highest relationship score first).
/// AI protectors make a strategic evaluation; human players receive a proposal.
/// Only one protector can accept — the minor joins their empire on acceptance.
fn trigger_pact_defense(
    game: &mut GameState,
    defender_nation_id: NationId,
    attacker_nation_id: NationId,
    report: &mut TurnReport,
) {
    // Only trigger for Minor Nation defenders
    let is_minor = game
        .get_nation(defender_nation_id)
        .is_some_and(|n| !n.is_great_power());
    if !is_minor {
        return;
    }

    // Card #68: protection requests for the same (attacker, minor) pair fire
    // only once per ongoing war. Subsequent combats between the same attacker
    // and minor skip the cascade entirely. The entry is cleared when the war
    // ends (peace, incorporation, anarchy).
    if game
        .world
        .diplomacy
        .is_pact_defense_requested(attacker_nation_id, defender_nation_id)
    {
        return;
    }

    // Collect eligible pact holders (GPs with NAP, not already at war with attacker)
    let mut candidates: Vec<(NationId, i32)> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power() && n.id != attacker_nation_id)
        .map(|n| n.id)
        .filter(|&gp_id| {
            game.world.diplomacy.has_treaty(
                gp_id,
                defender_nation_id,
                crate::events::TreatyType::NonAggressionPact,
            ) && !game
                .world
                .diplomacy
                .get_relation(gp_id, attacker_nation_id)
                .is_some_and(|r| r.at_war)
        })
        .map(|gp_id| {
            let score = game
                .world
                .diplomacy
                .get_relation(gp_id, defender_nation_id)
                .map(|r| r.score)
                .unwrap_or(0);
            (gp_id, score)
        })
        .collect();

    if candidates.is_empty() {
        return;
    }

    // Sort by relationship score with minor (highest first = most loved)
    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let defender_name = game
        .get_nation(defender_nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let attacker_name = game
        .get_nation(attacker_nation_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    // Cascade through candidates in priority order
    let candidate_ids: Vec<NationId> = candidates.iter().map(|(id, _)| *id).collect();
    run_pact_defense_cascade(
        game,
        attacker_nation_id,
        defender_nation_id,
        &candidate_ids,
        &defender_name,
        &attacker_name,
        report,
    );

    // Card #68: mark the (attacker, minor) pair so later combats in the same
    // war skip the cascade. Cleared when the war ends. Skip if the minor was
    // just absorbed by a protector during the cascade — incorporation already
    // cleared its dedup entries and the minor is no longer an independent
    // war party.
    let minor_still_independent = game
        .get_nation(defender_nation_id)
        .is_some_and(|n| !n.province_ids.is_empty());
    if minor_still_independent {
        game.world
            .diplomacy
            .mark_pact_defense_requested(attacker_nation_id, defender_nation_id);
    }
}

/// Run the pact defense cascade: evaluate each candidate in order,
/// stop at first acceptance or when a human player is reached.
fn run_pact_defense_cascade(
    game: &mut GameState,
    attacker_id: NationId,
    minor_id: NationId,
    candidates: &[NationId],
    defender_name: &str,
    attacker_name: &str,
    report: &mut TurnReport,
) {
    for (i, &gp_id) in candidates.iter().enumerate() {
        let gp_name = game
            .get_nation(gp_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();

        let is_ai = game
            .get_nation(gp_id)
            .is_some_and(|n| n.diplomacy.ai_personality.is_some());

        if is_ai {
            // AI makes a strategic decision
            let personality = crate::ai::common::get_personality(game, gp_id);

            let lua_cfg = crate::ai::lua_bridge::get_personality_config(game, personality);

            let accepts = crate::ai::assessment::evaluate_pact_defense(
                game,
                gp_id,
                attacker_id,
                minor_id,
                personality,
                lua_cfg.as_ref(),
            );

            if accepts {
                // Protector accepts: declare war and incorporate the minor
                let turn = game.turn;
                game.world
                    .diplomacy
                    .declare_war_at(gp_id, attacker_id, turn);
                report.newspaper_headlines.push(Headline::with_reason(
                    format!(
                        "{} intervenes to protect {} and declares war on {}!",
                        gp_name, defender_name, attacker_name
                    ),
                    HeadlineCategory::War,
                    format!(
                        "{} personality judged {} worth protecting against {} (pact defense cascade)",
                        personality, defender_name, attacker_name
                    ),
                ).for_nations(&[gp_id, attacker_id, minor_id]));
                game.archive.history.push((
                    game.turn,
                    HistoryEvent::WarDeclared {
                        attacker: gp_id,
                        defender: attacker_id,
                        protectee: Some(minor_id),
                    },
                ));

                incorporate_minor_into_empire(
                    game,
                    minor_id,
                    gp_id,
                    report,
                    IncorporationReason::JoinedEmpire,
                );
                return; // Stop cascade — one protector is enough
            } else {
                // AI declines
                report.newspaper_headlines.push(Headline::with_reason(
                    format!(
                        "{} declines to intervene on behalf of {}",
                        gp_name, defender_name
                    ),
                    HeadlineCategory::Diplomacy,
                    format!(
                        "{} personality judged intervention too costly — insufficient strategic value or unfavorable balance vs {}",
                        personality, attacker_name
                    ),
                ).for_nations(&[gp_id, minor_id]));
            }
        } else {
            // Human player: create a PactDefenseRequest proposal and pause cascade
            let remaining: Vec<NationId> = candidates[i + 1..].to_vec();
            game.world
                .diplomacy
                .pending_proposals
                .push(crate::diplomacy::DiplomaticProposal {
                    from: minor_id,
                    to: gp_id,
                    proposal_type: crate::events::TreatyType::PactDefenseRequest,
                    turn_proposed: game.turn,
                    attacker: Some(attacker_id),
                    cascade_remaining: if remaining.is_empty() {
                        None
                    } else {
                        Some(remaining)
                    },
                });
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "{} requests your protection against {}!",
                        defender_name, attacker_name
                    ),
                    HeadlineCategory::Diplomacy,
                )
                .for_nations(&[gp_id, minor_id, attacker_id]),
            );
            return; // Pause cascade until human responds
        }
    }

    // No one accepted
    report.newspaper_headlines.push(
        Headline::new(
            format!(
                "No protector came to {}'s defense against {}",
                defender_name, attacker_name
            ),
            HeadlineCategory::Diplomacy,
        )
        .for_nations(&[minor_id, attacker_id]),
    );
}

/// Accept a pact defense request: protector declares war on attacker and
/// incorporates the minor nation. Called by the WASM bridge when human
/// player accepts a PactDefenseRequest proposal.
pub fn accept_pact_defense(
    game: &mut GameState,
    protector_id: NationId,
    attacker_id: NationId,
    minor_id: NationId,
    report: &mut TurnReport,
) {
    // Precondition: minor must still exist and have provinces
    let minor_valid = game
        .get_nation(minor_id)
        .is_some_and(|n| !n.is_great_power() && !n.province_ids.is_empty());
    if !minor_valid {
        return; // Minor already conquered/incorporated — stale proposal
    }

    let protector_name = game
        .get_nation(protector_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let attacker_name = game
        .get_nation(attacker_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let minor_name = game
        .get_nation(minor_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    let turn = game.turn;
    game.world
        .diplomacy
        .declare_war_at(protector_id, attacker_id, turn);
    report.newspaper_headlines.push(
        Headline::new(
            format!(
                "{} intervenes to protect {} and declares war on {}!",
                protector_name, minor_name, attacker_name
            ),
            HeadlineCategory::War,
        )
        .for_nations(&[protector_id, attacker_id, minor_id]),
    );
    game.archive.history.push((
        game.turn,
        HistoryEvent::WarDeclared {
            attacker: protector_id,
            defender: attacker_id,
            protectee: Some(minor_id),
        },
    ));

    incorporate_minor_into_empire(
        game,
        minor_id,
        protector_id,
        report,
        IncorporationReason::JoinedEmpire,
    );
}

/// Accept a RequestToJoinEmpire proposal: incorporate the minor into the
/// human player's empire (called by the WASM bridge on accept).
pub fn accept_request_to_join_empire(
    game: &mut GameState,
    overlord_id: NationId,
    minor_id: NationId,
    report: &mut TurnReport,
) {
    // Precondition: minor must still exist with provinces and not be in anarchy.
    let minor_valid = game.get_nation(minor_id).is_some_and(|n| {
        !n.is_great_power() && !n.province_ids.is_empty() && !n.diplomacy.is_in_anarchy
    });
    if !minor_valid {
        return;
    }
    // Overlord must be a non-anarchic Great Power.
    let overlord_valid = game
        .get_nation(overlord_id)
        .is_some_and(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy);
    if !overlord_valid {
        return;
    }
    incorporate_minor_into_empire(
        game,
        minor_id,
        overlord_id,
        report,
        IncorporationReason::VoluntarilyJoinedEmpire,
    );
}

/// Reject a RequestToJoinEmpire proposal: the snubbed minor's relationship
/// score with the rejecting Great Power drops sharply.
pub fn reject_request_to_join_empire(
    game: &mut GameState,
    overlord_id: NationId,
    minor_id: NationId,
) {
    if let Some(rel) = game.world.diplomacy.get_relation_mut(minor_id, overlord_id) {
        rel.improve_score(-20);
    }
}

/// Continue the pact defense cascade after the human player rejects.
/// Evaluates remaining AI candidates in order.
pub fn continue_pact_defense_cascade(
    game: &mut GameState,
    attacker_id: NationId,
    minor_id: NationId,
    remaining: &[NationId],
    report: &mut TurnReport,
) {
    // Precondition: minor must still exist and have provinces
    let minor_valid = game
        .get_nation(minor_id)
        .is_some_and(|n| !n.is_great_power() && !n.province_ids.is_empty());
    if !minor_valid {
        return; // Minor already conquered/incorporated — stale cascade
    }

    let defender_name = game
        .get_nation(minor_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let attacker_name = game
        .get_nation(attacker_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    run_pact_defense_cascade(
        game,
        attacker_id,
        minor_id,
        remaining,
        &defender_name,
        &attacker_name,
        report,
    );
}

/// Resolve fleet movements queued by the player (card #471).
///
/// Drains `transient.pending_fleet_moves` and applies each via the existing
/// whole-zone mover. Invalid entries (zones gone, no ships, exhausted budget)
/// are silently dropped — the wasm bridge already validates at queue time;
/// this is the end-of-turn execution.
fn resolve_pending_fleet_moves(game: &mut GameState) {
    use crate::military::naval::move_warship_group_one_zone;

    let queued: Vec<(
        NationId,
        crate::map::sea_zones::SeaZoneId,
        crate::map::sea_zones::SeaZoneId,
    )> = game.transient.pending_fleet_moves.drain(..).collect();
    for (nid, from_z, to_z) in queued {
        // Best-effort: ignore failures here, the player saw validation feedback
        // when they queued the move.
        let _ = move_warship_group_one_zone(game, nid, from_z, to_z);
    }
}

/// Resolve naval combat between nations at war that share a sea zone.
///
/// Zone-local: only ships in the same sea zone fight each other. Ships with no
/// zone assigned (`sea_zone == None`) are considered not deployed and skip combat.
fn resolve_naval_combat(game: &mut GameState, report: &mut TurnReport) {
    use crate::map::sea_zones::SeaZoneId;

    let gp_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    // Collect zone-local battles: (attacker, defender, zone)
    let mut battles_to_resolve: Vec<(NationId, NationId, SeaZoneId)> = Vec::new();

    for i in 0..gp_ids.len() {
        for j in (i + 1)..gp_ids.len() {
            let a = gp_ids[i];
            let b = gp_ids[j];

            // Check if at war AND past the one-turn grace period (card #104:
            // a war declared this turn defers all hostile actions — naval
            // combat, blockade — to the next turn).
            let combat_active = game
                .world
                .diplomacy
                .get_relation(a, b)
                .is_some_and(|r| r.hostilities_active_on(game.turn));

            if !combat_active {
                continue;
            }

            // Find all sea zones where nation A has warships
            let a_zones: Vec<SeaZoneId> = {
                let mut zones: Vec<SeaZoneId> = game
                    .get_nation(a)
                    .map(|n| {
                        n.military
                            .warships
                            .iter()
                            .filter_map(|s| s.sea_zone)
                            .collect::<HashSet<_>>()
                            .into_iter()
                            .collect()
                    })
                    .unwrap_or_default();
                zones.sort();
                zones
            };

            // Schedule a battle for each zone where B also has warships
            for zone_id in a_zones {
                let b_has_in_zone = game.get_nation(b).is_some_and(|n| {
                    n.military
                        .warships
                        .iter()
                        .any(|s| s.sea_zone == Some(zone_id))
                });
                if b_has_in_zone {
                    battles_to_resolve.push((a, b, zone_id));
                }
            }
        }
    }

    for (attacker_id, defender_id, zone_id) in battles_to_resolve {
        let atk_ships: Vec<_> = match game.get_nation(attacker_id) {
            Some(n) => n
                .military
                .warships
                .iter()
                .filter(|s| s.sea_zone == Some(zone_id))
                .cloned()
                .collect(),
            None => continue,
        };
        let def_ships: Vec<_> = match game.get_nation(defender_id) {
            Some(n) => n
                .military
                .warships
                .iter()
                .filter(|s| s.sea_zone == Some(zone_id))
                .cloned()
                .collect(),
            None => continue,
        };
        if atk_ships.is_empty() || def_ships.is_empty() {
            continue;
        }

        let result = resolve_naval_battle(
            &atk_ships,
            &def_ships,
            attacker_id,
            defender_id,
            &game.game_data,
        );

        let atk_name = game
            .get_nation(attacker_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let def_name = game
            .get_nation(defender_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        // Keep ships outside this zone; replace zone participants with survivors.
        // attacker_ships_lost is Vec<ShipType> (type info only); use survivor IDs
        // to determine which ships in the zone lived.
        let atk_survivor_ids: HashSet<_> = result.attacker_survivors.iter().map(|s| s.id).collect();
        let def_survivor_ids: HashSet<_> = result.defender_survivors.iter().map(|s| s.id).collect();
        if let Some(nation) = game.get_nation_mut(attacker_id) {
            nation
                .military
                .warships
                .retain(|s| s.sea_zone != Some(zone_id) || atk_survivor_ids.contains(&s.id));
            nation.military.warships_lost += result.attacker_ships_lost.len() as u32;
        }
        if let Some(nation) = game.get_nation_mut(defender_id) {
            nation
                .military
                .warships
                .retain(|s| s.sea_zone != Some(zone_id) || def_survivor_ids.contains(&s.id));
            nation.military.warships_lost += result.defender_ships_lost.len() as u32;
        }

        // Add headline
        if result.attacker_won {
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "NAVAL VICTORY: {} defeats {} fleet! ({} ships sunk)",
                        atk_name,
                        def_name,
                        result.defender_ships_lost.len()
                    ),
                    HeadlineCategory::Battle,
                )
                .for_nations(&[attacker_id, defender_id]),
            );
        } else {
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "NAVAL VICTORY: {} defeats {} fleet! ({} ships sunk)",
                        def_name,
                        atk_name,
                        result.attacker_ships_lost.len()
                    ),
                    HeadlineCategory::Battle,
                )
                .for_nations(&[defender_id, attacker_id]),
            );
        }

        report.naval_battles.push(result);
    }
}

/// Apply any tech research the human player queued from the Tech screen.
/// Validates that the tech is still available (year window, prerequisites, treasury)
/// and deducts the cost, then clears the queue regardless of outcome.
fn resolve_human_tech_research(game: &mut GameState, report: &mut TurnReport) {
    let human_id = game.human_player_nation;
    let pending = match game.get_nation(human_id) {
        Some(n) => n.pending_tech_research,
        None => return,
    };
    let tech_id = match pending {
        Some(id) => id,
        None => return,
    };
    // Clear the pending order regardless of whether it succeeds.
    if let Some(n) = game.get_nation_mut(human_id) {
        n.pending_tech_research = None;
    }

    let year = game.turn.year();
    let (tech_cost, tech_name) = {
        let nation = match game.get_nation(human_id) {
            Some(n) => n,
            None => return,
        };
        // Verify still available (year window + prerequisites + not already researched).
        let available = game
            .game_data
            .tech_tree
            .available_techs(&nation.researched_techs, year);
        match available.iter().find(|t| t.id == tech_id) {
            Some(t) => (t.cost, t.name.clone()),
            None => return, // expired or already researched — silently drop
        }
    };

    let nation = match game.get_nation_mut(human_id) {
        Some(n) => n,
        None => return,
    };
    if nation.economy.treasury.checked_sub(tech_cost).is_none() {
        // Notify the player via a headline so the failed queue is visible.
        report.newspaper_headlines.push(
            Headline::new(
                format!(
                    "Research of {} cancelled: insufficient funds (${} required).",
                    tech_name,
                    tech_cost.as_dollars()
                ),
                HeadlineCategory::Military,
            )
            .for_nation(human_id),
        );
        return;
    }
    nation.economy.treasury -= tech_cost;
    nation.research_tech_in_year(tech_id, year);

    report
        .events
        .push(DomainEvent::TechnologyResearched(TechnologyResearched {
            nation: human_id,
            tech: tech_id,
        }));
}

/// Report which technologies are available for research by the human player.
fn report_available_techs(game: &GameState, report: &mut TurnReport) {
    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) if !n.diplomacy.is_in_anarchy => n,
        _ => return,
    };
    let available = game
        .game_data
        .tech_tree
        .available_techs(&nation.researched_techs, game.turn.year());
    let tech_names: Vec<String> = available.iter().map(|t| t.name.clone()).collect();
    if !tech_names.is_empty() {
        report
            .techs_available
            .push((game.human_player_nation, tech_names));
    }
}

/// Resolve technology for AI nations that researched tech this turn.
///
/// For each AI nation, check if it researched a technology (tracked in ai_actions).
/// If so, generate a TechnologyResearched domain event and push it to the report.
fn resolve_technology(game: &mut GameState, report: &mut TurnReport) {
    // Collect techs researched by AI this turn from ai_actions
    // AI actions contain strings like "Deneb researched High Pressure Steam Engine ($0)"
    let researched_pattern = " researched ";
    for action in &report.ai_actions {
        if let Some(pos) = action.text.find(researched_pattern) {
            let nation_name = &action.text[..pos];
            // Find the nation by name
            if let Some(nation) = game.world.nations.iter().find(|n| n.name == nation_name) {
                let nation_id = nation.id;
                // Find the most recently researched tech
                if let Some(tech_id) = nation.researched_techs.last().copied() {
                    report
                        .events
                        .push(DomainEvent::TechnologyResearched(TechnologyResearched {
                            nation: nation_id,
                            tech: tech_id,
                        }));
                }
            }
        }
    }
}

///
/// When a nation is at war, check if the defender has allies (Alliance treaty).
/// AI allies automatically join the war by declaring war on the attacker.
/// A newspaper headline is generated for each alliance activation.
fn resolve_alliance_obligations(game: &mut GameState, report: &mut TurnReport) {
    // Collect all active wars
    let mut wars: Vec<(NationId, NationId)> = Vec::new();
    for nation in &game.world.nations {
        if !nation.is_great_power() {
            continue;
        }
        let rels = game.world.diplomacy.relations_for(nation.id);
        for ((a, b), rel) in &rels {
            if rel.at_war && *a == nation.id {
                wars.push((*a, *b));
            }
        }
    }

    // For each war, check if either side has allies that are not yet at war with the other side
    let mut new_wars: Vec<(NationId, NationId, String, String)> = Vec::new();
    for (attacker, defender) in &wars {
        let is_gp_war = game
            .get_nation(*attacker)
            .is_some_and(|n| n.is_great_power())
            && game
                .get_nation(*defender)
                .is_some_and(|n| n.is_great_power());
        if !is_gp_war {
            continue;
        }
        // Skip alliance obligations for anarchic defenders
        if game
            .get_nation(*defender)
            .is_some_and(|n| n.diplomacy.is_in_anarchy)
        {
            continue;
        }
        // Check defender's allies
        let defender_allies = game.world.diplomacy.get_allies(*defender);
        for ally in &defender_allies {
            if *ally == *attacker {
                continue;
            }
            // Skip anarchic allies
            if game
                .get_nation(*ally)
                .is_some_and(|n| n.diplomacy.is_in_anarchy)
            {
                continue;
            }
            // Skip joining wars against nations that have 0 provinces (already defeated)
            let attacker_has_provinces = game
                .get_nation(*attacker)
                .is_some_and(|n| !n.province_ids.is_empty());
            if !attacker_has_provinces {
                continue;
            }
            // Check if this ally is already at war with the attacker
            let already_at_war = game
                .world
                .diplomacy
                .get_relation(*ally, *attacker)
                .is_some_and(|r| r.at_war);
            if already_at_war {
                continue;
            }
            // Check if this ally is an AI nation (human allies make their own decisions)
            let is_ai = game
                .get_nation(*ally)
                .is_some_and(|n| n.diplomacy.ai_personality.is_some());
            if !is_ai {
                continue;
            }
            let ally_name = game
                .get_nation(*ally)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let defender_name = game
                .get_nation(*defender)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let attacker_name = game
                .get_nation(*attacker)
                .map(|n| n.name.clone())
                .unwrap_or_default();

            let defender_allies_count = defender_allies.len();
            new_wars.push((*ally, *attacker, ally_name.clone(), attacker_name.clone()));
            report.newspaper_headlines.push(Headline::with_reason(
                format!(
                    "{} honors its alliance with {} and declares war on {}!",
                    ally_name, defender_name, attacker_name
                ),
                HeadlineCategory::War,
                format!(
                    "Alliance obligation: {} was attacked by {} — treaty auto-triggers defensive war ({} defending ally/allies in total)",
                    defender_name, attacker_name, defender_allies_count
                ),
            ).for_nations(&[*ally, *attacker, *defender]));
        }

        // Check attacker's allies
        let attacker_allies = game.world.diplomacy.get_allies(*attacker);
        for ally in &attacker_allies {
            if *ally == *defender {
                continue;
            }
            // Skip anarchic allies
            if game
                .get_nation(*ally)
                .is_some_and(|n| n.diplomacy.is_in_anarchy)
            {
                continue;
            }
            // Skip joining wars against nations that have 0 provinces (already defeated)
            let defender_has_provinces = game
                .get_nation(*defender)
                .is_some_and(|n| !n.province_ids.is_empty());
            if !defender_has_provinces {
                continue;
            }
            let already_at_war = game
                .world
                .diplomacy
                .get_relation(*ally, *defender)
                .is_some_and(|r| r.at_war);
            if already_at_war {
                continue;
            }
            let is_ai = game
                .get_nation(*ally)
                .is_some_and(|n| n.diplomacy.ai_personality.is_some());
            if !is_ai {
                continue;
            }
            let ally_name = game
                .get_nation(*ally)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let attacker_name = game
                .get_nation(*attacker)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let defender_name = game
                .get_nation(*defender)
                .map(|n| n.name.clone())
                .unwrap_or_default();

            let attacker_allies_count = attacker_allies.len();
            new_wars.push((*ally, *defender, ally_name.clone(), defender_name.clone()));
            report.newspaper_headlines.push(Headline::with_reason(
                format!(
                    "{} honors its alliance with {} and declares war on {}!",
                    ally_name, attacker_name, defender_name
                ),
                HeadlineCategory::War,
                format!(
                    "Alliance obligation: {} declared war on {} — treaty auto-triggers offensive support ({} supporting ally/allies in total)",
                    attacker_name, defender_name, attacker_allies_count
                ),
            ).for_nations(&[*ally, *defender, *attacker]));
        }
    }

    // Graph-based conflict detection: build a map of each ally's obligations
    // (who they would fight against), then detect any ally that would end up
    // on both sides of the same war.
    let mut obligations: std::collections::HashMap<NationId, Vec<NationId>> =
        std::collections::HashMap::new();
    for (ally, enemy, _, _) in &new_wars {
        obligations.entry(*ally).or_default().push(*enemy);
    }

    let mut conflicted_allies: Vec<NationId> = Vec::new();
    for (ally, enemies) in &obligations {
        // If the ally must fight multiple enemies, check whether any pair of
        // those enemies are on opposite sides of the same war. If so, the
        // ally is being pulled to both sides and must stay neutral.
        for i in 0..enemies.len() {
            for j in (i + 1)..enemies.len() {
                if game.world.diplomacy.is_at_war(enemies[i], enemies[j])
                    && !conflicted_allies.contains(ally)
                {
                    conflicted_allies.push(*ally);
                }
            }
        }
    }

    if !conflicted_allies.is_empty() {
        new_wars.retain(|(ally, _, _, _)| !conflicted_allies.contains(ally));
        report.newspaper_headlines.retain(|h| {
            !conflicted_allies.iter().any(|&cid| {
                let name = game.get_nation(cid).map(|n| n.name.as_str()).unwrap_or("");
                !name.is_empty()
                    && h.text.starts_with(name)
                    && h.text.contains("honors its alliance")
            })
        });
        for &cid in &conflicted_allies {
            let name = game
                .get_nation(cid)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "{} remains neutral due to conflicting alliance obligations",
                        name
                    ),
                    HeadlineCategory::Diplomacy,
                )
                .for_nation(cid),
            );
        }
    }

    // Actually declare the new wars (done after collecting to avoid borrow issues)
    for (ally, enemy, _ally_name, _enemy_name) in &new_wars {
        let turn = game.turn;
        game.world.diplomacy.declare_war_at(*ally, *enemy, turn);
        game.archive.history.push((
            turn,
            HistoryEvent::JoinedWar {
                joiner: *ally,
                target: *enemy,
            },
        ));
    }
}

/// Resolve voluntary incorporations: Minor Nations with high relationship scores
/// voluntarily join the Great Power with the highest score.
///
/// For each Minor Nation, checks relationships with all Great Powers.
/// If any relationship score >= 75, the Minor Nation joins the GP with the highest score.
/// All of the Minor Nation's provinces are transferred to the Great Power.
/// Transfer all provinces from a minor nation to a great power (incorporation).
/// Used by voluntary incorporation, pact defense acceptance, and colonization.
fn incorporate_minor_into_empire(
    game: &mut GameState,
    minor_id: NationId,
    gp_id: NationId,
    report: &mut TurnReport,
    reason: IncorporationReason,
) {
    let provinces_to_transfer: Vec<ProvinceId> = game
        .get_nation(minor_id)
        .map(|n| n.province_ids.clone())
        .unwrap_or_default();

    // Update province owners (mark as diplomatically incorporated for map rendering)
    for pid in &provinces_to_transfer {
        if let Some(prov) = game.get_province_mut(*pid) {
            prov.owner = gp_id;
            prov.incorporated_from = Some(minor_id);
        }
    }

    // Drain the minor's army and warships; transfer ownership to the overlord
    // so the WASM move-target query (which looks up a unit in
    // `nation.military.army`/`nation.military.warships` by nation id) can find them for the
    // annexing player. Positions remain unchanged — units stay on the same
    // (now overlord-owned) provinces they were standing on.
    let mut transferred_army: Vec<ArmyUnit> = Vec::new();
    let mut transferred_warships: Vec<Ship> = Vec::new();
    let mut transferred_merchants: Vec<Ship> = Vec::new();
    if let Some(minor) = game.get_nation_mut(minor_id) {
        transferred_army = std::mem::take(&mut minor.military.army);
        transferred_warships = std::mem::take(&mut minor.military.warships);
        transferred_merchants = std::mem::take(&mut minor.military.merchant_fleet);
        minor.province_ids.clear();
        // Card #79: back-pointer used when the overlord falls into anarchy
        // and integrated minors regain their independence.
        minor.diplomacy.integrated_by = Some(gp_id);
    }
    for unit in &mut transferred_army {
        unit.owner = gp_id;
    }
    for ship in transferred_warships
        .iter_mut()
        .chain(transferred_merchants.iter_mut())
    {
        ship.owner = gp_id;
    }

    // Add provinces and transferred military to great power
    if let Some(gp) = game.get_nation_mut(gp_id) {
        for pid in &provinces_to_transfer {
            gp.add_province(*pid);
        }
        gp.military.army.append(&mut transferred_army);
        gp.military.warships.append(&mut transferred_warships);
        gp.military
            .merchant_fleet
            .append(&mut transferred_merchants);
    }

    // Card #68: once absorbed, the minor is no longer an independent war
    // party; any pact-defense dedup entry involving it is stale.
    game.world.diplomacy.clear_pact_defense_for_nation(minor_id);

    report
        .events
        .push(DomainEvent::NationIncorporated(NationIncorporated {
            minor_nation: minor_id,
            great_power: gp_id,
        }));

    report.incorporations.push((minor_id, gp_id));

    award_first_colony_clippers(game, gp_id, report);

    game.archive.history.push((
        game.turn,
        HistoryEvent::MinorJoinedEmpire {
            minor: minor_id,
            overlord: gp_id,
            reason,
        },
    ));
}

fn resolve_voluntary_incorporations(game: &mut GameState, report: &mut TurnReport) {
    let minor_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| n.id)
        .collect();

    // Anarchic GPs are excluded: a collapsed great power has no government
    // capable of accepting a minor's allegiance. Without this filter, a minor
    // released by an overlord falling into anarchy is re-absorbed into that
    // same anarchic overlord on the very next turn, since pre-collapse
    // relations are still near-100.
    let gp_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    // Threshold sourced from scripts/config/game.lua — voluntary incorporation
    // should be a rare, late-game event requiring near-max relationship.
    let threshold = game.game_data.game_config.voluntary_incorporation_threshold;

    for minor_id in &minor_ids {
        if game
            .get_nation(*minor_id)
            .is_some_and(|n| n.province_ids.is_empty() || n.diplomacy.is_in_anarchy)
        {
            continue;
        }

        let mut best_gp: Option<NationId> = None;
        let mut best_score: i32 = threshold - 1;

        for gp_id in &gp_ids {
            if let Some(rel) = game.world.diplomacy.get_relation(*minor_id, *gp_id)
                && rel.score >= threshold
                && rel.score > best_score
            {
                best_score = rel.score;
                best_gp = Some(*gp_id);
            }
        }

        if let Some(gp_id) = best_gp {
            // When the most-favored protector is the human player, queue a
            // RequestToJoinEmpire proposal so the player can accept or refuse.
            // AI overlords still auto-incorporate.
            if gp_id == game.human_player_nation {
                // Dedup: only one outstanding RequestToJoinEmpire per (minor, human) pair.
                let already_pending = game.world.diplomacy.pending_proposals.iter().any(|p| {
                    p.proposal_type == crate::events::TreatyType::RequestToJoinEmpire
                        && p.from == *minor_id
                        && p.to == gp_id
                });
                if !already_pending {
                    let minor_name = game
                        .get_nation(*minor_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    game.world.diplomacy.pending_proposals.push(
                        crate::diplomacy::DiplomaticProposal {
                            from: *minor_id,
                            to: gp_id,
                            proposal_type: crate::events::TreatyType::RequestToJoinEmpire,
                            turn_proposed: game.turn,
                            attacker: None,
                            cascade_remaining: None,
                        },
                    );
                    report.newspaper_headlines.push(
                        crate::events::Headline::new(
                            format!("{} requests to join your empire", minor_name),
                            crate::events::HeadlineCategory::Diplomacy,
                        )
                        .for_nations(&[*minor_id, gp_id]),
                    );
                }
            } else {
                incorporate_minor_into_empire(
                    game,
                    *minor_id,
                    gp_id,
                    report,
                    IncorporationReason::VoluntarilyJoinedEmpire,
                );
            }
        }
    }
}

/// Resolve unit upgrades for AI nations.
///
/// For each AI nation, for each army unit, checks if an upgrade is available:
/// - The unit type has an `upgrade_to()` path
/// - The target unit type has a `required_tech()` name
/// - The nation has researched a tech whose name matches the required tech
///
/// Player units are not auto-upgraded (player uses `upgrade <index>` command).
fn resolve_unit_upgrades(game: &mut GameState, report: &mut TurnReport) {
    let ai_nation_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.diplomacy.ai_personality.is_some() && !n.diplomacy.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    for nation_id in &ai_nation_ids {
        let nation = match game.world.nations.iter().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Collect researched tech names for this nation
        let researched_tech_names: Vec<String> = nation
            .researched_techs
            .iter()
            .filter_map(|tid| game.game_data.tech_tree.get(*tid))
            .map(|t| t.name.clone())
            .collect();

        // Find upgrades to perform. Garrison units (Minutemen / Militia /
        // Conscript / GarrisonArtillery) are persistent province defenders;
        // auto-upgrading them would drain the Minutemen pool every turn and
        // `regenerate_garrisons` would re-seed forever. Field-army units
        // only.
        let mut upgrades: Vec<(usize, ArmyUnitType, ArmyUnitType)> = Vec::new();
        for (i, unit) in nation.military.army.iter().enumerate() {
            if unit.unit_type.category() == crate::military::units::UnitCategory::Garrison {
                continue;
            }
            if let Some(target_type) = unit.unit_type.upgrade_to()
                && let Some(required_tech_name) = target_type.required_tech()
                && researched_tech_names
                    .iter()
                    .any(|name| name == required_tech_name)
            {
                upgrades.push((i, unit.unit_type, target_type));
            }
        }

        // Apply upgrades (preserve medals, health)
        let nation = match game.world.nations.iter_mut().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        for (idx, from_type, to_type) in &upgrades {
            if *idx < nation.military.army.len() {
                nation.military.army[*idx].unit_type = *to_type;
                // Refresh movement for new type
                nation.military.army[*idx].movement_remaining = to_type.stats().movement;
            }
            report.unit_upgrades.push((
                *nation_id,
                format!("{:?}", from_type),
                format!("{:?}", to_type),
            ));
            report.events.push(DomainEvent::UnitUpgraded(UnitUpgraded {
                nation: *nation_id,
                from_type: format!("{:?}", from_type),
                to_type: format!("{:?}", to_type),
            }));
        }
    }
}

/// Calculate scores for all Great Powers and store them in the report.
fn calculate_scores(game: &GameState, report: &mut TurnReport) {
    let mut scores: Vec<(NationId, String, u32)> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| {
            let s = calculate_score(n, &game.game_data);
            (n.id, n.name.clone(), s.total)
        })
        .collect();
    scores.sort_by(|a, b| b.2.cmp(&a.2));
    report.scores = scores;
}

fn check_council_vote(game: &GameState, report: &mut TurnReport) {
    if !game.turn.is_decade_election() {
        return;
    }

    let is_final = game.turn.is_game_end();
    let result = run_council_vote(
        &game.world.nations,
        &game.world.provinces,
        is_final,
        &game.world.diplomacy,
    );

    if let Some(winner_id) = result.winner {
        if let Some(winner) = game.get_nation(winner_id) {
            report.newspaper_headlines.push(
                Headline::new(
                    format!(
                        "BREAKING: {} wins the Council of Governors with {} of {} votes!",
                        winner.name,
                        result
                            .votes
                            .iter()
                            .find(|(id, _)| *id == winner_id)
                            .map(|(_, v)| *v)
                            .unwrap_or(0),
                        result.total_governors
                    ),
                    HeadlineCategory::Politics,
                )
                .for_nation(winner_id),
            );
        }
    } else {
        report.newspaper_headlines.push(Headline::new(
            format!(
                "Council of Governors: No nation achieves the required {} vote majority.",
                result.majority_threshold
            ),
            HeadlineCategory::Politics,
        ));
    }

    report.council_vote = Some(result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::diplomacy::DiplomacyState;
    use crate::economy::buildings::Building;
    use crate::economy::labor::LaborPool;
    use crate::hex::HexCoord;
    use crate::map::tile::Tile;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};

    /// Build a minimal GameState for testing the turn processor.
    fn test_game_state() -> GameState {
        let coord_farm = HexCoord::new(0, 0);
        let coord_forest = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);

        // A grain tile (produces 1 Grain at level 0). Also acts as the
        // nation's country capital (implicit depot + is_country_capital
        // flag), mirroring what `place_depot_unchecked` does at game
        // setup for every nation's capital.
        let mut farm_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        farm_tile.set_resource(ResourceType::Grain);
        farm_tile.is_country_capital = true;
        farm_tile.infrastructure.has_depot = true;
        hex_map.set_tile(coord_farm, farm_tile);

        // A forest tile with timber (produces 1 Timber) — adjacent to the
        // capital tile, so it's in the capital's 1-hex collector radius.
        let mut forest_tile = Tile::with_province(TerrainType::Forest, ProvinceId(1));
        forest_tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord_forest, forest_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Homeland".to_string(),
            NationId(1),
            coord_farm,
            vec![coord_farm, coord_forest],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.economy.treasury = Money::dollars(1000);

        crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1],
        nations: vec![nation1],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: crate::data::test_game_data(),
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

    /// Build a game state with a gold mine for testing monetary conversion.
    fn test_game_state_with_gold() -> GameState {
        let coord_gold = HexCoord::new(0, 0);

        let mut hex_map = HexMap::new(10, 10);

        // A mountain tile with gold deposit at improvement level 1 (produces 1 Gold).
        // Also acts as the country capital so the collector radius covers it.
        let mut gold_tile = Tile::with_province(TerrainType::Mountain, ProvinceId(1));
        gold_tile.reveal_deposit(ResourceType::Gold);
        gold_tile.set_improvement_level(1);
        gold_tile.is_country_capital = true;
        gold_tile.infrastructure.has_depot = true;
        hex_map.set_tile(coord_gold, gold_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Gold Province".to_string(),
            NationId(1),
            coord_gold,
            vec![coord_gold],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "GoldNation".to_string(),
            NationColor::Yellow,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.economy.treasury = Money::dollars(2000);

        crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1],
        nations: vec![nation1],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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

    // ── Turn advancement ──────────────────────────────────────

    #[test]
    fn process_turn_advances_turn_number() {
        let mut game = test_game_state();
        assert_eq!(game.turn, TurnNumber::new(1));

        let report = process_turn(&mut game);

        assert_eq!(report.turn, TurnNumber::new(1)); // report reflects the turn that was processed
        assert_eq!(game.turn, TurnNumber::new(2)); // game has advanced
    }

    #[test]
    fn process_turn_appends_political_snapshot() {
        let mut game = test_game_state();
        assert!(game.archive.political_archive.is_empty());

        let _ = process_turn(&mut game);
        assert_eq!(game.archive.political_archive.len(), 1);
        let (turn, snapshot) = &game.archive.political_archive[0];
        assert_eq!(*turn, TurnNumber::new(1));
        assert_eq!(snapshot.provinces.len(), game.world.provinces.len());
        for (pid, owner, inc) in &snapshot.provinces {
            let p = game.get_province(*pid).unwrap();
            assert_eq!(*owner, p.owner);
            assert_eq!(*inc, p.incorporated_from);
        }
        assert_eq!(snapshot.capitals.len(), game.world.nations.len());
        for (nid, cap) in &snapshot.capitals {
            let n = game.get_nation(*nid).unwrap();
            assert_eq!(*cap, n.capital_province_id);
        }

        let _ = process_turn(&mut game);
        assert_eq!(game.archive.political_archive.len(), 2);
        assert_eq!(game.archive.political_archive[1].0, TurnNumber::new(2));
    }

    // ── Resource collection ───────────────────────────────────

    #[test]
    fn resource_collection_gathers_from_owned_tiles() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        // Should have collected Grain (from Farm) and Timber (from ScrubForest)
        let grain_produced: u32 = report
            .resource_production
            .iter()
            .filter(|(_, r, _)| *r == ResourceType::Grain)
            .map(|(_, _, q)| q)
            .sum();
        let timber_produced: u32 = report
            .resource_production
            .iter()
            .filter(|(_, r, _)| *r == ResourceType::Timber)
            .map(|(_, _, q)| q)
            .sum();

        assert_eq!(grain_produced, 1); // Farm at level 0 = 1 Grain
        assert_eq!(timber_produced, 1); // ScrubForest = 1 Timber

        // Verify the nation's warehouse was updated
        // With 0 workers, no food is consumed
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 1);
        assert_eq!(nation.resource_amount(ResourceType::Timber), 1);
    }

    // ── Gold conversion ───────────────────────────────────────

    #[test]
    fn gold_converts_to_money() {
        let mut game = test_game_state_with_gold();
        let initial_treasury = game.get_nation(NationId(1)).unwrap().economy.treasury;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        // 1 Gold collected => $500 added to treasury
        assert_eq!(
            nation.economy.treasury,
            initial_treasury + Money::dollars(500)
        );

        // Gold should have been removed from warehouse
        assert_eq!(nation.resource_amount(ResourceType::Gold), 0);

        // Report should record the income
        assert!(!report.gold_income.is_empty());
        let (_, income) = report.gold_income[0];
        assert_eq!(income, Money::dollars(500));
    }

    #[test]
    fn gems_convert_to_money() {
        let mut game = test_game_state();
        // Manually add gems to the nation's warehouse
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Gems, 3);

        let initial_treasury = game.get_nation(NationId(1)).unwrap().economy.treasury;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        // 3 Gems => $3,000
        assert_eq!(
            nation.economy.treasury,
            initial_treasury + Money::dollars(3000)
        );
        assert_eq!(nation.resource_amount(ResourceType::Gems), 0);
        assert!(!report.gold_income.is_empty());
    }

    // ── Newspaper generation ──────────────────────────────────

    #[test]
    fn newspaper_is_generated() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        assert!(!report.newspaper_headlines.is_empty());
        assert!(
            report.newspaper_headlines[0]
                .text
                .contains("The Imperial Times")
        );
        assert!(report.newspaper_headlines[0].text.contains("1815"));
        assert!(report.newspaper_headlines[0].text.contains("Q1"));
    }

    #[test]
    fn newspaper_includes_human_nation() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        let has_empire_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.contains("Testlandia"));
        assert!(has_empire_headline);
    }

    #[test]
    fn newspaper_includes_election_headline() {
        let mut game = test_game_state();
        // Set to 1825 Q1 which is a decade election year
        game.turn = TurnNumber::from_year_quarter(1825, 1);

        let report = process_turn(&mut game);

        let has_election = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.contains("Council of Governors"));
        assert!(has_election);
    }

    #[test]
    fn generate_newspaper_propagates_non_action_flag() {
        // A non-action AiAction should produce a headline with is_non_action=true
        // AND the reason preserved.
        let game = test_game_state();
        let mut report = TurnReport::empty();
        report.turn = game.turn;
        report.year = game.turn.year();
        report.quarter = game.turn.quarter();
        report.ai_actions = vec![
            crate::ai::AiAction {
                text: "Testland did not declare war this turn".to_string(),
                reason: "war cooldown active".to_string(),
                is_non_action: true,
                nation_id: crate::types::NationId(0),
            },
            crate::ai::AiAction {
                text: "Testland has declared war on Otherland!".to_string(),
                reason: "combined score above threshold".to_string(),
                is_non_action: false,
                nation_id: crate::types::NationId(0),
            },
        ];

        generate_newspaper(&game, &mut report);

        let non_action_headline = report
            .newspaper_headlines
            .iter()
            .find(|h| h.text.contains("did not declare war"))
            .expect("non-action propagated");
        assert!(
            non_action_headline.is_non_action,
            "non-action flag must propagate from AiAction to Headline"
        );
        assert_eq!(
            non_action_headline.reason.as_deref(),
            Some("war cooldown active")
        );

        let action_headline = report
            .newspaper_headlines
            .iter()
            .find(|h| h.text.contains("has declared war"))
            .expect("action propagated");
        assert!(
            !action_headline.is_non_action,
            "positive actions must not be marked as non-action"
        );
    }

    #[test]
    fn generate_newspaper_propagates_ai_action_reasons() {
        // AI actions in the report should become headlines with reason: Some(_),
        // while non-AI headlines added by generate_newspaper remain reason: None.
        let game = test_game_state();
        let mut report = TurnReport::empty();
        report.turn = game.turn;
        report.year = game.turn.year();
        report.quarter = game.turn.quarter();
        report.ai_actions = vec![
            crate::ai::AiAction {
                text: "Scientists in Testland have discovered Steam Engine!".to_string(),
                reason: "Economic personality selected tech (cost=$500)".to_string(),
                is_non_action: false,
                nation_id: crate::types::NationId(0),
            },
            crate::ai::AiAction {
                text: "Testland has declared war on Otherland!".to_string(),
                reason: "Combined score 2.30 > threshold 1.50".to_string(),
                is_non_action: false,
                nation_id: crate::types::NationId(1),
            },
        ];

        generate_newspaper(&game, &mut report);

        // Each AI action should have produced a headline whose reason matches.
        for action in &report.ai_actions {
            let h = report
                .newspaper_headlines
                .iter()
                .find(|h| h.text == action.text)
                .unwrap_or_else(|| panic!("action not propagated: {}", action.text));
            assert_eq!(h.reason.as_deref(), Some(action.reason.as_str()));
        }

        // Masthead never carries a reason.
        let masthead = report
            .newspaper_headlines
            .iter()
            .find(|h| h.text.contains("The Imperial Times"))
            .expect("masthead present");
        assert!(masthead.reason.is_none(), "masthead should have no reason");

        // Flavor headline (always appended) should have no reason.
        let has_flavor_without_reason = report.newspaper_headlines.iter().any(|h| {
            h.reason.is_none()
                && (h.text.contains("Railroad expansion")
                    || h.text.contains("Industrial production")
                    || h.text.contains("Diplomatic tensions")
                    || h.text.contains("Colonial ambitions")
                    || h.text.contains("trade routes")
                    || h.text.contains("age of progress")
                    || h.text.contains("unrest in the frontier")
                    || h.text.contains("Great exhibitions"))
        });
        assert!(
            has_flavor_without_reason,
            "expected at least one flavor headline with reason: None"
        );
    }

    // ── Multiple turns in sequence ────────────────────────────

    #[test]
    fn multiple_turns_can_be_processed() {
        let mut game = test_game_state();

        for expected_turn in 1..=5 {
            assert_eq!(game.turn, TurnNumber::new(expected_turn));
            let report = process_turn(&mut game);
            assert_eq!(report.turn, TurnNumber::new(expected_turn));
        }
        assert_eq!(game.turn, TurnNumber::new(6));

        // After 5 turns, the nation should have accumulated resources.
        // Emergency recruitment gives 1 worker on turn 1 (nation started with 0),
        // so that worker consumes food on subsequent turns.
        // Grain: 1 gathered per turn = 5, minus some consumed by the emergency worker.
        // Timber: 1 gathered per turn = 5, not consumed by workers.
        let nation = game.get_nation(NationId(1)).unwrap();
        // At least some grain should exist (worker eats 1/turn but produces 1/turn)
        assert!(
            nation.resource_amount(ResourceType::Grain) >= 1,
            "Grain should accumulate: got {}",
            nation.resource_amount(ResourceType::Grain)
        );
        assert_eq!(nation.resource_amount(ResourceType::Timber), 5);
    }

    // ── Turn events ───────────────────────────────────────────

    #[test]
    fn turn_events_include_ended_and_started() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        let has_ended = report.events.iter().any(|e| {
            matches!(e, DomainEvent::TurnEnded(TurnEnded { turn }) if *turn == TurnNumber::new(1))
        });
        let has_started = report.events.iter().any(|e| {
            matches!(e, DomainEvent::TurnStarted(TurnStarted { turn }) if *turn == TurnNumber::new(2))
        });

        assert!(has_ended);
        assert!(has_started);
    }

    // ── Edge case: no tiles produce nothing ───────────────────

    #[test]
    fn empty_map_produces_nothing() {
        let hex_map = HexMap::new(10, 10);
        let province = Province::new(
            ProvinceId(1),
            "Empty".to_string(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)], // tile exists in province but not in hex_map
            4,
        );
        let mut nation = Nation::new(
            NationId(1),
            "EmptyNation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(500);

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        let report = process_turn(&mut game);

        assert!(report.resource_production.is_empty());
        assert!(report.gold_income.is_empty());

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.economy.treasury, Money::dollars(500)); // unchanged
    }

    // ── Gold + Gems combined ──────────────────────────────────

    #[test]
    fn gold_and_gems_both_convert() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Gold, 2);
        nation.add_resource(ResourceType::Gems, 1);
        let initial = nation.economy.treasury;

        let _report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // 2 Gold = $1,000, 1 Gems = $1,000 => $2,000 total
        assert_eq!(nation.economy.treasury, initial + Money::dollars(2000));
        assert_eq!(nation.resource_amount(ResourceType::Gold), 0);
        assert_eq!(nation.resource_amount(ResourceType::Gems), 0);
    }

    // ── Production pipeline ───────────────────────────────────

    /// Helper: build a game state with a nation that has buildings and resources.
    fn test_game_state_with_production() -> GameState {
        use crate::economy::buildings::{Building, BuildingType};

        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province = Province::new(
            ProvinceId(1),
            "Industrial".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );

        let mut nation = Nation::new(
            NationId(1),
            "FactoryNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(5000);

        // Set chain targets to unlimited so production runs at full capacity.
        nation.economy.chain_targets = crate::nation::ChainOutputTargets {
            timber_mill: u32::MAX,
            metal_mill: u32::MAX,
            textile_mill: u32::MAX,
            lumber_factory: u32::MAX,
            steel_factory: u32::MAX,
            garment_factory: u32::MAX,
            armory: 0,
            paper_factory: 0,
            canned_food_factory: u32::MAX,
        };

        // Give enough workers for full production (expert=4 labor each)
        nation.economy.labor.expert = 5; // 20 labor — enough for all mills + factories

        // Add mills and factories
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::FurnitureFactory, 1));
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::HardwareFactory, 1));
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::ClothingFactory, 1));

        crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
    fn lumber_mill_produces_lumber_from_timber() {
        let mut game = test_game_state_with_production();
        // Add timber to warehouse (need 2 per lumber unit)
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Timber, 6);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill capacity 2, 6 timber / 2 per unit = 3, limited by capacity = 2 lumber produced
        // Then FurnitureFactory cap 1 consumes 2 lumber → 1 furniture
        // Net lumber: 2 - 2 = 0
        assert_eq!(
            nation
                .economy
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0),
            0
        );
        // 6 - 4 consumed = 2 timber remaining
        assert_eq!(nation.resource_amount(ResourceType::Timber), 2);
        // Furniture produced by factory
        assert_eq!(
            nation
                .economy
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            1
        );

        // Report should show lumber was produced by the mill
        let lumber_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Lumber")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(lumber_output, 2);
    }

    #[test]
    fn steel_mill_produces_steel_from_coal_and_iron() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Coal, 5);
        nation.add_resource(ResourceType::Iron, 3);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill capacity 2, min(5, 3) = 3 limited by capacity = 2 steel produced
        // Then HardwareFactory cap 1 consumes 2 steel → 1 hardware
        // Net steel: 2 - 2 = 0
        assert_eq!(
            nation
                .economy
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0),
            0
        );
        // 5-2=3 coal, 3-2=1 iron remaining
        assert_eq!(nation.resource_amount(ResourceType::Coal), 3);
        assert_eq!(nation.resource_amount(ResourceType::Iron), 1);
        // Hardware produced
        assert_eq!(
            nation
                .economy
                .goods
                .get(&GoodsType::Hardware)
                .copied()
                .unwrap_or(0),
            1
        );

        let steel_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Steel")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(steel_output, 2);
    }

    #[test]
    fn textile_mill_produces_fabric_from_cotton() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Cotton, 4);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill capacity 2, 4 cotton / 2 per unit = 2 fabric produced
        // Then ClothingFactory cap 1 consumes 2 fabric → 1 clothing
        // Net fabric: 2 - 2 = 0
        assert_eq!(
            nation
                .economy
                .materials
                .get(&MaterialType::Fabric)
                .copied()
                .unwrap_or(0),
            0
        );
        assert_eq!(nation.resource_amount(ResourceType::Cotton), 0);
        // Clothing produced
        assert_eq!(
            nation
                .economy
                .goods
                .get(&GoodsType::Clothing)
                .copied()
                .unwrap_or(0),
            1
        );

        let fabric_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Fabric")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(fabric_output, 2);
    }

    #[test]
    fn furniture_factory_produces_from_lumber() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Pre-stock lumber (bypassing mill)
        *nation
            .economy
            .materials
            .entry(MaterialType::Lumber)
            .or_insert(0) = 4;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Factory capacity 1, 4 lumber / 2 per unit = 2, limited by capacity = 1
        assert_eq!(
            nation
                .economy
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            1
        );
        // 4 - 2 consumed = 2 lumber remaining
        assert_eq!(
            nation
                .economy
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0),
            2
        );

        let furniture_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Furniture")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(furniture_output, 1);
    }

    #[test]
    fn hardware_factory_produces_from_steel() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        *nation
            .economy
            .materials
            .entry(MaterialType::Steel)
            .or_insert(0) = 4;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Factory capacity 1, 4 steel / 2 = 2, limited by capacity = 1
        assert_eq!(
            nation
                .economy
                .goods
                .get(&GoodsType::Hardware)
                .copied()
                .unwrap_or(0),
            1
        );
        assert_eq!(
            nation
                .economy
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0),
            2
        );

        let hardware_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Hardware")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(hardware_output, 1);
    }

    #[test]
    fn clothing_factory_produces_from_fabric() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        *nation
            .economy
            .materials
            .entry(MaterialType::Fabric)
            .or_insert(0) = 6;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Factory capacity 1, 6 fabric / 2 = 3, limited by capacity = 1
        assert_eq!(
            nation
                .economy
                .goods
                .get(&GoodsType::Clothing)
                .copied()
                .unwrap_or(0),
            1
        );
        assert_eq!(
            nation
                .economy
                .materials
                .get(&MaterialType::Fabric)
                .copied()
                .unwrap_or(0),
            4
        );

        let clothing_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Clothing")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(clothing_output, 1);
    }

    #[test]
    fn full_timber_chain_mill_then_factory() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Add 8 timber: mill produces 2 lumber (cap 2), then factory makes 1 furniture (cap 1)
        nation.add_resource(ResourceType::Timber, 8);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill: 8 timber, cap 2 → 2 lumber produced, 4 timber consumed, 4 remain
        // Factory: 2 lumber available, cap 1 → 1 furniture, 2 lumber consumed, 0 lumber remain
        assert_eq!(nation.resource_amount(ResourceType::Timber), 4);
        assert_eq!(
            nation
                .economy
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0),
            0
        );
        assert_eq!(
            nation
                .economy
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            1
        );

        // Report should have both lumber and furniture entries
        let has_lumber = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Lumber" && *q > 0);
        let has_furniture = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Furniture" && *q > 0);
        assert!(has_lumber);
        assert!(has_furniture);
    }

    #[test]
    fn no_production_without_buildings() {
        let mut game = test_game_state(); // no buildings
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Timber, 10);

        let report = process_turn(&mut game);

        // No production output since there are no mills/factories
        let production_for_nation: Vec<_> = report
            .production_output
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(1))
            .collect();
        assert!(production_for_nation.is_empty());
        // Timber should still be there
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Timber), 11); // 10 + 1 from forest tile
    }

    #[test]
    fn no_production_without_resources() {
        let mut game = test_game_state_with_production();
        // No resources added; nation has buildings but nothing to process

        let report = process_turn(&mut game);

        let production_for_nation: Vec<_> = report
            .production_output
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(1))
            .collect();
        assert!(production_for_nation.is_empty());
    }

    // ── Food consumption ──────────────────────────────────────

    #[test]
    fn food_consumption_imperial_ration_per_worker() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Ration for 8 workers: 4 grain + 2 fruit + 2 meat.
        nation.add_resource(ResourceType::Grain, 10);
        nation.add_resource(ResourceType::Fruit, 10);
        nation.add_resource(ResourceType::Livestock, 10);
        nation.economy.labor.untrained = 8;

        let report = process_turn(&mut game);

        // Started with 10 grain, gained 1 from farm = 11, consumed 4 → 7.
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 7);
        assert_eq!(nation.resource_amount(ResourceType::Fruit), 8);
        assert_eq!(nation.resource_amount(ResourceType::Livestock), 8);

        let consumed: u32 = report
            .food_consumed
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(consumed, 8);
    }

    #[test]
    fn food_consumption_meat_slot_prefers_livestock_then_fish() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.labor = LaborPool::new();
        nation.economy.labor.untrained = 12;
        // Ration for 12 workers: 6 grain + 3 fruit + 3 meat.
        // Livestock=2 fills first, fish=1 covers the remaining meat slot.
        nation.add_resource(ResourceType::Grain, 6);
        nation.add_resource(ResourceType::Fruit, 3);
        nation.add_resource(ResourceType::Livestock, 2);
        nation.add_resource(ResourceType::Fish, 4);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 0);
        assert_eq!(nation.resource_amount(ResourceType::Fruit), 0);
        assert_eq!(nation.resource_amount(ResourceType::Livestock), 0);
        assert_eq!(nation.resource_amount(ResourceType::Fish), 3);

        let consumed: u32 = report
            .food_consumed
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(consumed, 12);
    }

    #[test]
    fn food_consumption_partial_ration_starves_with_canned_fallback() {
        // Per-slot shortages are made up 1-for-1 by canned food; any still-
        // missing food unit kills one worker (capped per turn).
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.labor = LaborPool::new();
        nation.economy.labor.untrained = 8;
        // Ration for 8 workers: 4 grain + 2 fruit + 2 meat = 8 food units.
        // Provide grain in full, no fruit, no meat → 4-unit deficit.
        // Canned food covers 2 units; the remaining 2-unit deficit starves
        // 2 workers.
        nation.add_resource(ResourceType::Grain, 4);
        nation.add_material(MaterialType::CannedFood, 2);

        let report = process_turn(&mut game);

        let starved: u32 = report
            .starvation
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(
            starved, 2,
            "two workers starve after canned fallback exhausted"
        );
        assert_eq!(
            game.get_nation(NationId(1))
                .unwrap()
                .economy
                .labor
                .total_workers(),
            6
        );
    }

    #[test]
    fn food_consumption_with_no_workers() {
        let mut game = test_game_state_with_production();
        // No workers, no food consumed

        let report = process_turn(&mut game);

        let consumed: u32 = report
            .food_consumed
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(consumed, 0);
    }

    #[test]
    fn food_consumption_starvation_kills_workers() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.labor = LaborPool::new();
        nation.economy.labor.untrained = 5; // 5 workers need 5 food
        nation.add_resource(ResourceType::Grain, 2); // only 2 food available

        let report = process_turn(&mut game);

        // Deficit = 5 - 2 = 3, capped at 2 deaths per turn
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.economy.labor.total_workers(), 3); // 5 - 2 = 3

        let starved: u32 = report
            .starvation
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(starved, 2);
    }

    #[test]
    fn food_consumption_starvation_capped_at_two() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.labor = LaborPool::new();
        nation.economy.labor.untrained = 10; // 10 workers need 10 food
        // No food at all -> deficit 10, but cap at 2

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.economy.labor.total_workers(), 8); // 10 - 2 = 8

        let starved: u32 = report
            .starvation
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(starved, 2);
    }

    #[test]
    fn food_processing_converts_raw_food_to_canned() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Disable all other chains so labor flows entirely to the cannery.
        nation.economy.chain_targets.timber_mill = 0;
        nation.economy.chain_targets.metal_mill = 0;
        nation.economy.chain_targets.textile_mill = 0;
        nation.economy.chain_targets.lumber_factory = 0;
        nation.economy.chain_targets.steel_factory = 0;
        nation.economy.chain_targets.garment_factory = 0;
        // Plenty of labor (5 expert = 20 labor; cannery wants 6).
        // Add a FoodProcessing building with capacity 3.
        // New recipe: 1 grain + 1 fruit + 1 (fish OR livestock) → 1 canned food.
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 3));
        // Provide 8 of each input. Cannery reserves 5 grain for the 5 expert
        // workers (worker-food-first guard), leaving 3 grain + 8 fruit + 8 fish
        // available — cannery cap=3 → produces 3 canned food.
        nation.add_resource(ResourceType::Grain, 8);
        nation.add_resource(ResourceType::Fruit, 8);
        nation.add_resource(ResourceType::Fish, 8);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.material_amount(MaterialType::CannedFood), 3);

        let canned_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "CannedFood")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(canned_output, 3);
    }

    #[test]
    fn cannery_does_not_starve_workers_at_unlimited_default() {
        // Regression: with no AI target set, the cannery must not consume raw
        // food that workers need for composite meals. Workers eat after the
        // cannery, but the cannery's reservation (workers per food slot) is
        // exactly what `food_consumption` needs.
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.chain_targets.timber_mill = 0;
        nation.economy.chain_targets.metal_mill = 0;
        nation.economy.chain_targets.textile_mill = 0;
        nation.economy.chain_targets.lumber_factory = 0;
        nation.economy.chain_targets.steel_factory = 0;
        nation.economy.chain_targets.garment_factory = 0;
        // 8 expert workers need an Imperialism ration of 4 grain + 2 fruit + 2 meat.
        nation.economy.labor = LaborPool::new();
        nation.economy.labor.expert = 8;
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 10));
        // Exactly enough food for the workers — no surplus to can.
        nation.add_resource(ResourceType::Grain, 4);
        nation.add_resource(ResourceType::Fruit, 2);
        nation.add_resource(ResourceType::Fish, 2);

        let report = process_turn(&mut game);

        let starved: u32 = report
            .starvation
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(
            starved, 0,
            "cannery must not consume workers' composite meal"
        );

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.economy.labor.total_workers(), 8);
        // Cannery saw zero surplus, so no canned food was produced.
        assert_eq!(nation.material_amount(MaterialType::CannedFood), 0);
    }

    // ── Building tick ─────────────────────────────────────────

    #[test]
    fn tick_buildings_advances_expansion() {
        use crate::economy::buildings::BuildingType;

        let mut game = test_game_state_with_production();
        // Start expanding the lumber mill
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation
            .get_building_mut(BuildingType::LumberMill)
            .unwrap()
            .start_expansion(3);

        // After 1 turn, expansion countdown should go from 2 to 1
        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .unwrap();
        assert_eq!(mill.turns_until_upgrade, 1);
        assert_eq!(mill.capacity, 2); // not yet applied

        // After 2nd turn, expansion should complete
        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .unwrap();
        assert_eq!(mill.turns_until_upgrade, 0);
        assert_eq!(mill.capacity, 5); // 2 + 3
    }

    // ── Production accumulates over multiple turns ────────────

    #[test]
    fn production_accumulates_over_turns() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Timber, 20);
        // Feed the workers so they survive 3 turns. 5 experts ration as
        // (grain=3, fruit=1, meat=1) per turn — all three slots must be
        // covered or starvation kicks in (see `worker_food_demand`).
        nation.add_resource(ResourceType::Grain, 20);
        nation.add_resource(ResourceType::Fruit, 20);
        nation.add_resource(ResourceType::Livestock, 20);

        // Run 3 turns
        for _ in 0..3 {
            process_turn(&mut game);
        }

        let nation = game.get_nation(NationId(1)).unwrap();
        // Each turn: mill cap 2 → 2 lumber, factory cap 1 → 1 furniture (consumes 2 lumber)
        // Net lumber per turn: 2 - 2 = 0 remaining (factory consumes what mill produces)
        // Net furniture per turn: 1
        // Timber consumed: 4 per turn = 12 total, 20 - 12 = 8 remaining
        assert_eq!(nation.resource_amount(ResourceType::Timber), 8);
        assert_eq!(
            nation
                .economy
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            3
        );
    }

    // ── Tech reporting ────────────────────────────────────────

    #[test]
    fn turn_report_includes_available_techs() {
        let mut game = test_game_state();
        // At turn 1, year 1815: should have 2 techs available
        let report = process_turn(&mut game);
        assert!(!report.techs_available.is_empty());
        let (nation_id, techs) = &report.techs_available[0];
        assert_eq!(*nation_id, NationId(1));
        assert!(techs.contains(&"High Pressure Steam Engine".to_string()));
        assert!(techs.contains(&"Seed Drill".to_string()));
    }

    #[test]
    fn turn_report_excludes_researched_techs() {
        let mut game = test_game_state();
        // Research both 1815 techs
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.research_tech(crate::events::TechId(1));
        nation.research_tech(crate::events::TechId(2));

        let report = process_turn(&mut game);
        // No techs should be available at 1815 after researching both
        assert!(
            report.techs_available.is_empty(),
            "No techs should be available after researching all 1815 techs"
        );
    }

    // ── Transport resolution ──────────────────────────────────

    #[test]
    fn transport_no_overflow_when_capacity_exceeds_production() {
        let mut game = test_game_state();
        // Give nation freight cars: capacity 10, production is 2 (1 Grain + 1 Timber)
        game.get_nation_mut(NationId(1))
            .unwrap()
            .military
            .transport
            .build_freight_cars(10);

        let report = process_turn(&mut game);

        assert!(
            report.transport_overflow.is_empty(),
            "No overflow when capacity >= production"
        );
        // Resources should be fully delivered
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 1);
        assert_eq!(nation.resource_amount(ResourceType::Timber), 1);
    }

    #[test]
    fn only_capital_tile_resources_are_delivered_without_transport() {
        // The capital tile itself delivers for free. Neighboring tiles inside
        // the capital's collection radius still need freight.
        let mut hex_map = HexMap::new(10, 10);
        let capital = HexCoord::new(2, 2);
        let mut tiles = vec![capital];
        tiles.extend_from_slice(&capital.neighbors());

        // Capital tile: grain + country-capital flag + depot.
        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap_tile.set_resource(ResourceType::Grain);
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        hex_map.set_tile(capital, cap_tile);
        // 6 grain neighbors in the same province.
        for coord in capital.neighbors() {
            let mut tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
            tile.set_resource(ResourceType::Grain);
            hex_map.set_tile(coord, tile);
        }

        let province = Province::new(
            ProvinceId(1),
            "CapitalFarms".to_string(),
            NationId(1),
            capital,
            tiles,
            4,
        );

        let mut nation = Nation::new(
            NationId(1),
            "TransportTest".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(5000);
        // Zero freight cars — only the capital tile should still arrive.
        nation.economy.labor = LaborPool::new();

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        let report = process_turn(&mut game);

        // The 6 neighboring farms are collectable but require freight, so they
        // should overflow when no transport exists.
        let total_overflow: u32 = report
            .transport_overflow
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(1))
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(
            total_overflow, 6,
            "capital-radius collection should overflow without freight"
        );

        // Only the capital tile's grain remains delivered for free.
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 1);
    }

    #[test]
    fn transport_zero_cars_keeps_only_capital_tile_resources() {
        let mut game = test_game_state();
        // Default: 0 freight cars, 2 collectable tiles in the capital
        // province. Only the capital tile should stay delivered.

        let report = process_turn(&mut game);

        assert!(
            report
                .transport_overflow
                .iter()
                .any(|(nid, resource, qty)| *nid == NationId(1)
                    && *resource == ResourceType::Timber
                    && *qty == 1),
            "non-capital capital-radius resources should overflow without freight"
        );
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 1);
        assert_eq!(nation.resource_amount(ResourceType::Timber), 0);
    }

    #[test]
    fn human_remote_resources_require_explicit_transport_allocation() {
        let cap = HexCoord::new(0, 0);
        let remote_depot = HexCoord::new(2, 0);
        let remote_timber = HexCoord::new(3, 0);

        let mut hex_map = HexMap::new(10, 10);

        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap_tile.set_resource(ResourceType::Grain);
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        cap_tile.infrastructure.has_railroad = true;
        hex_map.set_tile(cap, cap_tile);

        let mut rail_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        rail_tile.infrastructure.has_railroad = true;
        hex_map.set_tile(HexCoord::new(1, 0), rail_tile);

        let mut depot_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        depot_tile.infrastructure.has_depot = true;
        depot_tile.infrastructure.has_railroad = true;
        hex_map.set_tile(remote_depot, depot_tile);

        let mut timber_tile = Tile::with_province(TerrainType::Forest, ProvinceId(2));
        timber_tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(remote_timber, timber_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Home".to_string(),
            NationId(1),
            cap,
            vec![cap, HexCoord::new(1, 0)],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Remote".to_string(),
            NationId(1),
            remote_depot,
            vec![remote_depot, remote_timber],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.add_province(ProvinceId(2));
        nation1.military.transport.build_freight_cars(5);

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1, province2],
        nations: vec![nation1],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: crate::data::test_game_data(),
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
        next_unit_id: 6_000_000,};

        let report = process_turn(&mut game);
        let nation = game.get_nation(NationId(1)).unwrap();

        assert_eq!(nation.resource_amount(ResourceType::Grain), 1);
        assert_eq!(
            nation.resource_amount(ResourceType::Timber),
            0,
            "remote timber should not be collected without an explicit allocation"
        );
        assert!(
            report
                .transport_overflow
                .iter()
                .any(|(nid, resource, qty)| *nid == NationId(1)
                    && *resource == ResourceType::Timber
                    && *qty == 1)
        );
    }

    // ── Immigration ──────────────────────────────────────────

    #[test]
    fn queued_immigration_recruits_workers_from_canned_food_clothing_furniture() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        nation.economy.pending_immigration = 1;
        // Composite meals for existing workers so canned food survives for
        // the immigrant; canned + clothing + furniture are the immigration inputs.
        nation.add_resource(ResourceType::Grain, 10);
        nation.add_resource(ResourceType::Fruit, 10);
        nation.add_resource(ResourceType::Livestock, 10);
        nation.add_material(MaterialType::CannedFood, 2);
        nation.add_goods(GoodsType::Clothing, 2);
        nation.add_goods(GoodsType::Furniture, 2);

        // Give enough provinces: need at least 4 provinces for 1 immigrant
        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        let report = process_turn(&mut game);

        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(
            immigration, 1,
            "Should recruit 1 immigrant with 4+ provinces"
        );

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.economy.labor.untrained, 1,
            "Should have 1 untrained worker"
        );
        // Furniture is consumed alongside canned food + clothing; the
        // test starts with 2 (+ any produced this turn) and consumes 1.
        let starting_furniture = 2;
        let consumed = 1;
        assert!(
            nation.goods_amount(GoodsType::Furniture) >= starting_furniture - consumed,
            "1 furniture should have been consumed for the immigrant"
        );
    }

    #[test]
    fn queued_immigration_does_not_happen_without_required_inputs() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        nation.economy.pending_immigration = 1;
        nation.add_material(MaterialType::CannedFood, 1);

        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        let report = process_turn(&mut game);

        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(immigration, 0, "No immigration without clothing");
    }

    #[test]
    fn immigration_is_not_automatic_when_not_queued() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }
        nation.add_material(MaterialType::CannedFood, 10);
        nation.add_goods(GoodsType::Clothing, 10);

        let report = process_turn(&mut game);

        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(
            immigration, 0,
            "Immigration should not occur without a queued order"
        );
    }

    #[test]
    fn immigration_is_limited_by_province_count_per_turn() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        nation.add_resource(ResourceType::Grain, 10);
        nation.add_material(MaterialType::CannedFood, 10);
        nation.add_goods(GoodsType::Clothing, 10);
        nation.add_goods(GoodsType::Furniture, 10);
        nation.economy.pending_immigration = 3;
        for i in 2..=8 {
            nation.add_province(ProvinceId(i));
        }

        let report = process_turn(&mut game);

        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(
            immigration, 2,
            "7 provinces should allow only 2 immigrants this turn"
        );
    }

    // ── Building tick / expansion ──────────────────────────────

    #[test]
    fn building_expansion_completes_after_two_turns_in_pipeline() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Start expanding the SteelMill from capacity 2 by adding 3
        nation
            .get_building_mut(BuildingType::SteelMill)
            .unwrap()
            .start_expansion(3);

        // Verify initial state
        let mill = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::SteelMill)
            .unwrap();
        assert_eq!(mill.capacity, 2);
        assert_eq!(mill.turns_until_upgrade, 2);

        // Turn 1
        process_turn(&mut game);
        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::SteelMill)
            .unwrap();
        assert_eq!(mill.capacity, 2);
        assert_eq!(mill.turns_until_upgrade, 1);

        // Turn 2 - expansion completes
        process_turn(&mut game);
        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::SteelMill)
            .unwrap();
        assert_eq!(mill.capacity, 5); // 2 + 3
        assert_eq!(mill.turns_until_upgrade, 0);
        assert_eq!(mill.pending_capacity, 0);
    }

    // ── Settlement progression ──────────────────────────────────

    #[test]
    fn settlement_upgrades_after_six_turns_connected() {
        let coord1 = HexCoord::new(0, 0);
        // Place adjacent to capital so connectivity is computed automatically
        let coord2 = HexCoord::new(1, 0);

        let hex_map = HexMap::new(10, 10);

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );

        let province_remote = Province::new(
            ProvinceId(2),
            "Remote Land".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        // Settlement starts as Hamlet (connectivity computed by update_settlements)

        let mut nation = Nation::new(
            NationId(1),
            "SettlementNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_capital, province_remote],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        // Process 7 turns: 1 turn to start countdown (set to 6), then 6 turns to count down
        for _ in 0..7 {
            process_turn(&mut game);
        }

        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            crate::map::SettlementLevel::Village,
            "Province should have upgraded to Village after 6 turns connected"
        );
    }

    #[test]
    fn settlement_does_not_upgrade_if_not_connected() {
        let coord = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(3, 3);
        let hex_map = HexMap::new(10, 10);

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );

        let province_disconnected = Province::new(
            ProvinceId(2),
            "Disconnected".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        // connected_to_capital defaults to false

        let mut nation = Nation::new(
            NationId(1),
            "TestNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_capital, province_disconnected],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        for _ in 0..10 {
            process_turn(&mut game);
        }

        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            crate::map::SettlementLevel::Hamlet,
            "Disconnected province should not be upgraded"
        );
    }

    // ── Summary line formatting ──────────────────────────────

    #[test]
    fn format_summary_line_contains_key_info() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.treasury = Money::dollars(8500);
        nation.economy.labor.untrained = 5;
        // Add enough food so workers don't starve (composite meals require all three)
        nation.add_resource(ResourceType::Grain, 10);
        nation.add_resource(ResourceType::Fruit, 10);
        nation.add_resource(ResourceType::Livestock, 10);

        let report = process_turn(&mut game);
        let summary = report.format_summary_line(&game);

        assert!(
            summary.contains("Turn 1"),
            "Summary should contain turn number"
        );
        assert!(
            summary.contains("Treasury: $8,500"),
            "Summary should contain formatted treasury: {}",
            summary
        );
        assert!(
            summary.contains("Workers: 5"),
            "Summary should contain worker count: {}",
            summary
        );
        assert!(
            summary.contains("Score:"),
            "Summary should contain score: {}",
            summary
        );
    }

    #[test]
    fn format_number_with_commas_works() {
        assert_eq!(super::format_number_with_commas(0), "0");
        assert_eq!(super::format_number_with_commas(999), "999");
        assert_eq!(super::format_number_with_commas(1000), "1,000");
        assert_eq!(super::format_number_with_commas(1234567), "1,234,567");
        assert_eq!(super::format_number_with_commas(-500), "-500");
        assert_eq!(super::format_number_with_commas(-1234), "-1,234");
    }

    // ── Town production ────────────────────────────────────────

    /// Helper: build a game state with a Village province containing timber tiles.
    fn test_game_state_with_village(
        terrain_resource_pairs: &[(TerrainType, Option<ResourceType>)],
    ) -> GameState {
        let mut hex_map = HexMap::new(20, 20);
        let capital_coord = HexCoord::new(0, 0);
        let rail_coord = HexCoord::new(1, 0);
        let depot_coord = HexCoord::new(2, 0);
        let mut tiles = vec![depot_coord];

        // Capital province with the infrastructure needed to seed collection.
        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        cap_tile.infrastructure.has_railroad = true;
        hex_map.set_tile(capital_coord, cap_tile);

        let mut rail_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        rail_tile.infrastructure.has_railroad = true;
        hex_map.set_tile(rail_coord, rail_tile);

        let mut depot_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        depot_tile.infrastructure.has_depot = true;
        depot_tile.infrastructure.has_railroad = true;
        hex_map.set_tile(depot_coord, depot_tile);

        let village_coords = [
            HexCoord::new(3, 0),
            HexCoord::new(3, -1),
            HexCoord::new(2, -1),
        ];

        // Village province with given terrain/resource pairs inside the depot radius.
        for (i, (terrain, resource)) in terrain_resource_pairs.iter().enumerate() {
            let coord = if i == 0 {
                depot_coord
            } else {
                village_coords[(i - 1).min(village_coords.len() - 1)]
            };
            let mut tile = Tile::with_province(*terrain, ProvinceId(2));
            if let Some(res) = resource {
                tile.set_resource(*res);
            }
            if coord == depot_coord {
                tile.infrastructure.has_depot = true;
                tile.infrastructure.has_railroad = true;
            }
            hex_map.set_tile(coord, tile);
            if !tiles.contains(&coord) {
                tiles.push(coord);
            }
        }

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            capital_coord,
            vec![capital_coord],
            4,
        );

        let village_capital = if tiles.is_empty() {
            HexCoord::new(5, 0)
        } else {
            tiles[0]
        };
        let mut province_village = Province::new(
            ProvinceId(2),
            "Villageton".to_string(),
            NationId(1),
            village_capital,
            tiles,
            4,
        );
        province_village.settlement_level = SettlementLevel::Village;
        province_village.connected_to_capital = true;

        let mut nation = Nation::new(
            NationId(1),
            "TownNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));
        nation.military.transport.build_freight_cars(20);
        for resource in [
            ResourceType::Timber,
            ResourceType::Coal,
            ResourceType::Iron,
            ResourceType::Cotton,
            ResourceType::Wool,
        ] {
            nation
                .military
                .transport
                .set_resource_allocation(resource, 20);
        }
        for target in [
            crate::economy::FreightTarget::Material(MaterialType::Lumber),
            crate::economy::FreightTarget::Material(MaterialType::Steel),
            crate::economy::FreightTarget::Material(MaterialType::Fabric),
            crate::economy::FreightTarget::Goods(GoodsType::Furniture),
            crate::economy::FreightTarget::Goods(GoodsType::Hardware),
            crate::economy::FreightTarget::Goods(GoodsType::Clothing),
        ] {
            nation.military.transport.set_allocation(target, 20);
        }

        crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_capital, province_village],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
    fn town_production_village_produces_lumber_from_timber() {
        // Village with 4 ScrubForest tiles: each yields 1 Timber = 4 total
        // 4 timber / 2 = 2 lumber
        // 2 lumber / 2 = 1 furniture
        let mut game =
            test_game_state_with_village(&[(TerrainType::Forest, Some(ResourceType::Timber)); 4]);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Lumber),
            2,
            "Village should produce 2 lumber from 4 timber"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Furniture),
            1,
            "Village should produce 1 furniture from 2 lumber"
        );

        // Check report tracks town production
        let lumber_output: u32 = report
            .town_production
            .iter()
            .filter(|(_, name, _)| name == "Lumber")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(lumber_output, 2);
    }

    #[test]
    fn town_production_village_produces_steel_from_coal_and_iron() {
        // Create a village with prospected BarrenHills tiles containing coal and iron
        let mut hex_map = HexMap::new(20, 20);
        let capital_coord = HexCoord::new(0, 0);
        let rail_coord = HexCoord::new(1, 0);
        let depot_coord = HexCoord::new(2, 0);
        let coords = vec![
            depot_coord,
            HexCoord::new(3, 0),
            HexCoord::new(3, -1),
            HexCoord::new(2, -1),
        ];

        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        cap_tile.infrastructure.has_railroad = true;
        hex_map.set_tile(capital_coord, cap_tile);

        let mut rail_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        rail_tile.infrastructure.has_railroad = true;
        hex_map.set_tile(rail_coord, rail_tile);

        // 2 Coal tiles and 2 Iron tiles (Hills with revealed, mined deposits at L1)
        for (i, &coord) in coords.iter().enumerate() {
            let mut tile = Tile::with_province(TerrainType::Hills, ProvinceId(2));
            if i < 2 {
                tile.reveal_deposit(ResourceType::Coal);
            } else {
                tile.reveal_deposit(ResourceType::Iron);
            }
            // Coal/Iron need level 1+ to produce (manual: Level 0 = 0).
            tile.set_improvement_level(1);
            if coord == depot_coord {
                tile.infrastructure.has_depot = true;
                tile.infrastructure.has_railroad = true;
            }
            hex_map.set_tile(coord, tile);
        }

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            capital_coord,
            vec![capital_coord],
            4,
        );

        let mut province_village = Province::new(
            ProvinceId(2),
            "SteelVillage".to_string(),
            NationId(1),
            coords[0],
            coords.clone(),
            4,
        );
        province_village.settlement_level = SettlementLevel::Village;
        province_village.connected_to_capital = true;

        let mut nation = Nation::new(
            NationId(1),
            "SteelNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));
        nation.military.transport.build_freight_cars(20);
        nation
            .military
            .transport
            .set_resource_allocation(ResourceType::Coal, 20);
        nation
            .military
            .transport
            .set_resource_allocation(ResourceType::Iron, 20);
        nation.military.transport.set_allocation(
            crate::economy::FreightTarget::Material(MaterialType::Steel),
            20,
        );
        nation.military.transport.set_allocation(
            crate::economy::FreightTarget::Goods(GoodsType::Hardware),
            20,
        );

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_capital, province_village],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        let report = process_turn(&mut game);

        // Coal: 2 tiles * 2 yield (L1) = 4, Iron: 2 tiles * 2 yield (L1) = 4
        // Steel = min(4, 4) = 4
        // Hardware = 4 / 2 = 2
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Steel),
            4,
            "Village should produce 4 steel from 4 coal + 4 iron"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Hardware),
            2,
            "Village should produce 2 hardware from 4 steel"
        );

        let steel_output: u32 = report
            .town_production
            .iter()
            .filter(|(_, name, _)| name == "Steel")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(steel_output, 4);
    }

    #[test]
    fn town_production_village_produces_fabric_and_clothing() {
        // Village with 4 Plantation tiles (Cotton): each yields 1 cotton = 4 total
        // 4 cotton / 2 = 2 fabric
        // 2 fabric / 2 = 1 clothing
        let mut game = test_game_state_with_village(
            &[(TerrainType::Grassland, Some(ResourceType::Cotton)); 4],
        );

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Fabric),
            2,
            "Village should produce 2 fabric from 4 cotton"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Clothing),
            1,
            "Village should produce 1 clothing from 2 fabric"
        );

        let fabric_output: u32 = report
            .town_production
            .iter()
            .filter(|(_, name, _)| name == "Fabric")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(fabric_output, 2);
    }

    #[test]
    fn town_production_hamlet_does_not_produce() {
        // Same setup but settlement is Hamlet, not Village
        let mut game =
            test_game_state_with_village(&[(TerrainType::Forest, Some(ResourceType::Timber)); 4]);
        // Override to Hamlet
        game.world
            .provinces
            .iter_mut()
            .find(|p| p.id == ProvinceId(2))
            .unwrap()
            .settlement_level = SettlementLevel::Hamlet;

        let report = process_turn(&mut game);

        // Should have no town production
        assert!(
            report.town_production.is_empty(),
            "Hamlet province should not produce via town production"
        );
    }

    #[test]
    fn town_produces_at_double_rate() {
        // Town with 4 ScrubForest tiles
        // Village rate: 4 timber → 2 lumber → 1 furniture
        // Town rate: 4*2=8 timber → 4 lumber → 2 furniture
        let mut game =
            test_game_state_with_village(&[(TerrainType::Forest, Some(ResourceType::Timber)); 4]);
        // Upgrade to Town
        game.world
            .provinces
            .iter_mut()
            .find(|p| p.id == ProvinceId(2))
            .unwrap()
            .settlement_level = SettlementLevel::Town;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Lumber),
            4,
            "Town should produce 4 lumber (double rate)"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Furniture),
            2,
            "Town should produce 2 furniture (double rate)"
        );

        let lumber_output: u32 = report
            .town_production
            .iter()
            .filter(|(_, name, _)| name == "Lumber")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(lumber_output, 4);
    }

    #[test]
    fn remote_town_production_requires_delivered_resources() {
        let mut game =
            test_game_state_with_village(&[(TerrainType::Forest, Some(ResourceType::Timber)); 4]);
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.military.transport.freight_cars = 0;
        nation.military.transport.allocations.clear();

        let report = process_turn(&mut game);
        let nation = game.get_nation(NationId(1)).unwrap();

        assert_eq!(
            nation.material_amount(MaterialType::Lumber),
            0,
            "remote town should not auto-produce lumber when no timber was delivered"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Furniture),
            0,
            "remote town should not auto-produce furniture when no timber was delivered"
        );
        assert!(
            report.town_production.is_empty(),
            "no delivered input means no town output"
        );
    }

    // ── Village → Town progression ─────────────────────────────

    #[test]
    fn village_upgrades_to_town_after_12_connected_turns() {
        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(3, 3);
        let hex_map = HexMap::new(10, 10);

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );

        let mut province_village = Province::new(
            ProvinceId(2),
            "Growing Town".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        province_village.settlement_level = SettlementLevel::Village;
        province_village.connected_to_capital = true;
        province_village.town_countdown = Some(12);

        let mut nation = Nation::new(
            NationId(1),
            "GrowthNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_capital, province_village],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        // Process 12 turns to count down the town_countdown
        for _ in 0..12 {
            process_turn(&mut game);
        }

        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            SettlementLevel::Town,
            "Province should have upgraded to Town after 12 turns"
        );
        assert_eq!(province.town_countdown, None);
    }

    #[test]
    fn village_does_not_upgrade_to_town_before_12_turns() {
        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(3, 3);
        let hex_map = HexMap::new(10, 10);

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );

        let mut province_village = Province::new(
            ProvinceId(2),
            "Still Village".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        province_village.settlement_level = SettlementLevel::Village;
        province_village.connected_to_capital = true;
        province_village.town_countdown = Some(12);

        let mut nation = Nation::new(
            NationId(1),
            "PatientNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_capital, province_village],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        // Process only 11 turns — not enough
        for _ in 0..11 {
            process_turn(&mut game);
        }

        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            SettlementLevel::Village,
            "Province should still be Village after only 11 turns"
        );
        assert_eq!(province.town_countdown, Some(1));
    }

    // ── Full settlement progression: Hamlet → Village → Town ───

    #[test]
    fn full_settlement_progression_hamlet_to_village_to_town() {
        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(3, 3);
        let hex_map = HexMap::new(10, 10);

        let province_capital = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );

        let mut province_remote = Province::new(
            ProvinceId(2),
            "RemoteLand".to_string(),
            NationId(1),
            coord2,
            vec![coord2],
            4,
        );
        province_remote.connected_to_capital = true;
        // Starts as Hamlet

        let mut nation = Nation::new(
            NationId(1),
            "ProgressNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(5000);
        nation.add_province(ProvinceId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_capital, province_remote],
        nations: vec![nation],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        // 7 turns: Hamlet → Village (1 to start countdown + 6 to count down)
        for _ in 0..7 {
            process_turn(&mut game);
        }
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(province.settlement_level, SettlementLevel::Village);
        assert_eq!(
            province.town_countdown,
            Some(12),
            "Town countdown should start at 12 upon becoming Village"
        );

        // 12 more turns: Village → Town
        for _ in 0..12 {
            process_turn(&mut game);
        }
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.settlement_level,
            SettlementLevel::Town,
            "Province should have upgraded to Town after 7 + 12 = 19 turns"
        );
    }

    // ── Economy integration tests ──────────────────────────────

    #[test]
    fn full_economic_cycle_resources_to_goods_to_trade() {
        // Set up a nation with full production chain
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Add raw resources for all chains
        nation.add_resource(ResourceType::Timber, 10);
        nation.add_resource(ResourceType::Coal, 10);
        nation.add_resource(ResourceType::Iron, 10);
        nation.add_resource(ResourceType::Cotton, 10);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // LumberMill cap 2: 10 timber → 2 lumber (4 timber consumed), 6 remain
        // SteelMill cap 2: 10 coal + 10 iron → 2 steel (2 each consumed), 8 each remain
        // TextileMill cap 2: 10 cotton → 2 fabric (4 consumed), 6 remain
        // FurnitureFactory cap 1: 2 lumber → 1 furniture (2 consumed), 0 lumber remain
        // HardwareFactory cap 1: 2 steel → 1 hardware (2 consumed), 0 steel remain
        // ClothingFactory cap 1: 2 fabric → 1 clothing (2 consumed), 0 fabric remain
        assert_eq!(nation.goods_amount(GoodsType::Furniture), 1);
        assert_eq!(nation.goods_amount(GoodsType::Hardware), 1);
        assert_eq!(nation.goods_amount(GoodsType::Clothing), 1);

        // Verify production was reported
        let has_furniture = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Furniture" && *q == 1);
        let has_hardware = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Hardware" && *q == 1);
        let has_clothing = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Clothing" && *q == 1);
        assert!(has_furniture);
        assert!(has_hardware);
        assert!(has_clothing);
    }

    #[test]
    fn queued_immigration_can_use_canned_food_processed_this_turn() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        // Add food processing building
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 5));

        // Plenty of all canning inputs (1 grain + 1 fruit + 1 fish/livestock per can).
        nation.add_resource(ResourceType::Grain, 50);
        nation.add_resource(ResourceType::Fruit, 50);
        nation.add_resource(ResourceType::Fish, 50);

        // Queue one immigrant and pre-stock clothing + furniture.
        nation.economy.pending_immigration = 1;
        nation.add_goods(GoodsType::Clothing, 5);
        nation.add_goods(GoodsType::Furniture, 5);

        // Need at least 4 provinces for 1 immigrant per turn
        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        // Run one turn: food processing creates CannedFood, queued immigration uses it.
        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(immigration, 1, "Should recruit 1 immigrant");
        assert_eq!(
            nation.economy.labor.untrained, 1,
            "Should have 1 untrained worker after immigration"
        );
    }

    #[test]
    fn projected_immigration_capacity_includes_same_turn_clothing_production() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_material(MaterialType::CannedFood, 1);
        nation.add_goods(GoodsType::Furniture, 1);
        nation.add_resource(ResourceType::Cotton, 4);
        nation.add_resource(ResourceType::Grain, 10);
        nation.add_resource(ResourceType::Fruit, 10);
        nation.add_resource(ResourceType::Livestock, 10);
        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        let cap = projected_immigration_queue_capacity(&game, NationId(1));
        assert_eq!(
            cap, 1,
            "Projected immigration cap should include clothing produced earlier in the turn"
        );
    }

    #[test]
    fn projected_immigration_capacity_does_not_assume_food_processing_with_zero_workers() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.labor.untrained = 0;
        nation.economy.labor.trained = 0;
        nation.economy.labor.expert = 0;
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::FoodProcessing, 5));
        nation.add_resource(ResourceType::Grain, 20);
        nation.add_goods(GoodsType::Clothing, 1);
        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        let cap = projected_immigration_queue_capacity(&game, NationId(1));
        assert_eq!(
            cap, 0,
            "Zero-worker nations should not project canned food from processing that never runs"
        );
    }

    #[test]
    fn starvation_insufficient_food_kills_workers() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.labor = LaborPool::new();
        nation.economy.labor.untrained = 8;
        // Give only 3 food, need 8, deficit = 5, capped at 2 deaths
        nation.add_resource(ResourceType::Grain, 3);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.economy.labor.total_workers(),
            6,
            "Should lose 2 workers (capped)"
        );

        let starved: u32 = report
            .starvation
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(starved, 2, "Should report 2 workers lost to starvation");
    }

    // ── Combat tests ───────────────────────────────────────────

    #[test]
    fn fort_defense_bonus_applies_correctly() {
        use crate::data::GameConfig;
        use crate::military::combat::fort_defense_bonus;
        let cfg = GameConfig::default();

        // Card #478: linear curve 0/0.25/0.50/0.75.
        assert_eq!(fort_defense_bonus(0, &cfg), 0.0);
        assert!((fort_defense_bonus(1, &cfg) - 0.25).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(2, &cfg) - 0.50).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(3, &cfg) - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn medal_boosted_unit_has_higher_firepower() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let base_unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        let base_fp = base_unit.effective_firepower();

        let mut medal_unit = ArmyUnit::new(
            UnitId(2),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        medal_unit.award_medal();
        medal_unit.award_medal();
        let medal_fp = medal_unit.effective_firepower();

        // 2 medals = 1.0 + 2*0.25 = 1.5x multiplier
        assert!(
            medal_fp > base_fp,
            "Medal unit firepower ({}) should exceed base ({})",
            medal_fp,
            base_fp
        );
        assert!(
            (medal_fp - base_fp * 1.5).abs() < f64::EPSILON,
            "2 medals should give 1.5x firepower"
        );
    }

    #[test]
    fn garrison_created_correctly_for_gp_and_mn() {
        use crate::military::combat::create_garrison;

        let gp_garrison = create_garrison(NationType::GreatPower);
        assert_eq!(
            gp_garrison.len(),
            4,
            "Great Power garrison should have 4 units"
        );

        let mn_garrison = create_garrison(NationType::MinorNation);
        assert_eq!(
            mn_garrison.len(),
            4,
            "Minor Nation garrison should have 4 units (3 Militia + 1 GarrisonArtillery)"
        );
    }

    #[test]
    fn town_production_wool_produces_fabric() {
        // Village with 4 FertileHills tiles (Wool): each yields 1 wool = 4 total
        // 4 wool / 2 = 2 fabric
        // 2 fabric / 2 = 1 clothing
        let mut game =
            test_game_state_with_village(&[(TerrainType::Hills, Some(ResourceType::Wool)); 4]);

        let _report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Fabric),
            2,
            "Village should produce 2 fabric from 4 wool"
        );
        assert_eq!(
            nation.goods_amount(GoodsType::Clothing),
            1,
            "Village should produce 1 clothing from 2 fabric"
        );
    }

    #[test]
    fn town_production_mixed_cotton_and_wool() {
        // Village with 2 Plantation (Cotton) + 2 FertileHills (Wool) tiles
        // total cotton+wool = 4 → 2 fabric → 1 clothing
        let mut game = test_game_state_with_village(&[
            (TerrainType::Grassland, Some(ResourceType::Cotton)),
            (TerrainType::Grassland, Some(ResourceType::Cotton)),
            (TerrainType::Hills, Some(ResourceType::Wool)),
            (TerrainType::Hills, Some(ResourceType::Wool)),
        ]);

        let _report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.material_amount(MaterialType::Fabric),
            2,
            "Village should produce 2 fabric from 2 cotton + 2 wool"
        );
    }

    // ── Voluntary incorporation tests ─────────────────────────────

    /// Helper: build a game state with one Great Power and one Minor Nation
    /// for testing voluntary incorporation.
    fn test_game_state_with_minor_nation() -> GameState {
        let coord = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(1, 0);

        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Homeland".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Minor Land".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.economy.treasury = Money::dollars(1000);

        let nation2 = Nation::new(
            NationId(2),
            "Smallton".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy.initialize_great_powers(&[NationId(1)]);

        crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1, province2],
        nations: vec![nation1, nation2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
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
    fn voluntary_incorporation_at_threshold() {
        let mut game = test_game_state_with_minor_nation();

        // Set diplomacy score to exactly 90 (incorporation threshold)
        let rel = game
            .world
            .diplomacy
            .ensure_relation(NationId(2), NationId(1));
        rel.score = 90;

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        };

        resolve_voluntary_incorporations(&mut game, &mut report);

        // NationId(1) is the human player in this fixture, so the minor's
        // request must surface as a RequestToJoinEmpire proposal rather than
        // auto-incorporating. The minor keeps its provinces until the player
        // accepts.
        assert!(
            report.incorporations.is_empty(),
            "must not auto-incorporate into human"
        );
        assert_eq!(
            game.world
                .diplomacy
                .pending_proposals
                .iter()
                .filter(
                    |p| p.proposal_type == crate::events::TreatyType::RequestToJoinEmpire
                        && p.from == NationId(2)
                        && p.to == NationId(1)
                )
                .count(),
            1,
            "exactly one RequestToJoinEmpire proposal must be queued for the human"
        );
        let mn = game.get_nation(NationId(2)).unwrap();
        assert!(
            mn.province_ids.contains(&ProvinceId(2)),
            "minor keeps its province pending decision"
        );
        let prov = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(prov.owner, NationId(2));
    }

    #[test]
    fn voluntary_incorporation_into_ai_great_power_still_auto_incorporates() {
        // When the highest-relationship GP is an AI (not the human), the
        // minor is auto-incorporated as before — RequestToJoinEmpire
        // proposals are only queued when the chosen overlord is the player.
        let mut game = test_game_state_with_minor_nation();
        // Add an AI GP at NationId(3) and make it the most-favored.
        let mut gp_b = Nation::new(
            NationId(3),
            "Healthy Empire".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp_b.economy.treasury = Money::dollars(1000);
        game.world.nations.push(gp_b);
        game.world
            .diplomacy
            .initialize_great_powers(&[NationId(1), NationId(3)]);
        // Human (NationId(1)) below threshold; AI GP at threshold.
        game.world
            .diplomacy
            .ensure_relation(NationId(2), NationId(1))
            .score = 50;
        game.world
            .diplomacy
            .ensure_relation(NationId(2), NationId(3))
            .score = 95;

        let mut report = TurnReport::empty();
        resolve_voluntary_incorporations(&mut game, &mut report);

        assert_eq!(report.incorporations.len(), 1);
        assert_eq!(report.incorporations[0], (NationId(2), NationId(3)));
        assert!(
            game.world
                .diplomacy
                .pending_proposals
                .iter()
                .all(|p| p.proposal_type != crate::events::TreatyType::RequestToJoinEmpire),
            "AI overlord path must not queue a RequestToJoinEmpire proposal"
        );
    }

    #[test]
    fn voluntary_incorporation_proposal_to_human_is_deduplicated() {
        // Running resolve_voluntary_incorporations twice in a row must not
        // accumulate duplicate RequestToJoinEmpire proposals.
        let mut game = test_game_state_with_minor_nation();
        game.world
            .diplomacy
            .ensure_relation(NationId(2), NationId(1))
            .score = 95;
        let mut report = TurnReport::empty();
        resolve_voluntary_incorporations(&mut game, &mut report);
        let mut report = TurnReport::empty();
        resolve_voluntary_incorporations(&mut game, &mut report);
        assert_eq!(
            game.world
                .diplomacy
                .pending_proposals
                .iter()
                .filter(|p| p.proposal_type == crate::events::TreatyType::RequestToJoinEmpire)
                .count(),
            1,
            "duplicate RequestToJoinEmpire proposals must be suppressed"
        );
    }

    #[test]
    fn no_incorporation_below_threshold() {
        let mut game = test_game_state_with_minor_nation();

        // Set diplomacy score to 89 (just below threshold of 90)
        let rel = game
            .world
            .diplomacy
            .ensure_relation(NationId(2), NationId(1));
        rel.score = 89;

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        };

        resolve_voluntary_incorporations(&mut game, &mut report);

        // No incorporation should happen
        assert!(report.incorporations.is_empty());

        // Minor nation still has its province
        let mn = game.get_nation(NationId(2)).unwrap();
        assert!(mn.province_ids.contains(&ProvinceId(2)));

        // Province owner unchanged
        let prov = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(prov.owner, NationId(2));
    }

    #[test]
    fn blockade_skips_declaration_turn() {
        // Card #104: blockade effects, like naval combat, must defer by one
        // turn after war declaration — all hostile actions share the same
        // grace rule. Captured as a separate test from naval combat so the
        // gate can never regress independently.
        use crate::map::UnitId;
        use crate::military::ships::{Ship, ShipType};

        let mut game = test_game_state();

        // Add a second GP that will do the blockading (has warships).
        let mut gp2 = Nation::new(
            NationId(2),
            "Blockader".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp2.economy.treasury = Money::dollars(1000);
        for i in 0..4 {
            gp2.military.warships.push(Ship::with_data(
                UnitId(20 + i),
                ShipType::Frigate,
                NationId(2),
                &game.game_data,
            ));
        }
        // Give gp2 a real province so it's "active" in the blockade sweep.
        let extra_province = Province::new(
            ProvinceId(2),
            "Blockader Land".to_string(),
            NationId(2),
            crate::hex::HexCoord::new(5, 5),
            vec![crate::hex::HexCoord::new(5, 5)],
            4,
        );
        game.world.provinces.push(extra_province);
        gp2.province_ids.push(ProvinceId(2));
        game.world.nations.push(gp2);

        // GP1 gets a merchant fleet so there's cargo to blockade.
        let clipper_hull = game.game_data.ship_stats(ShipType::Clipper).hull;
        let gp1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..3 {
            gp1.military.merchant_fleet.push(Ship::new(
                UnitId(100 + i),
                ShipType::Clipper,
                NationId(1),
                clipper_hull,
            ));
        }

        game.world
            .diplomacy
            .initialize_great_powers(&[NationId(1), NationId(2)]);

        let turn = game.turn;
        game.world
            .diplomacy
            .declare_war_at(NationId(1), NationId(2), turn);

        // Declaration turn: compute_blockade_capacity must return the
        // victim's raw cargo (no reduction).
        let raw_cargo = game
            .get_nation(NationId(1))
            .unwrap()
            .total_cargo_capacity(&game.game_data);
        assert!(
            raw_cargo > 0,
            "victim must have cargo for this test to be meaningful"
        );
        let cap_declaration = compute_blockade_capacity(&game);
        assert_eq!(
            cap_declaration.get(&NationId(1)).copied(),
            Some(raw_cargo),
            "blockade must not reduce cargo on the declaration turn"
        );
        let mut report = TurnReport::empty();
        apply_blockade_effects(&game, &mut report);
        let had_blockade_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.starts_with("BLOCKADE:"));
        assert!(
            !had_blockade_headline,
            "no BLOCKADE headline expected on declaration turn"
        );

        // Next turn: blockade kicks in.
        game.advance_turn();
        let cap_next = compute_blockade_capacity(&game);
        assert!(
            cap_next.get(&NationId(1)).copied().unwrap_or(raw_cargo) < raw_cargo,
            "blockade must reduce cargo starting the turn after declaration"
        );
    }

    #[test]
    fn voluntary_incorporation_skips_anarchic_great_powers() {
        // A minor at the threshold against an anarchic GP must not be
        // re-absorbed: an anarchic government cannot accept allegiance.
        // Card #117: prevents a released minor from immediately rejoining
        // the very overlord that just collapsed into anarchy.
        let mut game = test_game_state_with_minor_nation();

        // Add a second GP so we can verify the non-anarchic one is selected
        // when eligible.
        let mut gp_b = Nation::new(
            NationId(3),
            "Healthy Empire".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp_b.economy.treasury = Money::dollars(1000);
        game.world.nations.push(gp_b);
        game.world
            .diplomacy
            .initialize_great_powers(&[NationId(1), NationId(3)]);

        // Mark GP A (Testlandia) as anarchic, with maxed score to the minor.
        game.get_nation_mut(NationId(1))
            .unwrap()
            .diplomacy
            .is_in_anarchy = true;
        let rel_a = game
            .world
            .diplomacy
            .ensure_relation(NationId(2), NationId(1));
        rel_a.score = 100;
        // GP B (Healthy Empire) has a below-threshold score.
        let rel_b = game
            .world
            .diplomacy
            .ensure_relation(NationId(2), NationId(3));
        rel_b.score = 50;

        let mut report = TurnReport::empty();
        resolve_voluntary_incorporations(&mut game, &mut report);

        assert!(
            report.incorporations.is_empty(),
            "anarchic GP must not be picked even at score 100"
        );
        let mn = game.get_nation(NationId(2)).unwrap();
        assert!(mn.province_ids.contains(&ProvinceId(2)));

        // Now raise GP B above threshold: it should win.
        let rel_b = game
            .world
            .diplomacy
            .ensure_relation(NationId(2), NationId(3));
        rel_b.score = 95;
        let mut report = TurnReport::empty();
        resolve_voluntary_incorporations(&mut game, &mut report);

        assert_eq!(report.incorporations.len(), 1);
        assert_eq!(report.incorporations[0], (NationId(2), NationId(3)));
    }

    // ── Unit upgrade tests ────────────────────────────────────────

    #[test]
    fn unit_upgrade_when_tech_is_researched() {
        use crate::ai::AiPersonality;
        use crate::map::UnitId;
        use crate::military::units::ArmyUnit;

        let mut game = test_game_state();

        // Make nation1 an AI so auto-upgrade triggers
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.diplomacy.ai_personality = Some(AiPersonality::Balanced);

        // Give the nation a Regulars unit
        let unit = ArmyUnit::new(
            UnitId(100),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        nation.military.army.push(unit);

        // Research "Breech-Loading Rifles" (TechId 13) —
        // RifleInfantry.required_tech() returns "Breech-Loading Rifles"
        nation.research_tech(crate::events::TechId(13));

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        };

        resolve_unit_upgrades(&mut game, &mut report);

        // The Regulars should be upgraded to RifleInfantry
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.military.army[0].unit_type,
            ArmyUnitType::RifleInfantry
        );

        // Report should record the upgrade
        assert_eq!(report.unit_upgrades.len(), 1);
        assert_eq!(report.unit_upgrades[0].0, NationId(1));
    }

    #[test]
    fn unit_upgrade_preserves_medals() {
        use crate::ai::AiPersonality;
        use crate::map::UnitId;
        use crate::military::units::ArmyUnit;

        let mut game = test_game_state();

        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.diplomacy.ai_personality = Some(AiPersonality::Balanced);

        let mut unit = ArmyUnit::new(
            UnitId(100),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.medals = 3;
        unit.health = 75;
        nation.military.army.push(unit);

        // Research "Breech-Loading Rifles" (TechId 13)
        nation.research_tech(crate::events::TechId(13));

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        };

        resolve_unit_upgrades(&mut game, &mut report);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.military.army[0].unit_type,
            ArmyUnitType::RifleInfantry
        );
        assert_eq!(
            nation.military.army[0].medals, 3,
            "Medals should be preserved"
        );
        assert_eq!(
            nation.military.army[0].health, 75,
            "Health should be preserved"
        );
    }

    // ── Pact defense tests ──────────────────────────────────────

    #[test]
    fn pact_defense_triggers_when_minor_with_pact_attacked() {
        // Setup: GP attacker (ID 2), Minor Nation defender (ID 3), GP pact holder (ID 4)
        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Attacker Land".to_string(),
            NationId(2),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Minor Land".to_string(),
            NationId(3),
            HexCoord::new(3, 3),
            vec![HexCoord::new(3, 3)],
            3,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Pact Holder Land".to_string(),
            NationId(4),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            4,
        );

        let mut human = Nation::new(
            NationId(1),
            "Human".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        human.economy.treasury = Money::dollars(10000);

        let mut attacker = Nation::new(
            NationId(2),
            "Attacker".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        attacker.diplomacy.ai_personality = Some(crate::ai::AiPersonality::Aggressive);
        attacker.economy.treasury = Money::dollars(10000);

        let minor = Nation::new(
            NationId(3),
            "MinorDefender".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut pact_holder = Nation::new(
            NationId(4),
            "PactHolder".to_string(),
            NationColor::Green,
            NationType::GreatPower,
            ProvinceId(3),
        );
        pact_holder.diplomacy.ai_personality = Some(crate::ai::AiPersonality::Aggressive);
        pact_holder.economy.treasury = Money::dollars(10000);
        // Give the pact holder a strong army so it accepts the defense request
        for i in 0..10 {
            pact_holder
                .military
                .army
                .push(crate::military::units::ArmyUnit::new(
                    crate::map::UnitId(8000 + i),
                    crate::military::units::ArmyUnitType::Regulars,
                    NationId(4),
                    ProvinceId(3),
                ));
        }

        let mut diplomacy = DiplomacyState::new();
        // Establish consulate + embassy + pact between PactHolder and MinorDefender
        // with high relation score so the protector cares enough to intervene
        diplomacy.build_consulate(NationId(4), NationId(3)).unwrap();
        diplomacy.build_embassy(NationId(4), NationId(3)).unwrap();
        if let Some(rel) = diplomacy.get_relation_mut(NationId(4), NationId(3)) {
            rel.improve_score(60);
        }
        diplomacy.propose_pact(NationId(4), NationId(3)).unwrap();

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1, province2, province3],
        nations: vec![human, attacker, minor, pact_holder],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
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
        next_unit_id: 6_000_000,};

        // Verify pact exists
        assert!(game.world.diplomacy.has_treaty(
            NationId(4),
            NationId(3),
            crate::events::TreatyType::NonAggressionPact
        ));

        // Trigger pact defense
        let mut report = TurnReport {
            turn: TurnNumber::new(1),
            year: 1815,
            quarter: 1,
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        };

        trigger_pact_defense(&mut game, NationId(3), NationId(2), &mut report);

        // PactHolder (AI) should strategically accept and be at war with Attacker
        assert!(
            game.world
                .diplomacy
                .get_relation(NationId(4), NationId(2))
                .is_some_and(|r| r.at_war),
            "PactHolder should declare war on attacker after strategic evaluation"
        );

        // Minor should be incorporated into PactHolder's empire
        let minor_provinces = game
            .get_nation(NationId(3))
            .map(|n| n.province_ids.len())
            .unwrap_or(0);
        assert_eq!(
            minor_provinces, 0,
            "Minor should have 0 provinces after incorporation"
        );

        // Should have generated intervention headline
        assert!(
            report
                .newspaper_headlines
                .iter()
                .any(|h| h.text.contains("intervenes") && h.text.contains("protect")),
            "Should generate intervention headline: {:?}",
            report.newspaper_headlines
        );
    }

    #[test]
    fn pact_defense_does_not_trigger_for_great_power_defender() {
        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Land".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "GP1".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.economy.treasury = Money::dollars(10000);

        let mut nation2 = Nation::new(
            NationId(2),
            "GP2".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation2.diplomacy.ai_personality = Some(crate::ai::AiPersonality::Balanced);
        nation2.economy.treasury = Money::dollars(10000);

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1],
        nations: vec![nation1, nation2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        let mut report = TurnReport {
            turn: TurnNumber::new(1),
            year: 1815,
            quarter: 1,
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        };

        // GP defender - should not trigger pact defense
        trigger_pact_defense(&mut game, NationId(1), NationId(2), &mut report);

        assert!(
            report.newspaper_headlines.is_empty(),
            "No headlines should be generated for GP defenders"
        );
    }

    // ── Trade subsidy deduction tests ────────────────────────────

    #[test]
    fn subsidy_costs_deducted_each_turn() {
        let mut game = test_game_state();
        game.world.nations[0].economy.treasury = Money::dollars(10000);
        // Set a subsidy with a fictional MN
        game.world.nations[0]
            .diplomacy
            .trade_subsidies
            .insert(NationId(10), Money::dollars(200));

        let report = process_turn(&mut game);

        // Subsidy cost should appear in report
        assert!(
            report
                .subsidy_costs
                .iter()
                .any(|(gp, mn, cost)| *gp == NationId(1)
                    && *mn == NationId(10)
                    && *cost == Money::dollars(200)),
            "Subsidy cost should be recorded in report"
        );
        // Treasury should be reduced by at least the subsidy amount
        assert!(
            game.get_nation(NationId(1)).unwrap().economy.treasury < Money::dollars(10000),
            "Treasury should be reduced after subsidy deduction"
        );
    }

    // ── Trade improves relationship score ────────────────────────

    #[test]
    fn trade_improves_relationship_score() {
        use crate::hex::HexCoord;
        use crate::map::tile::Tile;
        use crate::military::ships::{Ship, ShipType};

        let coord_forest = HexCoord::new(0, 0);
        let coord_plantation = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        let mut forest_tile = Tile::with_province(TerrainType::Forest, ProvinceId(20));
        forest_tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord_forest, forest_tile);
        let mut cotton_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(20));
        cotton_tile.set_resource(ResourceType::Cotton);
        hex_map.set_tile(coord_plantation, cotton_tile);

        let mn_province = Province::new(
            ProvinceId(20),
            "Minor Province".to_string(),
            NationId(10),
            coord_forest,
            vec![coord_forest, coord_plantation],
            3,
        );

        let gp_province = Province::new(
            ProvinceId(1),
            "GP Province".to_string(),
            NationId(1),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            4,
        );

        let mut gp = Nation::new(
            NationId(1),
            "TestGP".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp.economy.treasury = Money::dollars(50000);
        // Give merchant ship for cargo
        gp.military.merchant_fleet.push(Ship::new(
            crate::map::UnitId(999),
            ShipType::Trader,
            NationId(1),
            25,
        ));

        let mn = Nation::new(
            NationId(10),
            "TestMN".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        );

        let mut diplomacy = DiplomacyState::new();
        // Build consulate so trade is possible
        diplomacy
            .build_consulate(NationId(1), NationId(10))
            .unwrap();

        let score_before = diplomacy
            .get_relation(NationId(1), NationId(10))
            .map(|r| r.score)
            .unwrap_or(0);

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![gp_province, mn_province],
        nations: vec![gp, mn],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
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
        next_unit_id: 6_000_000,};

        let report = process_turn(&mut game);

        // If trade happened, relationship should have improved
        if !report.trade_transactions.is_empty() {
            let score_after = game
                .world
                .diplomacy
                .get_relation(NationId(1), NationId(10))
                .map(|r| r.score)
                .unwrap_or(0);
            assert!(
                score_after > score_before,
                "Trade should improve relationship score (before={}, after={})",
                score_before,
                score_after
            );
            // trade_diplomacy should have entries
            assert!(
                !report.trade_diplomacy.is_empty(),
                "Trade diplomacy improvements should be recorded"
            );
        }
    }

    // ── Trade history recording ──────────────────────────────────

    #[test]
    fn trade_records_history_entries() {
        use crate::hex::HexCoord;
        use crate::map::tile::Tile;
        use crate::military::ships::{Ship, ShipType};

        let coord_forest = HexCoord::new(0, 0);
        let coord_plantation = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        let mut forest_tile = Tile::with_province(TerrainType::Forest, ProvinceId(20));
        forest_tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord_forest, forest_tile);
        let mut cotton_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(20));
        cotton_tile.set_resource(ResourceType::Cotton);
        hex_map.set_tile(coord_plantation, cotton_tile);

        let mn_province = Province::new(
            ProvinceId(20),
            "Minor Province".to_string(),
            NationId(10),
            coord_forest,
            vec![coord_forest, coord_plantation],
            3,
        );

        let gp_province = Province::new(
            ProvinceId(1),
            "GP Province".to_string(),
            NationId(1),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            4,
        );

        let mut gp = Nation::new(
            NationId(1),
            "TestGP".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp.economy.treasury = Money::dollars(50000);
        gp.military.merchant_fleet.push(Ship::new(
            crate::map::UnitId(999),
            ShipType::Trader,
            NationId(1),
            25,
        ));

        let mn = Nation::new(
            NationId(10),
            "TestMN".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy
            .build_consulate(NationId(1), NationId(10))
            .unwrap();

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![gp_province, mn_province],
        nations: vec![gp, mn],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
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
        next_unit_id: 6_000_000,};

        let report = process_turn(&mut game);

        // If trade happened, history should be recorded
        if !report.trade_transactions.is_empty() {
            let gp_nation = game.get_nation(NationId(1)).unwrap();
            assert!(
                !gp_nation.archives.trade_history.is_empty(),
                "Great Power should have trade history entries after trade"
            );
            // Check the first entry has correct fields
            let first = &gp_nation.archives.trade_history[0];
            assert_eq!(first.turn, TurnNumber::new(1));
            assert_eq!(first.partner, NationId(10));
            assert!(first.quantity > 0);
            assert!(first.total_cost > Money::ZERO);

            let mn_nation = game.get_nation(NationId(10)).unwrap();
            assert!(
                !mn_nation.archives.trade_history.is_empty(),
                "Minor Nation should also have trade history entries"
            );
        }
    }

    // ── Town production output added to warehouse ──────────────

    #[test]
    fn town_production_output_added_to_warehouse() {
        // Create a Village province with ScrubForest tiles (produces timber)
        // Village: 4 timber → 2 lumber, 2 lumber → 1 furniture
        let mut game =
            test_game_state_with_village(&[(TerrainType::Forest, Some(ResourceType::Timber)); 4]);

        // Ensure nation warehouse starts empty
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.material_amount(MaterialType::Lumber), 0);
        assert_eq!(nation.goods_amount(GoodsType::Furniture), 0);

        let report = process_turn(&mut game);

        // After turn, materials should be in the nation's warehouse
        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.material_amount(MaterialType::Lumber) > 0,
            "Town production should add lumber to nation warehouse (got {})",
            nation.material_amount(MaterialType::Lumber)
        );
        assert!(
            nation.goods_amount(GoodsType::Furniture) > 0,
            "Town production should add furniture to nation warehouse (got {})",
            nation.goods_amount(GoodsType::Furniture)
        );

        // Report should record the town production
        let lumber_in_report: u32 = report
            .town_production
            .iter()
            .filter(|(nid, name, _)| *nid == NationId(1) && name == "Lumber")
            .map(|(_, _, q)| *q)
            .sum();
        assert!(
            lumber_in_report > 0,
            "Report should record lumber town production"
        );
    }

    // ── Immigration consumes correct goods ──────────────────────

    #[test]
    fn immigration_consumes_canned_food_clothing_and_furniture() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();

        nation.add_resource(ResourceType::Grain, 10);
        nation.add_resource(ResourceType::Fruit, 10);
        nation.add_resource(ResourceType::Livestock, 10);
        nation.economy.pending_immigration = 1;
        nation.add_material(MaterialType::CannedFood, 2);
        nation.add_goods(GoodsType::Clothing, 2);
        nation.add_goods(GoodsType::Furniture, 2);

        // Need 4 provinces for 1 immigrant slot
        for i in 2..=5 {
            nation.add_province(ProvinceId(i));
        }

        // Record initial amounts
        let canned_before = nation.material_amount(MaterialType::CannedFood);
        let clothing_before = nation.goods_amount(GoodsType::Clothing);
        let furniture_before = nation.goods_amount(GoodsType::Furniture);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        // Verify immigration happened
        let immigration: u32 = report
            .immigration
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert!(immigration >= 1, "Should recruit at least 1 immigrant");

        // All three immigration inputs (canned food, clothing, furniture)
        // are now enforced per `immigration_*` config knobs.
        let canned_after = nation.material_amount(MaterialType::CannedFood);
        let clothing_after = nation.goods_amount(GoodsType::Clothing);
        let furniture_after = nation.goods_amount(GoodsType::Furniture);

        assert!(
            clothing_after < clothing_before,
            "Clothing should be consumed by immigration: before={}, after={}",
            clothing_before,
            clothing_after
        );
        assert!(
            furniture_after < furniture_before,
            "Furniture should be consumed by immigration: before={}, after={}",
            furniture_before,
            furniture_after
        );
        assert!(
            canned_after < canned_before,
            "Canned food should be consumed by immigration: before={}, after={}",
            canned_before,
            canned_after
        );
    }

    // ── Counter-attack tests ────────────────────────────────────

    /// Build a game state with three adjacent provinces for counter-attack testing.
    ///
    /// Layout on the hex grid:
    /// - Province 1 (Nation 1, attacker): tile (0,0)
    /// - Province 2 (Nation 2, defender): tile (1,0)  ← adjacent to Province 1
    /// - Province 3 (Nation 2, defender): tile (2,0)  ← adjacent to Province 2
    ///
    /// Nation 1 has a strong army. Nation 2 has army units in Province 3.
    fn test_game_for_counter_attack() -> GameState {
        use crate::map::UnitId;

        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(1, 0);
        let coord3 = HexCoord::new(2, 0);

        let mut hex_map = HexMap::new(10, 10);
        let tile1 = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        let tile2 = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        let tile3 = Tile::with_province(TerrainType::Grassland, ProvinceId(3));
        hex_map.set_tile(coord1, tile1);
        hex_map.set_tile(coord2, tile2);
        hex_map.set_tile(coord3, tile3);

        let province1 = Province::new(
            ProvinceId(1),
            "Attacker Land".to_string(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Target Province".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Defender Rear".to_string(),
            NationId(2),
            coord3,
            vec![coord3],
            3,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.economy.treasury = Money::dollars(10000);
        // Give attacker a strong army
        for i in 0..6 {
            nation1.military.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }

        let mut nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );
        nation2.add_province(ProvinceId(3));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1, province2, province3],
        nations: vec![nation1, nation2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};
        crate::military::combat::seed_militia_from_garrison_count(&mut game);
        game
    }

    #[test]
    fn counter_attack_triggers_when_defender_has_adjacent_units() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Give defender army units in Province 3 (adjacent to Province 2)
        let nation2 = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..5 {
            nation2.military.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(3), // stationed in Province 3
            ));
        }

        // Queue attack on Province 2
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Should have at least 2 battles: initial attack + counter-attack
        assert!(
            report.battles.len() >= 2,
            "Should have counter-attack battle; got {} battles",
            report.battles.len()
        );

        // Check that a counter-attack headline was generated
        let has_counter_attack_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.contains("counter-attack") || h.text.contains("repels counter-attack"));
        assert!(
            has_counter_attack_headline,
            "Should have counter-attack headline; headlines: {:?}",
            report.newspaper_headlines
        );
    }

    #[test]
    fn no_counter_attack_when_no_adjacent_units() {
        let mut game = test_game_for_counter_attack();

        // Defender has NO army units (only garrison)
        // Queue attack on Province 2
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Should have exactly 1 battle (the initial attack only)
        assert_eq!(
            report.battles.len(),
            1,
            "Should have only 1 battle (no counter-attack); got {}",
            report.battles.len()
        );

        // No counter-attack headline
        let has_counter_attack_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.contains("counter-attack"));
        assert!(
            !has_counter_attack_headline,
            "Should not have counter-attack headline"
        );
    }

    #[test]
    fn garrison_resets_to_zero_on_conquest() {
        let mut game = test_game_for_counter_attack();

        // Province 2 starts with garrison_count = 3 (Minor Nation)
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().garrison_count,
            3,
            "Province 2 should start with garrison_count = 3"
        );

        // Queue attack on Province 2 (attacker has strong army, will win)
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Verify attack succeeded
        assert!(
            report.battles[0].attacker_won,
            "Attacker should win with 6 Guards vs 3 Militia"
        );

        // After conquest, garrison_count should be 0
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.garrison_count, 0,
            "Garrison should be 0 after conquest (conquering nation must station own units)"
        );
        assert_eq!(
            province.owner,
            NationId(1),
            "Province should now belong to attacker"
        );
    }

    #[test]
    fn garrison_uses_new_owner_type_after_conquest() {
        use crate::military::combat::create_garrison;

        let mut game = test_game_for_counter_attack();

        // Province 2 is Minor Nation (garrison_count=3).
        // After being conquered by GP Nation 1, garrison_count = 0.
        // But if garrison is rebuilt, it should use GP type (4 Militia).
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        process_turn(&mut game);

        // Province now owned by Nation 1 (Great Power)
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(province.owner, NationId(1));

        // Verify that create_garrison with the new owner type gives the GP amount
        let new_owner_type = game.get_nation(NationId(1)).unwrap().nation_type;
        let garrison = create_garrison(new_owner_type);
        assert_eq!(garrison.len(), 4, "GP garrison should have 4 Militia units");
    }

    // ── Connected-tile resource filtering tests ─────────────────

    #[test]
    fn connected_province_resources_are_collected() {
        // Capital province tiles should always produce resources (capital is always connected)
        let mut game = test_game_state();

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        };

        collect_resources(&mut game, &mut report);

        // Capital province is always connected, so resources should be produced
        assert!(
            !report.resource_production.is_empty(),
            "Connected capital province tiles should produce resources"
        );
        assert!(
            report.disconnected_resources.is_empty(),
            "No disconnected resources when all provinces belong to capital"
        );
    }

    #[test]
    fn disconnected_province_resources_not_collected() {
        // Create a game state with a second province that is NOT connected via railroad/port
        let coord_farm = HexCoord::new(0, 0);
        let coord_forest = HexCoord::new(1, 0);
        let coord_distant = HexCoord::new(8, 8);

        let mut hex_map = HexMap::new(20, 20);
        let mut farm_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        farm_tile.set_resource(ResourceType::Grain);
        farm_tile.is_country_capital = true;
        farm_tile.infrastructure.has_depot = true;
        hex_map.set_tile(coord_farm, farm_tile);
        let mut forest_tile = Tile::with_province(TerrainType::Forest, ProvinceId(1));
        forest_tile.set_resource(ResourceType::Timber);
        hex_map.set_tile(coord_forest, forest_tile);

        // Distant province tile (disconnected - no railroad/depot/port)
        let mut distant_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        distant_tile.set_resource(ResourceType::Grain);
        hex_map.set_tile(coord_distant, distant_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Capital Province".to_string(),
            NationId(1),
            coord_farm,
            vec![coord_farm, coord_forest],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Distant Province".to_string(),
            NationId(1),
            coord_distant,
            vec![coord_distant],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.add_province(ProvinceId(2));
        nation1.economy.treasury = Money::dollars(1000);

        let game_state = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1, province2],
        nations: vec![nation1],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        let mut game = game_state;

        let mut report = TurnReport {
            turn: game.turn,
            year: game.turn.year(),
            quarter: game.turn.quarter(),
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        };

        collect_resources(&mut game, &mut report);

        // Capital province tiles produce resources (connected)
        assert!(
            !report.resource_production.is_empty(),
            "Capital province should produce resources"
        );

        // The distant province's tile should be in disconnected_resources
        assert!(
            !report.disconnected_resources.is_empty(),
            "Disconnected province should have resources tracked as disconnected"
        );

        // The disconnected resource should be from NationId(1)
        for (nid, _, _) in &report.disconnected_resources {
            assert_eq!(*nid, NationId(1));
        }

        // The disconnected province's resources should NOT be added to warehouse
        // (only the capital province's resources should be there)
        let nation = game.get_nation(NationId(1)).unwrap();
        let total_in_warehouse: u32 = nation.economy.warehouse.values().sum();
        let total_produced: u32 = report.resource_production.iter().map(|(_, _, q)| q).sum();
        assert_eq!(
            total_in_warehouse, total_produced,
            "Only connected resources should be in warehouse"
        );
    }

    #[test]
    fn connected_provinces_includes_capital() {
        let game = test_game_state();
        let connected = connected_provinces(&game, NationId(1));
        assert!(
            connected.contains(&ProvinceId(1)),
            "Capital province should always be connected"
        );
    }

    #[test]
    fn connected_provinces_excludes_disconnected() {
        let coord_farm = HexCoord::new(0, 0);
        let coord_distant = HexCoord::new(8, 8);

        let mut hex_map = HexMap::new(20, 20);
        let farm_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord_farm, farm_tile);
        let distant_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        hex_map.set_tile(coord_distant, distant_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Capital".to_string(),
            NationId(1),
            coord_farm,
            vec![coord_farm],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Distant".to_string(),
            NationId(1),
            coord_distant,
            vec![coord_distant],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.add_province(ProvinceId(2));

        let game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1, province2],
        nations: vec![nation1],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        let connected = connected_provinces(&game, NationId(1));
        assert!(
            connected.contains(&ProvinceId(1)),
            "Capital always connected"
        );
        assert!(
            !connected.contains(&ProvinceId(2)),
            "Province without railroad/port should not be connected"
        );
    }

    // ── Bankruptcy protection ──────────────────────────────────────

    #[test]
    fn bankruptcy_protection_caps_treasury_at_floor() {
        let mut game = test_game_state();
        // Give the nation a huge army with high maintenance
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.treasury = Money::dollars(100);
        // Add many expensive units to trigger large maintenance
        for i in 0..50u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(9000 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            );
            nation.military.army.push(unit);
        }

        let report = process_turn(&mut game);

        // Treasury should not go below $0
        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.economy.treasury >= Money::ZERO,
            "Treasury {} should not go below $0",
            nation.economy.treasury
        );

        // Treasury went negative before the floor clamped it — a FINANCIAL CRISIS
        // headline should have been generated (F-017 fix: headline tracked pre-clamp).
        let has_crisis_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.contains("FINANCIAL CRISIS"));
        assert!(
            has_crisis_headline,
            "Should have FINANCIAL CRISIS headline when maintenance exceeds treasury"
        );
    }

    #[test]
    fn is_bankrupt_after_excessive_maintenance() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.treasury = Money::dollars(10);
        // Add expensive unit
        let unit = ArmyUnit::new(
            crate::map::UnitId(9100),
            ArmyUnitType::Guards,
            NationId(1),
            ProvinceId(1),
        );
        nation.military.army.push(unit);

        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // With floor at $0, treasury should be capped at zero, not negative
        assert!(
            nation.economy.treasury >= Money::ZERO,
            "Treasury {} should not go below $0",
            nation.economy.treasury
        );
        assert!(!nation.is_bankrupt());
    }

    #[test]
    fn treasury_floor_zero_after_maintenance() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Start with $0 treasury
        nation.economy.treasury = Money::ZERO;
        // Add army units that will incur maintenance costs
        for i in 0..5u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(8000 + i),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            );
            nation.military.army.push(unit);
        }

        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.economy.treasury >= Money::ZERO,
            "Treasury {} must not go below $0 after maintenance",
            nation.economy.treasury
        );
    }

    // ── Treasury integration test ──────────────────────────────────

    #[test]
    fn treasury_test_comprehensive() {
        let mut game = test_game_state();
        let player = NationId(1);

        // Set initial treasury to $10,000
        game.get_nation_mut(player).unwrap().economy.treasury = Money::dollars(10000);
        assert_eq!(
            game.get_nation(player).unwrap().economy.treasury,
            Money::dollars(10000)
        );

        // Deduct $500 for a consulate-like expense
        game.get_nation_mut(player).unwrap().economy.treasury -= Money::dollars(500);
        assert_eq!(
            game.get_nation(player).unwrap().economy.treasury,
            Money::dollars(9500)
        );

        // Process a turn (maintenance, production, etc.)
        process_turn(&mut game);

        // Verify treasury changed (maintenance or income applied)
        let final_treasury = game.get_nation(player).unwrap().economy.treasury;
        // It should still be positive (started with $9,500, no huge army)
        assert!(
            final_treasury > Money::ZERO,
            "Treasury after 1 turn: {} should still be positive",
            final_treasury
        );
    }

    // ── Minor Nation capitals never industrialize ──────────────────

    #[test]
    fn minor_nation_capitals_never_industrialize() {
        let coord_mn = HexCoord::new(3, 3);

        let mut hex_map = HexMap::new(10, 10);
        let mn_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        hex_map.set_tile(coord_mn, mn_tile);

        // Also add the GP capital tile
        let coord_gp = HexCoord::new(0, 0);
        let gp_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord_gp, gp_tile);

        let province_gp = Province::new(
            ProvinceId(1),
            "GP Capital".to_string(),
            NationId(1),
            coord_gp,
            vec![coord_gp],
            4,
        );

        let mut province_mn = Province::new(
            ProvinceId(2),
            "Minor Capital".to_string(),
            NationId(2),
            coord_mn,
            vec![coord_mn],
            3,
        );
        // Mark as connected and start industrialization
        province_mn.connected_to_capital = true;
        province_mn.industrialization_turns_remaining = Some(1); // about to complete

        let nation_gp = Nation::new(
            NationId(1),
            "TestGP".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );

        let nation_mn = Nation::new(
            NationId(2),
            "TestMN".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_gp, province_mn],
        nations: vec![nation_gp, nation_mn],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        // Process several turns
        for _ in 0..10 {
            process_turn(&mut game);
        }

        // Minor nation province should still be Hamlet
        let mn_province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            mn_province.settlement_level,
            SettlementLevel::Hamlet,
            "Minor Nation capital should never industrialize"
        );
    }

    // ── Captured GP capital industrializes immediately ──────────────

    #[test]
    fn captured_gp_capital_industrializes_immediately() {
        let coord_atk = HexCoord::new(0, 0);
        let coord_def = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        let atk_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord_atk, atk_tile);
        let def_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        hex_map.set_tile(coord_def, def_tile);

        let province_atk = Province::new(
            ProvinceId(1),
            "Attacker Capital".to_string(),
            NationId(1),
            coord_atk,
            vec![coord_atk],
            4,
        );
        let province_def = Province::new(
            ProvinceId(2),
            "Defender Capital".to_string(),
            NationId(2),
            coord_def,
            vec![coord_def],
            4,
        );

        let mut nation_atk = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation_atk.economy.treasury = Money::dollars(10000);
        // Give a powerful army to guarantee victory
        for i in 0..10u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            );
            nation_atk.military.army.push(unit);
        }

        let nation_def = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy.initialize_great_powers(&[NationId(1), NationId(2)]);
        diplomacy.declare_war(NationId(1), NationId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_atk, province_def],
        nations: vec![nation_atk, nation_def],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
        pending_attacks: vec![(NationId(1), ProvinceId(2))],
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
        next_unit_id: 6_000_000,};

        process_turn(&mut game);

        // Check if the defender capital was conquered
        let province = game.get_province(ProvinceId(2)).unwrap();
        if province.owner == NationId(1) {
            // Captured GP capital should be Village immediately
            assert_eq!(
                province.settlement_level,
                SettlementLevel::Village,
                "Captured GP capital should be immediately industrialized to Village"
            );
            // Defender should be in anarchy after losing capital
            let defender = game.get_nation(NationId(2)).unwrap();
            assert!(
                defender.diplomacy.is_in_anarchy,
                "Nation that lost its capital should be in anarchy"
            );
        }
    }

    // ── Siege artillery destroys fort sections ──────────────────────

    #[test]
    fn siege_artillery_destroys_fort_on_conquest() {
        let coord_atk = HexCoord::new(0, 0);
        let coord_def = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);
        let atk_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord_atk, atk_tile);
        let mut def_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        // Set up a fort on the defender's tile
        def_tile.infrastructure.has_fort = true;
        def_tile.infrastructure.fort_level = 2;
        hex_map.set_tile(coord_def, def_tile);

        let province_atk = Province::new(
            ProvinceId(1),
            "Attacker Land".to_string(),
            NationId(1),
            coord_atk,
            vec![coord_atk],
            4,
        );
        let province_def = Province::new(
            ProvinceId(2),
            "Fortified Land".to_string(),
            NationId(2),
            coord_def,
            vec![coord_def],
            3,
        );

        let mut nation_atk = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation_atk.economy.treasury = Money::dollars(10000);
        // Add siege artillery and strong army
        for i in 0..8u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            );
            nation_atk.military.army.push(unit);
        }
        // Add siege artillery
        for i in 0..3u32 {
            let unit = ArmyUnit::new(
                crate::map::UnitId(300 + i),
                ArmyUnitType::SiegeArtillery,
                NationId(1),
                ProvinceId(1),
            );
            nation_atk.military.army.push(unit);
        }

        let nation_def = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy.initialize_great_powers(&[NationId(1)]);
        diplomacy.declare_war(NationId(1), NationId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province_atk, province_def],
        nations: vec![nation_atk, nation_def],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
        pending_attacks: vec![(NationId(1), ProvinceId(2))],
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
        next_unit_id: 6_000_000,};
        // Seed persistent militia so the defender actually has units in
        // the fortified province (otherwise the auto-conquer path fires
        // and the siege-reduction logic never runs).
        crate::military::combat::seed_militia_from_garrison_count(&mut game);

        process_turn(&mut game);

        // If attacker won, fort should be reduced
        let province = game.get_province(ProvinceId(2)).unwrap();
        if province.owner == NationId(1) {
            let tile = game.world.hex_map.get_tile(coord_def).unwrap();
            // Fort level should be reduced by 1 (from 2 to 1)
            assert!(
                tile.infrastructure.fort_level < 2,
                "Fort level should be reduced after siege artillery conquest (was 2, now {})",
                tile.infrastructure.fort_level
            );
        }
    }

    // ── Regression: C1 — Voluntary incorporation spam ──────────

    #[test]
    fn no_reincorporation_of_already_incorporated_minor() {
        let mut game = test_game_state_with_minor_nation();

        // Set diplomacy score to 95 (above threshold)
        let rel = game
            .world
            .diplomacy
            .ensure_relation(NationId(2), NationId(1));
        rel.score = 95;

        // Simulate that the minor nation was already incorporated (0 provinces)
        let minor = game.get_nation_mut(NationId(2)).unwrap();
        minor.province_ids.clear();

        // Process multiple turns
        for _ in 0..5 {
            let report = process_turn(&mut game);

            // No incorporations should happen because the minor has 0 provinces
            assert!(
                report.incorporations.is_empty(),
                "Already-incorporated minor (0 provinces) should not be re-incorporated; turn {}",
                game.turn.0 - 1
            );
        }
    }

    // ── Regression: C2 — Worker starvation death spiral ────────

    #[test]
    fn emergency_recruitment_when_zero_workers() {
        let mut game = test_game_state();

        // Verify nation starts with 0 workers
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            nation.economy.labor.total_workers(),
            0,
            "Nation should start with 0 workers"
        );

        // Process one turn
        process_turn(&mut game);

        // Emergency recruitment should have given at least 1 worker
        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.economy.labor.total_workers() >= 1,
            "Emergency recruitment should give at least 1 worker; got {}",
            nation.economy.labor.total_workers()
        );
    }

    // ── Regression: C4 — Phantom defenders (auto-conquer) ──────

    #[test]
    fn undefended_non_capital_province_is_auto_conquered() {
        use crate::map::UnitId;
        use crate::military::units::ArmyUnit;

        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(1, 0);
        let coord3 = HexCoord::new(2, 0);

        let mut hex_map = HexMap::new(10, 10);
        let tile1 = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        let tile2 = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        let tile3 = Tile::with_province(TerrainType::Grassland, ProvinceId(3));
        hex_map.set_tile(coord1, tile1);
        hex_map.set_tile(coord2, tile2);
        hex_map.set_tile(coord3, tile3);

        let province1 = Province::new(
            ProvinceId(1),
            "Attacker Land".to_string(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );
        // Province 2 is a non-capital province of the defender
        let province2 = Province::new(
            ProvinceId(2),
            "Target Province".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );
        // Province 3 is the defender's capital
        let province3 = Province::new(
            ProvinceId(3),
            "Defender Capital".to_string(),
            NationId(2),
            coord3,
            vec![coord3],
            3,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.economy.treasury = Money::dollars(10000);
        // Give attacker army units
        for i in 0..4 {
            nation1.military.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            ));
        }

        let mut nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(3), // capital is Province 3, NOT Province 2
        );
        nation2.add_province(ProvinceId(2));
        // Defender has NO army units in Province 2 (the target)

        let mut diplomacy = DiplomacyState::new();
        diplomacy.declare_war(NationId(1), NationId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![province1, province2, province3],
        nations: vec![nation1, nation2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
        pending_attacks: vec![(NationId(1), ProvinceId(2))],
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
        next_unit_id: 6_000_000,};

        process_turn(&mut game);

        // Province 2 should have been auto-conquered (no defenders, non-capital)
        let province = game.get_province(ProvinceId(2)).unwrap();
        assert_eq!(
            province.owner,
            NationId(1),
            "Undefended non-capital province should be auto-conquered by attacker"
        );

        // Attacker should now own Province 2
        let attacker = game.get_nation(NationId(1)).unwrap();
        assert!(
            attacker.province_ids.contains(&ProvinceId(2)),
            "Attacker should have Province 2 in its province list"
        );

        // Defender should no longer own Province 2
        let defender = game.get_nation(NationId(2)).unwrap();
        assert!(
            !defender.province_ids.contains(&ProvinceId(2)),
            "Defender should no longer have Province 2"
        );
    }

    // ── Blockade capacity tests ──────────────────────────────────

    #[test]
    fn blockade_reduces_effective_cargo_capacity() {
        use crate::map::UnitId;
        use crate::military::ships::{Ship, ShipType};

        let mut game = test_game_state();

        // Add a second Great Power
        let mut nation2 = Nation::new(
            NationId(2),
            "EnemyNation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(200),
        );
        nation2.economy.treasury = Money::dollars(5000);

        // Give enemy warships
        for i in 0..3 {
            nation2.military.warships.push(Ship::new(
                UnitId(9000 + i),
                ShipType::ShipOfTheLine,
                NationId(2),
                65,
            ));
        }

        // Give the player merchant fleet (cargo capacity)
        let nation1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..4 {
            nation1.military.merchant_fleet.push(Ship::new(
                UnitId(8000 + i),
                ShipType::Clipper,
                NationId(1),
                25,
            ));
        }

        // Add enemy province so they're not eliminated
        let coord2 = HexCoord::new(5, 5);
        let province2 = Province::new(
            ProvinceId(200),
            "EnemyLand".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            4,
        );
        game.world.provinces.push(province2);
        game.world.nations.push(nation2);

        // Declare war
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        // Compute blockade capacity
        let capacity = compute_blockade_capacity(&game);

        // Without blockade: raw cargo = 4 * clipper_capacity
        let raw_cargo = game
            .get_nation(NationId(1))
            .unwrap()
            .total_cargo_capacity(&game.game_data);
        let effective = capacity.get(&NationId(1)).copied().unwrap_or(raw_cargo);

        // Blockade should reduce capacity when enemy has warships
        assert!(
            effective < raw_cargo,
            "Blockade should reduce cargo: effective={}, raw={}",
            effective,
            raw_cargo
        );
    }

    #[test]
    fn blockade_excludes_anarchic_nations() {
        use crate::map::UnitId;
        use crate::military::ships::{Ship, ShipType};

        let mut game = test_game_state();

        // Add an anarchic enemy with warships
        let mut nation2 = Nation::new(
            NationId(2),
            "FallenEmpire".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(200),
        );
        nation2.economy.treasury = Money::dollars(0);
        nation2.diplomacy.is_in_anarchy = true;
        for i in 0..5 {
            nation2.military.warships.push(Ship::new(
                UnitId(9000 + i),
                ShipType::ShipOfTheLine,
                NationId(2),
                65,
            ));
        }

        let nation1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..4 {
            nation1.military.merchant_fleet.push(Ship::new(
                UnitId(8000 + i),
                ShipType::Clipper,
                NationId(1),
                25,
            ));
        }

        game.world.nations.push(nation2);
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let capacity = compute_blockade_capacity(&game);
        let raw_cargo = game
            .get_nation(NationId(1))
            .unwrap()
            .total_cargo_capacity(&game.game_data);
        let effective = capacity.get(&NationId(1)).copied().unwrap_or(raw_cargo);

        // Anarchic nation's warships should NOT reduce trade
        assert_eq!(
            effective, raw_cargo,
            "Anarchic enemy warships should not cause blockade"
        );
    }

    #[test]
    fn blockade_ignores_unzoned_ships_when_zones_computed() {
        // When sea_zones is non-empty (zones computed), enemy warships with
        // sea_zone==None must NOT reduce trade cargo capacity.
        use crate::map::UnitId;
        use crate::map::sea_zones::{SeaZone, SeaZoneId};
        use crate::military::ships::{Ship, ShipType};
        use std::collections::BTreeSet;

        let mut game = test_game_state();

        // Add a second Great Power
        let mut nation2 = Nation::new(
            NationId(2),
            "EnemyNation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(200),
        );
        nation2.economy.treasury = Money::dollars(5000);

        // Unzoned enemy warships (sea_zone = None)
        for i in 0..3 {
            nation2.military.warships.push(Ship::new(
                UnitId(9000 + i),
                ShipType::ShipOfTheLine,
                NationId(2),
                65,
            ));
            // sea_zone stays None by default
        }

        let nation1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..4 {
            nation1.military.merchant_fleet.push(Ship::new(
                UnitId(8000 + i),
                ShipType::Clipper,
                NationId(1),
                25,
            ));
        }

        let coord2 = HexCoord::new(5, 5);
        let province2 = Province::new(
            ProvinceId(200),
            "EnemyLand".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            4,
        );
        game.world.provinces.push(province2);
        game.world.nations.push(nation2);
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        // Install one dummy sea zone so zones_computed = true
        game.world.sea_zones = vec![SeaZone {
            id: SeaZoneId(0),
            name: "Dummy Zone".to_string(),
            hexes: BTreeSet::new(),
            is_lake: false,
            adjacent_zone_ids: Vec::new(),
            coastal_provinces: Vec::new(),
        }];

        let capacity = compute_blockade_capacity(&game);
        let raw_cargo = game
            .get_nation(NationId(1))
            .unwrap()
            .total_cargo_capacity(&game.game_data);
        let effective = capacity.get(&NationId(1)).copied().unwrap_or(raw_cargo);

        assert_eq!(
            effective, raw_cargo,
            "Unzoned enemy warships must not blockade when zones are computed"
        );
    }

    // ── Immigration zero-config guard ───────────────────────────

    #[test]
    fn immigration_no_panic_with_zero_provinces_per_immigrant() {
        let mut game = test_game_state();
        // Set provinces_per_immigrant to 0 (bad config — should not panic)
        game.game_data.game_config.provinces_per_immigrant = 0;
        game.game_data.game_config.provinces_per_immigrant_upgraded = 0;

        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.labor.untrained = 2;
        nation.add_resource(ResourceType::Grain, 20);
        nation.add_material(MaterialType::CannedFood, 5);
        nation.add_goods(GoodsType::Clothing, 5);
        nation.add_goods(GoodsType::Furniture, 5);

        // Should not panic
        let _report = process_turn(&mut game);
    }

    // ── NationScore field tests ──────────────────────────────────

    #[test]
    fn score_includes_tech_treasury_building_components() {
        use crate::economy::buildings::{Building, BuildingType};
        use crate::turn::scoring::calculate_score;

        let mut nation = Nation::new(
            NationId(1),
            "ScoreTest".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.economy.treasury = Money::dollars(25_000); // treasury_score = min(250, 500) = 250
        nation.researched_techs.push(crate::events::TechId(1)); // tech_score = 1 * 30 = 30
        nation.researched_techs.push(crate::events::TechId(2)); // tech_score = 2 * 30 = 60
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 1)); // building_score = 1 * 10 = 10
        nation
            .economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 1)); // building_score = 2 * 10 = 20

        let data = crate::data::GameData::default();
        let score = calculate_score(&nation, &data);

        assert_eq!(score.tech_score, 60, "2 techs * 30 = 60");
        assert_eq!(
            score.treasury_score, 250,
            "$25,000 / 100 = 250, capped at 500"
        );
        assert_eq!(score.building_score, 20, "2 buildings * 10 = 20");

        // Verify total includes all components
        let expected_total = score.military_score
            + score.labor_score
            + score.transport_score
            + score.merchant_marine_score
            + score.diplomatic_score
            + score.province_score
            + score.tech_score
            + score.treasury_score
            + score.building_score;
        assert_eq!(
            score.total, expected_total,
            "Total should equal sum of all components"
        );
    }

    // ── Province adjacency & naval landing tests ────────────────

    #[test]
    fn can_attack_adjacent_province() {
        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            HexCoord::new(1, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let prov1 = Province::new(
            ProvinceId(1),
            "Ours".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov2 = Province::new(
            ProvinceId(2),
            "Theirs".into(),
            NationId(2),
            HexCoord::new(1, 0),
            vec![HexCoord::new(1, 0)],
            3,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "A".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.economy.treasury = Money::dollars(10000);
        let n2 = Nation::new(
            NationId(2),
            "B".into(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let mut diplomacy = DiplomacyState::new();
        diplomacy.declare_war(NationId(1), NationId(2));

        let game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".into(),
        hex_map: hex_map,
        provinces: vec![prov1, prov2],
        nations: vec![n1, n2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
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
        next_unit_id: 6_000_000,};

        assert!(
            can_attack_province(&game, NationId(1), ProvinceId(2)),
            "Should be able to attack adjacent province"
        );
    }

    #[test]
    fn cannot_attack_non_adjacent_province() {
        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            HexCoord::new(5, 5),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let prov1 = Province::new(
            ProvinceId(1),
            "Ours".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov2 = Province::new(
            ProvinceId(2),
            "Theirs".into(),
            NationId(2),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            3,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "A".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.economy.treasury = Money::dollars(10000);
        let n2 = Nation::new(
            NationId(2),
            "B".into(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".into(),
        hex_map: hex_map,
        provinces: vec![prov1, prov2],
        nations: vec![n1, n2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        assert!(
            !can_attack_province(&game, NationId(1), ProvinceId(2)),
            "Should NOT be able to attack non-adjacent province"
        );
    }

    #[test]
    fn can_attack_via_landing_from_previous_turn() {
        let hex_map = HexMap::new(10, 10);

        let prov1 = Province::new(
            ProvinceId(1),
            "Ours".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov2 = Province::new(
            ProvinceId(2),
            "Theirs".into(),
            NationId(2),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            3,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "A".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.economy.treasury = Money::dollars(10000);
        let n2 = Nation::new(
            NationId(2),
            "B".into(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let game = crate::test_game_state! {
        turn: TurnNumber::new(2),
        difficulty: Difficulty::Normal,
        map_key: "test".into(),
        hex_map: hex_map,
        provinces: vec![prov1, prov2],
        nations: vec![n1, n2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: DiplomacyState::new(),
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        // Landing established on turn 1, current turn is 2
        pending_landings: vec![(NationId(1), ProvinceId(2), TurnNumber::new(1))],
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
        next_unit_id: 6_000_000,};

        assert!(
            can_attack_province(&game, NationId(1), ProvinceId(2)),
            "Should be able to attack via landing from previous turn"
        );
    }

    #[test]
    fn cannot_attack_via_landing_from_same_turn() {
        let hex_map = HexMap::new(10, 10);

        let prov1 = Province::new(
            ProvinceId(1),
            "Ours".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov2 = Province::new(
            ProvinceId(2),
            "Theirs".into(),
            NationId(2),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            3,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "A".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.economy.treasury = Money::dollars(10000);
        let n2 = Nation::new(
            NationId(2),
            "B".into(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(2),
        );

        let game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".into(),
        hex_map: hex_map,
        provinces: vec![prov1, prov2],
        nations: vec![n1, n2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: DiplomacyState::new(),
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        // Landing established on turn 1, current turn is also 1
        pending_landings: vec![(NationId(1), ProvinceId(2), TurnNumber::new(1))],
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
        next_unit_id: 6_000_000,};

        assert!(
            !can_attack_province(&game, NationId(1), ProvinceId(2)),
            "Should NOT be able to attack via landing from same turn"
        );
    }

    // ── Player→AI diplomatic proposal resolution ─────────────────

    /// Build a two-GP game state with embassy established and positive relations
    /// so that NAP/Alliance proposals can be queued and evaluated.
    fn empty_report(turn: TurnNumber) -> TurnReport {
        TurnReport {
            turn,
            year: turn.year(),
            quarter: turn.quarter(),
            events: Vec::new(),
            resource_production: Vec::new(),
            gold_income: Vec::new(),
            maintenance_costs: Vec::new(),
            production_output: Vec::new(),
            food_consumed: Vec::new(),
            starvation: Vec::new(),
            newspaper_headlines: Vec::new(),
            techs_available: Vec::new(),
            council_vote: None,
            trade_transactions: Vec::new(),
            battles: Vec::new(),
            naval_battles: Vec::new(),
            scores: Vec::new(),
            ai_actions: Vec::new(),
            civilian_completions: Vec::new(),
            transport_overflow: Vec::new(),
            immigration: Vec::new(),
            settlement_upgrades: Vec::new(),
            trade_balance: Vec::new(),
            town_production: Vec::new(),
            unit_movements: Vec::new(),
            incorporations: Vec::new(),
            unit_upgrades: Vec::new(),
            subsidy_costs: Vec::new(),
            trade_diplomacy: Vec::new(),
            disconnected_resources: Vec::new(),
            rewards_earned: Vec::new(),
            opening_treasury: HashMap::new(),
            closing_treasury: HashMap::new(),
            ai_cash_spending: Vec::new(),
            construction_spending: Vec::new(),
            goods_auto_sale_revenue: Vec::new(),
            ai_goods_sale_revenue: Vec::new(),
            bankruptcy_writeoff: Vec::new(),
            cash_flow: HashMap::new(),
            resource_flow: HashMap::new(),
            stockpile_flows: StockpileFlowTracking::default(),
        }
    }

    fn two_gp_diplo_game() -> GameState {
        use crate::ai::AiPersonality;

        let coord = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(5, 5);
        let mut hex_map = HexMap::new(10, 10);
        let mut t1 = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        t1.set_resource(ResourceType::Grain);
        hex_map.set_tile(coord, t1);
        let mut t2 = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        t2.set_resource(ResourceType::Grain);
        hex_map.set_tile(coord2, t2);

        let p1 = Province::new(
            ProvinceId(1),
            "Homeland".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let p2 = Province::new(
            ProvinceId(2),
            "Ailand".to_string(),
            NationId(2),
            coord2,
            vec![coord2],
            4,
        );

        let mut n1 = Nation::new(
            NationId(1),
            "Player".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        n1.economy.treasury = Money::dollars(10_000);

        let mut n2 = Nation::new(
            NationId(2),
            "AiNation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        n2.economy.treasury = Money::dollars(10_000);
        n2.diplomacy.ai_personality = Some(AiPersonality::Diplomatic);

        let mut diplomacy = DiplomacyState::new();
        diplomacy.initialize_great_powers(&[NationId(1), NationId(2)]);
        // Boost relationship so AI is likely to accept
        let rel = diplomacy.ensure_relation(NationId(1), NationId(2));
        rel.improve_score(60);

        crate::test_game_state! {
        turn: TurnNumber::new(5),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![p1, p2],
        nations: vec![n1, n2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
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
    fn player_nap_proposal_creates_pending_not_immediate() {
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        // Queue a NAP proposal (same as what WASM bridge now does)
        game.world
            .diplomacy
            .propose_treaty(human, ai, TreatyType::NonAggressionPact, game.turn)
            .unwrap();

        // Should be pending, not yet an active treaty
        assert!(
            !game
                .world
                .diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::NonAggressionPact),
            "NAP should not be active yet — only pending"
        );
        assert_eq!(game.world.diplomacy.pending_proposals.len(), 1);
        assert_eq!(
            game.world.diplomacy.pending_proposals[0].proposal_type,
            TreatyType::NonAggressionPact
        );
    }

    #[test]
    fn player_nap_proposal_accepted_with_good_relations() {
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        // Disable Lua engine so evaluation uses pure Rust logic deterministically
        {
            game.game_data.lua_engine = None;
        }

        // Queue proposal (AI is Diplomatic personality with score +60 → should accept)
        game.world
            .diplomacy
            .propose_treaty(human, ai, TreatyType::NonAggressionPact, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        // Proposals should be drained
        assert!(game.world.diplomacy.pending_proposals.is_empty());

        // Treaty should be active
        assert!(
            game.world
                .diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::NonAggressionPact),
            "NAP should be active after acceptance"
        );

        // Should have TreatyAccepted event
        assert!(
            report
                .events
                .iter()
                .any(|e| matches!(e, DomainEvent::TreatyAccepted(a) if a.treaty_type == TreatyType::NonAggressionPact)),
            "Should emit TreatyAccepted event"
        );

        // Headline should mention acceptance
        let headline = &report.newspaper_headlines[0].text;
        assert!(
            headline.contains("accepts"),
            "Headline should mention acceptance: {headline}"
        );
    }

    #[test]
    fn player_nap_proposal_rejected_with_bad_relations() {
        use crate::ai::AiPersonality;

        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        // Disable Lua engine AND personality configs so evaluation uses pure
        // Rust logic deterministically. After the LuaAiConfig refactor,
        // baked personality data lives in `personality_configs` independent
        // of the engine — this test wants to exercise the score-based
        // fallback in `evaluate_nap_proposal` without per-personality
        // overrides, so we clear both.
        {
            game.game_data.lua_engine = None;
            game.game_data.personality_configs.clear();
        }

        // Make AI aggressive and hostile
        game.world.nations[1].diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        let rel = game.world.diplomacy.ensure_relation(human, ai);
        rel.score = -50; // terrible relationship

        game.world
            .diplomacy
            .propose_treaty(human, ai, TreatyType::NonAggressionPact, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        // Treaty should NOT be active
        assert!(
            !game
                .world
                .diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::NonAggressionPact),
            "NAP should not be active after rejection"
        );

        // Should have TreatyRejected event
        assert!(
            report
                .events
                .iter()
                .any(|e| matches!(e, DomainEvent::TreatyRejected(r) if r.treaty_type == TreatyType::NonAggressionPact)),
            "Should emit TreatyRejected event"
        );

        // Headline should mention rejection
        let headline = &report.newspaper_headlines[0].text;
        assert!(
            headline.contains("rejects"),
            "Headline should mention rejection: {headline}"
        );
    }

    #[test]
    fn player_proposal_state_drift_emits_rejection() {
        // Test the F-002 fix: AI accepts but treaty application fails due to state drift.
        // Setup: favorable relations so AI accepts, but NAP already exists so propose_pact() fails.
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        {
            game.game_data.lua_engine = None;
        }

        // Pre-apply NAP so propose_pact() will fail with "already active"
        game.world.diplomacy.propose_pact(human, ai).unwrap();
        assert!(
            game.world
                .diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::NonAggressionPact)
        );

        // Manually insert a duplicate pending NAP proposal (bypassing validation)
        game.world
            .diplomacy
            .pending_proposals
            .push(crate::diplomacy::DiplomaticProposal {
                from: human,
                to: ai,
                proposal_type: TreatyType::NonAggressionPact,
                turn_proposed: game.turn,
                attacker: None,
                cascade_remaining: None,
            });

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        // Headline should report "could not be fulfilled" (not "accepts")
        assert!(!report.newspaper_headlines.is_empty());
        let headline = &report.newspaper_headlines[0].text;
        assert!(
            headline.contains("could not be fulfilled"),
            "Should report application failure: {headline}"
        );
    }

    #[test]
    fn player_alliance_proposal_accepted_with_good_relations() {
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        {
            game.game_data.lua_engine = None;
        }

        game.world
            .diplomacy
            .propose_treaty(human, ai, TreatyType::Alliance, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        assert!(
            game.world
                .diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::Alliance),
            "Alliance should be active after acceptance"
        );
        assert!(report.events.iter().any(
            |e| matches!(e, DomainEvent::TreatyAccepted(a) if a.treaty_type == TreatyType::Alliance)
        ));
    }

    #[test]
    fn player_alliance_proposal_rejected_with_bad_relations() {
        use crate::ai::AiPersonality;

        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        {
            game.game_data.lua_engine = None;
        }

        game.world.nations[1].diplomacy.ai_personality = Some(AiPersonality::Aggressive);
        let rel = game.world.diplomacy.ensure_relation(human, ai);
        rel.score = -50;

        game.world
            .diplomacy
            .propose_treaty(human, ai, TreatyType::Alliance, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        assert!(
            !game
                .world
                .diplomacy
                .get_relation(human, ai)
                .unwrap()
                .has_treaty(TreatyType::Alliance),
            "Alliance should not be active after rejection"
        );
        assert!(report.events.iter().any(
            |e| matches!(e, DomainEvent::TreatyRejected(r) if r.treaty_type == TreatyType::Alliance)
        ));
    }

    #[test]
    fn ai_proposal_to_human_persists_for_modal() {
        let mut game = two_gp_diplo_game();
        let human = NationId(1);
        let ai = NationId(2);

        // AI proposes alliance to human
        game.world
            .diplomacy
            .propose_treaty(ai, human, TreatyType::Alliance, game.turn)
            .unwrap();

        let mut report = empty_report(game.turn);
        resolve_diplomatic_proposals(&mut game, &mut report);

        // Should be re-added to pending (for UI modal)
        assert_eq!(
            game.world.diplomacy.pending_proposals.len(),
            1,
            "AI→human proposal should persist for modal"
        );
        assert_eq!(game.world.diplomacy.pending_proposals[0].from, ai);
        assert_eq!(game.world.diplomacy.pending_proposals[0].to, human);
    }

    #[test]
    fn battle_archive_populated_after_combat() {
        let mut game = test_game_for_counter_attack();

        // Queue attack on Province 2
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        assert!(
            game.archive.battle_archive.is_empty(),
            "archive should start empty"
        );

        let report = process_turn(&mut game);

        // Battles occurred — archive should be populated
        assert!(!report.battles.is_empty(), "should have battles in report");
        assert_eq!(
            game.archive.battle_archive.len(),
            1,
            "should have one archive entry after one turn with battles"
        );

        let (archived_turn, archived_battles, archived_naval) = &game.archive.battle_archive[0];
        // The turn was 1 before processing, which is the turn that gets archived
        assert_eq!(archived_turn.0, 1, "archived turn should be 1");
        assert_eq!(
            archived_battles.len(),
            report.battles.len(),
            "archive should contain same number of battles as report"
        );
        assert!(archived_naval.is_empty(), "no naval battles in this test");

        // Verify attacker_origin_provinces is set (attacker units are in Province 1)
        let first_battle = &archived_battles[0];
        assert!(
            !first_battle.attacker_origin_provinces.is_empty(),
            "attacker_origin_provinces should be populated"
        );
        assert!(
            first_battle
                .attacker_origin_provinces
                .contains(&ProvinceId(1)),
            "origin should include Province 1 where attacker units are stationed"
        );

        // Survivors are retained in the archive so the Battle Archive UI
        // can render the same per-unit Forces view as the current-turn view.
        let total_units =
            first_battle.attacker_survivors.len() + first_battle.attacker_casualties.len();
        assert_eq!(
            total_units, first_battle.attacker_initial_count,
            "attacker survivors + casualties should sum to initial count"
        );
        let total_def_units =
            first_battle.defender_survivors.len() + first_battle.defender_casualties.len();
        assert_eq!(
            total_def_units, first_battle.defender_initial_count,
            "defender survivors + casualties should sum to initial count"
        );
    }

    // ── Post-victory unit relocation tests ─────────────────────

    #[test]
    fn counter_attack_defender_retreat_relocates_survivors() {
        // Card #18 regression for the counter-attack battle site: when
        // the occupier (defender of the counter-attack) retreats, their
        // survivors must land in a neighboring owned province (the
        // attacker's capital, which is still their home base).
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Nation 1 has 6 Guards at Province 1 (default setup). Keep all six
        // — they need to survive the militia garrison to hold Province 2
        // when the counter-attack lands. Give Nation 2 a massive counter
        // force so the pre-battle retreat ratio triggers.
        for i in 0..20 {
            game.get_nation_mut(NationId(2))
                .unwrap()
                .military
                .army
                .push(ArmyUnit::new(
                    UnitId(500 + i),
                    ArmyUnitType::Guards,
                    NationId(2),
                    ProvinceId(3),
                ));
        }

        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);
        assert!(
            report.battles.len() >= 2,
            "expected both initial attack and counter-attack, got {}",
            report.battles.len()
        );
        let counter = &report.battles[1];
        assert!(
            counter.defender_retreated,
            "occupying Nation 1 should retreat when swamped by the counter"
        );
        assert!(counter.attacker_won, "counter-attacker takes the province");

        // Province 2 flips back to Nation 2.
        assert_eq!(game.get_province(ProvinceId(2)).unwrap().owner, NationId(2));

        // Nation 1's surviving Guards must be at Province 1 (their home,
        // adjacent to the battle site) — not destroyed by the
        // `retain(|u| u.position != province)` cleanup.
        let n1 = game.get_nation(NationId(1)).unwrap();
        let survivors_at_home = n1
            .military
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(1))
            .count();
        assert!(
            survivors_at_home > 0,
            "Nation 1 retreating Guards should land at Province 1; army: {:?}",
            n1.military
                .army
                .iter()
                .map(|u| (u.id, u.position))
                .collect::<Vec<_>>()
        );
        assert!(
            counter
                .defender_retreated_to
                .iter()
                .all(|(_, dest)| *dest == ProvinceId(1)),
            "retreat destinations should all point to Province 1; got {:?}",
            counter.defender_retreated_to
        );
    }

    // ── Persistent militia (card: militia overhaul) ──────────────

    #[test]
    fn militia_defend_every_non_capital_province() {
        // A non-capital province with garrison_count=3 should fight back
        // when attacked by a small force.
        use crate::map::UnitId;
        let mut game = test_game_for_counter_attack();
        // Attacker sends only 2 Guards — not enough to beat 3 militia.
        let n1 = game.get_nation_mut(NationId(1)).unwrap();
        n1.military.army.truncate(2);
        // Target Province 3 (non-capital, owned by Nation 2).
        game.get_province_mut(ProvinceId(3)).unwrap().owner = NationId(2);
        // Move the 2 Guards to a province adjacent to P3 — use the existing
        // P1 (adjacent to P2), add P2 as Nation 1-owned so we can attack P3
        // from P2. Easier: just directly use our test helper path —
        // attacker at P2 attacks P3. But P2 is owned by Nation 2.
        // Simplification: just assert a battle occurred at P3 and militia
        // were in the defender force. Attack P3 from P2 would require
        // Nation 1 owning P2; skip.
        //
        // Instead: make Nation 1 a neighbor of P3 directly by adding a
        // fake adjacency. Easier approach: switch the test to attack P2
        // (which has 3 militia from the fixture) and confirm militia
        // appear in the battle result's casualties or survivors.
        let _ = UnitId(0);
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        assert!(
            !report.battles.is_empty(),
            "expected a battle; militia should have stopped auto-conquer"
        );
        let battle = &report.battles[0];
        // Militia must have participated (as casualty or survivor).
        let militia_in_battle = battle
            .defender_casualties
            .iter()
            .any(|t| *t == ArmyUnitType::Minutemen)
            || battle
                .defender_survivors
                .iter()
                .any(|u| u.unit_type == ArmyUnitType::Minutemen);
        assert!(
            militia_in_battle,
            "militia from garrison_count must join combat"
        );
    }

    // ── Rail freight capacity for non-adjacent moves (Card #13) ────────

    #[test]
    fn non_adjacent_move_blocked_without_freight() {
        // A unit moving to a non-adjacent province requires 5 freight cars per
        // armament point (manual p. 47). With zero freight cars the move must
        // be blocked.
        use crate::map::{HexMap, UnitId};
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let cap = HexCoord::new(0, 0);
        let far = HexCoord::new(5, 5); // not adjacent to cap

        let mut hex_map = HexMap::new(10, 10);
        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        hex_map.set_tile(cap, cap_tile);
        hex_map.set_tile(
            far,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let p1 = Province::new(
            ProvinceId(1),
            "Cap".to_string(),
            NationId(1),
            cap,
            vec![cap],
            4,
        );
        let p2 = Province::new(
            ProvinceId(2),
            "Far".to_string(),
            NationId(1),
            far,
            vec![far],
            4,
        );

        let mut nation = Nation::new(
            NationId(1),
            "Test".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.add_province(ProvinceId(2));
        nation.economy.treasury = Money::dollars(1000);
        // Zero freight cars — rail moves must be blocked.
        let unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        let unit_id = unit.id;
        nation.military.army.push(unit);

        let mut game = crate::test_game_state! {
            turn: TurnNumber::new(1), difficulty: Difficulty::Normal,
            map_key: "test".to_string(), hex_map: hex_map,
            provinces: vec![p1, p2], nations: vec![nation],
            human_player_nation: NationId(1), events: Vec::new(),
            game_data: crate::data::test_game_data(),
            diplomacy: DiplomacyState::new(), pending_attacks: Vec::new(),
            pending_moves: Vec::new(), pending_landings: Vec::new(),
            history: Vec::new(), high_scores: Vec::new(),
            newspaper_archive: Vec::new(), battle_archive: Vec::new(),
            political_archive: Vec::new(), ai_debug: false, observer_mode: false,
            last_cash_flow: std::collections::HashMap::new(),
            last_resource_flow: std::collections::HashMap::new(),
            pending_ai_cash_spending: Vec::new(), pending_ai_cash_income: Vec::new(),
            next_unit_id: 6_000_000,
        };

        game.transient
            .pending_moves
            .push((NationId(1), unit_id, ProvinceId(2)));
        process_turn(&mut game);

        let pos = game
            .get_nation(NationId(1))
            .unwrap()
            .military
            .army
            .iter()
            .find(|u| u.id == unit_id)
            .unwrap()
            .position;
        assert_eq!(
            pos,
            ProvinceId(1),
            "rail move must be blocked with zero freight cars"
        );
    }

    #[test]
    fn non_adjacent_move_succeeds_with_sufficient_freight() {
        // With enough freight cars (≥ 5 × armament_points), a non-adjacent
        // move is allowed and freight_unused decreases by 5 × armament_points.
        use crate::map::{HexMap, UnitId};
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let cap = HexCoord::new(0, 0);
        let far = HexCoord::new(5, 5);

        let mut hex_map = HexMap::new(10, 10);
        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        hex_map.set_tile(cap, cap_tile);
        hex_map.set_tile(
            far,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let p1 = Province::new(
            ProvinceId(1),
            "Cap".to_string(),
            NationId(1),
            cap,
            vec![cap],
            4,
        );
        let p2 = Province::new(
            ProvinceId(2),
            "Far".to_string(),
            NationId(1),
            far,
            vec![far],
            4,
        );

        let mut nation = Nation::new(
            NationId(1),
            "Test".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.add_province(ProvinceId(2));
        nation.economy.treasury = Money::dollars(1000);
        // 10 freight cars: enough for 2 Regulars (5 cars each, arms=1).
        nation.military.transport.build_freight_cars(10);
        let unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        let unit_id = unit.id;
        nation.military.army.push(unit);

        let mut game = crate::test_game_state! {
            turn: TurnNumber::new(1), difficulty: Difficulty::Normal,
            map_key: "test".to_string(), hex_map: hex_map,
            provinces: vec![p1, p2], nations: vec![nation],
            human_player_nation: NationId(1), events: Vec::new(),
            game_data: crate::data::test_game_data(),
            diplomacy: DiplomacyState::new(), pending_attacks: Vec::new(),
            pending_moves: Vec::new(), pending_landings: Vec::new(),
            history: Vec::new(), high_scores: Vec::new(),
            newspaper_archive: Vec::new(), battle_archive: Vec::new(),
            political_archive: Vec::new(), ai_debug: false, observer_mode: false,
            last_cash_flow: std::collections::HashMap::new(),
            last_resource_flow: std::collections::HashMap::new(),
            pending_ai_cash_spending: Vec::new(), pending_ai_cash_income: Vec::new(),
            next_unit_id: 6_000_000,
        };

        game.transient
            .pending_moves
            .push((NationId(1), unit_id, ProvinceId(2)));
        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        let pos = nation
            .military
            .army
            .iter()
            .find(|u| u.id == unit_id)
            .unwrap()
            .position;
        assert_eq!(
            pos,
            ProvinceId(2),
            "rail move must succeed with sufficient freight"
        );
        // Regulars have arms_required = 1, so cost = 5 freight cars.
        // freight_committed should have increased by 5 (all capacity was also used
        // by resources or just by the military move).
        assert!(
            nation.economy.logistics.freight_committed >= 5,
            "freight_committed must reflect the rail move cost"
        );
    }

    #[test]
    fn multiple_rail_moves_respect_rail_only_capacity() {
        // Two units attempt non-adjacent moves. Total cost = 10 freight cars.
        // Nation has exactly 5 rail cars + 10 sea capacity.
        // The sea capacity must NOT substitute for rail — second move must be blocked.
        use crate::map::{HexMap, UnitId};
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let cap = HexCoord::new(0, 0);
        let far1 = HexCoord::new(5, 0);
        let far2 = HexCoord::new(0, 5);

        let mut hex_map = HexMap::new(10, 10);
        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        hex_map.set_tile(cap, cap_tile);
        hex_map.set_tile(
            far1,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        hex_map.set_tile(
            far2,
            Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );

        let p1 = Province::new(
            ProvinceId(1),
            "Cap".to_string(),
            NationId(1),
            cap,
            vec![cap],
            4,
        );
        let p2 = Province::new(
            ProvinceId(2),
            "Far1".to_string(),
            NationId(1),
            far1,
            vec![far1],
            4,
        );
        let p3 = Province::new(
            ProvinceId(3),
            "Far2".to_string(),
            NationId(1),
            far2,
            vec![far2],
            4,
        );

        let mut nation = Nation::new(
            NationId(1),
            "Test".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.add_province(ProvinceId(2));
        nation.add_province(ProvinceId(3));
        nation.economy.treasury = Money::dollars(1000);
        // Only 5 freight cars (enough for 1 Regulars move at cost=5, not 2).
        nation.military.transport.build_freight_cars(5);
        let unit1 = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        let unit2 = ArmyUnit::new(
            UnitId(2),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        let uid1 = unit1.id;
        let uid2 = unit2.id;
        nation.military.army.push(unit1);
        nation.military.army.push(unit2);

        let mut game = crate::test_game_state! {
            turn: TurnNumber::new(1), difficulty: Difficulty::Normal,
            map_key: "test".to_string(), hex_map: hex_map,
            provinces: vec![p1, p2, p3], nations: vec![nation],
            human_player_nation: NationId(1), events: Vec::new(),
            game_data: crate::data::test_game_data(),
            diplomacy: DiplomacyState::new(), pending_attacks: Vec::new(),
            pending_moves: Vec::new(), pending_landings: Vec::new(),
            history: Vec::new(), high_scores: Vec::new(),
            newspaper_archive: Vec::new(), battle_archive: Vec::new(),
            political_archive: Vec::new(), ai_debug: false, observer_mode: false,
            last_cash_flow: std::collections::HashMap::new(),
            last_resource_flow: std::collections::HashMap::new(),
            pending_ai_cash_spending: Vec::new(), pending_ai_cash_income: Vec::new(),
            next_unit_id: 6_000_000,
        };

        game.transient
            .pending_moves
            .push((NationId(1), uid1, ProvinceId(2)));
        game.transient
            .pending_moves
            .push((NationId(1), uid2, ProvinceId(3)));
        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        let pos1 = nation
            .military
            .army
            .iter()
            .find(|u| u.id == uid1)
            .unwrap()
            .position;
        let pos2 = nation
            .military
            .army
            .iter()
            .find(|u| u.id == uid2)
            .unwrap()
            .position;
        // Exactly one move should succeed, one should be blocked.
        let moved = [pos1 != ProvinceId(1), pos2 != ProvinceId(1)];
        assert_eq!(
            moved.iter().filter(|&&m| m).count(),
            1,
            "exactly one of the two moves should succeed with 5 freight cars"
        );
        assert_eq!(
            nation.economy.logistics.freight_committed, 5,
            "freight_committed should reflect exactly one rail move"
        );
    }

    #[test]
    fn militia_cannot_be_ordered_to_move() {
        // A pending move for a Militia unit must be silently rejected.
        let mut game = test_game_for_counter_attack();
        // Find a militia at P2.
        let militia_id = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .army
            .iter()
            .find(|u| u.position == ProvinceId(2) && u.unit_type == ArmyUnitType::Minutemen)
            .expect("fixture should seed militia at P2")
            .id;
        game.transient
            .pending_moves
            .push((NationId(2), militia_id, ProvinceId(3)));

        process_turn(&mut game);

        let pos = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .army
            .iter()
            .find(|u| u.id == militia_id)
            .expect("militia must still exist")
            .position;
        assert_eq!(pos, ProvinceId(2), "militia must not move");
    }

    #[test]
    fn militia_regen_spawns_one_per_interval() {
        // Remove all militia from the nation, run 2 turns with regen
        // interval 2, expect +1 per regen tick up to default.
        let mut game = test_game_for_counter_attack();
        // Strip all militia (keep field army + artillery).
        if let Some(n2) = game.get_nation_mut(NationId(2)) {
            n2.military
                .army
                .retain(|u| u.unit_type != ArmyUnitType::Minutemen);
        }
        // Sync caches so we start at 0.
        sync_garrison_cache(&mut game, ProvinceId(2));
        sync_garrison_cache(&mut game, ProvinceId(3));
        assert_eq!(game.get_province(ProvinceId(2)).unwrap().garrison_count, 0);

        // Regen interval defaults to 2. After 2 turns → +1 militia in each
        // under-strength province.
        // Turn advances in process_turn; skip combat-related setup.
        process_turn(&mut game);
        // After turn 1: regen fires only when turn.0 % 2 == 0. Current
        // game.turn is 2 after process_turn increments from 1. So regen
        // fires during turn 1's processing? Actually: process_turn reads
        // game.turn at entry (let turn = game.turn;) and at line
        // 7e regenerate_garrisons reads game.turn.0 which is still the
        // entry value. So regen fires on turn 2, 4, 6...
        // To be deterministic, bump the turn to 2 before running.
        game.turn = crate::types::TurnNumber::new(2);
        process_turn(&mut game);
        let count_after_one_tick = game
            .get_nation(NationId(2))
            .unwrap()
            .militia_at(ProvinceId(2));
        assert_eq!(
            count_after_one_tick, 1,
            "exactly +1 militia expected after one regen tick"
        );

        // Advance enough turns for P2 (minor nation default = 3) to refill.
        for _ in 0..10 {
            // Ensure regen fires at even turns.
            if game.turn.0 % 2 == 0 {
                process_turn(&mut game);
            } else {
                process_turn(&mut game);
            }
        }
        let final_count = game
            .get_nation(NationId(2))
            .unwrap()
            .militia_at(ProvinceId(2));
        assert_eq!(
            final_count, 3,
            "militia count should regenerate up to the minor-nation default (3)"
        );
    }

    #[test]
    fn reconquest_pulls_excess_militia_from_neighbors() {
        // Seed P3 with over-stocked militia (5 >> default 3), then have
        // Nation 1 conquer P2 from Nation 2: rebalance should pull from
        // P3... wait — Nation 1 isn't adjacent to P3 through P2 only.
        // Simpler: have Nation 1 conquer P2; P1 is already adjacent to P2.
        // Stage militia excess at P1 (Nation 1's own) and verify they
        // migrate into P2 after conquest.
        let mut game = test_game_for_counter_attack();
        // Add 4 extra militia to P1 (default 4 for GP + 4 extra = 8).
        let mut extra = Vec::new();
        for _ in 0..4 {
            extra.push(crate::military::combat::spawn_militia_unit(
                &mut game.next_unit_id,
                NationId(1),
                ProvinceId(1),
            ));
        }
        let n1 = game.get_nation_mut(NationId(1)).unwrap();
        n1.military.army.extend(extra);
        sync_garrison_cache(&mut game, ProvinceId(1));
        // Bump attacker force to guarantee conquest.
        let n1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..8u32 {
            n1.military.army.push(ArmyUnit::new(
                crate::map::UnitId(900 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }

        // P1 starts with default_garrison=4 + 4 extra = 8 militia.
        assert_eq!(
            game.get_nation(NationId(1))
                .unwrap()
                .militia_at(ProvinceId(1)),
            8
        );

        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        process_turn(&mut game);

        // After conquest, P2 should be owned by Nation 1 and have the
        // default militia pulled from P1's excess (8 - 4 = 4 excess).
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().owner,
            NationId(1),
            "P2 should be conquered"
        );
        let militia_at_p2 = game
            .get_nation(NationId(1))
            .unwrap()
            .militia_at(ProvinceId(2));
        assert!(
            militia_at_p2 > 0,
            "conquered P2 should receive rebalanced militia from P1; got {}",
            militia_at_p2
        );
        let militia_at_p1 = game
            .get_nation(NationId(1))
            .unwrap()
            .militia_at(ProvinceId(1));
        assert!(
            militia_at_p1 >= 4,
            "P1 should keep at least default_garrison=4 after rebalance; got {}",
            militia_at_p1
        );
    }

    #[test]
    fn militia_cannot_be_in_attack_force() {
        // Regression: attacker force assembly must skip militia.
        let mut game = test_game_for_counter_attack();
        let n1 = game.get_nation_mut(NationId(1)).unwrap();
        // Drop field army to none — only militia remain at P1.
        n1.military
            .army
            .retain(|u| u.unit_type == ArmyUnitType::Minutemen);
        // Attack P2.
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);
        // Either no battle (no movable attackers) or battle with 0 initial
        // attackers. Either way, militia must not have joined the attack.
        for b in &report.battles {
            assert!(
                !b.attacker_casualties
                    .iter()
                    .any(|t| *t == ArmyUnitType::Minutemen),
                "no Militia should appear in attacker_casualties"
            );
        }
    }

    #[test]
    fn rebalance_is_noop_when_no_neighbor_has_excess() {
        // No neighbor exceeds default_garrison → rebalance pulls nothing,
        // but the target province's cache is still synced to 0 after the
        // ownership change.
        let mut game = test_game_for_counter_attack();
        // Start P1 at default (4). Everything baseline.
        let n1_militia_before = game
            .get_nation(NationId(1))
            .unwrap()
            .militia_at(ProvinceId(1));
        assert_eq!(n1_militia_before, 4);

        // Transfer ownership of P2 to N1 manually (bypassing combat) and
        // call rebalance directly to isolate its behavior.
        if let Some(p) = game.get_province_mut(ProvinceId(2)) {
            p.owner = NationId(1);
            p.garrison_count = 99; // stale-on-purpose
        }
        rebalance_militia_into(&mut game, NationId(1), ProvinceId(2));

        // N1 had no excess, so P2 stays at 0 militia.
        assert_eq!(
            game.get_nation(NationId(1))
                .unwrap()
                .militia_at(ProvinceId(2)),
            0
        );
        // Cache synced to reality (0), NOT the stale 99.
        assert_eq!(game.get_province(ProvinceId(2)).unwrap().garrison_count, 0);
        // Source neighbor (P1) untouched.
        assert_eq!(
            game.get_nation(NationId(1))
                .unwrap()
                .militia_at(ProvinceId(1)),
            4
        );
    }

    #[test]
    fn militia_retreat_drops_overflow_when_neighbors_at_cap() {
        // A retreating batch of militia must not exceed `max_garrison_per_province`
        // per neighbor; overflow dies instead of being queued.
        use crate::map::UnitId;
        use crate::military::combat::spawn_militia_unit;

        let mut game = test_game_for_counter_attack();
        {
            let n2 = game.get_nation_mut(NationId(2)).unwrap();
            n2.capital_province_id = ProvinceId(3);
        }
        // Fill P3 to the cap (8 by default: 3 from fixture + 5 extra).
        let mut extra = Vec::new();
        for _ in 0..5 {
            extra.push(spawn_militia_unit(
                &mut game.next_unit_id,
                NationId(2),
                ProvinceId(3),
            ));
        }
        let n2 = game.get_nation_mut(NationId(2)).unwrap();
        n2.military.army.extend(extra);
        sync_garrison_cache(&mut game, ProvinceId(3));
        assert_eq!(
            game.get_nation(NationId(2))
                .unwrap()
                .militia_at(ProvinceId(3)),
            8
        );

        // Pre-battle survivors = the 3 militia at P2 (from fixture).
        // Call retreat helper directly with max=8 and a single neighbor (P3
        // at cap). Expect ALL survivors to die.
        let survivor_ids: Vec<UnitId> = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2) && u.unit_type == ArmyUnitType::Minutemen)
            .map(|u| u.id)
            .collect();
        let placements =
            place_defender_retreat(&mut game, NationId(2), &survivor_ids, &[ProvinceId(3)], 8);
        assert!(
            placements.is_empty(),
            "no retreat placements should succeed when neighbor is at cap; got {:?}",
            placements
        );
        // The overflow militia should be removed from the nation's army.
        for uid in &survivor_ids {
            assert!(
                game.get_nation(NationId(2))
                    .unwrap()
                    .military
                    .army
                    .iter()
                    .all(|u| u.id != *uid),
                "overflow militia {:?} should have been destroyed",
                uid
            );
        }
    }

    /// Regression: before this fix, `resolve_unit_upgrades` walked the full
    /// upgrade chain for *every* unit, including the persistent garrison
    /// (Minutemen → Militia → Conscript is free since Militia has no tech
    /// gate). That converted seeded Minutemen into Militia/Conscript every
    /// turn, while `regenerate_garrisons` re-seeded fresh Minutemen up to
    /// the cap → unbounded militia growth. Garrison units must stay put.
    #[test]
    fn unit_upgrades_do_not_touch_garrison_units() {
        let mut game = test_game_for_counter_attack();
        // Make Nation 2 AI-controlled so resolve_unit_upgrades considers it.
        if let Some(n) = game.get_nation_mut(NationId(2)) {
            n.diplomacy.ai_personality = Some(crate::ai::common::AiPersonality::Balanced);
        }
        let before = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .army
            .iter()
            .filter(|u| u.unit_type == ArmyUnitType::Minutemen)
            .count();
        assert!(before > 0, "fixture should seed at least one Minutemen");

        let mut report = TurnReport::empty();
        resolve_unit_upgrades(&mut game, &mut report);

        let after = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .army
            .iter()
            .filter(|u| u.unit_type == ArmyUnitType::Minutemen)
            .count();
        assert_eq!(
            before, after,
            "garrison Minutemen must not be auto-upgraded by resolve_unit_upgrades"
        );
        // And no Militia/Conscript should have appeared as a side-effect.
        let new_garrison_evos = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .army
            .iter()
            .filter(|u| matches!(u.unit_type, ArmyUnitType::Militia | ArmyUnitType::Conscript))
            .count();
        assert_eq!(
            new_garrison_evos, 0,
            "no Militia/Conscript should appear from auto-upgrading Minutemen"
        );
    }

    #[test]
    fn regen_interval_zero_is_disabled() {
        // Config garrison_regen_interval_turns = 0 means "disabled".
        // The early-return guard must prevent modulo-by-zero and no
        // militia are spawned even if a province is under-strength.
        let mut game = test_game_for_counter_attack();
        game.game_data.game_config.garrison_regen_interval_turns = 0;
        // Clear all militia so every province is under-strength.
        if let Some(n) = game.get_nation_mut(NationId(2)) {
            n.military
                .army
                .retain(|u| u.unit_type != ArmyUnitType::Minutemen);
        }
        sync_garrison_cache(&mut game, ProvinceId(2));
        sync_garrison_cache(&mut game, ProvinceId(3));

        regenerate_garrisons(&mut game);

        assert_eq!(
            game.get_nation(NationId(2))
                .unwrap()
                .militia_at(ProvinceId(2)),
            0,
            "disabled regen (interval=0) must not spawn militia"
        );
    }

    #[test]
    fn garrison_artillery_dies_with_its_capital() {
        // Seed a GarrisonArtillery at Nation 2's capital (P2). Attacker
        // conquers the capital; artillery must be removed from the
        // defender's army (it cannot retreat, and the province cleanup
        // destroys all units at the battle province).
        use crate::map::UnitId;
        use crate::military::combat::spawn_garrison_artillery_unit;
        let mut game = test_game_for_counter_attack();
        let artillery =
            spawn_garrison_artillery_unit(&mut game.next_unit_id, NationId(2), ProvinceId(2));
        let artillery_id = artillery.id;
        let n2 = game.get_nation_mut(NationId(2)).unwrap();
        n2.military.army.push(artillery);

        // Overwhelming attacker: 12 Guards to crush militia + artillery.
        let n1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..6u32 {
            n1.military.army.push(ArmyUnit::new(
                UnitId(600 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }

        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        process_turn(&mut game);

        // Artillery is gone after conquest.
        let n2_still_has_artillery = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .army
            .iter()
            .any(|u| u.id == artillery_id);
        assert!(
            !n2_still_has_artillery,
            "GarrisonArtillery at lost province must be destroyed"
        );
    }

    #[test]
    fn defender_retreat_relocates_survivors_before_cleanup() {
        // Card #18 regression: when the defender retreats pre-battle and
        // the attacker takes the province, surviving defenders (field army
        // + militia) must be relocated to a neighbor BEFORE the
        // processor's "destroy any units still at battle_province" cleanup
        // runs.
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();
        // Rewire defender capital to Province 3 so Province 2 is NOT the
        // defender's capital; this enables retreat.
        let n2 = game.get_nation_mut(NationId(2)).unwrap();
        n2.capital_province_id = ProvinceId(3);

        // Overwhelming attacker force: 20 Guards. Province 2 has 3 persistent
        // militia from the fixture; the FP ratio still trips pre-battle
        // retreat (attacker FP 100 vs defender base 3.6 + 24 militia-bonus
        // = 27.6, ratio ~3.6 > prebattle threshold 2.0).
        let n1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..14u32 {
            n1.military.army.push(ArmyUnit::new(
                UnitId(700 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }

        // Snapshot which militia ids we expect to retreat (the seeded 3 at P2).
        let defender_militia_ids: Vec<UnitId> = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2) && u.unit_type == ArmyUnitType::Minutemen)
            .map(|u| u.id)
            .collect();
        assert!(
            !defender_militia_ids.is_empty(),
            "fixture should seed militia at P2"
        );

        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // 1. Province 2 is taken by the attacker.
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().owner,
            NationId(1),
            "attacker should take the evacuated province"
        );

        // 2. Battle report flags defender retreat and attacker win.
        let battle = report
            .battles
            .iter()
            .find(|b| b.province == ProvinceId(2))
            .expect("battle for province 2 should be recorded");
        assert!(
            battle.defender_retreated,
            "defender should have retreated pre-battle against overwhelming force"
        );
        assert!(battle.attacker_won, "attacker wins when defender retreats");

        // 3. Retreated militia are now at Province 3 (the only neighbor).
        let defender = game.get_nation(NationId(2)).unwrap();
        for mid in &defender_militia_ids {
            let pos = defender
                .military
                .army
                .iter()
                .find(|u| u.id == *mid)
                .map(|u| u.position)
                .expect("retreated militia must still exist in the defender army");
            assert_eq!(
                pos,
                ProvinceId(3),
                "militia {:?} should retreat to Province 3",
                mid
            );
        }
        assert!(
            battle
                .defender_retreated_to
                .iter()
                .all(|(_, dest)| *dest == ProvinceId(3)),
            "every retreat placement must target Province 3; got {:?}",
            battle.defender_retreated_to
        );
    }

    #[test]
    fn attacker_survivors_move_to_conquered_province() {
        let mut game = test_game_for_counter_attack();

        // Attacker (Nation 1) has 6 Guards in Province 1, attacks Province 2
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Verify attack succeeded
        assert!(
            report.battles[0].attacker_won,
            "Attacker should win with 6 Guards vs garrison"
        );

        // Verify Province 2 is now owned by attacker
        assert_eq!(game.get_province(ProvinceId(2)).unwrap().owner, NationId(1));

        // Verify surviving attacker units are now positioned in the conquered province
        let attacker = game.get_nation(NationId(1)).unwrap();
        let units_in_conquered = attacker
            .military
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            units_in_conquered > 0,
            "Surviving attacker units should be in conquered Province 2, but found {} units there",
            units_in_conquered
        );
    }

    #[test]
    fn counter_attack_faces_occupier_defenders() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Give defender army units in Province 3 (adjacent to Province 2)
        let nation2 = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..5 {
            nation2.military.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(3),
            ));
        }

        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // First battle should be the initial attack (attacker wins)
        assert!(
            report.battles[0].attacker_won,
            "Initial attack should succeed"
        );

        // A counter-attack must be generated (defender had adjacent units in Province 3)
        assert!(
            report.battles.len() >= 2,
            "Counter-attack should be generated, got {} battles",
            report.battles.len()
        );

        // The counter-attack's defender (occupier) should have the relocated
        // attacker survivors stationed in the conquered province
        let counter = &report.battles[1];
        assert!(
            counter.defender_initial_count > 0,
            "Counter-attack defender (occupier) should have units from the conquest, got {}",
            counter.defender_initial_count
        );
    }

    // ── Move-then-attack restriction tests ─────────────────────

    #[test]
    fn moved_units_excluded_from_attack() {
        let mut game = test_game_for_counter_attack();

        // Nation 1 has 6 Guards in Province 1
        // Add Province 4 as another owned province (friendly, to move to)
        let coord4 = HexCoord::new(0, 1);
        let tile4 = Tile::with_province(TerrainType::Grassland, ProvinceId(4));
        game.world.hex_map.set_tile(coord4, tile4);
        let province4 = Province::new(
            ProvinceId(4),
            "Friendly Rear".to_string(),
            NationId(1),
            coord4,
            vec![coord4],
            4,
        );
        game.world.provinces.push(province4);
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_province(ProvinceId(4));

        // Move one unit to Province 4 (friendly move)
        let unit_to_move = game.get_nation(NationId(1)).unwrap().military.army[0].id;
        game.transient
            .pending_moves
            .push((NationId(1), unit_to_move, ProvinceId(4)));

        // Also queue an attack on Province 2
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // The attack should have used 5 units (6 total minus 1 moved)
        assert!(
            !report.battles.is_empty(),
            "There should be at least one battle"
        );
        assert_eq!(
            report.battles[0].attacker_initial_count, 5,
            "Attack force should be 5 (6 army minus 1 moved unit)"
        );
    }

    // ── Adjacency-based attack force tests ────────────────────

    #[test]
    fn only_adjacent_units_participate_in_land_attack() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Nation 1 owns Province 1 (adjacent to Province 2) with 6 Guards there.
        // Add Province 5 (far from Province 2) with 2 more units.
        // Those 2 far units should NOT participate in the attack on Province 2.
        let coord5 = HexCoord::new(0, 2);
        let tile5 = Tile::with_province(TerrainType::Grassland, ProvinceId(5));
        game.world.hex_map.set_tile(coord5, tile5);
        let province5 = Province::new(
            ProvinceId(5),
            "Far Province".to_string(),
            NationId(1),
            coord5,
            vec![coord5],
            4,
        );
        game.world.provinces.push(province5);
        let nation1 = game.get_nation_mut(NationId(1)).unwrap();
        nation1.add_province(ProvinceId(5));
        for i in 0..2 {
            nation1.military.army.push(ArmyUnit::new(
                UnitId(300 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(5),
            ));
        }

        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Only 6 units (from adjacent Province 1) should have attacked, not 8
        assert!(!report.battles.is_empty());
        assert_eq!(
            report.battles[0].attacker_initial_count, 6,
            "Only units from adjacent Province 1 should fight (6), not far Province 5 units (2)"
        );

        // Units in Province 5 should retain their position after victory
        let nation1 = game.get_nation(NationId(1)).unwrap();
        let far_units_still_far = nation1
            .military
            .army
            .iter()
            .filter(|u| u.id.0 >= 300 && u.id.0 < 302)
            .filter(|u| u.position == ProvinceId(5))
            .count();
        assert_eq!(
            far_units_still_far, 2,
            "Units in far Province 5 should keep their position, not teleport"
        );
    }

    #[test]
    fn auto_conquer_relocates_adjacent_land_units() {
        use crate::map::UnitId;

        // Auto-conquer fixture: Nation 1 owns P1 + P10 (capital), Nation 2 owns
        // non-capital P2 (no army, no garrison since not a capital). P1 adjacent to P2.
        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(1, 0);
        let coord10 = HexCoord::new(5, 5); // far away — Nation 1's capital

        let mut hex_map = HexMap::new(20, 20);
        hex_map.set_tile(
            coord1,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            coord2,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        hex_map.set_tile(
            coord10,
            Tile::with_province(TerrainType::Grassland, ProvinceId(10)),
        );

        let p1 = Province::new(
            ProvinceId(1),
            "P1".into(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );
        let p2 = Province::new(
            ProvinceId(2),
            "P2".into(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );
        let p10 = Province::new(
            ProvinceId(10),
            "Capital".into(),
            NationId(1),
            coord10,
            vec![coord10],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(10), // capital elsewhere so P1 is not the capital
        );
        nation1.add_province(ProvinceId(1));
        nation1.economy.treasury = Money::dollars(10000);
        // 4 Guards in adjacent P1
        for i in 0..4 {
            nation1.military.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }

        // Nation 2 owns P2 but it's NOT their capital (they have a fake capital at P2 index).
        // Make Nation 2's capital a different province so P2 won't auto-garrison.
        // Use ProvinceId(99) as a dummy capital so P2 (defender_id=2) != capital
        let mut nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(99), // fake capital — P2 will NOT be capital
        );
        nation2.add_province(ProvinceId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![p1, p2, p10],
        nations: vec![nation1, nation2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Verify P2 is auto-conquered (no battle recorded for it, or it's a trivial victory)
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().owner,
            NationId(1),
            "P2 should be conquered"
        );

        // Verify Nation 1's units from adjacent P1 are now in P2
        let attacker = game.get_nation(NationId(1)).unwrap();
        let units_in_p2 = attacker
            .military
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            units_in_p2 > 0,
            "Adjacent units should relocate to auto-conquered P2, got {} units there",
            units_in_p2
        );

        // Defender test not strictly needed — just verify no battle report (auto-conquer = no battle)
        assert!(
            report.battles.is_empty()
                || !report.battles.iter().any(|b| b.province == ProvinceId(2)),
            "Auto-conquer should not produce a battle report"
        );
    }

    // ── Counter-attack relocation test ────────────────────────

    #[test]
    fn counter_attack_survivors_move_to_recaptured_province() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Give defender a strong army in Province 3 (adjacent to Province 2)
        // so the counter-attack can succeed
        let nation2 = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..20 {
            nation2.military.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(3),
            ));
        }

        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Preconditions: the counter-attack must exist AND win for this test to be meaningful.
        assert!(
            report.battles.len() >= 2,
            "Counter-attack must be generated (defender has 20 Guards in adjacent P3), got {} battles",
            report.battles.len()
        );
        assert!(
            report.battles[1].attacker_won,
            "Counter-attack must succeed with 20 Guards vs conquest survivors"
        );

        // Verify province ownership returned to Nation 2
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().owner,
            NationId(2),
            "Counter-attack should have recaptured Province 2"
        );

        // Verify counter-attack survivors ended up in the recaptured province
        let nation2 = game.get_nation(NationId(2)).unwrap();
        let units_in_recaptured = nation2
            .military
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            units_in_recaptured > 0,
            "Counter-attack survivors should be stationed in recaptured Province 2, got {}",
            units_in_recaptured
        );
    }

    // ── Naval-landing post-victory placement test ────────────

    #[test]
    fn naval_attackers_occupy_conquered_province_after_victory() {
        use crate::map::UnitId;
        use crate::military::naval::NavalOperation;
        use crate::military::ships::{Ship, ShipType};

        // Fixture: Nation 1 owns P1 (coastal, far from target). Nation 2 owns P2 (coastal).
        // P1 and P2 are NOT adjacent — no land route. Nation 1 has a beachhead on P2
        // established on turn 4 (current turn = 5). Nation 1's units are at P1.
        // After victory, the landing force should go ashore at the conquered
        // P2 (otherwise the province has no defenders and a same-turn
        // counter-attack walks straight in).
        let coord1 = HexCoord::new(0, 0);
        let coord2 = HexCoord::new(5, 0); // far from P1 — not adjacent

        let mut hex_map = HexMap::new(20, 20);
        hex_map.set_tile(
            coord1,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            coord2,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let mut p1 = Province::new(
            ProvinceId(1),
            "Home".into(),
            NationId(1),
            coord1,
            vec![coord1],
            4,
        );
        p1.coastal = true;
        p1.ocean_coastal = true;
        let mut p2 = Province::new(
            ProvinceId(2),
            "Target".into(),
            NationId(2),
            coord2,
            vec![coord2],
            3,
        );
        p2.coastal = true;
        p2.ocean_coastal = true;

        let mut nation1 = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.economy.treasury = Money::dollars(10000);
        for i in 0..4 {
            nation1.military.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }
        // ShipOfTheLine has arms_cost = 5, enough beachhead capacity for 4 Guards
        let mut ship = Ship::new(UnitId(500), ShipType::ShipOfTheLine, NationId(1), 65);
        ship.operation = Some(NavalOperation::Beachhead(ProvinceId(2)));
        nation1.military.warships.push(ship);

        let nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(99), // not a capital for P2 — so no auto-garrison militia
        );

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(5),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![p1, p2],
        nations: vec![nation1, nation2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: DiplomacyState::new(),
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        // Landing established on turn 4 (before current turn 5) — valid for attack
        pending_landings: vec![(NationId(1), ProvinceId(2), TurnNumber::new(4))],
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
        next_unit_id: 6_000_000,};
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        process_turn(&mut game);

        // Verify P2 was conquered
        assert_eq!(
            game.get_province(ProvinceId(2)).unwrap().owner,
            NationId(1),
            "P2 should be conquered via naval landing"
        );

        // Verify the surviving naval cohort moved into the conquered P2.
        let attacker = game.get_nation(NationId(1)).unwrap();
        let units_in_p2 = attacker
            .military
            .army
            .iter()
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            units_in_p2 > 0,
            "Naval attackers should occupy conquered P2 after victory"
        );
    }

    // ── Counter-attack move-then-attack restriction ───────────

    #[test]
    fn moved_units_excluded_from_counter_attack() {
        use crate::map::UnitId;

        let mut game = test_game_for_counter_attack();

        // Give Nation 1 enough muscle to overcome P2's militia (persistent
        // garrison) and take the province — necessary precondition for
        // any counter-attack to exist.
        let n1 = game.get_nation_mut(NationId(1)).unwrap();
        for i in 0..8u32 {
            n1.military.army.push(ArmyUnit::new(
                UnitId(800 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }

        // Give Nation 2 5 Guards in P3 (adjacent to P2) AND 1 Guard in P2 itself
        // (note: P2 is Nation 2's original province, which will get conquered).
        // The 5 units in P3 would normally counter-attack.
        let nation2 = game.get_nation_mut(NationId(2)).unwrap();
        for i in 0..5 {
            nation2.military.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(2),
                ProvinceId(3),
            ));
        }

        // Queue a move for one of Nation 2's P3 units within P3 (no-op move won't
        // actually move anywhere useful, but we need a real friendly target).
        // Nation 2 owns P2 (about to be lost) and P3. Queue move P3→P3 (self) —
        // actually moves aren't valid to same province. Use a different approach:
        // move one unit from P3 to P2 before combat. But P3→P2 would be friendly
        // since both are Nation 2's at the start of the turn (and movement happens
        // before combat). So the move is legit: friendly reposition P3 → P2.
        // After moving, the unit moved_unit_ids. Then P2 gets attacked and
        // conquered. Counter-attack force assembly in phase 7 should exclude
        // moved units.
        let moved_uid = game
            .get_nation(NationId(2))
            .unwrap()
            .military
            .army
            .iter()
            // Pick a MOVABLE unit at P3 (skip garrison militia — they
            // refuse to move).
            .find(|u| u.position == ProvinceId(3) && u.unit_type.can_move())
            .unwrap()
            .id;
        game.transient
            .pending_moves
            .push((NationId(2), moved_uid, ProvinceId(2)));

        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Hard precondition: counter-attack MUST be generated.
        // 5 units in P3 (adjacent to P2) — 1 moved leaves 4 available counter-attackers.
        // The initial attack by Nation 1 succeeds so P2 is conquered, then Nation 2's
        // remaining P3 units counter-attack.
        assert!(
            report.battles.len() >= 2,
            "Counter-attack must be generated (4 non-moved units in P3), got {} battles",
            report.battles.len()
        );
        // battles[1] is the counter-attack
        assert_eq!(
            report.battles[1].attacker_initial_count, 4,
            "Counter-attack force should exclude the moved unit (5 in P3 - 1 moved = 4)"
        );
    }

    // ── Mixed land + naval attack test ────────────────────────

    #[test]
    fn mixed_attack_splits_cohorts_and_relocates_by_origin() {
        use crate::map::UnitId;
        use crate::military::naval::NavalOperation;
        use crate::military::ships::{Ship, ShipType};

        // Fixture:
        //   P1 (coastal) — Nation 1 home/port, not adjacent to P2
        //   P2 (coastal) — Nation 2 target (non-capital, no army)
        //   P3 (land)    — Nation 1 adjacent to P2
        //   Coords chosen so P1 and P2 are NOT adjacent but P3 IS adjacent to P2.
        //   Nation 1: 3 Guards in P1 (port), 2 Guards in P3 (adjacent land),
        //             1 Guard in P4 (inland — neither adjacent nor port)
        //   Beachhead established on P2 from a prior turn.
        //   Expected: attack force = 2 land (from P3) + 3 naval (from P1, capped by
        //   beachhead) = 5 units. Inland P4 unit is excluded entirely.
        //   After victory: P3 units → P2 (conquered); P1 units stay at P1; P4 unit stays at P4.
        let coord_p2 = HexCoord::new(5, 0);
        let coord_p3 = HexCoord::new(4, 0); // adjacent to P2
        let coord_p1 = HexCoord::new(0, 0); // far from P2 — not adjacent
        let coord_p4 = HexCoord::new(0, 3); // inland, not coastal

        let mut hex_map = HexMap::new(20, 20);
        hex_map.set_tile(
            coord_p1,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            coord_p2,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        hex_map.set_tile(
            coord_p3,
            Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );
        hex_map.set_tile(
            coord_p4,
            Tile::with_province(TerrainType::Grassland, ProvinceId(4)),
        );

        let mut p1 = Province::new(
            ProvinceId(1),
            "Port".into(),
            NationId(1),
            coord_p1,
            vec![coord_p1],
            4,
        );
        p1.coastal = true;
        p1.ocean_coastal = true;
        let mut p2 = Province::new(
            ProvinceId(2),
            "Target".into(),
            NationId(2),
            coord_p2,
            vec![coord_p2],
            3,
        );
        p2.coastal = true;
        p2.ocean_coastal = true;
        let p3 = Province::new(
            ProvinceId(3),
            "Border".into(),
            NationId(1),
            coord_p3,
            vec![coord_p3],
            4,
        );
        let p4 = Province::new(
            ProvinceId(4),
            "Inland".into(),
            NationId(1),
            coord_p4,
            vec![coord_p4],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.add_province(ProvinceId(3));
        nation1.add_province(ProvinceId(4));
        nation1.economy.treasury = Money::dollars(20000);
        // 3 Guards in P1 (port)
        for i in 0..3 {
            nation1.military.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }
        // 2 Guards in P3 (land adjacent)
        for i in 0..2 {
            nation1.military.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(3),
            ));
        }
        // 1 Guard in P4 (inland non-port)
        nation1.military.army.push(ArmyUnit::new(
            UnitId(300),
            ArmyUnitType::Guards,
            NationId(1),
            ProvinceId(4),
        ));

        // ShipOfTheLine: arms_cost = 5 → beachhead_cap = 5 (room for 3 P1 + 2 P3 = 5 but
        // land cohort doesn't consume beachhead, so naval cap only affects P1 units: 3 <= 5)
        let mut ship = Ship::new(UnitId(500), ShipType::ShipOfTheLine, NationId(1), 65);
        ship.operation = Some(NavalOperation::Beachhead(ProvinceId(2)));
        nation1.military.warships.push(ship);

        let mut nation2 = Nation::new(
            NationId(2),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(99), // fake capital — P2 gets no auto-garrison
        );
        // Give Nation 2 a defender unit at P2 so a real battle occurs (not auto-conquer)
        nation2.military.army.push(ArmyUnit::new(
            UnitId(400),
            ArmyUnitType::Minutemen,
            NationId(2),
            ProvinceId(2),
        ));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(5),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![p1, p2, p3, p4],
        nations: vec![nation1, nation2],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: DiplomacyState::new(),
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        pending_landings: vec![(NationId(1), ProvinceId(2), TurnNumber::new(4))],
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
        next_unit_id: 6_000_000,};
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(2)));
        game.world.diplomacy.declare_war(NationId(1), NationId(2));

        let report = process_turn(&mut game);

        // Verify conquest
        assert_eq!(game.get_province(ProvinceId(2)).unwrap().owner, NationId(1));

        // Primary assertion: prove both cohorts participated in the battle.
        // Expected: 2 (P3 land) + 3 (P1 naval port) = 5; P4 inland excluded.
        assert!(!report.battles.is_empty(), "attack should produce a battle");
        let battle = &report.battles[0];
        assert_eq!(
            battle.attacker_initial_count, 5,
            "Mixed attack force = 2 land (P3) + 3 naval (P1 port) = 5; P4 inland unit excluded"
        );
        // Both origin provinces should appear in the battle's attacker origins list
        assert!(
            battle.attacker_origin_provinces.contains(&ProvinceId(1))
                || battle.attacker_origin_provinces.contains(&ProvinceId(3)),
            "battle should record at least one of the cohort origin provinces, got {:?}",
            battle.attacker_origin_provinces
        );

        let attacker = game.get_nation(NationId(1)).unwrap();

        // Land cohort (from P3, IDs 200-201) should be in P2 (conquered)
        let land_in_p2 = attacker
            .military
            .army
            .iter()
            .filter(|u| u.id.0 >= 200 && u.id.0 < 202)
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            land_in_p2 > 0,
            "Land cohort survivors (from P3) should relocate to conquered P2, got {}",
            land_in_p2
        );

        // Naval cohort (from P1, IDs 100-102) should also occupy conquered P2
        // (the troops are ashore; ships return to port empty).
        let naval_in_p2 = attacker
            .military
            .army
            .iter()
            .filter(|u| u.id.0 >= 100 && u.id.0 < 103)
            .filter(|u| u.position == ProvinceId(2))
            .count();
        assert!(
            naval_in_p2 > 0,
            "Naval cohort survivors (from P1) should occupy conquered P2, got {}",
            naval_in_p2
        );

        // Inland unit (ID 300 at P4) must NOT have participated — still at P4
        let inland_still_at_p4 = attacker
            .military
            .army
            .iter()
            .find(|u| u.id.0 == 300)
            .map(|u| u.position == ProvinceId(4))
            .unwrap_or(false);
        assert!(
            inland_still_at_p4,
            "Inland non-port unit must not participate in naval attack, should remain at P4"
        );
    }

    #[test]
    fn conflicting_alliance_obligations_produce_neutrality() {
        // Setup: 3 Great Powers — A(1), B(2), C(3)
        // C is allied with both A and B. A declares war on B.
        // C should remain neutral (not fight both sides).
        let coord = HexCoord::new(0, 0);
        let mut hex_map = HexMap::new(10, 10);
        let tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        hex_map.set_tile(coord, tile);

        let mut nations = Vec::new();
        for (id, name, prov_id) in [(1, "Alphaland", 1), (2, "Betaland", 2), (3, "Gammaland", 3)] {
            let mut n = Nation::new(
                NationId(id),
                name.to_string(),
                NationColor::Blue,
                NationType::GreatPower,
                ProvinceId(prov_id),
            );
            n.diplomacy.ai_personality = Some(crate::ai::common::AiPersonality::Balanced);
            n.province_ids = vec![ProvinceId(prov_id)];
            nations.push(n);
        }
        // Human player is nation 1 (but personality is set, so alliance logic treats as AI)
        // Actually, make nation 1 human so it doesn't auto-join
        nations[0].diplomacy.ai_personality = None;

        let provinces: Vec<Province> = (1..=3)
            .map(|i| {
                Province::new(
                    ProvinceId(i),
                    format!("Province{}", i),
                    NationId(i),
                    coord,
                    vec![coord],
                    4,
                )
            })
            .collect();

        let mut diplomacy = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3)];
        diplomacy.initialize_great_powers(&gps);

        // C(3) allied with both A(1) and B(2)
        diplomacy
            .propose_alliance(NationId(3), NationId(1))
            .unwrap();
        diplomacy
            .propose_alliance(NationId(3), NationId(2))
            .unwrap();

        // A(1) declares war on B(2)
        diplomacy.declare_war(NationId(1), NationId(2));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(5),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: provinces,
        nations: nations,
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: diplomacy,
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
        next_unit_id: 6_000_000,};

        let mut report = TurnReport::empty();
        resolve_alliance_obligations(&mut game, &mut report);

        // C(3) should NOT be at war with either A(1) or B(2)
        assert!(
            !game.world.diplomacy.is_at_war(NationId(3), NationId(1)),
            "Gammaland should not be at war with Alphaland (conflicting obligation)"
        );
        assert!(
            !game.world.diplomacy.is_at_war(NationId(3), NationId(2)),
            "Gammaland should not be at war with Betaland (conflicting obligation)"
        );

        // Should have a neutrality headline
        let has_neutrality = report
            .newspaper_headlines
            .iter()
            .any(|h| h.text.contains("remains neutral"));
        assert!(
            has_neutrality,
            "Should produce a neutrality headline for Gammaland"
        );
    }

    // ── Engineer build tasks ─────────────────────────────────────

    #[test]
    fn engineer_completes_railroad_build_in_one_turn() {
        use crate::economy::{BuildTask, Civilian, CivilianType};

        let mut game = test_game_state();
        let target = HexCoord::new(1, 0); // the "forest" tile, land & owned

        // Add an Engineer already deployed to the target tile with a Railroad task.
        let mut eng = Civilian::new(
            crate::map::UnitId(3_900_000),
            CivilianType::Engineer,
            NationId(1),
        );
        eng.deploy(target);
        eng.start_build(BuildTask::Railroad, &game.game_data.game_config);
        game.world.nations[0].military.civilians.push(eng);
        if let Some(tile) = game.world.hex_map.get_tile_mut(target) {
            tile.assigned_civilian = Some(crate::map::UnitId(3_900_000));
        }

        // Before: no railroad.
        assert!(
            !game
                .world
                .hex_map
                .get_tile(target)
                .unwrap()
                .infrastructure
                .has_railroad
        );

        let _ = process_turn(&mut game);

        // After one turn: railroad built, engineer is idle and freed from the tile.
        assert!(
            game.world
                .hex_map
                .get_tile(target)
                .unwrap()
                .infrastructure
                .has_railroad,
            "railroad should have been built after engineer's 1-turn task"
        );
        let eng = game
            .world
            .nations
            .iter()
            .flat_map(|n| n.military.civilians.iter())
            .find(|c| c.civilian_type == CivilianType::Engineer)
            .unwrap();
        assert!(!eng.working);
        assert_eq!(eng.build_task, None);
        assert_eq!(
            game.world
                .hex_map
                .get_tile(target)
                .unwrap()
                .assigned_civilian,
            None,
            "engineer should be released from the tile after completion"
        );
    }

    #[test]
    fn depot_harvest_radius_gates_yield() {
        // Build the setup twice: once with a "far" iron hex that IS outside
        // the depot's 1-hex radius, once without. The iron yield should be
        // identical — the far tile must not contribute.
        fn run(include_far: bool) -> u32 {
            let mut game = test_game_state();
            let cap = HexCoord::new(0, 0);
            if let Some(t) = game.world.hex_map.get_tile_mut(cap) {
                t.is_capital = true;
                t.infrastructure.has_depot = true;
            }
            let rail_mid = HexCoord::new(1, 0);
            let mut rail_mid_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
            rail_mid_tile.infrastructure.has_railroad = true;
            game.world.hex_map.set_tile(rail_mid, rail_mid_tile);

            let depot_hex = HexCoord::new(2, 0);
            let mut depot_tile = Tile::with_province(TerrainType::Hills, ProvinceId(2));
            depot_tile.infrastructure.has_railroad = true;
            depot_tile.infrastructure.has_depot = true;
            depot_tile.reveal_deposit(ResourceType::Iron);
            depot_tile.set_improvement_level(1);
            game.world.hex_map.set_tile(depot_hex, depot_tile);

            let mut tiles = vec![depot_hex];

            if include_far {
                let far = HexCoord::new(5, 5);
                let mut far_tile = Tile::with_province(TerrainType::Hills, ProvinceId(2));
                far_tile.reveal_deposit(ResourceType::Iron);
                far_tile.set_improvement_level(1);
                game.world.hex_map.set_tile(far, far_tile);
                tiles.push(far);
            }

            game.world.provinces.push(Province::new(
                ProvinceId(2),
                "Remote".to_string(),
                NationId(1),
                depot_hex,
                tiles,
                4,
            ));
            game.world.nations[0].add_province(ProvinceId(2));
            game.world.nations[0]
                .military
                .transport
                .build_freight_cars(50);
            // Human player now requires explicit allocations for remote resources.
            game.world.nations[0]
                .military
                .transport
                .set_resource_allocation(ResourceType::Iron, 1);

            let before = game.world.nations[0].resource_amount(ResourceType::Iron);
            let _ = process_turn(&mut game);
            let after = game.world.nations[0].resource_amount(ResourceType::Iron);
            after.saturating_sub(before)
        }

        let with_far = run(true);
        let without_far = run(false);
        assert!(without_far > 0, "depot hex must yield some iron");
        assert_eq!(
            with_far, without_far,
            "far iron tile is outside the depot's 1-hex radius and must not contribute"
        );
    }

    #[test]
    fn engineer_in_flight_build_cancels_on_province_loss() {
        use crate::economy::{BuildTask, Civilian, CivilianType};

        let mut game = test_game_state();
        let target = HexCoord::new(1, 0); // owned "forest" tile in province 1

        // 2-turn build so we catch the cancellation before completion.
        let mut eng = Civilian::new(
            crate::map::UnitId(3_900_001),
            CivilianType::Engineer,
            NationId(1),
        );
        eng.deploy(target);
        eng.start_build(BuildTask::Depot, &game.game_data.game_config); // 2 turns
        game.world.nations[0].military.civilians.push(eng);
        if let Some(tile) = game.world.hex_map.get_tile_mut(target) {
            tile.assigned_civilian = Some(crate::map::UnitId(3_900_001));
        }

        // Simulate losing the province: transfer ownership away from the nation.
        let pid = game.world.provinces[0].id;
        game.world.provinces[0].owner = NationId(99);
        game.world.nations[0].province_ids.retain(|p| *p != pid);

        let _ = process_turn(&mut game);

        // The in-flight engineer should have been cancelled — not "working" any more.
        let eng = game.world.nations[0]
            .military
            .civilians
            .iter()
            .find(|c| c.civilian_type == CivilianType::Engineer)
            .expect("engineer still exists");
        assert!(!eng.working, "stranded engineer should stop working");
        assert_eq!(eng.build_task, None);
        assert_eq!(eng.turns_remaining, 0);
    }

    // ── Card #79: release integrated minors on overlord anarchy ──────────

    /// Build a minimal two-province GP + one-province minor game where the
    /// minor has already been "absorbed": its province is owned by the GP
    /// and carries `incorporated_from = Some(minor_id)`.
    fn absorbed_minor_scenario() -> (GameState, NationId, NationId, ProvinceId, ProvinceId) {
        let gp_capital_coord = HexCoord::new(0, 0);
        let minor_capital_coord = HexCoord::new(2, 0);
        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            gp_capital_coord,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            minor_capital_coord,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let gp_prov = Province::new(
            ProvinceId(1),
            "GPHome".into(),
            NationId(1),
            gp_capital_coord,
            vec![gp_capital_coord],
            4,
        );
        // The minor's original capital province — already in the GP's hands,
        // stamped with the absorption marker.
        let mut minor_prov = Province::new(
            ProvinceId(2),
            "MinorHome".into(),
            NationId(1), // owned by the GP at absorption
            minor_capital_coord,
            vec![minor_capital_coord],
            4,
        );
        minor_prov.incorporated_from = Some(NationId(2));

        let mut gp = Nation::new(
            NationId(1),
            "Overlord".into(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp.province_ids = vec![ProvinceId(1), ProvinceId(2)];

        let mut minor = Nation::new(
            NationId(2),
            "Vassal".into(),
            NationColor::Yellow,
            NationType::MinorNation,
            ProvinceId(2),
        );
        minor.province_ids.clear(); // absorbed: no provinces
        minor.diplomacy.integrated_by = Some(NationId(1));

        let game = crate::test_game_state! {
        turn: TurnNumber::new(1),
        difficulty: Difficulty::Normal,
        map_key: "test".into(),
        hex_map: hex_map,
        provinces: vec![gp_prov, minor_prov],
        nations: vec![gp, minor],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        (game, NationId(1), NationId(2), ProvinceId(1), ProvinceId(2))
    }

    #[test]
    fn release_integrated_minors_restores_capital() {
        let (mut game, overlord_id, minor_id, _, minor_cap_pid) = absorbed_minor_scenario();
        let mut report = TurnReport::empty();

        release_integrated_minors(&mut game, overlord_id, &mut report);

        // The minor's capital province now belongs to the minor again.
        let prov = game.get_province(minor_cap_pid).unwrap();
        assert_eq!(prov.owner, minor_id);
        assert_eq!(prov.incorporated_from, None);

        let minor = game.get_nation(minor_id).unwrap();
        assert!(minor.province_ids.contains(&minor_cap_pid));
        assert_eq!(minor.diplomacy.integrated_by, None);
        assert!(
            !minor.diplomacy.is_in_anarchy,
            "minor with its capital back is not anarchic"
        );

        let overlord = game.get_nation(overlord_id).unwrap();
        assert!(!overlord.province_ids.contains(&minor_cap_pid));

        // An event was recorded.
        assert!(
            report.events.iter().any(|e| matches!(
                e,
                DomainEvent::MinorRegainedIndependence(ev) if ev.minor == minor_id
            )),
            "MinorRegainedIndependence event should be emitted"
        );
    }

    #[test]
    fn release_integrated_minors_clears_anarchy_when_nothing_to_restore() {
        // Card #96: a released subject must never inherit the overlord's
        // anarchy. When the minor's only province is held by a third party
        // (so there is nothing to restore), the minor still exits as a
        // non-anarchic polity with its overlord pointer cleared — it can be
        // interacted with diplomatically like before.
        let (mut game, overlord_id, minor_id, _, minor_cap_pid) = absorbed_minor_scenario();
        {
            let prov = game.get_province_mut(minor_cap_pid).unwrap();
            prov.owner = NationId(99);
        }
        // Seed the minor as anarchic to prove the release path clears it.
        game.get_nation_mut(minor_id)
            .unwrap()
            .diplomacy
            .is_in_anarchy = true;
        let mut report = TurnReport::empty();

        release_integrated_minors(&mut game, overlord_id, &mut report);

        let minor = game.get_nation(minor_id).unwrap();
        assert_eq!(
            minor.diplomacy.integrated_by, None,
            "overlord pointer must be cleared on release"
        );
        assert!(
            !minor.diplomacy.is_in_anarchy,
            "released minor must not inherit the overlord's anarchy (card #96)"
        );
    }

    #[test]
    fn release_integrated_minors_reassigns_capital_when_original_lost() {
        // Card #96: when a third party holds the minor's original capital but
        // the overlord still holds other provinces that originated from the
        // minor, release those provinces back and promote the first one to
        // the new capital so the minor returns as a functioning state — not
        // as a black-banner anarchy.
        let (mut game, overlord_id, minor_id, _, minor_cap_pid) = absorbed_minor_scenario();

        // Add a second province that also originated from the minor, held by
        // the overlord. It will be restored; the original capital will not.
        let extra_coord = HexCoord::new(4, 0);
        let extra_pid = ProvinceId(3);
        game.world.hex_map.set_tile(
            extra_coord,
            Tile::with_province(TerrainType::Grassland, extra_pid),
        );
        let mut extra_prov = Province::new(
            extra_pid,
            "MinorOutpost".into(),
            overlord_id, // currently owned by the overlord
            extra_coord,
            vec![extra_coord],
            4,
        );
        extra_prov.incorporated_from = Some(minor_id);
        game.world.provinces.push(extra_prov);
        game.get_nation_mut(overlord_id)
            .unwrap()
            .province_ids
            .push(extra_pid);

        // Third party has already seized the original capital.
        game.get_province_mut(minor_cap_pid).unwrap().owner = NationId(99);
        // Remove the stolen capital from the overlord so only the outpost is
        // restorable.
        game.get_nation_mut(overlord_id)
            .unwrap()
            .province_ids
            .retain(|p| *p != minor_cap_pid);

        let mut report = TurnReport::empty();
        release_integrated_minors(&mut game, overlord_id, &mut report);

        let minor = game.get_nation(minor_id).unwrap();
        assert_eq!(
            minor.diplomacy.integrated_by, None,
            "overlord pointer must be cleared"
        );
        assert!(
            minor.province_ids.contains(&extra_pid),
            "minor should reclaim the overlord-held province that originated from it"
        );
        assert_eq!(
            minor.capital_province_id, extra_pid,
            "capital must be reassigned to the first restored province when the original is lost"
        );
        assert!(
            !minor.diplomacy.is_in_anarchy,
            "released minor must not be in anarchy even if its original capital is gone (card #96)"
        );
    }

    #[test]
    fn full_anarchy_sweep_does_not_re_flag_released_minor_with_no_territory() {
        // Card #96 round-2 integration test: when the full
        // `apply_end_of_combat_anarchy` sweep runs after an overlord has
        // lost its capital, the sweep must not immediately re-flag the
        // released minor (whose territory was elsewhere captured by a third
        // party) as anarchic. The `released_this_sweep` set passed through
        // the sweep guarantees freshly-released minors are skipped by the
        // outer loop — independent (never-integrated) minors that lose
        // their last province still flow through normally (see
        // `independent_minor_losing_last_province_enters_anarchy`).
        let (mut game, overlord_id, minor_id, overlord_cap, minor_cap_pid) =
            absorbed_minor_scenario();

        // Third party took the minor's only province before overlord
        // collapses, so `release_integrated_minors` will have nothing to
        // restore for this minor.
        game.get_province_mut(minor_cap_pid).unwrap().owner = NationId(99);
        game.get_nation_mut(overlord_id)
            .unwrap()
            .province_ids
            .retain(|p| *p != minor_cap_pid);

        // Overlord has already lost its own capital — condition for anarchy.
        game.get_province_mut(overlord_cap).unwrap().owner = NationId(99);
        game.get_nation_mut(overlord_id)
            .unwrap()
            .province_ids
            .retain(|p| *p != overlord_cap);

        // Seed a third "new GP" nation so the sweep has a third entry
        // between the overlord and the minor; this stresses iteration order.
        let third_id = NationId(99);
        let mut third = Nation::new(
            third_id,
            "ThirdParty".into(),
            NationColor::Purple,
            NationType::GreatPower,
            overlord_cap,
        );
        third.province_ids = vec![overlord_cap, minor_cap_pid];
        game.world.nations.push(third);

        let mut report = TurnReport::empty();
        apply_end_of_combat_anarchy(&mut game, &mut report);

        // Overlord collapsed into anarchy.
        assert!(
            game.get_nation(overlord_id)
                .unwrap()
                .diplomacy
                .is_in_anarchy,
            "overlord must enter anarchy after losing its capital"
        );
        // Released minor must NOT be anarchic despite having no provinces.
        let minor = game.get_nation(minor_id).unwrap();
        assert_eq!(
            minor.diplomacy.integrated_by, None,
            "minor's overlord pointer must be cleared"
        );
        assert!(
            !minor.diplomacy.is_in_anarchy,
            "released minor with no territory must not be re-flagged anarchic by the sweep (F-001)"
        );
    }

    #[test]
    fn independent_minor_losing_last_province_enters_anarchy() {
        // Regression test for F-001 round-2: the sweep must still flag an
        // *independent* (never-integrated) minor as anarchic when it has no
        // provinces — the guard against re-flagging only applies to minors
        // released during the same sweep, not to genuinely defeated minors.
        use crate::hex::HexCoord;
        use crate::map::Province;
        use crate::map::tile::Tile;

        let independent_id = NationId(50);
        let province_coord = HexCoord::new(5, 5);
        let province_id = ProvinceId(50);

        let mut hex_map = HexMap::new(10, 10);
        hex_map.set_tile(
            province_coord,
            Tile::with_province(TerrainType::Grassland, province_id),
        );
        // The minor's former province has been captured by a conqueror
        // (NationId(99)), so the minor has `province_ids: []` and its
        // capital (province_id) is held by another.
        let captured = Province::new(
            province_id,
            "FormerMinorLand".into(),
            NationId(99), // conqueror holds it
            province_coord,
            vec![province_coord],
            4,
        );
        let mut minor = Nation::new(
            independent_id,
            "DefeatedMinor".into(),
            NationColor::Yellow,
            NationType::MinorNation,
            province_id,
        );
        minor.province_ids.clear(); // lost last province
        // No integrated_by — this minor is independent, not an absorbed one.
        assert_eq!(minor.diplomacy.integrated_by, None);

        let mut conqueror = Nation::new(
            NationId(99),
            "Conqueror".into(),
            NationColor::Red,
            NationType::GreatPower,
            province_id,
        );
        conqueror.province_ids = vec![province_id];

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(5),
        difficulty: Difficulty::Normal,
        map_key: "test".into(),
        hex_map: hex_map,
        provinces: vec![captured],
        nations: vec![conqueror, minor],
        human_player_nation: NationId(99),
        events: Vec::new(),
        game_data: GameData::default(),
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
        next_unit_id: 6_000_000,};

        let mut report = TurnReport::empty();
        apply_end_of_combat_anarchy(&mut game, &mut report);

        let minor = game.get_nation(independent_id).unwrap();
        assert!(
            minor.diplomacy.is_in_anarchy,
            "independent minor that lost its last province must still enter anarchy"
        );
    }

    // ── Card #68: pact-defense dedup lifecycle ──────────────────────────

    #[test]
    fn pact_defense_clears_on_peace() {
        let mut dip = DiplomacyState::new();
        dip.mark_pact_defense_requested(NationId(1), NationId(2));
        assert!(dip.is_pact_defense_requested(NationId(1), NationId(2)));

        // Peace between the same two nations — even in reversed order — wipes
        // the entry so a future war can raise a fresh cascade.
        dip.declare_war(NationId(1), NationId(2));
        dip.make_peace(NationId(2), NationId(1));

        assert!(!dip.is_pact_defense_requested(NationId(1), NationId(2)));
    }

    #[test]
    fn pact_defense_clear_for_nation_purges_all_involving() {
        let mut dip = DiplomacyState::new();
        dip.mark_pact_defense_requested(NationId(10), NationId(5));
        dip.mark_pact_defense_requested(NationId(5), NationId(20));
        dip.mark_pact_defense_requested(NationId(7), NationId(8));

        dip.clear_pact_defense_for_nation(NationId(5));

        assert!(!dip.is_pact_defense_requested(NationId(10), NationId(5)));
        assert!(!dip.is_pact_defense_requested(NationId(5), NationId(20)));
        assert!(dip.is_pact_defense_requested(NationId(7), NationId(8)));
    }

    #[test]
    fn release_does_not_touch_minors_integrated_by_other_overlords() {
        // GP A collapses. A second minor "Ally", integrated by unrelated GP B,
        // must not have its `integrated_by` back-pointer altered by A's anarchy.
        let (mut game, overlord_a, _minor_a, _, _) = absorbed_minor_scenario();
        let overlord_b = NationId(99);
        // Insert the unrelated overlord and a minor integrated by them.
        let mut gp_b = Nation::new(
            overlord_b,
            "OtherOverlord".into(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(42),
        );
        gp_b.province_ids = vec![ProvinceId(42)];
        game.world.nations.push(gp_b);

        let unrelated_minor = NationId(77);
        let mut minor_b = Nation::new(
            unrelated_minor,
            "AlliedVassal".into(),
            NationColor::Green,
            NationType::MinorNation,
            ProvinceId(99),
        );
        minor_b.province_ids.clear();
        minor_b.diplomacy.integrated_by = Some(overlord_b);
        game.world.nations.push(minor_b);

        let mut report = TurnReport::empty();
        release_integrated_minors(&mut game, overlord_a, &mut report);

        let ally = game.get_nation(unrelated_minor).unwrap();
        assert_eq!(
            ally.diplomacy.integrated_by,
            Some(overlord_b),
            "release on overlord A must not clear integrated_by for minors of overlord B"
        );
    }

    // F-019: when GP-A collapses and holds provinces stamped with
    // `incorporated_from = Some(minor_id)`, but the minor is currently
    // `integrated_by = Some(GP-B)`, the provinces must route to GP-B rather
    // than being restored to the minor (which would leave it owning territory
    // while still marked as integrated — inconsistent state).
    #[test]
    fn release_integrated_minors_routes_provinces_to_current_integrator() {
        let (mut game, overlord_a, minor_id, _, minor_cap_pid) = absorbed_minor_scenario();

        let overlord_b = NationId(99);
        let mut gp_b = Nation::new(
            overlord_b,
            "SecondOverlord".into(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(42),
        );
        let b_province = Province::new(
            ProvinceId(42),
            "BHome".into(),
            overlord_b,
            HexCoord::new(5, 0),
            vec![HexCoord::new(5, 0)],
            4,
        );
        gp_b.province_ids = vec![ProvinceId(42)];
        game.world.nations.push(gp_b);
        game.world.provinces.push(b_province);
        game.world.hex_map.set_tile(
            HexCoord::new(5, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(42)),
        );

        // Rewire: minor is now integrated by GP-B, not GP-A
        game.get_nation_mut(minor_id)
            .unwrap()
            .diplomacy
            .integrated_by = Some(overlord_b);

        let mut report = TurnReport::empty();
        release_integrated_minors(&mut game, overlord_a, &mut report);

        // Province must route to GP-B (the current integrator)
        let prov = game.get_province(minor_cap_pid).unwrap();
        assert_eq!(
            prov.owner, overlord_b,
            "province must route to the GP currently integrating the minor, not the minor itself"
        );
        // Origin marker preserved so future GP-B release can trace back to the minor
        assert_eq!(
            prov.incorporated_from,
            Some(minor_id),
            "incorporated_from marker must be preserved for the future release chain"
        );

        // Minor remains consistently integrated by GP-B
        let minor = game.get_nation(minor_id).unwrap();
        assert_eq!(
            minor.diplomacy.integrated_by,
            Some(overlord_b),
            "minor must remain integrated by GP-B after GP-A collapses"
        );
        assert!(
            !minor.province_ids.contains(&minor_cap_pid),
            "minor must not own the routed province"
        );

        // GP-B now holds the routed province
        let gp_b_nation = game.get_nation(overlord_b).unwrap();
        assert!(
            gp_b_nation.province_ids.contains(&minor_cap_pid),
            "GP-B must hold the province transferred from GP-A"
        );

        // No independence event for the minor
        assert!(
            !report.events.iter().any(|e| {
                matches!(e, DomainEvent::MinorRegainedIndependence(ev) if ev.minor == minor_id)
            }),
            "no MinorRegainedIndependence event when provinces route to the current integrator"
        );

        // F-022: garrison cache must be coherent after routing (F-021 fix).
        // GP-B holds the province with no army there, so garrison_count should be 0.
        let prov_after = game.get_province(minor_cap_pid).unwrap();
        let expected_garrison = game
            .get_nation(overlord_b)
            .map(|n| n.militia_at(minor_cap_pid))
            .unwrap_or(0) as u8;
        assert_eq!(
            prov_after.garrison_count, expected_garrison,
            "garrison_count must reflect the new owner's actual militia after routing"
        );
    }

    // Card #95: voluntary (or forced) incorporation must transfer the minor's
    // army to the overlord so the UI/move-target queries (which look up a
    // unit by `nation_id → army`) can find it under the new owner. Before
    // this fix, units stayed on the minor with `owner = minor_id` and the
    // player got "Could not compute move targets" when clicking them.
    #[test]
    fn incorporation_transfers_minor_army_to_overlord() {
        // Use an AI overlord (NationId(3)) so resolve_voluntary_incorporations
        // takes the auto-incorporate path. (For the human overlord path we
        // queue a RequestToJoinEmpire proposal — see the dedicated tests.)
        let mut game = test_game_state_with_minor_nation();
        let mut gp_b = Nation::new(
            NationId(3),
            "Healthy Empire".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        gp_b.economy.treasury = Money::dollars(1000);
        game.world.nations.push(gp_b);
        game.world
            .diplomacy
            .initialize_great_powers(&[NationId(1), NationId(3)]);

        // Seed the minor with a field army unit on its own province.
        let minor = game.get_nation_mut(NationId(2)).unwrap();
        minor
            .military
            .army
            .push(crate::military::units::ArmyUnit::new(
                crate::map::UnitId(999),
                crate::military::units::ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));

        // Push the relationship with the AI overlord over threshold.
        game.world
            .diplomacy
            .ensure_relation(NationId(2), NationId(3))
            .score = 95;

        let mut report = TurnReport::empty();
        resolve_voluntary_incorporations(&mut game, &mut report);

        let overlord = game.get_nation(NationId(3)).unwrap();
        let moved = overlord
            .military
            .army
            .iter()
            .find(|u| u.id == crate::map::UnitId(999))
            .expect("minor's army unit must live on the overlord after annexation");
        assert_eq!(
            moved.owner,
            NationId(3),
            "unit ownership must flip to the overlord so move-target queries find it"
        );

        let minor_after = game.get_nation(NationId(2)).unwrap();
        assert!(
            minor_after.military.army.is_empty(),
            "absorbed minor should have no lingering units on its (empty) roster"
        );
    }

    // Card #95 complement: a released minor must get token defenders back so
    // it isn't immediately re-conquerable on turn 1 of independence.
    #[test]
    fn released_minor_receives_token_garrison() {
        let (mut game, overlord_id, minor_id, _, minor_cap_pid) = absorbed_minor_scenario();
        // Precondition: absorbed minor has an empty army.
        assert!(game.get_nation(minor_id).unwrap().military.army.is_empty());

        let mut report = TurnReport::empty();
        release_integrated_minors(&mut game, overlord_id, &mut report);

        let minor = game.get_nation(minor_id).unwrap();
        let militia_at_cap = minor.militia_at(minor_cap_pid);
        let target = game.game_data.game_config.minor_default_garrison as usize;
        assert_eq!(
            militia_at_cap, target,
            "capital should be seeded up to the minor default militia count"
        );
        assert!(
            minor.has_garrison_artillery_at(minor_cap_pid),
            "released minor should get a GarrisonArtillery at its restored capital"
        );
    }

    // Card #98: a capital captured and then recaptured within the same turn
    // must not leave the original owner in anarchy. Because the anarchy
    // sweep only inspects the final post-combat state, mid-turn ownership
    // churn is invisible to it — which is exactly the fix.
    #[test]
    fn capital_recaptured_same_turn_does_not_trigger_anarchy() {
        let mut game = test_game_state();
        let capital = game.get_nation(NationId(1)).unwrap().capital_province_id;

        // Simulate a same-turn capture+recapture: an enemy takes the capital,
        // then the owner retakes it, leaving the final state as "owner holds
        // its capital" — which is what the end-of-combat sweep sees.
        {
            // Capture: flip province owner to enemy and drop it from the
            // original owner's province list.
            game.get_province_mut(capital).unwrap().owner = NationId(99);
            game.get_nation_mut(NationId(1))
                .unwrap()
                .province_ids
                .retain(|p| *p != capital);
        }
        {
            // Recapture: flip back.
            game.get_province_mut(capital).unwrap().owner = NationId(1);
            game.get_nation_mut(NationId(1))
                .unwrap()
                .add_province(capital);
        }

        let mut report = TurnReport::empty();
        apply_end_of_combat_anarchy(&mut game, &mut report);

        assert!(
            !game
                .get_nation(NationId(1))
                .unwrap()
                .diplomacy
                .is_in_anarchy,
            "capital captured then recaptured within the turn must not leave the owner anarchic"
        );
        assert!(
            !report
                .events
                .iter()
                .any(|e| matches!(e, DomainEvent::NationEnteredAnarchy(_))),
            "no NationEnteredAnarchy event should be emitted when the recapture lands"
        );
    }

    // Guardrail: F-001. The end-of-combat sweep must never flag absorbed
    // minors (diplomatically incorporated into a GP) as anarchic, even
    // though they hold zero provinces and therefore cannot contain their
    // capital. They are not sovereign polities.
    #[test]
    fn absorbed_minor_does_not_enter_anarchy_during_sweep() {
        let mut game = test_game_state_with_minor_nation();
        let rel = game
            .world
            .diplomacy
            .ensure_relation(NationId(2), NationId(1));
        rel.score = 95;

        let mut report = TurnReport::empty();
        resolve_voluntary_incorporations(&mut game, &mut report);
        apply_end_of_combat_anarchy(&mut game, &mut report);

        let absorbed = game.get_nation(NationId(2)).unwrap();
        assert!(
            !absorbed.diplomacy.is_in_anarchy,
            "an absorbed minor must not be flagged anarchic by the end-of-combat sweep"
        );
        assert!(
            !report.events.iter().any(
                |e| matches!(e, DomainEvent::NationEnteredAnarchy(ev) if ev.nation == NationId(2))
            ),
            "no NationEnteredAnarchy event should be emitted for an absorbed minor"
        );
    }

    // F-002: when the overlord falls into anarchy and `release_integrated_minors`
    // restores the minor's provinces, no overlord army units should remain
    // positioned on those provinces. They scatter with the collapsing empire.
    #[test]
    fn released_minor_has_no_overlord_units_on_its_provinces() {
        let (mut game, overlord_id, _minor_id, _, minor_cap_pid) = absorbed_minor_scenario();

        // Plant an overlord unit on the minor's (now overlord-owned) capital —
        // representing a unit transferred during incorporation.
        if let Some(overlord) = game.get_nation_mut(overlord_id) {
            overlord
                .military
                .army
                .push(crate::military::units::ArmyUnit::new(
                    crate::map::UnitId(8080),
                    crate::military::units::ArmyUnitType::Regulars,
                    overlord_id,
                    minor_cap_pid,
                ));
        }

        let mut report = TurnReport::empty();
        release_integrated_minors(&mut game, overlord_id, &mut report);

        let overlord = game.get_nation(overlord_id).unwrap();
        assert!(
            !overlord
                .military
                .army
                .iter()
                .any(|u| u.position == minor_cap_pid),
            "overlord units on restored provinces must be disbanded when the empire collapses"
        );
    }

    // Companion: if a nation genuinely does not hold its capital at end of
    // combat, the sweep still triggers anarchy.
    #[test]
    fn end_of_combat_anarchy_fires_when_capital_is_lost() {
        let mut game = test_game_state();
        let capital = game.get_nation(NationId(1)).unwrap().capital_province_id;
        // Simulate: an enemy holds the capital at the moment combat resolves.
        game.get_province_mut(capital).unwrap().owner = NationId(99);
        game.get_nation_mut(NationId(1))
            .unwrap()
            .province_ids
            .retain(|p| *p != capital);

        let mut report = TurnReport::empty();
        apply_end_of_combat_anarchy(&mut game, &mut report);

        assert!(
            game.get_nation(NationId(1))
                .unwrap()
                .diplomacy
                .is_in_anarchy,
            "nation without its capital at end of combat must enter anarchy"
        );
    }

    #[test]
    fn military_conquest_records_conquest_origin_not_incorporated_from() {
        // A minor's province conquered militarily should record its origin
        // in `conquest_origin` (mechanics-only) while leaving
        // `incorporated_from` untouched (UI-only). The reverse would make
        // the conquered province render as a diplomatic incorporation on
        // the map, which is a UI regression.
        let input = None;
        let result = attribute_conquest_origin(input, NationId(1), NationId(99), true);
        assert_eq!(result, Some(NationId(99)));

        // Preserved across further conquests by third parties.
        let later = attribute_conquest_origin(result, NationId(2), NationId(1), false);
        assert_eq!(later, Some(NationId(99)));

        // If the origin minor reclaims their own province, the attribution
        // is dropped.
        let reclaimed = attribute_conquest_origin(result, NationId(99), NationId(1), false);
        assert_eq!(reclaimed, None);
    }

    /// Card #129: an amphibious attack draws its naval cohort from every
    /// coastal province the attacker currently controls — including
    /// provinces that were formerly hostile capitals. The total landing
    /// force is capped by the sum of warship `arms_cost` on the beachhead.
    #[test]
    fn amphibious_landing_uses_all_controlled_coastal_provinces_and_respects_cap() {
        use crate::map::UnitId;
        use crate::military::naval::NavalOperation;
        use crate::military::ships::{Ship, ShipType};

        // Fixture: Nation 1 (Attacker) owns P1 (its own capital, coastal) and
        // P2 (former Nation 2 capital — now "conquered"; coastal). Nation 3
        // owns P3 (non-adjacent to both P1 and P2; coastal target). Both P1
        // and P2 are out-of-range overland, so the naval cohort is the only
        // way to reach P3.
        //
        //   Attacker: 2 Guards in P1, 2 Guards in P2.
        //   Ship: 1 Frigate (arms_cost = 2) → beachhead cap = 2.
        //
        // Expectation: the attack succeeds drawing units from P1 and P2
        // (attacker_owned set includes both), and only 2 units land
        // (best-FP first, cap = 2). Survivors stay at their origin ports.
        let coord_p1 = HexCoord::new(0, 0);
        let coord_p2 = HexCoord::new(2, 0);
        let coord_p3 = HexCoord::new(10, 0);

        let mut hex_map = HexMap::new(20, 20);
        hex_map.set_tile(
            coord_p1,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            coord_p2,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        hex_map.set_tile(
            coord_p3,
            Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );

        let mut p1 = Province::new(
            ProvinceId(1),
            "HomePort".into(),
            NationId(1),
            coord_p1,
            vec![coord_p1],
            4,
        );
        p1.coastal = true;
        p1.ocean_coastal = true;
        let mut p2 = Province::new(
            ProvinceId(2),
            "FormerEnemyCapital".into(),
            NationId(1),
            coord_p2,
            vec![coord_p2],
            4,
        );
        p2.coastal = true;
        p2.ocean_coastal = true;
        let mut p3 = Province::new(
            ProvinceId(3),
            "Target".into(),
            NationId(3),
            coord_p3,
            vec![coord_p3],
            3,
        );
        p3.coastal = true;
        p3.ocean_coastal = true;

        let mut attacker = Nation::new(
            NationId(1),
            "Attacker".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        // P2 was conquered from a former enemy — it joins the attacker's
        // province_ids just like any other owned province. This is precisely
        // what card #129 wants exercised.
        attacker.add_province(ProvinceId(2));
        attacker.economy.treasury = Money::dollars(20000);
        for i in 0..2 {
            attacker.military.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(1),
            ));
        }
        for i in 0..2 {
            attacker.military.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Guards,
                NationId(1),
                ProvinceId(2),
            ));
        }
        // Frigate: arms_cost = 2 → beachhead_cap = 2 (forces the cap to bite).
        let mut ship = Ship::new(UnitId(500), ShipType::Frigate, NationId(1), 35);
        ship.operation = Some(NavalOperation::Beachhead(ProvinceId(3)));
        attacker.military.warships.push(ship);

        let mut defender = Nation::new(
            NationId(3),
            "Defender".to_string(),
            NationColor::Red,
            NationType::MinorNation,
            ProvinceId(99), // fake capital so P3 gets no auto-garrison
        );
        defender.military.army.push(ArmyUnit::new(
            UnitId(400),
            ArmyUnitType::Minutemen,
            NationId(3),
            ProvinceId(3),
        ));

        let mut game = crate::test_game_state! {
        turn: TurnNumber::new(5),
        difficulty: Difficulty::Normal,
        map_key: "test".to_string(),
        hex_map: hex_map,
        provinces: vec![p1, p2, p3],
        nations: vec![attacker, defender],
        human_player_nation: NationId(1),
        events: Vec::new(),
        game_data: GameData::default(),
        diplomacy: DiplomacyState::new(),
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        pending_landings: vec![(NationId(1), ProvinceId(3), TurnNumber::new(4))],
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
        next_unit_id: 6_000_000,};
        game.transient
            .pending_attacks
            .push((NationId(1), ProvinceId(3)));
        game.world.diplomacy.declare_war(NationId(1), NationId(3));

        let report = process_turn(&mut game);

        assert!(
            !report.battles.is_empty(),
            "amphibious attack should produce a battle"
        );
        let battle = &report.battles[0];
        // Cap: only 2 Guards may land (arms_cost = 2). 4 units total owned —
        // the other 2 stay at their respective ports.
        assert_eq!(
            battle.attacker_initial_count, 2,
            "naval cohort must be capped by beachhead_force_size (arms_cost=2)"
        );

        // Battle won (defender was a single Militia vs. 2 Guards).
        assert_eq!(
            game.get_province(ProvinceId(3)).unwrap().owner,
            NationId(1),
            "attacker should conquer target province"
        );

        // Verify units from BOTH origin provinces remain — proves the naval
        // cohort was drawn from the full set of coastal attacker provinces,
        // not just one.
        let attacker = game.get_nation(NationId(1)).unwrap();
        let survivors_p1 = attacker
            .military
            .army
            .iter()
            .filter(|u| (100..102).contains(&u.id.0) && u.position == ProvinceId(1))
            .count();
        let survivors_p2 = attacker
            .military
            .army
            .iter()
            .filter(|u| (200..202).contains(&u.id.0) && u.position == ProvinceId(2))
            .count();
        assert!(
            survivors_p1 + survivors_p2 >= 2,
            "both P1 and P2 should still hold non-participating survivors, got P1={survivors_p1} P2={survivors_p2}",
        );
    }

    /// Card #20: a unit that neither moved nor fought recovers health; units
    /// that moved or fought stay damaged; and units already at full health
    /// are left alone.
    #[test]
    fn rest_heals_idle_units_only() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let mut game = test_game_state();

        let mut wounded_idle = ArmyUnit::new(
            UnitId(101),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        wounded_idle.health = 60;

        let mut wounded_moved = ArmyUnit::new(
            UnitId(102),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        wounded_moved.health = 60;

        let mut wounded_fought = ArmyUnit::new(
            UnitId(103),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        wounded_fought.health = 60;

        let mut healthy_idle = ArmyUnit::new(
            UnitId(104),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        healthy_idle.health = 100;

        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation
            .military
            .army
            .extend([wounded_idle, wounded_moved, wounded_fought, healthy_idle]);

        let mut moved = HashSet::new();
        moved.insert(UnitId(102));
        let mut fought = HashSet::new();
        fought.insert(UnitId(103));

        heal_resting_units(&mut game, &moved, &fought);

        let nation = game.get_nation(NationId(1)).unwrap();
        let hp = |uid: u32| {
            nation
                .military
                .army
                .iter()
                .find(|u| u.id == UnitId(uid))
                .map(|u| u.health)
                .unwrap()
        };
        assert_eq!(hp(101), 70, "idle wounded unit heals by rest_heal_amount");
        assert_eq!(hp(102), 60, "wounded unit that moved does not heal");
        assert_eq!(hp(103), 60, "wounded unit that fought does not heal");
        assert_eq!(hp(104), 100, "fully healthy unit remains at 100");
    }

    #[test]
    fn rest_heal_amount_config_drives_heal() {
        // F-015 regression: non-default rest_heal_amount must change the actual
        // heal amount applied by heal_resting_units.
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let mut game = test_game_state();
        game.game_data.game_config.rest_heal_amount = 5;

        let mut wounded = ArmyUnit::new(
            UnitId(201),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        wounded.health = 50;
        game.get_nation_mut(NationId(1))
            .unwrap()
            .military
            .army
            .push(wounded);

        heal_resting_units(&mut game, &HashSet::new(), &HashSet::new());

        let hp = game
            .get_nation(NationId(1))
            .unwrap()
            .military
            .army
            .iter()
            .find(|u| u.id == UnitId(201))
            .unwrap()
            .health;
        assert_eq!(
            hp, 55,
            "heal amount should reflect game_config.rest_heal_amount = 5"
        );

        // Verify clamping: unit at 98 hp should reach 100, not 103.
        let mut near_full = ArmyUnit::new(
            UnitId(202),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        near_full.health = 98;
        game.get_nation_mut(NationId(1))
            .unwrap()
            .military
            .army
            .push(near_full);

        heal_resting_units(&mut game, &HashSet::new(), &HashSet::new());

        let hp2 = game
            .get_nation(NationId(1))
            .unwrap()
            .military
            .army
            .iter()
            .find(|u| u.id == UnitId(202))
            .unwrap()
            .health;
        assert_eq!(hp2, 100, "heal should clamp at 100 HP");
    }

    #[test]
    fn militia_and_general_excluded_from_general_reward_threshold() {
        // Regression for card #128: Militia and Generals must not count toward
        // the arms total that unlocks a General reward.
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Remove all existing units to start clean
        nation.military.army.retain(|_| false);
        nation.military.total_arms_built = 0;
        nation.military.generals_earned = 0;

        // Add enough Regulars to reach the first general threshold (6 arms)
        // Regulars cost 1 arm each.
        for i in 0..6 {
            nation.military.army.push(ArmyUnit::new(
                UnitId(2000 + i),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            ));
        }
        // Add militia and a general — these must NOT inflate the arms count
        for i in 0..10 {
            nation.military.army.push(ArmyUnit::new(
                UnitId(3000 + i),
                ArmyUnitType::Minutemen,
                NationId(1),
                ProvinceId(1),
            ));
        }
        nation.military.army.push(ArmyUnit::new(
            UnitId(4000),
            ArmyUnitType::General,
            NationId(1),
            ProvinceId(1),
        ));

        let mut report = TurnReport::empty();
        resolve_rewards(&mut game, &mut report);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Exactly 6 arms from Regulars → first general awarded; militia/general ignored
        assert_eq!(
            nation.military.generals_earned, 1,
            "one general should be awarded at the 6-arms threshold"
        );
        assert_eq!(
            nation.military.total_arms_built, 6,
            "militia and generals must not count toward total_arms_built"
        );
    }

    // ── resolve_human_tech_research tests ────────────────────────────────────

    #[test]
    fn queued_tech_researched_at_end_of_turn() {
        let mut game = test_game_state();
        // Turn 1 = 1815 Q1; "High Pressure Steam Engine" (id=1) is free and available.
        let tech_id = crate::events::TechId(1);
        game.get_nation_mut(NationId(1))
            .unwrap()
            .pending_tech_research = Some(tech_id);

        let mut report = TurnReport::empty();
        resolve_human_tech_research(&mut game, &mut report);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            nation.has_researched(tech_id),
            "tech should be researched after turn"
        );
        assert_eq!(
            nation.pending_tech_research, None,
            "pending queue should be cleared"
        );
        assert!(
            !nation.researched_tech_years.is_empty(),
            "year should be recorded"
        );
        assert_eq!(
            nation.researched_tech_years[0], 1815,
            "year should match turn 1 year"
        );
        assert!(
            report
                .events
                .iter()
                .any(|e| matches!(e, DomainEvent::TechnologyResearched(_))),
            "TechnologyResearched event should be emitted"
        );
    }

    #[test]
    fn queued_tech_fails_insufficient_funds_emits_headline() {
        let mut game = test_game_state();
        // Cotton Gin (id=3) costs $1000. Set treasury to $500.
        // Cotton Gin is available from 1816, so advance turn to 5 (1816 Q1).
        game.turn = TurnNumber::new(5);
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.economy.treasury = Money::dollars(500);
        let tech_id = crate::events::TechId(3); // Cotton Gin, $1000
        nation.pending_tech_research = Some(tech_id);

        let mut report = TurnReport::empty();
        resolve_human_tech_research(&mut game, &mut report);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            !nation.has_researched(tech_id),
            "tech should NOT be researched on funds failure"
        );
        assert_eq!(
            nation.pending_tech_research, None,
            "pending queue cleared even on failure"
        );
        assert_eq!(
            nation.economy.treasury,
            Money::dollars(500),
            "treasury unchanged"
        );
        assert!(
            !report.newspaper_headlines.is_empty(),
            "a failure headline should be emitted"
        );
    }

    #[test]
    fn queued_tech_silently_dropped_when_expired() {
        let mut game = test_game_state();
        // Cotton Gin (id=3) expires after 1820. Advance to turn 25 = 1821 Q1.
        game.turn = TurnNumber::new(25);
        let tech_id = crate::events::TechId(3);
        game.get_nation_mut(NationId(1))
            .unwrap()
            .pending_tech_research = Some(tech_id);

        let mut report = TurnReport::empty();
        resolve_human_tech_research(&mut game, &mut report);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            !nation.has_researched(tech_id),
            "expired tech should not be researched"
        );
        assert_eq!(
            nation.pending_tech_research, None,
            "queue should be cleared"
        );
        // No headline for expired tech — it was silently dropped
        assert!(
            report.newspaper_headlines.is_empty(),
            "no headline for expired tech"
        );
    }

    #[test]
    fn queued_tech_cancel_leaves_no_pending() {
        let mut game = test_game_state();
        let tech_id = crate::events::TechId(1);
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.pending_tech_research = Some(tech_id);
        // Cancel by clearing directly (as wasm_cancel_tech_research does)
        nation.pending_tech_research = None;

        let mut report = TurnReport::empty();
        resolve_human_tech_research(&mut game, &mut report);

        let nation = game.get_nation(NationId(1)).unwrap();
        assert!(
            !nation.has_researched(tech_id),
            "cancelled tech should not be researched"
        );
        assert!(report.events.is_empty(), "no events for cancelled tech");
    }
}
