//! End-turn resolution off the main thread. The session is moved out of
//! [`SessionRes`] into an async task so a multi-second turn never blocks
//! rendering; a polling system moves it back when the task finishes.

use bevy::prelude::*;
use bevy::tasks::futures_lite::future;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on};
use frontend_api::Session;

use crate::game::resources::{
    CurrentTurnNews, DataVersion, DeferredProposals, GameMeta, SessionRes, TurnInfo,
};
use crate::game::vm;
use crate::state::{Screen, TurnPhase};
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
/// The resolved turn's headlines and battles are stashed in
/// [`CurrentTurnNews`] and the newspaper interstitial opens. War declarations
/// addressed to the player are auto-acknowledged with a toast; any remaining
/// proposals are deferred until the newspaper is dismissed (web end-turn
/// order: turn → newspaper → proposal modal).
pub fn poll_turn_task(
    mut active: ResMut<ActiveTurn>,
    mut session_res: ResMut<SessionRes>,
    mut data_version: ResMut<DataVersion>,
    mut turn_info: ResMut<TurnInfo>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
    mut next_screen: ResMut<NextState<Screen>>,
    meta: Res<GameMeta>,
    mut deferred: ResMut<DeferredProposals>,
    mut news: ResMut<CurrentTurnNews>,
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

    stash_report(
        &mut news,
        &outcome.report,
        turn.0,
        turn.year(),
        turn.quarter(),
    );

    if !meta.observer {
        deferred.0 = check_proposals(&mut session, meta.player_nation, &mut toasts);
    }

    session_res.0 = Some(session);
    data_version.0 += 1;
    next_phase.set(TurnPhase::Idle);
    // Newspaper interstitial (web: `setActiveScreen('newspaper')`).
    next_screen.set(Screen::News);
}

/// Decode the turn report's headlines and battles into [`CurrentTurnNews`].
/// Decode failures degrade to empty lists with a warning — the report shape
/// is pinned by the wasm contract fixtures.
fn stash_report(
    news: &mut CurrentTurnNews,
    report: &serde_json::Value,
    turn_number: u32,
    year: u32,
    quarter: u32,
) {
    let body = &report["report"];
    let decode = |what: &str, value: &serde_json::Value| -> serde_json::Value {
        if value.is_null() {
            warn!("turn report missing {what}");
            serde_json::Value::Array(Vec::new())
        } else {
            value.clone()
        }
    };
    news.has_report = true;
    news.turn_number = turn_number;
    news.year = i64::from(year);
    news.quarter = quarter;
    news.headlines = vm::parse_headlines(decode("headlines", &body["headlines"]))
        .map_err(|err| warn!("headline decode failed: {err}"))
        .unwrap_or_default();
    news.battles = vm::parse_land_battles(decode("battles", &body["battles"]))
        .map_err(|err| warn!("battle decode failed: {err}"))
        .unwrap_or_default();
    news.naval_battles = vm::parse_naval_battles(decode("naval_battles", &body["naval_battles"]))
        .map_err(|err| warn!("naval-battle decode failed: {err}"))
        .unwrap_or_default();
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
