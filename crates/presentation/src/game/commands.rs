//! Single funnel for player actions. Every UI affordance writes a
//! [`GameCommand`] message; `apply_command` is the only place that touches
//! the session in response. Every command merely queues state on the game —
//! nothing resolves before end turn (the end-turn pipeline applies it all).

use bevy::prelude::*;

use crate::game::resources::{
    DataVersion, DeployMode, GameMeta, SelectedCivilian, SelectedShips, SelectedUnits, SessionRes,
};
use crate::game::turn_runner::{self, ActiveTurn};
use crate::game::vm;
use crate::state::TurnPhase;
use crate::widgets::Toast;

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum GameCommand {
    EndTurn,
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
    /// Engineer deploy chain: (recall →) deploy → start build task.
    EngineerBuild {
        civilian_id: u32,
        q: i32,
        r: i32,
        /// "railroad" | "depot" | "port".
        kind: &'static str,
        recall_first: bool,
    },
    MoveFleet {
        from_zone: u32,
        to_zone: u32,
    },
    CancelFleetMove {
        from_zone: u32,
    },
}

pub fn apply_command(
    mut messages: MessageReader<GameCommand>,
    mut session: ResMut<SessionRes>,
    mut active: ResMut<ActiveTurn>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
    meta: Res<GameMeta>,
    mut data_version: ResMut<DataVersion>,
    mut toasts: MessageWriter<Toast>,
    mut selected_units: ResMut<SelectedUnits>,
    mut selected_ships: ResMut<SelectedShips>,
    mut selected_civilian: ResMut<SelectedCivilian>,
    mut deploy: ResMut<DeployMode>,
) {
    for command in messages.read() {
        if let GameCommand::EndTurn = command {
            turn_runner::start_end_turn(&mut session, &mut active, &mut next_phase);
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
            GameCommand::EndTurn => unreachable!(),

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

            GameCommand::EngineerBuild {
                civilian_id,
                q,
                r,
                kind,
                recall_first,
            } => {
                if *recall_first
                    && let Err(err) = frontend_api::units::recall_civilian(game, *civilian_id)
                {
                    toasts.write(Toast::error(format!("Recall failed: {}", err.message())));
                    continue;
                }
                bump = *recall_first;
                match frontend_api::units::deploy_civilian(game, *civilian_id, *q, *r) {
                    Ok(()) => {
                        bump = true;
                        deploy.0 = None;
                        // The deploy stands even when the build order fails
                        // (web parity: finalJson falls back to deployCmd).
                        if let Err(err) =
                            frontend_api::units::engineer_build(game, *civilian_id, kind)
                        {
                            toasts.write(Toast::error(format!("Build failed: {}", err.message())));
                        }
                    }
                    Err(err) => {
                        toasts.write(Toast::error(format!("Deploy failed: {}", err.message())));
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
        }

        if bump {
            data_version.0 += 1;
        }
    }
}
