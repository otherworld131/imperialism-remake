/// Builder helpers for creating test GameStates with sensible defaults.
///
/// These reduce boilerplate in integration and simulation tests by
/// providing pre-configured game states at common configurations.
use domain::game_state::{GameState, new_game, new_game_with_data};
use domain::turn::process_turn;
use domain::types::Difficulty;

/// Load real game data (tech tree, unit stats, ship stats) from the data directory.
pub fn real_game_data() -> domain::data::GameData {
    infrastructure::data_loader::load_game_data(std::path::Path::new("data"))
}

/// Create a minimal game with Normal difficulty and default settings.
pub fn minimal_game() -> GameState {
    new_game("test", Difficulty::Normal, 0)
}

/// Create a game with a real tech tree (from disk) at Normal difficulty.
pub fn game_with_real_data(map_key: &str) -> GameState {
    new_game_with_data(map_key, Difficulty::Normal, 0, real_game_data())
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
