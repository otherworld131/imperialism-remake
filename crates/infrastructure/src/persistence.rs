use domain::data::GameData;
use domain::game_state::GameState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Current save file format version.
pub const CURRENT_SAVE_VERSION: u32 = 2;

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
    /// The game state contained in this save.
    pub game: GameState,
}

/// Get the current time as an ISO 8601 string (UTC-like, no external deps).
fn current_timestamp() -> String {
    // Use UNIX_EPOCH + SystemTime to produce a basic timestamp.
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Convert to a basic date-time string (approximate, no leap-second handling)
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since 1970-01-01 to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm for Gregorian calendar
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
///
/// The game state is wrapped in a [`SaveFile`] with the current format version
/// and metadata (nation name, turn display, difficulty, timestamp).
pub fn save_game(game: &GameState, path: &Path) -> Result<(), String> {
    let nation_name = game
        .get_nation(game.human_player_nation)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let turn_display = format!("{} Q{}", game.turn.year(), game.turn.quarter());
    let difficulty = format!("{:?}", game.difficulty);
    let timestamp = current_timestamp();

    let save = SaveFile {
        version: CURRENT_SAVE_VERSION,
        nation_name,
        turn_display,
        difficulty,
        timestamp,
        game: clone_game_state_for_save(game),
    };
    let json =
        serde_json::to_string_pretty(&save).map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(path, json).map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}

/// Load a game state from a JSON file at the given path.
///
/// Checks the save file version and rejects files from future versions.
/// After deserialization, transient fields (`tech_tree`, `events`) are
/// reconstructed since they are skipped during serialization.
pub fn load_game(path: &Path) -> Result<GameState, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("Read error: {}", e))?;

    // Try to load as versioned SaveFile first.
    if let Ok(save) = serde_json::from_str::<SaveFile>(&json) {
        if save.version > CURRENT_SAVE_VERSION {
            return Err(format!(
                "Save file version {} is newer than the supported version {}. \
                 Please update the game to load this save.",
                save.version, CURRENT_SAVE_VERSION
            ));
        }
        let mut game = save.game;
        game.game_data = GameData::default();
        return Ok(game);
    }

    Err("Unrecognized save format".to_string())
}

/// Read save file metadata without loading the full game state.
/// Returns (nation_name, turn_display, difficulty, timestamp) or None if unreadable.
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
pub fn delete_save(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| format!("Delete error: {}", e))
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

/// Serialize and re-deserialize the game state to produce a clean copy
/// suitable for saving (strips transient fields via serde skip).
fn clone_game_state_for_save(game: &GameState) -> GameState {
    // Serialize to JSON value and back — this respects all serde attributes
    // (skip, default, etc.) so the saved copy matches what load would produce.
    let value = serde_json::to_value(game).expect("GameState should always serialize");
    let mut copy: GameState =
        serde_json::from_value(value).expect("GameState should always deserialize");
    copy.game_data = GameData::default();
    copy
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

        // Save
        save_game(&game, &path).unwrap();
        assert!(path.exists());

        // Load
        let loaded = load_game(&path).unwrap();

        // Verify key fields match
        assert_eq!(loaded.turn, game.turn);
        assert_eq!(loaded.difficulty, game.difficulty);
        assert_eq!(loaded.world.map_key, game.world.map_key);
        assert_eq!(loaded.human_player_nation, game.human_player_nation);
        assert_eq!(loaded.world.nations.len(), game.world.nations.len());
        assert_eq!(loaded.world.provinces.len(), game.world.provinces.len());
        assert_eq!(loaded.world.hex_map.tile_count(), game.world.hex_map.tile_count());

        // Verify tech_tree was reconstructed
        assert_eq!(loaded.game_data.tech_tree.all_techs().len(), 28);

        // Verify events are empty (they are transient)
        assert!(loaded.transient.events.is_empty());

        // Verify nation data roundtripped
        let original_player = game.get_nation(game.human_player_nation).unwrap();
        let loaded_player = loaded.get_nation(loaded.human_player_nation).unwrap();
        assert_eq!(loaded_player.name, original_player.name);
        assert_eq!(loaded_player.economy.treasury, original_player.economy.treasury);
        assert_eq!(
            loaded_player.province_count(),
            original_player.province_count()
        );

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn save_and_load_after_turns() {
        let mut game = new_game("test", Difficulty::Easy, 2);

        // Advance a few turns
        game.advance_turn();
        game.advance_turn();
        game.advance_turn();

        // Modify some nation state
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
        // Easy difficulty starts with 10 timber + 5 coal, plus the 15 + 8 added above
        assert_eq!(
            loaded_player.resource_amount(domain::types::ResourceType::Timber),
            25
        );
        assert_eq!(
            loaded_player.resource_amount(domain::types::ResourceType::Coal),
            13
        );
        assert_eq!(loaded_player.economy.treasury, domain::types::Money::dollars(7500));

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_game(Path::new("/tmp/nonexistent_save_12345.json"));
        match result {
            Err(e) => assert!(
                e.contains("Read error"),
                "Expected 'Read error', got: {}",
                e
            ),
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
        match result {
            Err(e) => assert!(
                e.contains("Deserialization error"),
                "Expected 'Deserialization error', got: {}",
                e
            ),
            Ok(_) => panic!("Expected error for invalid JSON"),
        }

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    // ── Save file versioning tests ──────────────────────────────

    #[test]
    fn save_file_includes_version() {
        let game = new_game("test", Difficulty::Normal, 0);

        let dir = std::env::temp_dir().join("imperialism_test_version");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("versioned_save.json");

        save_game(&game, &path).unwrap();

        // Read the raw JSON and verify it contains a version field.
        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            parsed["version"].as_u64().unwrap(),
            CURRENT_SAVE_VERSION as u64
        );

        // Load and verify roundtrip
        let loaded = load_game(&path).unwrap();
        assert_eq!(loaded.turn, game.turn);
        assert_eq!(loaded.difficulty, game.difficulty);

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn version_mismatch_detected() {
        let game = new_game("test", Difficulty::Normal, 0);

        let dir = std::env::temp_dir().join("imperialism_test_version_mismatch");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("future_save.json");

        // Save normally
        save_game(&game, &path).unwrap();

        // Tamper with the version to simulate a future version
        let raw = std::fs::read_to_string(&path).unwrap();
        let mut parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        parsed["version"] = serde_json::Value::from(CURRENT_SAVE_VERSION + 1);
        let tampered = serde_json::to_string_pretty(&parsed).unwrap();
        std::fs::write(&path, tampered).unwrap();

        // Loading should fail with a version error
        let result = load_game(&path);
        match result {
            Err(e) => {
                assert!(
                    e.contains("newer than the supported version"),
                    "Expected version mismatch error, got: {}",
                    e
                );
            }
            Ok(_) => panic!("Expected error for future save version"),
        }

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn backwards_compatibility_version_1_loads() {
        // Simulate a v1 save file (has version + game, but no metadata fields)
        let game = new_game("test", Difficulty::Easy, 0);

        let dir = std::env::temp_dir().join("imperialism_test_backwards_compat");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v1_save.json");

        // Build a v1-style JSON manually (version + game, no metadata)
        let game_copy = clone_game_state_for_save(&game);
        let v1_json = serde_json::json!({
            "version": 1,
            "game": serde_json::to_value(&game_copy).unwrap()
        });
        std::fs::write(&path, serde_json::to_string_pretty(&v1_json).unwrap()).unwrap();

        // Verify it loads successfully
        let loaded = load_game(&path).unwrap();
        assert_eq!(loaded.turn, game.turn);
        assert_eq!(loaded.difficulty, game.difficulty);
        assert_eq!(loaded.world.map_key, game.world.map_key);
        assert_eq!(loaded.world.nations.len(), game.world.nations.len());

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn unversioned_save_loads_via_fallback() {
        // Simulate an old save file that has no version wrapper — just raw GameState.
        let game = new_game("test", Difficulty::Normal, 0);

        let dir = std::env::temp_dir().join("imperialism_test_unversioned");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old_save.json");

        // Write raw GameState JSON (no SaveFile wrapper)
        let raw_json = serde_json::to_string_pretty(&game).unwrap();
        std::fs::write(&path, &raw_json).unwrap();

        // Should still load via backwards-compatible fallback
        let loaded = load_game(&path).unwrap();
        assert_eq!(loaded.turn, game.turn);
        assert_eq!(loaded.difficulty, game.difficulty);

        // Cleanup
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

        // Read the raw JSON and verify metadata fields
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

        // Verify metadata via read_save_metadata
        let meta = read_save_metadata(&path).unwrap();
        assert_eq!(meta.version, CURRENT_SAVE_VERSION);
        assert!(!meta.nation_name.is_empty());
        assert!(meta.turn_display.contains("Q"));
        assert_eq!(meta.difficulty, "Normal");
        assert!(meta.timestamp.contains("T"));

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    // ── delete_save tests ───────────────────────────────────────

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

        // Cleanup
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn delete_save_nonexistent_returns_error() {
        let result = delete_save(Path::new("/tmp/nonexistent_save_99999.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Delete error"));
    }

    // ── list_saves tests ────────────────────────────────────────

    #[test]
    fn list_saves_returns_json_files() {
        let dir = std::env::temp_dir().join("imperialism_test_list_saves");
        std::fs::create_dir_all(&dir).unwrap();

        // Create a few JSON files
        std::fs::write(dir.join("save1.json"), "{}").unwrap();
        std::fs::write(dir.join("save2.json"), "{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a save").unwrap();

        let saves = list_saves(&dir);
        assert_eq!(saves.len(), 2, "Should find exactly 2 .json files");
        assert!(saves.iter().all(|p| p.extension().unwrap() == "json"));

        // Cleanup
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

        // Cleanup
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn list_saves_nonexistent_directory() {
        let saves = list_saves(Path::new("/tmp/nonexistent_dir_999999"));
        assert!(saves.is_empty());
    }

    // ── Save metadata (saveinfo) tests ──────────────────────────

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

        // Cleanup
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

        // Cleanup
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

        // Verify autosave exists and is loadable
        assert!(path.exists());
        let loaded = load_game(&path).unwrap();
        assert_eq!(loaded.turn, game.turn);

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
