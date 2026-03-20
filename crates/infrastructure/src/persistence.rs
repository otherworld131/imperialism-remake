use domain::game_state::GameState;
use domain::tech::TechTree;
use std::path::Path;

/// Save a game state to a JSON file at the given path.
pub fn save_game(game: &GameState, path: &Path) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(game).map_err(|e| format!("Serialization error: {}", e))?;
    std::fs::write(path, json).map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}

/// Load a game state from a JSON file at the given path.
///
/// After deserialization, transient fields (`tech_tree`, `events`) are
/// reconstructed since they are skipped during serialization.
pub fn load_game(path: &Path) -> Result<GameState, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("Read error: {}", e))?;
    let mut game: GameState =
        serde_json::from_str(&json).map_err(|e| format!("Deserialization error: {}", e))?;
    // Reconstruct non-serialized fields
    game.tech_tree = TechTree::new();
    Ok(game)
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
}
