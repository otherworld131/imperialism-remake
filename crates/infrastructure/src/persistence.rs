use domain::game_state::GameState;
use domain::tech::TechTree;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Current save file format version.
pub const CURRENT_SAVE_VERSION: u32 = 1;

/// Versioned save file wrapper around the game state.
#[derive(Serialize, Deserialize)]
pub struct SaveFile {
    /// Format version of this save file.
    pub version: u32,
    /// The game state contained in this save.
    pub game: GameState,
}

/// Save a game state to a JSON file at the given path.
///
/// The game state is wrapped in a [`SaveFile`] with the current format version.
pub fn save_game(game: &GameState, path: &Path) -> Result<(), String> {
    let save = SaveFile {
        version: CURRENT_SAVE_VERSION,
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
        game.tech_tree = TechTree::new();
        return Ok(game);
    }

    // Fall back to loading unversioned saves for backwards compatibility.
    let mut game: GameState =
        serde_json::from_str(&json).map_err(|e| format!("Deserialization error: {}", e))?;
    // Reconstruct non-serialized fields
    game.tech_tree = TechTree::new();
    Ok(game)
}

/// Serialize and re-deserialize the game state to produce a clean copy
/// suitable for saving (strips transient fields via serde skip).
fn clone_game_state_for_save(game: &GameState) -> GameState {
    // Serialize to JSON value and back — this respects all serde attributes
    // (skip, default, etc.) so the saved copy matches what load would produce.
    let value = serde_json::to_value(game).expect("GameState should always serialize");
    let mut copy: GameState =
        serde_json::from_value(value).expect("GameState should always deserialize");
    copy.tech_tree = TechTree::new();
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
        assert_eq!(loaded.map_key, game.map_key);
        assert_eq!(loaded.human_player_nation, game.human_player_nation);
        assert_eq!(loaded.nations.len(), game.nations.len());
        assert_eq!(loaded.provinces.len(), game.provinces.len());
        assert_eq!(loaded.hex_map.tile_count(), game.hex_map.tile_count());

        // Verify tech_tree was reconstructed
        assert_eq!(loaded.tech_tree.all_techs().len(), 28);

        // Verify events are empty (they are transient)
        assert!(loaded.events.is_empty());

        // Verify nation data roundtripped
        let original_player = game.get_nation(game.human_player_nation).unwrap();
        let loaded_player = loaded.get_nation(loaded.human_player_nation).unwrap();
        assert_eq!(loaded_player.name, original_player.name);
        assert_eq!(loaded_player.treasury, original_player.treasury);
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
            player.treasury = domain::types::Money::dollars(7500);
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
            15
        );
        assert_eq!(
            loaded_player.resource_amount(domain::types::ResourceType::Coal),
            8
        );
        assert_eq!(loaded_player.treasury, domain::types::Money::dollars(7500));

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
        let game = new_game("test", Difficulty::Easy, 0);

        let dir = std::env::temp_dir().join("imperialism_test_backwards_compat");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v1_save.json");

        // Save with current version (which is 1)
        save_game(&game, &path).unwrap();

        // Verify it loads successfully
        let loaded = load_game(&path).unwrap();
        assert_eq!(loaded.turn, game.turn);
        assert_eq!(loaded.difficulty, game.difficulty);
        assert_eq!(loaded.map_key, game.map_key);
        assert_eq!(loaded.nations.len(), game.nations.len());

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
}
