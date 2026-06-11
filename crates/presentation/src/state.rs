//! App-wide state machines. M2 only needs the turn phase; screen/app states
//! slot in beside it in later milestones.

use bevy::prelude::*;

/// Whether the player can act or a turn is resolving on a background thread.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TurnPhase {
    #[default]
    Idle,
    Processing,
}
