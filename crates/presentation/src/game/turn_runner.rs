//! End-turn resolution off the main thread. The session is moved out of
//! [`SessionRes`] into an async task so a multi-second turn never blocks
//! rendering; a polling system moves it back when the task finishes.

use bevy::prelude::*;
use bevy::tasks::futures_lite::future;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on};
use frontend_api::Session;

use crate::game::resources::{DataVersion, SessionRes, TurnInfo};
use crate::state::TurnPhase;

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
pub fn poll_turn_task(
    mut active: ResMut<ActiveTurn>,
    mut session_res: ResMut<SessionRes>,
    mut data_version: ResMut<DataVersion>,
    mut turn_info: ResMut<TurnInfo>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
) {
    let Some(task) = active.0.as_mut() else {
        return;
    };
    let Some(outcome) = block_on(future::poll_once(task)) else {
        return;
    };
    active.0 = None;
    session_res.0 = Some(outcome.session);
    if let Some(label) = outcome
        .report
        .pointer("/report/turn")
        .and_then(|v| v.as_str())
    {
        turn_info.label = label.to_string();
    }
    data_version.0 += 1;
    next_phase.set(TurnPhase::Idle);
}
