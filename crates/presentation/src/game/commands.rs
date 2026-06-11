//! Single funnel for player actions. Every UI affordance writes a
//! [`GameCommand`] message; `apply_command` is the only place that touches
//! the session in response. Later milestones add variants that call
//! `frontend_api` command functions with `session.game_mut()`.

use bevy::prelude::*;

use crate::game::resources::SessionRes;
use crate::game::turn_runner::{self, ActiveTurn};
use crate::state::TurnPhase;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameCommand {
    EndTurn,
}

pub fn apply_command(
    mut messages: MessageReader<GameCommand>,
    mut session: ResMut<SessionRes>,
    mut active: ResMut<ActiveTurn>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
) {
    for command in messages.read() {
        match command {
            GameCommand::EndTurn => {
                turn_runner::start_end_turn(&mut session, &mut active, &mut next_phase);
            }
        }
    }
}
