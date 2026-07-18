//! Native game session: owns the [`GameState`] as an opaque token so the
//! presentation layer never depends on `domain` directly. All reads go
//! through the query modules; all writes through the command functions.

use crate::ApiError;
use domain::game_state::GameState;
use std::path::{Path, PathBuf};

pub struct Session {
    game: GameState,
    /// Paused mid-turn session (card #494): present between
    /// `turn_session::begin_turn` and `turn_session::finish_turn`.
    pending_turn: Option<domain::turn::TurnSession>,
}

/// Metadata card for one save file in a saves directory (native save
/// browser). Additive — no wasm export reads this.
#[derive(Debug, Clone)]
pub struct SaveSummary {
    /// File name within the saves directory (e.g. `mymap-turn3.json.gz`).
    pub file_name: String,
    pub path: PathBuf,
    pub nation_name: String,
    /// Human-readable turn display, e.g. "1820 Q1".
    pub turn_display: String,
    pub difficulty: String,
    /// ISO 8601 timestamp the save was written at.
    pub timestamp: String,
}

/// List the save files in `dir` (newest first) with their metadata.
/// Unreadable/corrupt files are listed with empty metadata so the UI can
/// still show (and overwrite) them.
pub fn list_saves(dir: &Path) -> Vec<SaveSummary> {
    infrastructure::persistence::list_saves(dir)
        .into_iter()
        .map(|path| {
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            match infrastructure::persistence::read_save_metadata(&path) {
                Some(meta) => SaveSummary {
                    file_name,
                    path,
                    nation_name: meta.nation_name,
                    turn_display: meta.turn_display,
                    difficulty: meta.difficulty,
                    timestamp: meta.timestamp,
                },
                None => SaveSummary {
                    file_name,
                    path,
                    nation_name: String::new(),
                    turn_display: String::new(),
                    difficulty: String::new(),
                    timestamp: String::new(),
                },
            }
        })
        .collect()
}

impl Session {
    /// Wrap an already-created game (see [`crate::setup`] constructors).
    pub fn from_game(game: GameState) -> Self {
        Session {
            game,
            pending_turn: None,
        }
    }

    /// Load a session from a native save file (SaveFile v4, CLI-compatible).
    pub fn load(path: &Path) -> Result<Self, ApiError> {
        let game = infrastructure::persistence::load_game_with_data(
            path,
            infrastructure::data_loader::load_embedded_game_data(),
        )
        .map_err(|e| ApiError::json(format!("load: {e}")))?;
        Ok(Session {
            game,
            pending_turn: None,
        })
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

    /// Whether this session is an observer game (all nations AI-driven;
    /// player commands are rejected by the API).
    pub fn observer_mode(&self) -> bool {
        self.game.observer_mode
    }

    /// Nation id of the human seat (the viewpoint nation in observer mode).
    pub fn human_nation(&self) -> u32 {
        self.game.human_player_nation.0
    }

    /// Map seed key the world was generated from.
    pub fn map_key(&self) -> &str {
        &self.game.world.map_key
    }

    /// Current turn number (1-based).
    pub fn turn_number(&self) -> u32 {
        self.game.turn.0
    }

    /// Difficulty as the frontend's 0..=4 scale (see
    /// [`crate::setup::difficulty_from_u8`]).
    pub fn difficulty_u8(&self) -> u8 {
        use domain::types::Difficulty;
        match self.game.difficulty {
            Difficulty::Introductory => 0,
            Difficulty::Easy => 1,
            Difficulty::Normal => 2,
            Difficulty::Hard => 3,
            Difficulty::NighOnImpossible => 4,
        }
    }

    pub fn game_mut(&mut self) -> &mut GameState {
        &mut self.game
    }

    /// Move the game out (used by async turn processing).
    pub fn into_game(self) -> GameState {
        self.game
    }

    /// The paused mid-turn session, if `turn_session::begin_turn` ran and
    /// `turn_session::finish_turn` hasn't yet.
    pub fn pending_turn(&self) -> Option<&domain::turn::TurnSession> {
        self.pending_turn.as_ref()
    }

    pub(crate) fn pending_turn_mut(&mut self) -> &mut Option<domain::turn::TurnSession> {
        &mut self.pending_turn
    }
}
