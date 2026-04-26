//! Reconciliation tests for the per-turn cash-flow ledger.
//!
//! The invariant the ledger must uphold:
//!   `closing_treasury − opening_treasury  ==  Σ income − Σ expense`
//! for every nation, on every turn. When this fails, a treasury mutation
//! site somewhere (in `turn/processor.rs` or one of the `ai/*` modules) is
//! missing a recorder — the fix is to find the site and add it to the
//! aggregator in `finalize_cash_flow` or push into `pending_ai_cash_*`.

use domain::game_state::new_observer_game;
use domain::turn::process_turn;
use domain::types::Difficulty;

#[test]
fn observer_cash_flow_reconciles_for_all_gps_over_five_turns() {
    let mut game = new_observer_game("cashflow_obs_5", Difficulty::Normal);
    let gp_ids: Vec<_> = game.great_powers().iter().map(|n| n.id).collect();

    let mut mismatches: Vec<String> = Vec::new();

    for _t in 0..20 {
        let report = process_turn(&mut game);
        for id in &gp_ids {
            let flow = match report.cash_flow.get(id) {
                Some(f) => f,
                None => {
                    mismatches.push(format!("no cash_flow entry for nation {:?}", id));
                    continue;
                }
            };
            if !flow.reconciles() {
                let nation_name = game
                    .get_nation(*id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| format!("{:?}", id));
                let income = flow.income_totals_by_source();
                let expense = flow.expense_totals_by_sink();
                mismatches.push(format!(
                    "turn={:?} nation={} opening=${} closing=${} observed_delta=${} \
                     accounted_delta=${} mismatch=${} \nincome={:?}\nexpense={:?}",
                    report.turn,
                    nation_name,
                    flow.opening_treasury.as_dollars(),
                    flow.closing_treasury.as_dollars(),
                    flow.observed_delta().as_dollars(),
                    flow.accounted_delta().as_dollars(),
                    flow.reconciliation_mismatch().as_dollars(),
                    income,
                    expense,
                ));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "cash-flow reconciliation failed for {} (nation, turn) pairs:\n{}",
        mismatches.len(),
        mismatches.join("\n---\n"),
    );
}

#[test]
fn observer_cash_flow_populates_cumulative_totals_on_nations() {
    let mut game = new_observer_game("cashflow_cumulative", Difficulty::Normal);

    for _t in 0..3 {
        process_turn(&mut game);
    }

    // At least one Great Power should have recorded some income or expense
    // over 3 turns (gold/gems conversion, maintenance, or AI spending).
    let any_activity = game
        .great_powers()
        .iter()
        .any(|n| !n.archives.cash_income_totals.is_empty() || !n.archives.cash_expense_totals.is_empty());
    assert!(
        any_activity,
        "expected at least one GP to have non-empty cash totals after 3 observer turns"
    );
}

#[test]
#[ignore]
fn debug_dump_turn_one() {
    let mut game = new_observer_game("cashflow_debug", Difficulty::Normal);
    println!(
        "Starting treasuries: {:?}",
        game.nations
            .iter()
            .filter(|n| n.is_great_power())
            .map(|n| (n.name.clone(), n.economy.treasury.as_dollars()))
            .collect::<Vec<_>>()
    );
    let report = process_turn(&mut game);
    for n in game.nations.iter().filter(|n| n.is_great_power()) {
        let flow = report.cash_flow.get(&n.id).unwrap();
        println!(
            "  {}: opening=${} closing=${} mismatch=${} income={:?} expense={:?}",
            n.name,
            flow.opening_treasury.as_dollars(),
            flow.closing_treasury.as_dollars(),
            flow.reconciliation_mismatch().as_dollars(),
            flow.income_totals_by_source(),
            flow.expense_totals_by_sink()
        );
    }
}

#[test]
fn standalone_run_ai_turns_does_not_bleed_into_next_turn_ledger() {
    // Regression test for F-004: if `run_ai_turns` is called outside of
    // `process_turn`, any AI-side treasury mutations push into the pending
    // collectors. `process_turn` must clear them at its start so those stale
    // entries don't leak into the next turn's cash_flow breakdown.
    use domain::ai::run_ai_turns;

    let mut game = new_observer_game("cashflow_standalone_ai", Difficulty::Normal);
    run_ai_turns(&mut game);
    // Pending collectors now contain entries. If we don't clear them at the
    // top of `process_turn`, they would show up in this next turn's ledger —
    // and reconciliation would break because the treasury mutations already
    // happened last "turn".
    let report = process_turn(&mut game);
    for nation in game.great_powers() {
        let flow = report
            .cash_flow
            .get(&nation.id)
            .expect("cash_flow entry for GP");
        assert!(
            flow.reconciles(),
            "reconciliation broke for {} after standalone run_ai_turns: \
             mismatch=${}",
            nation.name,
            flow.reconciliation_mismatch().as_dollars(),
        );
    }
}

#[test]
fn bankruptcy_writeoff_only_covers_clamp_delta_not_missing_recorders() {
    // F-003 analysis: the reconciliation invariant
    //   Σ income − Σ expense == closing − opening
    // is not trivially satisfied by BankruptcyWriteoff. The writeoff records
    // exactly `BANKRUPTCY_FLOOR - treasury_pre_clamp` ($0 − negative). If a
    // treasury mutation site SOMEWHERE was missing its recorder, the writeoff
    // alone cannot absorb it silently:
    //
    //   Scenario A (clean, no missing recorder):
    //     open=$0, recorded_expense=$100 → treasury=-$100 → clamp → closing=$0, writeoff=$100
    //     accounted = writeoff($100) - expense($100) = $0; observed = $0 - $0 = $0 ✓
    //
    //   Scenario B (missing recorder for same $100 expense):
    //     open=$0, unrecorded_expense=$100 → treasury=-$100 → clamp → closing=$0, writeoff=$100
    //     accounted = writeoff($100) - expense($0) = +$100; observed = $0 ≠ +$100 ✗
    //
    // So the existing 20-turn reconciliation test already catches missing
    // recorders on bankrupt turns — the writeoff can't mask them.
    //
    // This test locks the property in by running 20 turns and asserting
    // reconciliation holds even on turns where writeoffs are present.
    let mut game = new_observer_game("cashflow_writeoff_invariant", Difficulty::Normal);

    let mut turns_with_writeoff = 0usize;
    for _t in 0..20 {
        let report = process_turn(&mut game);
        for (nation_id, flow) in &report.cash_flow {
            // Writeoff present means bankruptcy clamp fired for this nation.
            let had_writeoff = flow
                .income
                .keys()
                .any(|s| matches!(s, domain::economy::CashSource::BankruptcyWriteoff));
            if had_writeoff {
                turns_with_writeoff += 1;
                assert!(
                    flow.reconciles(),
                    "reconciliation broke on bankrupt turn for nation {:?}: \
                     mismatch=${}",
                    nation_id,
                    flow.reconciliation_mismatch().as_dollars(),
                );
            }
        }
    }
    assert!(
        turns_with_writeoff > 0,
        "observer scenario should produce bankruptcy writeoffs in 20 turns \
         (Normal difficulty AIs routinely overspend) — test is vacuous if 0"
    );
}

#[test]
fn observer_cash_flow_persists_last_turn_snapshot_on_game_state() {
    let mut game = new_observer_game("cashflow_last", Difficulty::Normal);
    process_turn(&mut game);
    assert!(
        !game.last_cash_flow.is_empty(),
        "game.last_cash_flow should be populated after a turn"
    );
    for nation in game.great_powers() {
        assert!(
            game.last_cash_flow.contains_key(&nation.id),
            "missing last_cash_flow entry for GP {}",
            nation.name
        );
    }
}
