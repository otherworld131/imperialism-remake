//! Single funnel for player actions. Every UI affordance writes a
//! [`GameCommand`] message; `apply_command` is the only place that touches
//! the session in response. Every command merely queues state on the game —
//! nothing resolves before end turn (the end-turn pipeline applies it all).

use bevy::prelude::*;

use crate::game::resources::{
    DataVersion, DeployMode, GameMeta, PerspectiveNation, ProposalPrompt, QueuedDiplomacyAction,
    SelectedCivilian, SelectedShips, SelectedUnits, SessionRes,
};
use crate::game::turn_runner::{self, ActiveSkip, ActiveTurn, SkipSpec};
use crate::game::vm;
use crate::state::TurnPhase;
use crate::widgets::Toast;

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum GameCommand {
    EndTurn,
    /// Top bar "Skip N": process up to N turns (1..=500) on the compute
    /// pool with progress + cancel.
    SkipTurns {
        count: u32,
    },
    /// Top bar "Skip Until": process turns until a headline matches the
    /// text (case-insensitive substring), capped at 1000 turns.
    SkipUntil {
        text: String,
    },
    /// Observer viewpoint switch (web header dropdown): re-seat the
    /// human/viewpoint nation by Great-Power index.
    SetViewpoint {
        index: usize,
    },
    /// Queue every selected unit's move to one province, all-or-nothing:
    /// any failure rolls back the units queued earlier in the batch.
    QueueUnitMoves {
        unit_ids: Vec<u32>,
        dest_province_id: u32,
    },
    CancelUnitMove {
        unit_id: u32,
    },
    CancelUnitMoves {
        unit_ids: Vec<u32>,
    },
    DisbandUnits {
        unit_ids: Vec<u32>,
    },
    UpgradeUnit {
        unit_id: u32,
    },
    /// Per-unit failures don't abort the batch (web `upgrade_units` parity).
    UpgradeUnits {
        unit_ids: Vec<u32>,
    },
    DeployCivilian {
        civilian_id: u32,
        q: i32,
        r: i32,
        /// Idle redeploy: recall from the current tile first.
        recall_first: bool,
    },
    RecallCivilian {
        civilian_id: u32,
    },
    /// Order a deployed engineer to build on the hex it stands on (card
    /// #495: the engineer was placed on an earlier turn).
    EngineerBuild {
        civilian_id: u32,
        /// "railroad" | "depot" | "port".
        kind: &'static str,
    },
    MoveFleet {
        from_zone: u32,
        to_zone: u32,
    },
    CancelFleetMove {
        from_zone: u32,
    },
    // ── M7: Industry screen ──────────────────────────────────────────
    /// Output target for a production-chain step. `u32::MAX` = unlimited.
    SetChainTarget {
        chain: &'static str,
        step: &'static str,
        target: u32,
    },
    ExpandBuilding {
        building_type: String,
    },
    SetPendingTraining {
        to_trained: u32,
        to_expert: u32,
    },
    SetPendingImmigration {
        count: u32,
    },
    SetPendingFreightCars {
        count: u32,
    },
    SetPendingArmyRecruits {
        unit_type: String,
        count: u32,
    },
    SetPendingShips {
        ship_type: String,
        count: u32,
    },
    SetPendingCivilianHire {
        civilian_type: String,
        count: u32,
    },
    // ── M7: Transport screen ─────────────────────────────────────────
    SetTransportAllocation {
        resource: String,
        units: u32,
    },
    // ── M7: Trade screen ─────────────────────────────────────────────
    SetAutoTradeWithMinors {
        enabled: bool,
    },
    SetTradeSubsidy {
        nation_id: u32,
        amount: i64,
    },
    SetSellOrder {
        resource: String,
        quantity: u32,
    },
    SetBuyOrder {
        resource: String,
        quantity: u32,
        max_price: i64,
    },
    // ── M8: Diplomacy screen ─────────────────────────────────────────
    /// Fire an armed diplomatic action at a target nation. The action only
    /// queues pending state (consulate/embassy/grant/war/break-treaty) or
    /// files a proposal (NAP/alliance/peace) — everything resolves at end
    /// turn.
    QueueDiplomacy {
        action: QueuedDiplomacyAction,
        target: u32,
    },
    /// Dismiss a pending diplomacy action / outgoing proposal via its map
    /// marker (web `dismiss_pending_action`).
    DismissPendingDiplomacy {
        target: u32,
        action_key: String,
    },
    // ── M8: Proposal modal ───────────────────────────────────────────
    AcceptProposal {
        index: u32,
    },
    RejectProposal {
        index: u32,
    },
    // ── M8: Tech screen ──────────────────────────────────────────────
    QueueTechResearch {
        name: String,
    },
    CancelTechResearch,
}

/// Toast on failure; always returns `true` so the data version bumps and the
/// UI re-reads the authoritative queued state (success and failure alike).
fn report(
    result: Result<(), frontend_api::ApiError>,
    context: &str,
    toasts: &mut MessageWriter<Toast>,
) -> bool {
    if let Err(err) = result {
        toasts.write(Toast::error(format!("{context}: {}", err.message())));
    }
    true
}

#[allow(clippy::too_many_arguments)]
pub fn apply_command(
    mut messages: MessageReader<GameCommand>,
    mut session: ResMut<SessionRes>,
    mut active: ResMut<ActiveTurn>,
    mut active_skip: ResMut<ActiveSkip>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
    mut meta: ResMut<GameMeta>,
    mut perspective: ResMut<PerspectiveNation>,
    mut data_version: ResMut<DataVersion>,
    mut toasts: MessageWriter<Toast>,
    mut selected_units: ResMut<SelectedUnits>,
    mut selected_ships: ResMut<SelectedShips>,
    mut selected_civilian: ResMut<SelectedCivilian>,
    mut deploy: ResMut<DeployMode>,
    mut proposal_prompt: ResMut<ProposalPrompt>,
) {
    for command in messages.read() {
        if let GameCommand::EndTurn = command {
            turn_runner::start_end_turn(&mut session, &mut active, &mut next_phase);
            continue;
        }
        if let GameCommand::SkipTurns { count } = command {
            turn_runner::start_skip(
                &mut session,
                &mut active_skip,
                &mut next_phase,
                SkipSpec::Count(*count),
            );
            continue;
        }
        if let GameCommand::SkipUntil { text } = command {
            turn_runner::start_skip(
                &mut session,
                &mut active_skip,
                &mut next_phase,
                SkipSpec::Until(text.clone()),
            );
            continue;
        }
        // Viewpoint switching is the one command that works in observer
        // mode — it only moves the viewpoint seat, never queues actions.
        if let GameCommand::SetViewpoint { index } = command {
            let Some(session) = session.0.as_mut() else {
                continue;
            };
            match frontend_api::setup::set_human_player(session.game_mut(), *index) {
                Ok(()) => {
                    meta.player_nation = session.human_nation();
                    perspective.0 = meta.player_nation;
                    data_version.0 += 1;
                }
                Err(err) => {
                    toasts.write(Toast::error(format!(
                        "Viewpoint switch failed: {}",
                        err.message()
                    )));
                }
            }
            continue;
        }
        // Observer games are read-only; the panels that emit these commands
        // are hidden, so just drop anything that slips through.
        if meta.observer {
            continue;
        }
        let Some(session) = session.0.as_mut() else {
            continue;
        };
        let game = session.game_mut();
        let nation = meta.player_nation;
        let mut bump = false;

        match command {
            GameCommand::EndTurn
            | GameCommand::SkipTurns { .. }
            | GameCommand::SkipUntil { .. }
            | GameCommand::SetViewpoint { .. } => unreachable!(),

            GameCommand::QueueUnitMoves {
                unit_ids,
                dest_province_id,
            } => {
                // Snapshot prior pending moves so a mid-batch failure can
                // restore them (web behavior: on any error, no units move).
                let prior: Vec<vm::PendingMoveVm> =
                    frontend_api::units::get_pending_unit_moves(game, nation)
                        .ok()
                        .and_then(|v| vm::parse_pending_moves(v).ok())
                        .unwrap_or_default();
                let mut failed: Option<String> = None;
                let mut done: Vec<u32> = Vec::new();
                for &unit_id in unit_ids {
                    match frontend_api::units::queue_unit_move(
                        game,
                        nation,
                        unit_id,
                        *dest_province_id,
                    ) {
                        Ok(()) => done.push(unit_id),
                        Err(err) => {
                            failed = Some(err.message());
                            break;
                        }
                    }
                }
                if let Some(err) = failed {
                    // Roll back this batch: restore each unit's prior queued
                    // move, or cancel the one we just added.
                    for unit_id in done {
                        let prior_move = prior.iter().find(|m| m.unit_id == unit_id);
                        match prior_move {
                            Some(m) => {
                                let _ = frontend_api::units::queue_unit_move(
                                    game,
                                    nation,
                                    unit_id,
                                    m.dest_province_id as u32,
                                );
                            }
                            None => {
                                let _ = frontend_api::units::cancel_unit_move(game, unit_id);
                            }
                        }
                    }
                    toasts.write(Toast::error(format!("Move failed: {err}. No units moved.")));
                } else {
                    bump = true;
                }
                selected_units.0.clear();
            }

            GameCommand::CancelUnitMove { unit_id } => {
                match frontend_api::units::cancel_unit_move(game, *unit_id) {
                    Ok(()) => bump = true,
                    Err(err) => {
                        toasts.write(Toast::error(format!("Cancel failed: {}", err.message())));
                    }
                }
            }

            GameCommand::CancelUnitMoves { unit_ids } => {
                let mut succeeded = 0usize;
                let mut failed = 0usize;
                for &unit_id in unit_ids {
                    match frontend_api::units::cancel_unit_move(game, unit_id) {
                        Ok(()) => succeeded += 1,
                        Err(_) => failed += 1,
                    }
                }
                if succeeded > 0 {
                    bump = true;
                }
                if failed > 0 {
                    toasts.write(Toast::error(format!(
                        "Canceled {succeeded} of {} moves — {failed} failed",
                        unit_ids.len()
                    )));
                }
            }

            GameCommand::DisbandUnits { unit_ids } => {
                let mut succeeded = 0usize;
                let mut failed = 0usize;
                for &unit_id in unit_ids {
                    match frontend_api::units::disband_unit(game, unit_id) {
                        Ok(()) => succeeded += 1,
                        Err(_) => failed += 1,
                    }
                }
                if succeeded > 0 {
                    bump = true;
                    selected_units.0.clear();
                }
                if failed > 0 {
                    toasts.write(Toast::error(format!(
                        "Dismissed {succeeded} of {} units — {failed} failed",
                        unit_ids.len()
                    )));
                }
            }

            GameCommand::UpgradeUnit { unit_id } => {
                match frontend_api::units::upgrade_unit(game, nation, *unit_id) {
                    Ok(()) => bump = true,
                    Err(err) => {
                        toasts.write(Toast::error(format!("Upgrade failed: {}", err.message())));
                    }
                }
            }

            GameCommand::UpgradeUnits { unit_ids } => {
                let mut upgraded = 0usize;
                let mut first_error: Option<String> = None;
                let mut failed = 0usize;
                for &unit_id in unit_ids {
                    match frontend_api::units::upgrade_unit(game, nation, unit_id) {
                        Ok(()) => upgraded += 1,
                        Err(err) => {
                            failed += 1;
                            if first_error.is_none() {
                                first_error = Some(err.message());
                            }
                        }
                    }
                }
                if upgraded > 0 {
                    bump = true;
                }
                if failed > 0 {
                    toasts.write(Toast::error(format!(
                        "Upgraded {upgraded} — {failed} failed ({})",
                        first_error.unwrap_or_default()
                    )));
                }
            }

            GameCommand::DeployCivilian {
                civilian_id,
                q,
                r,
                recall_first,
            } => {
                if *recall_first
                    && let Err(err) = frontend_api::units::recall_civilian(game, *civilian_id)
                {
                    toasts.write(Toast::error(format!("Recall failed: {}", err.message())));
                    continue;
                }
                match frontend_api::units::deploy_civilian(game, *civilian_id, *q, *r) {
                    Ok(()) => {
                        bump = true;
                        deploy.0 = None;
                    }
                    Err(err) => {
                        // Keep deploy mode active so the player can retry.
                        bump = *recall_first; // the recall already mutated state
                        toasts.write(Toast::error(format!("Deploy failed: {}", err.message())));
                    }
                }
            }

            GameCommand::RecallCivilian { civilian_id } => {
                match frontend_api::units::recall_civilian(game, *civilian_id) {
                    Ok(()) => {
                        bump = true;
                        if selected_civilian.0 == Some(i64::from(*civilian_id)) {
                            selected_civilian.0 = None;
                        }
                    }
                    Err(err) => {
                        toasts.write(Toast::error(format!("Recall failed: {}", err.message())));
                    }
                }
            }

            GameCommand::EngineerBuild { civilian_id, kind } => {
                match frontend_api::units::engineer_build(game, *civilian_id, kind) {
                    Ok(()) => {
                        bump = true;
                        deploy.0 = None;
                    }
                    Err(err) => {
                        toasts.write(Toast::error(format!("Build failed: {}", err.message())));
                    }
                }
            }

            GameCommand::MoveFleet { from_zone, to_zone } => {
                match frontend_api::units::move_fleet(game, nation, *from_zone, *to_zone) {
                    Ok(()) => {
                        bump = true;
                        // The fleet stays selected; the sync system re-selects
                        // its warships from the refreshed view models.
                        selected_ships.0.clear();
                    }
                    Err(err) => {
                        toasts.write(Toast::error(format!(
                            "Fleet move failed: {}",
                            err.message()
                        )));
                    }
                }
            }

            GameCommand::CancelFleetMove { from_zone } => {
                match frontend_api::units::cancel_fleet_move(game, nation, *from_zone) {
                    Ok(()) => bump = true,
                    Err(err) => {
                        toasts.write(Toast::error(format!("Cancel failed: {}", err.message())));
                    }
                }
            }

            // ── M7 pending-state setters ─────────────────────────────
            // All of these only queue state for end-turn resolution. On
            // failure the data version is bumped anyway so sliders snap
            // back to the real queued value.
            GameCommand::SetChainTarget {
                chain,
                step,
                target,
            } => {
                bump = report(
                    frontend_api::industry::set_chain_target(game, nation, chain, step, *target),
                    "Set target failed",
                    &mut toasts,
                );
            }

            GameCommand::ExpandBuilding { building_type } => {
                bump = report(
                    frontend_api::industry::expand_building(game, nation, building_type),
                    "Expand failed",
                    &mut toasts,
                );
            }

            GameCommand::SetPendingTraining {
                to_trained,
                to_expert,
            } => {
                bump = report(
                    frontend_api::units::set_pending_training(
                        game,
                        nation,
                        *to_trained,
                        *to_expert,
                    ),
                    "Set training failed",
                    &mut toasts,
                );
            }

            GameCommand::SetPendingImmigration { count } => {
                bump = report(
                    frontend_api::units::set_pending_immigration(game, nation, *count),
                    "Set immigration failed",
                    &mut toasts,
                );
            }

            GameCommand::SetPendingFreightCars { count } => {
                bump = report(
                    frontend_api::transport::set_pending_freight_cars(game, nation, *count),
                    "Set freight cars failed",
                    &mut toasts,
                );
            }

            GameCommand::SetPendingArmyRecruits { unit_type, count } => {
                bump = report(
                    frontend_api::units::set_pending_army_recruits(game, nation, unit_type, *count),
                    "Set recruits failed",
                    &mut toasts,
                );
            }

            GameCommand::SetPendingShips { ship_type, count } => {
                bump = report(
                    frontend_api::units::set_pending_ships(game, nation, ship_type, *count),
                    "Set ship orders failed",
                    &mut toasts,
                );
            }

            GameCommand::SetPendingCivilianHire {
                civilian_type,
                count,
            } => {
                bump = report(
                    frontend_api::units::set_pending_civilian_hire(
                        game,
                        nation,
                        civilian_type,
                        *count,
                    ),
                    "Set hire failed",
                    &mut toasts,
                );
            }

            GameCommand::SetTransportAllocation { resource, units } => {
                bump = report(
                    frontend_api::transport::set_transport_allocation(
                        game, nation, resource, *units,
                    ),
                    "Set allocation failed",
                    &mut toasts,
                );
            }

            GameCommand::SetAutoTradeWithMinors { enabled } => {
                bump = report(
                    frontend_api::trade::set_auto_trade_with_minors(game, nation, *enabled),
                    "Toggle failed",
                    &mut toasts,
                );
            }

            GameCommand::SetTradeSubsidy { nation_id, amount } => {
                bump = report(
                    frontend_api::trade::set_trade_subsidy(game, nation, *nation_id, *amount),
                    "Set subsidy failed",
                    &mut toasts,
                );
            }

            GameCommand::SetSellOrder { resource, quantity } => {
                bump = report(
                    frontend_api::trade::set_player_sell_order(
                        game, nation, "resource", resource, *quantity,
                    ),
                    "Sell order failed",
                    &mut toasts,
                );
            }

            GameCommand::SetBuyOrder {
                resource,
                quantity,
                max_price,
            } => {
                bump = report(
                    frontend_api::trade::set_player_buy_order(
                        game, nation, "resource", resource, *quantity, *max_price,
                    ),
                    "Buy order failed",
                    &mut toasts,
                );
            }

            // ── M8: Diplomacy ────────────────────────────────────────
            GameCommand::QueueDiplomacy { action, target } => {
                let target = *target;
                let (result, context) = match action {
                    QueuedDiplomacyAction::Consulate => (
                        frontend_api::diplomacy::build_consulate(game, nation, target),
                        "Consulate failed",
                    ),
                    QueuedDiplomacyAction::Embassy => (
                        frontend_api::diplomacy::build_embassy(game, nation, target),
                        "Embassy failed",
                    ),
                    QueuedDiplomacyAction::Nap => (
                        frontend_api::diplomacy::propose_nap(game, nation, target),
                        "NAP failed",
                    ),
                    QueuedDiplomacyAction::Alliance => (
                        frontend_api::diplomacy::propose_alliance(game, nation, target),
                        "Alliance failed",
                    ),
                    QueuedDiplomacyAction::Peace => (
                        frontend_api::diplomacy::propose_peace(game, nation, target),
                        "Peace failed",
                    ),
                    QueuedDiplomacyAction::Grant { amount } => (
                        frontend_api::diplomacy::send_grant(game, nation, target, *amount),
                        "Grant failed",
                    ),
                    QueuedDiplomacyAction::BreakTreaty { treaty_type } => (
                        frontend_api::diplomacy::break_treaty(game, nation, target, treaty_type),
                        "Break treaty failed",
                    ),
                    QueuedDiplomacyAction::War => (
                        frontend_api::diplomacy::declare_war(game, nation, target),
                        "Declare war failed",
                    ),
                };
                bump = report(result, context, &mut toasts);
            }

            GameCommand::DismissPendingDiplomacy { target, action_key } => {
                match frontend_api::diplomacy::dismiss_pending_action(
                    game, nation, *target, action_key,
                ) {
                    Ok(()) => bump = true,
                    // Web parity: stale-marker errors are silently ignored.
                    Err(err) => {
                        let msg = err.message();
                        if !msg.contains("no pending diplomacy action")
                            && !msg.contains("no outgoing proposal")
                        {
                            toasts.write(Toast::error(format!("Dismiss failed: {msg}")));
                        }
                        bump = true;
                    }
                }
            }

            // ── M8: Proposal modal ───────────────────────────────────
            GameCommand::AcceptProposal { index } | GameCommand::RejectProposal { index } => {
                let accept = matches!(command, GameCommand::AcceptProposal { .. });
                let result = if accept {
                    frontend_api::diplomacy::accept_proposal(game, nation, *index)
                } else {
                    frontend_api::diplomacy::reject_proposal(game, nation, *index)
                };
                let context = if accept {
                    "Accept failed"
                } else {
                    "Reject failed"
                };
                bump = report(result, context, &mut toasts);
                // Keep the modal in step with the authoritative list
                // (indices shift after a removal).
                proposal_prompt.0 = frontend_api::diplomacy::get_pending_proposals(game, nation)
                    .ok()
                    .and_then(|v| vm::parse_proposals(v).ok())
                    .filter(|p| !p.proposals.is_empty());
            }

            // ── M8: Tech screen ──────────────────────────────────────
            GameCommand::QueueTechResearch { name } => {
                bump = report(
                    frontend_api::tech::queue_tech_research(game, name),
                    "Queue research failed",
                    &mut toasts,
                );
            }

            GameCommand::CancelTechResearch => {
                bump = report(
                    frontend_api::tech::cancel_tech_research(game),
                    "Cancel research failed",
                    &mut toasts,
                );
            }
        }

        if bump {
            data_version.0 += 1;
        }
    }
}
