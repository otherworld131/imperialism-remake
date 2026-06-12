//! End-turn resolution off the main thread. The session is moved out of
//! [`SessionRes`] into an async task so a multi-second turn never blocks
//! rendering; a polling system moves it back when the task finishes.

use bevy::prelude::*;
use bevy::tasks::futures_lite::future;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on};
use frontend_api::Session;

use crate::game::resources::{DataVersion, GameMeta, ProposalPrompt, SessionRes, TurnInfo};
use crate::game::vm;
use crate::state::TurnPhase;
use crate::widgets::Toast;

pub struct TurnOutcome {
    pub session: Session,
    pub report: serde_json::Value,
}

#[derive(Resource, Default)]
pub struct ActiveTurn(pub Option<Task<TurnOutcome>>);

/// Move the session into an async end-turn task and enter `Processing`.
pub fn start_end_turn(
    session_res: &mut SessionRes,
    active: &mut ActiveTurn,
    next_phase: &mut NextState<TurnPhase>,
) {
    if active.0.is_some() {
        return;
    }
    let Some(mut session) = session_res.0.take() else {
        return;
    };
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let report = frontend_api::turn::process_turn(session.game_mut());
        TurnOutcome { session, report }
    });
    active.0 = Some(task);
    next_phase.set(TurnPhase::Processing);
}

/// Poll the in-flight turn task; on completion reinstate the session, bump
/// the data version so view models and layers refresh, and return to `Idle`.
/// War declarations addressed to the player are auto-acknowledged with a
/// toast; any remaining proposals open the proposal modal (web end-turn
/// parity — the newspaper interstitial arrives in M9).
pub fn poll_turn_task(
    mut active: ResMut<ActiveTurn>,
    mut session_res: ResMut<SessionRes>,
    mut data_version: ResMut<DataVersion>,
    mut turn_info: ResMut<TurnInfo>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
    meta: Res<GameMeta>,
    mut proposal_prompt: ResMut<ProposalPrompt>,
    mut toasts: MessageWriter<Toast>,
) {
    let Some(task) = active.0.as_mut() else {
        return;
    };
    let Some(outcome) = block_on(future::poll_once(task)) else {
        return;
    };
    active.0 = None;
    let mut session = outcome.session;
    // The report labels the turn that just *resolved*; the top bar and the
    // ledger's previous-turn rotation key off the *new* current turn (web
    // `state.turn` parity), so read it from the game state.
    let turn = session.game().turn;
    turn_info.label = format!("{turn}");
    turn_info.year = turn.year();

    if !meta.observer {
        proposal_prompt.0 = check_proposals(&mut session, meta.player_nation, &mut toasts);
    }

    session_res.0 = Some(session);
    data_version.0 += 1;
    next_phase.set(TurnPhase::Idle);
}

/// Fetch the player's pending proposals, auto-acknowledging war declarations
/// (Acknowledge-only in the UI; the war is already in effect). Returns the
/// remaining proposals for the modal, or `None` when there is nothing left.
fn check_proposals(
    session: &mut Session,
    nation: u32,
    toasts: &mut MessageWriter<Toast>,
) -> Option<vm::ProposalsVm> {
    let fetch = |session: &Session| {
        frontend_api::diplomacy::get_pending_proposals(session.game(), nation)
            .ok()
            .and_then(|v| vm::parse_proposals(v).ok())
    };
    let mut proposals = fetch(session)?;

    let mut war_decls: Vec<&vm::ProposalVm> = proposals
        .proposals
        .iter()
        .filter(|p| p.proposal_type == "WarDeclaration")
        .collect();
    if !war_decls.is_empty() {
        let mut declared_by: Vec<String> = war_decls
            .iter()
            .map(|p| p.from_nation_name.clone())
            .collect();
        declared_by.dedup();
        // Highest index first so earlier removals don't shift later ones.
        war_decls.sort_by(|a, b| b.index.cmp(&a.index));
        let indices: Vec<u32> = war_decls.iter().map(|p| p.index).collect();
        for index in indices {
            if let Err(err) =
                frontend_api::diplomacy::accept_proposal(session.game_mut(), nation, index)
            {
                toasts.write(Toast::error(format!(
                    "War notice acknowledgement failed: {}",
                    err.message()
                )));
                break;
            }
        }
        toasts.write(Toast::info(format!(
            "War declared by {}. Declaration acknowledged automatically.",
            declared_by.join(", ")
        )));
        proposals = fetch(session)?;
    }

    (!proposals.proposals.is_empty()).then_some(proposals)
}
