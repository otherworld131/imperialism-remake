//! Edge case tests for unusual game situations.
//!
//! These tests verify that the game engine handles extreme or degenerate
//! scenarios gracefully — no panics, no corruption, no infinite loops.

use domain::game_state::{GameState, new_game};
use domain::turn::process_turn;
use domain::types::*;

/// Helper: run a game for a given number of turns, verifying invariants each turn.
fn run_game(
    map_key: &str,
    turns: u32,
    difficulty: Difficulty,
    human_nation_index: usize,
) -> GameState {
    let mut game = new_game(map_key, difficulty, human_nation_index);
    for _ in 0..turns {
        process_turn(&mut game);
        // Core invariants must hold every turn:
        assert_eq!(game.world.nations.len(), 23, "Nation count must stay at 23");
        assert_eq!(game.world.provinces.len(), 120, "Province count must stay at 120");
    }
    game
}

/// Verify common post-game invariants.
fn assert_state_valid(game: &GameState) {
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    for province in &game.world.provinces {
        assert!(
            nation_ids.contains(&province.owner),
            "Province {} has invalid owner {}",
            province.name,
            province.owner
        );
    }
    assert_eq!(game.great_powers().len(), 7);
    assert_eq!(game.minor_nations().len(), 16);
}

// ── Test: All AI nations declare war on human — game survives ───

#[test]
fn all_ai_declare_war_on_human_game_survives() {
    let mut game = new_game("war_test", Difficulty::Normal, 0);
    let human_id = game.human_player_nation;

    // Every other Great Power declares war on the human player.
    let ai_gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power() && n.id != human_id)
        .map(|n| n.id)
        .collect();

    for ai_id in &ai_gp_ids {
        game.world.diplomacy.declare_war(*ai_id, human_id);
    }

    // Run 50 turns — the game should not panic despite being at war with everyone.
    for _ in 0..50 {
        process_turn(&mut game);
        assert_eq!(game.world.nations.len(), 23);
        assert_eq!(game.world.provinces.len(), 120);
    }

    assert_state_valid(&game);
}

// ── Test: Empty map key produces valid game ─────────────────────

#[test]
fn empty_map_key_produces_valid_game() {
    let game = new_game("", Difficulty::Normal, 0);
    assert_state_valid(&game);
    assert_eq!(game.turn, TurnNumber::new(1));

    // Run a few turns to make sure it works.
    let game = run_game("", 10, Difficulty::Normal, 0);
    assert_state_valid(&game);
    assert_eq!(game.turn, TurnNumber::new(11));
}

// ── Test: Very long map key produces valid game ─────────────────

#[test]
fn very_long_map_key_produces_valid_game() {
    let long_key = "a".repeat(10000);
    let game = new_game(&long_key, Difficulty::Normal, 0);
    assert_state_valid(&game);

    // Run a few turns.
    let mut game = new_game(&long_key, Difficulty::Normal, 0);
    for _ in 0..5 {
        process_turn(&mut game);
    }
    assert_state_valid(&game);
}

// ── Test: Save/load roundtrip preserves state ───────────────────

#[test]
fn save_load_cycle_preserves_state() {
    use infrastructure::persistence::{load_game, save_game};

    let mut game = new_game("save_load_cycle", Difficulty::Normal, 0);

    // Run 20 turns to build up some game state.
    for _ in 0..20 {
        process_turn(&mut game);
    }

    let dir = std::env::temp_dir().join("imperialism_edge_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("edge_save.json");

    // Save -> Load -> Verify.
    save_game(&game, &path).unwrap();
    let loaded = load_game(&path).unwrap();

    assert_eq!(loaded.turn, game.turn);
    assert_eq!(loaded.difficulty, game.difficulty);
    assert_eq!(loaded.world.map_key, game.world.map_key);
    assert_eq!(loaded.human_player_nation, game.human_player_nation);
    assert_eq!(loaded.world.nations.len(), game.world.nations.len());
    assert_eq!(loaded.world.provinces.len(), game.world.provinces.len());
    assert_eq!(loaded.world.hex_map.tile_count(), game.world.hex_map.tile_count());

    // Verify nation state roundtripped.
    for (orig, load) in game.world.nations.iter().zip(loaded.world.nations.iter()) {
        assert_eq!(orig.id, load.id);
        assert_eq!(orig.name, load.name);
        assert_eq!(orig.economy.treasury, load.economy.treasury);
        assert_eq!(orig.province_count(), load.province_count());
    }

    // Cleanup
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

// ── Test: Multiple save/load cycles preserve state ──────────────

#[test]
fn multiple_save_load_cycles_preserve_state() {
    use infrastructure::persistence::{load_game, save_game};

    let mut game = new_game("multi_cycle", Difficulty::Easy, 2);

    let dir = std::env::temp_dir().join("imperialism_multi_save_test");
    std::fs::create_dir_all(&dir).unwrap();

    // Do 3 cycles of: run turns -> save -> load -> verify.
    for cycle in 0..3 {
        for _ in 0..10 {
            process_turn(&mut game);
        }

        let path = dir.join(format!("cycle_{cycle}.json"));
        save_game(&game, &path).unwrap();
        let loaded = load_game(&path).unwrap();

        assert_eq!(loaded.turn, game.turn, "Turn mismatch at cycle {cycle}");
        assert_eq!(
            loaded.world.nations.len(),
            game.world.nations.len(),
            "Nation count mismatch at cycle {cycle}"
        );
        for (orig, load) in game.world.nations.iter().zip(loaded.world.nations.iter()) {
            assert_eq!(
                orig.economy.treasury, load.economy.treasury,
                "Treasury mismatch for {} at cycle {cycle}",
                orig.name
            );
        }

        // Continue game from loaded state.
        game = loaded;

        let _ = std::fs::remove_file(&path);
    }

    assert_state_valid(&game);

    // Cleanup
    let _ = std::fs::remove_dir(&dir);
}

// ── Test: All difficulties survive full game ────────────────────

#[test]
fn all_difficulties_survive_10_turns() {
    let difficulties = [
        Difficulty::Introductory,
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
        Difficulty::NighOnImpossible,
    ];

    for difficulty in &difficulties {
        let game = run_game("difficulty_edge", 10, *difficulty, 0);
        assert_state_valid(&game);
        assert_eq!(
            game.turn,
            TurnNumber::new(11),
            "Failed for difficulty {:?}",
            difficulty
        );
    }
}

// ── Test: Every Great Power index works as human player ─────────

#[test]
fn every_nation_index_works_as_human() {
    for index in 0..7 {
        let game = run_game("nation_index_test", 5, Difficulty::Normal, index);
        assert_state_valid(&game);
    }
}

// ── Test: Extreme nation index clamps gracefully ────────────────

#[test]
fn extreme_nation_index_clamps_gracefully() {
    // new_game clamps to the last valid index.
    let game = new_game("clamp_test", Difficulty::Normal, 999);
    assert_state_valid(&game);
    // Should have selected the last Great Power (index 6).
    let gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();
    assert!(gp_ids.contains(&game.human_player_nation));
}

// ── Test: Game state after exactly 1 turn ───────────────────────

#[test]
fn single_turn_produces_valid_state() {
    let game = run_game("single_turn", 1, Difficulty::Normal, 0);
    assert_state_valid(&game);
    assert_eq!(game.turn, TurnNumber::new(2));
}

// ── Test: Special map keys (numeric, symbolic) ──────────────────

#[test]
fn special_map_keys_produce_valid_games() {
    let keys = [
        "0",
        "1",
        "-1",
        "99999999999999999999",
        "null",
        "undefined",
        "true",
        "false",
        "NaN",
        "Infinity",
        "\t\t\t",
        "   ",
    ];

    for key in &keys {
        let game = new_game(key, Difficulty::Normal, 0);
        assert_state_valid(&game);
        assert_eq!(game.turn, TurnNumber::new(1), "Failed for key: '{}'", key);
    }
}

// ── Test: War then peace cycle does not corrupt state ────────────

#[test]
fn war_peace_cycle_does_not_corrupt_state() {
    let mut game = new_game("war_peace_cycle", Difficulty::Normal, 0);

    let gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    // Cycle war and peace between first two GPs 10 times.
    for _ in 0..10 {
        game.world.diplomacy.declare_war(gp_ids[0], gp_ids[1]);
        process_turn(&mut game);
        game.world.diplomacy.make_peace(gp_ids[0], gp_ids[1]);
        process_turn(&mut game);
    }

    assert_state_valid(&game);
}

// ── Test: Save/load roundtrip with active wars and treaties ─────

#[test]
fn save_load_roundtrip_with_wars_and_treaties() {
    use domain::events::TreatyType;
    use infrastructure::persistence::{load_game, save_game};

    let mut game = new_game("war_save", Difficulty::Normal, 0);
    let gp_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();
    let mn_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| n.id)
        .collect();

    // Set up complex diplomatic state
    game.world.diplomacy.declare_war(gp_ids[0], gp_ids[1]);
    game.world.diplomacy
        .propose_alliance(gp_ids[2], gp_ids[3])
        .unwrap();
    game.world.diplomacy
        .build_consulate(gp_ids[0], mn_ids[0])
        .unwrap();
    game.world.diplomacy.build_embassy(gp_ids[0], mn_ids[0]).unwrap();
    game.world.diplomacy.propose_pact(gp_ids[0], mn_ids[0]).unwrap();

    // Run a few turns to build up economic/military state
    for _ in 0..10 {
        process_turn(&mut game);
    }

    let dir = std::env::temp_dir().join("imperialism_war_save_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("war_save.json");

    save_game(&game, &path).unwrap();
    let loaded = load_game(&path).unwrap();

    // Verify diplomatic state roundtripped
    assert!(loaded.world.diplomacy.is_at_war(gp_ids[0], gp_ids[1]));
    assert!(
        loaded
            .world.diplomacy
            .has_treaty(gp_ids[2], gp_ids[3], TreatyType::Alliance)
    );
    assert!(loaded.world.diplomacy.has_consulate(gp_ids[0], mn_ids[0]));
    assert!(loaded.world.diplomacy.has_embassy(gp_ids[0], mn_ids[0]));
    assert!(
        loaded
            .world.diplomacy
            .has_treaty(gp_ids[0], mn_ids[0], TreatyType::NonAggressionPact)
    );

    // Verify military state roundtripped
    for (orig, load) in game.world.nations.iter().zip(loaded.world.nations.iter()) {
        assert_eq!(
            orig.military.army.len(),
            load.military.army.len(),
            "Army mismatch for {}",
            orig.name
        );
        assert_eq!(
            orig.military.warships.len(),
            load.military.warships.len(),
            "Warship mismatch for {}",
            orig.name
        );
        assert_eq!(
            orig.economy.treasury, load.economy.treasury,
            "Treasury mismatch for {}",
            orig.name
        );
    }

    // Verify the loaded game can continue without panics
    let mut continued = loaded;
    for _ in 0..5 {
        process_turn(&mut continued);
    }
    assert_state_valid(&continued);

    // Cleanup
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

// ── Test: Treasury can go negative without panics ───────────────

#[test]
fn negative_treasury_does_not_panic() {
    let mut game = new_game("negative_treasury", Difficulty::NighOnImpossible, 0);
    let human_id = game.human_player_nation;

    // Drain the human player's treasury.
    {
        let player = game.get_nation_mut(human_id).unwrap();
        player.economy.treasury = Money::dollars(-10000);
    }

    // Run turns — should not panic even with negative treasury.
    for _ in 0..20 {
        process_turn(&mut game);
        assert_eq!(game.world.nations.len(), 23);
    }

    assert_state_valid(&game);
}
