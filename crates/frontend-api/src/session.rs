//! Native game session: owns the [`GameState`] as an opaque token so the
//! presentation layer never depends on `domain` directly. All reads go
//! through the query modules; all writes through the command functions.

use crate::ApiError;
use domain::game_state::GameState;
use std::path::Path;

pub struct Session {
    game: GameState,
}

impl Session {
    /// Wrap an already-created game (see [`crate::setup`] constructors).
    pub fn from_game(game: GameState) -> Self {
        Session { game }
    }

    /// Load a session from a native save file (SaveFile v4, CLI-compatible).
    pub fn load(path: &Path) -> Result<Self, ApiError> {
        let game = infrastructure::persistence::load_game_with_data(
            path,
            infrastructure::data_loader::load_embedded_game_data(),
        )
        .map_err(|e| ApiError::json(format!("load: {e}")))?;
        Ok(Session { game })
    }

    /// Save to a native save file (compressed JSON envelope).
    pub fn save(&self, path: &Path) -> Result<(), ApiError> {
        infrastructure::persistence::save_game_compressed(
            &self.game,
            path,
            infrastructure::persistence::SaveCompression::Gzip,
        )
        .map_err(|e| ApiError::json(format!("save: {e}")))
    }

    pub fn game(&self) -> &GameState {
        &self.game
    }

    pub fn game_mut(&mut self) -> &mut GameState {
        &mut self.game
    }

    /// Move the game out (used by async turn processing).
    pub fn into_game(self) -> GameState {
        self.game
    }
}
