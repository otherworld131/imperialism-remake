use crate::PersistenceError;
use domain::data::GameData;
use domain::game_state::GameState;
use domain_snapshot::game_state::GameState as SnapshotGameState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Current save file format version.
pub const CURRENT_SAVE_VERSION: u32 = 3;

/// Versioned save file wrapper around the game state.
#[derive(Serialize, Deserialize)]
pub struct SaveFile {
    /// Format version of this save file.
    pub version: u32,
    /// Name of the player's nation.
    #[serde(default)]
    pub nation_name: String,
    /// Human-readable turn display, e.g. "1820 Q1".
    #[serde(default)]
    pub turn_display: String,
    /// Difficulty setting name.
    #[serde(default)]
    pub difficulty: String,
    /// ISO 8601 timestamp when the save was created.
    #[serde(default)]
    pub timestamp: String,
    /// The game state contained in this save (snapshot type, serde-clean).
    pub game: SnapshotGameState,
}

/// Get the current time as an ISO 8601 string (UTC-like, no external deps).
fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since 1970-01-01 to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let mut remaining = days as i64;
    let mut year: i64 = 1970;

    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let leap = is_leap(year);
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month: usize = 0;
    while month < 12 && remaining >= month_days[month] {
        remaining -= month_days[month];
        month += 1;
    }

    (year as u64, (month + 1) as u64, (remaining + 1) as u64)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Save a game state to a JSON file at the given path.
pub fn save_game(game: &GameState, path: &Path) -> Result<(), PersistenceError> {
    let nation_name = game
        .get_nation(game.human_player_nation)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let turn_display = format!("{} Q{}", game.turn.year(), game.turn.quarter());
    let difficulty = format!("{:?}", game.difficulty);
    let timestamp = current_timestamp();

    let snapshot: SnapshotGameState = game.into();
    let save = SaveFile {
        version: CURRENT_SAVE_VERSION,
        nation_name,
        turn_display,
        difficulty,
        timestamp,
        game: snapshot,
    };
    let json = serde_json::to_string_pretty(&save)
        .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Load a game state from a JSON file at the given path.
///
/// Rejects any file whose version does not exactly match `CURRENT_SAVE_VERSION`.
/// After deserialization, `game_data` is populated with `GameData::default()`
/// (minimal placeholder). Callers that need the full tech tree should replace
/// `game_data` with the result of `load_game_data()`.
pub fn load_game(path: &Path) -> Result<GameState, PersistenceError> {
    load_game_with_data(path, GameData::default())
}

/// Load a game state and hydrate it with caller-supplied runtime `GameData`.
pub fn load_game_with_data(
    path: &Path,
    game_data: GameData,
) -> Result<GameState, PersistenceError> {
    let json = std::fs::read_to_string(path)?;

    if let Ok(save) = serde_json::from_str::<SaveFile>(&json) {
        if save.version != CURRENT_SAVE_VERSION {
            return Err(PersistenceError::UnsupportedVersion {
                found: save.version,
                max_supported: CURRENT_SAVE_VERSION,
            });
        }
        let mut game: GameState = save.game.into();
        game.game_data = game_data;
        return Ok(game);
    }

    Err(PersistenceError::UnrecognizedFormat)
}

/// Read save file metadata without loading the full game state.
/// Returns metadata or None if unreadable.
pub fn read_save_metadata(path: &Path) -> Option<SaveFileMetadata> {
    let json = std::fs::read_to_string(path).ok()?;
    if let Ok(save) = serde_json::from_str::<SaveFile>(&json) {
        Some(SaveFileMetadata {
            version: save.version,
            nation_name: save.nation_name,
            turn_display: save.turn_display,
            difficulty: save.difficulty,
            timestamp: save.timestamp,
        })
    } else {
        None
    }
}

/// Metadata extracted from a save file for display in the save browser.
pub struct SaveFileMetadata {
    pub version: u32,
    pub nation_name: String,
    pub turn_display: String,
    pub difficulty: String,
    pub timestamp: String,
}

/// Delete a save file.
pub fn delete_save(path: &Path) -> Result<(), PersistenceError> {
    std::fs::remove_file(path)?;
    Ok(())
}

/// List all save files (`.json`) in a directory, sorted by modification time (newest first).
pub fn list_saves(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| {
        let time_a = a
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let time_b = b
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        time_b.cmp(&time_a)
    });
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::game_state::new_game;
    use domain::types::Difficulty;

    #[test]
    fn save_and_load_roundtrip() {
        let game = new_game("test", Difficulty::Normal, 0);

        let dir = std::env::temp_dir().join("imperialism_test_saves");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_save.json");

        save_game(&game, &path).unwrap();
        assert!(path.exists());

        let loaded = load_game(&path).unwrap();

        assert_eq!(loaded.turn, game.turn);
        assert_eq!(loaded.difficulty, game.difficulty);
        assert_eq!(loaded.world.map_key, game.world.map_key);
        assert_eq!(loaded.human_player_nation, game.human_player_nation);
        assert_eq!(loaded.world.nations.len(), game.world.nations.len());
        assert_eq!(loaded.world.provinces.len(), game.world.provinces.len());
        assert_eq!(loaded.world.hex_map.tile_count(), game.world.hex_map.tile_count());

        // events are transient — not persisted
        assert!(loaded.transient.events.is_empty());

        let original_player = game.get_nation(game.human_player_nation).unwrap();
        let loaded_player = loaded.get_nation(loaded.human_player_nation).unwrap();
        assert_eq!(loaded_player.name, original_player.name);
        assert_eq!(loaded_player.economy.treasury, original_player.economy.treasury);
        assert_eq!(
            loaded_player.province_count(),
            original_player.province_count()
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn save_and_load_after_turns() {
        let mut game = new_game("test", Difficulty::Easy, 2);

        game.advance_turn();
        game.advance_turn();
        game.advance_turn();

        let player_id = game.human_player_nation;
        {
            let player = game.get_nation_mut(player_id).unwrap();
            player.add_resource(domain::types::ResourceType::Timber, 15);
            player.add_resource(domain::types::ResourceType::Coal, 8);
            player.economy.treasury = domain::types::Money::dollars(7500);
        }

        let dir = std::env::temp_dir().join("imperialism_test_saves_2");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_save_turns.json");

        save_game(&game, &path).unwrap();
        let loaded = load_game(&path).unwrap();

        assert_eq!(loaded.turn, game.turn);
        let loaded_player = loaded.get_nation(player_id).unwrap();
        assert_eq!(
            loaded_player.resource_amount(domain::types::ResourceType::Timber),
            35  // 20 starting (Easy) + 15 added
        );
        assert_eq!(
            loaded_player.resource_amount(domain::types::ResourceType::Coal),
            18  // 10 starting (Easy) + 8 added
        );
        assert_eq!(loaded_player.economy.treasury, domain::types::Money::dollars(7500));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_game(Path::new("/tmp/nonexistent_save_12345.json"));
        match result {
            Err(PersistenceError::Io(_)) => {}
            Err(e) => panic!("Expected Io error, got: {}", e),
            Ok(_) => panic!("Expected error for nonexistent file"),
        }
    }

    #[test]
    fn load_invalid_json_returns_error() {
        let dir = std::env::temp_dir().join("imperialism_test_saves_3");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("invalid.json");
        std::fs::write(&path, "this is not valid json").unwrap();

        let result = load_game(&path);
        assert!(
            matches!(result, Err(PersistenceError::UnrecognizedFormat)),
            "Expected UnrecognizedFormat error, got: {:?}",
            result.err()
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn save_file_includes_version() {
        let game = new_game("test", Difficulty::Normal, 0);

        let dir = std::env::temp_dir().join("imperialism_test_version");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("versioned_save.json");

        save_game(&game, &path).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            parsed["version"].as_u64().unwrap(),
            CURRENT_SAVE_VERSION as u64
        );

        let loaded = load_game(&path).unwrap();
        assert_eq!(loaded.turn, game.turn);
        assert_eq!(loaded.difficulty, game.difficulty);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn version_mismatch_detected() {
        let game = new_game("test", Difficulty::Normal, 0);

        let dir = std::env::temp_dir().join("imperialism_test_version_mismatch");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("future_save.json");

        save_game(&game, &path).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        let mut parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        parsed["version"] = serde_json::Value::from(CURRENT_SAVE_VERSION + 1);
        let tampered = serde_json::to_string_pretty(&parsed).unwrap();
        std::fs::write(&path, tampered).unwrap();

        let result = load_game(&path);
        match result {
            Err(PersistenceError::UnsupportedVersion { found, max_supported }) => {
                assert_eq!(found, CURRENT_SAVE_VERSION + 1);
                assert_eq!(max_supported, CURRENT_SAVE_VERSION);
            }
            Err(e) => panic!("Expected UnsupportedVersion error, got: {}", e),
            Ok(_) => panic!("Expected error for future save version"),
        }

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn old_version_save_is_rejected() {
        let game = new_game("test", Difficulty::Easy, 0);
        let dir = std::env::temp_dir().join("imperialism_test_old_version");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v1_save.json");
        let snapshot: SnapshotGameState = (&game).into();
        let v1_json = serde_json::json!({
            "version": 1,
            "game": serde_json::to_value(&snapshot).unwrap()
        });
        std::fs::write(&path, serde_json::to_string_pretty(&v1_json).unwrap()).unwrap();
        let result = load_game(&path);
        assert!(
            matches!(result, Err(PersistenceError::UnsupportedVersion { .. }) | Err(PersistenceError::UnrecognizedFormat)),
            "Expected version-mismatch or unrecognized-format error, got {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn unversioned_save_is_rejected() {
        let game = new_game("test", Difficulty::Normal, 0);
        let dir = std::env::temp_dir().join("imperialism_test_unversioned");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old_save.json");
        let snapshot: SnapshotGameState = (&game).into();
        let raw_json = serde_json::to_string_pretty(&snapshot).unwrap();
        std::fs::write(&path, &raw_json).unwrap();
        let result = load_game(&path);
        assert!(
            matches!(result, Err(PersistenceError::UnsupportedVersion { .. }) | Err(PersistenceError::UnrecognizedFormat)),
            "Expected version-mismatch or unrecognized-format error, got {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn save_file_includes_metadata() {
        let game = new_game("test", Difficulty::Normal, 0);

        let dir = std::env::temp_dir().join("imperialism_test_metadata");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("metadata_save.json");

        save_game(&game, &path).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            parsed["version"].as_u64().unwrap(),
            CURRENT_SAVE_VERSION as u64
        );
        assert!(!parsed["nation_name"].as_str().unwrap().is_empty());
        assert!(!parsed["turn_display"].as_str().unwrap().is_empty());
        assert_eq!(parsed["difficulty"].as_str().unwrap(), "Normal");
        assert!(!parsed["timestamp"].as_str().unwrap().is_empty());

        let meta = read_save_metadata(&path).unwrap();
        assert_eq!(meta.version, CURRENT_SAVE_VERSION);
        assert!(!meta.nation_name.is_empty());
        assert!(meta.turn_display.contains("Q"));
        assert_eq!(meta.difficulty, "Normal");
        assert!(meta.timestamp.contains("T"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn delete_save_removes_file() {
        let dir = std::env::temp_dir().join("imperialism_test_delete_save");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("to_delete.json");

        let game = new_game("test", Difficulty::Normal, 0);
        save_game(&game, &path).unwrap();
        assert!(path.exists());

        let result = delete_save(&path);
        assert!(result.is_ok());
        assert!(!path.exists());

        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn delete_save_nonexistent_returns_error() {
        let result = delete_save(Path::new("/tmp/nonexistent_save_99999.json"));
        assert!(matches!(result, Err(PersistenceError::Io(_))));
    }

    #[test]
    fn list_saves_returns_json_files() {
        let dir = std::env::temp_dir().join("imperialism_test_list_saves");
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("save1.json"), "{}").unwrap();
        std::fs::write(dir.join("save2.json"), "{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a save").unwrap();

        let saves = list_saves(&dir);
        assert_eq!(saves.len(), 2, "Should find exactly 2 .json files");
        assert!(saves.iter().all(|p| p.extension().unwrap() == "json"));

        let _ = std::fs::remove_file(dir.join("save1.json"));
        let _ = std::fs::remove_file(dir.join("save2.json"));
        let _ = std::fs::remove_file(dir.join("notes.txt"));
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn list_saves_empty_directory() {
        let dir = std::env::temp_dir().join("imperialism_test_list_saves_empty");
        std::fs::create_dir_all(&dir).unwrap();

        let saves = list_saves(&dir);
        assert!(saves.is_empty());

        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn list_saves_nonexistent_directory() {
        let saves = list_saves(Path::new("/tmp/nonexistent_dir_999999"));
        assert!(saves.is_empty());
    }

    #[test]
    fn read_save_metadata_includes_version() {
        let game = new_game("test", Difficulty::Hard, 0);

        let dir = std::env::temp_dir().join("imperialism_test_saveinfo");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("saveinfo_test.json");

        save_game(&game, &path).unwrap();

        let meta = read_save_metadata(&path).unwrap();
        assert_eq!(meta.version, CURRENT_SAVE_VERSION);
        assert!(!meta.nation_name.is_empty());
        assert!(meta.turn_display.contains("Q"));
        assert_eq!(meta.difficulty, "Hard");
        assert!(meta.timestamp.contains("T"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn read_save_metadata_returns_none_for_invalid_file() {
        let dir = std::env::temp_dir().join("imperialism_test_saveinfo_invalid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("invalid_meta.json");
        std::fs::write(&path, "not valid json at all").unwrap();

        let meta = read_save_metadata(&path);
        assert!(meta.is_none());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn read_save_metadata_returns_none_for_nonexistent_file() {
        let meta = read_save_metadata(Path::new("/tmp/nonexistent_saveinfo_99999.json"));
        assert!(meta.is_none());
    }

    #[test]
    fn autosave_works_across_turns() {
        let mut game = new_game("autosave_test", Difficulty::Normal, 0);
        let dir = std::env::temp_dir().join("imperialism_test_autosaves");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("autosave.json");

        for _ in 0..5 {
            domain::turn::process_turn(&mut game);
            save_game(&game, &path).unwrap();
        }

        assert!(path.exists());
        let loaded = load_game(&path).unwrap();
        assert_eq!(loaded.turn, game.turn);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
