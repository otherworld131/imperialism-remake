/// Builder helpers for creating test GameStates with sensible defaults.
///
/// These reduce boilerplate in integration and simulation tests by
/// providing pre-configured game states at common configurations.
use domain::game_state::{GameState, new_game};
use domain::turn::process_turn;
use domain::types::Difficulty;

/// Create a minimal game with Normal difficulty and default settings.
pub fn minimal_game() -> GameState {
    new_game("test", Difficulty::Normal, 0)
}

/// Create a game and advance it to the given turn number.
///
/// The game starts at turn 1, so `game_at_turn(1)` returns the initial state
/// and `game_at_turn(11)` processes 10 turns.
pub fn game_at_turn(turn: u32) -> GameState {
    let mut game = minimal_game();
    for _ in 1..turn {
        process_turn(&mut game);
    }
    game
}

/// Create a new game with the specified difficulty.
pub fn game_with_difficulty(difficulty: Difficulty) -> GameState {
    new_game("test", difficulty, 0)
}

/// Create a new game with the specified map key.
#[allow(dead_code)]
pub fn game_with_map_key(map_key: &str) -> GameState {
    new_game(map_key, Difficulty::Normal, 0)
}

/// Create a new game with a specific human nation index.
#[allow(dead_code)]
pub fn game_with_nation_index(human_nation_index: usize) -> GameState {
    new_game("test", Difficulty::Normal, human_nation_index)
}
