//! Domain service trait definitions.
//! These define the contracts for core game operations.
//! Implementations live in the domain crate itself (not infrastructure).

use crate::game_state::GameState;
use crate::turn::TurnReport;
use crate::types::*;

/// Generates a random game map from a seed string.
pub trait MapGenerator {
    fn generate(&self, map_key: &str) -> GameState;
}

/// Processes a full turn of the game.
pub trait TurnProcessor {
    fn process_turn(&self, game: &mut GameState) -> TurnReport;
}

/// Resolves land and naval combat.
pub trait CombatResolver {
    fn resolve_land_battle(
        &self,
        attacker: &[crate::military::units::ArmyUnit],
        defender: &[crate::military::units::ArmyUnit],
        terrain_bonus: f64,
        fort_level: u8,
    ) -> crate::military::combat::BattleResult;
}

/// Evaluates the Council of Governors vote.
pub trait VictoryChecker {
    fn check_victory(&self, game: &GameState) -> Option<NationId>;
}
