//! End-turn resolution off the main thread. The session is moved out of
//! [`SessionRes`] into an async task so a multi-second turn never blocks
//! rendering; a polling system moves it back when the task finishes.
//!
//! The same machinery powers the top bar's **Skip N** and **Skip Until**
//! conveniences: a long-running task processes turns in a loop, streaming
//! progress lines back over a channel ("Processing 1834 Q2… (12/50)") and
//! honoring a shared cancel flag wired to the busy overlay's Cancel button.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

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

/// What a skip run is asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipSpec {
    /// Process exactly N turns (clamped to 1..=500).
    Count(u32),
    /// Process until a headline (text or reason) contains the needle,
    /// case-insensitively, capped at 1000 turns (web `MAX_TURNS`).
    Until(String),
}

pub struct SkipOutcome {
    pub session: Session,
    /// The last resolved turn's report summary (`{report: {...}}` shape),
    /// `None` when zero turns ran (instant cancel / game already over).
    pub last_report: Option<serde_json::Value>,
    pub processed: u32,
    pub matched: bool,
    pub spec: SkipSpec,
}

/// In-flight skip run: the task, its cancel flag, and the progress channel.
#[derive(Resource, Default)]
pub struct ActiveSkip(pub Option<SkipRun>);

pub struct SkipRun {
    pub task: Task<SkipOutcome>,
    pub cancel: Arc<AtomicBool>,
    /// `std::sync::mpsc::Receiver` is `Send` but not `Sync`; the mutex makes
    /// the resource `Sync` for Bevy.
    pub progress: std::sync::Mutex<Receiver<String>>,
}

/// Latest progress line for the busy overlay (empty = default text).
#[derive(Resource, Default)]
pub struct BusyProgress(pub String);

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

/// Move the session into a multi-turn skip task and enter `Processing`.
pub fn start_skip(
    session_res: &mut SessionRes,
    active: &mut ActiveSkip,
    next_phase: &mut NextState<TurnPhase>,
    spec: SkipSpec,
) {
    if active.0.is_some() {
        return;
    }
    let Some(session) = session_res.0.take() else {
        return;
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = channel::<String>();
    let task_cancel = cancel.clone();
    let task_spec = spec.clone();
    let task = AsyncComputeTaskPool::get()
        .spawn(async move { run_skip(session, task_spec, task_cancel, sender) });
    active.0 = Some(SkipRun {
        task,
        cancel,
        progress: std::sync::Mutex::new(receiver),
    });
    next_phase.set(TurnPhase::Processing);
}

/// The skip loop body (runs on the compute pool).
fn run_skip(
    mut session: Session,
    spec: SkipSpec,
    cancel: Arc<AtomicBool>,
    progress: Sender<String>,
) -> SkipOutcome {
    let (total, needle) = match &spec {
        SkipSpec::Count(n) => ((*n).clamp(1, 500), None),
        SkipSpec::Until(text) => (1000, Some(text.trim().to_lowercase())),
    };
    let mut last_report: Option<serde_json::Value> = None;
    let mut processed = 0u32;
    let mut matched = false;

    while processed < total {
        if cancel.load(Ordering::Relaxed) || session.game().is_game_over() {
            break;
        }
        let turn = session.game().turn;
        let _ = progress.send(format!("Processing {turn}… ({}/{total})", processed + 1));
        let report = frontend_api::turn::process_turn(session.game_mut());
        processed += 1;
        if let Some(needle) = needle.as_deref()
            && !needle.is_empty()
            && headlines_match(&report, needle)
        {
            matched = true;
        }
        last_report = Some(report);
        if matched {
            break;
        }
    }

    SkipOutcome {
        session,
        last_report,
        processed,
        matched,
        spec,
    }
}

/// Case-insensitive substring match over a turn report's headline text and
/// reasons (web `handleSkipUntil` parity).
fn headlines_match(report: &serde_json::Value, needle: &str) -> bool {
    report["report"]["headlines"]
        .as_array()
        .map(|headlines| {
            headlines.iter().any(|h| {
                let text = h["text"].as_str().unwrap_or_default();
                let reason = h["reason"].as_str().unwrap_or_default();
                text.to_lowercase().contains(needle) || reason.to_lowercase().contains(needle)
            })
        })
        .unwrap_or(false)
}

/// Poll the in-flight turn task; on completion reinstate the session, bump
/// the data version so view models and layers refresh, and return to `Idle`.
/// The resolved turn's headlines and battles are stashed in
/// [`CurrentTurnNews`] and the newspaper interstitial opens. War declarations
/// addressed to the player are auto-acknowledged with a toast; any remaining
/// proposals are deferred until the newspaper is dismissed (web end-turn
/// order: turn → newspaper → proposal modal).
#[allow(clippy::too_many_arguments)]
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
    finish_resolved_turns(
        outcome.session,
        Some(&outcome.report),
        &mut session_res,
        &mut data_version,
        &mut turn_info,
        &mut next_phase,
        &mut next_screen,
        &meta,
        &mut deferred,
        &mut news,
        &mut toasts,
    );
}

/// Poll the in-flight skip task: stream progress into [`BusyProgress`], and
/// on completion run the same post-turn pipeline as a single end turn, with
/// the *last* resolved turn's news in the newspaper (web parity).
#[allow(clippy::too_many_arguments)]
pub fn poll_skip_task(
    mut active: ResMut<ActiveSkip>,
    mut progress: ResMut<BusyProgress>,
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
    let Some(run) = active.0.as_mut() else {
        return;
    };
    if let Ok(receiver) = run.progress.lock() {
        while let Ok(line) = receiver.try_recv() {
            progress.0 = line;
        }
    }
    let Some(outcome) = block_on(future::poll_once(&mut run.task)) else {
        return;
    };
    active.0 = None;
    progress.0.clear();

    if let SkipSpec::Until(needle) = &outcome.spec {
        let needle = needle.trim();
        if !needle.is_empty() && !outcome.matched {
            toasts.write(Toast::info(format!(
                "Skip Until: no match for \"{needle}\" after {} turns",
                outcome.processed
            )));
        }
    }

    finish_resolved_turns(
        outcome.session,
        outcome.last_report.as_ref(),
        &mut session_res,
        &mut data_version,
        &mut turn_info,
        &mut next_phase,
        &mut next_screen,
        &meta,
        &mut deferred,
        &mut news,
        &mut toasts,
    );
}

/// Shared post-resolution pipeline for single end turns and skips: update
/// the turn display, stash the freshest report into the newspaper, check
/// proposals, reinstate the session, and open the newspaper interstitial.
#[allow(clippy::too_many_arguments)]
fn finish_resolved_turns(
    mut session: Session,
    report: Option<&serde_json::Value>,
    session_res: &mut SessionRes,
    data_version: &mut DataVersion,
    turn_info: &mut TurnInfo,
    next_phase: &mut NextState<TurnPhase>,
    next_screen: &mut NextState<Screen>,
    meta: &GameMeta,
    deferred: &mut DeferredProposals,
    news: &mut CurrentTurnNews,
    toasts: &mut MessageWriter<Toast>,
) {
    // The report labels the turn that just *resolved*; the top bar and the
    // ledger's previous-turn rotation key off the *new* current turn (web
    // `state.turn` parity), so read it from the game state.
    let turn = session.game().turn;
    turn_info.label = format!("{turn}");
    turn_info.year = turn.year();

    if let Some(report) = report {
        stash_report(news, report, turn.0, turn.year(), turn.quarter());
    }

    if !meta.observer {
        deferred.0 = check_proposals(&mut session, meta.player_nation, toasts);
    }

    session_res.0 = Some(session);
    data_version.0 += 1;
    next_phase.set(TurnPhase::Idle);
    if report.is_some() {
        // Newspaper interstitial (web: `setActiveScreen('newspaper')`).
        next_screen.set(Screen::News);
    }
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
