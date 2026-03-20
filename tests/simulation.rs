use domain::game_state::new_game;
use domain::turn::process_turn;
use domain::types::*;

/// Run a full game simulation and verify invariants hold throughout.
fn run_simulation(
    map_key: &str,
    turns: u32,
    difficulty: Difficulty,
    human_nation_index: usize,
) -> domain::game_state::GameState {
    let mut game = new_game(map_key, difficulty, human_nation_index);
    for _ in 0..turns {
        process_turn(&mut game);
        // Invariants that must hold every turn:
        assert_eq!(game.nations.len(), 23, "Nation count must stay at 23");
        assert!(!game.nations.is_empty());
        // Total provinces should stay at 120 (provinces don't get created/destroyed, just change owners)
        assert_eq!(game.provinces.len(), 120, "Province count must stay at 120");
    }
    game
}

/// Verify common invariants on a game state that has been simulated.
fn assert_valid_game_state(game: &domain::game_state::GameState) {
    // All provinces must have an owner that exists in the nations list.
    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();
    for province in &game.provinces {
        assert!(
            nation_ids.contains(&province.owner),
            "Province {} has owner {} which is not a valid nation",
            province.name,
            province.owner
        );
    }
    // Nation counts
    assert_eq!(game.great_powers().len(), 7);
    assert_eq!(game.minor_nations().len(), 16);
}

#[test]
fn test_10_turn_smoke_test() {
    let game = run_simulation("smoke_test", 10, Difficulty::Normal, 0);

    assert_valid_game_state(&game);

    // Turn number should have advanced: started at 1, processed 10 turns => now at 11
    assert_eq!(game.turn, TurnNumber::new(11));

    // No nation should have an absurdly negative treasury (maintenance can cause
    // small negatives, but check for reasonable bounds).
    for nation in &game.nations {
        if nation.treasury.is_negative() {
            // Negative treasury is allowed by the game (maintenance costs can push it
            // negative), but it should not be catastrophically negative.
            assert!(
                nation.treasury.as_dollars() > -100_000,
                "Nation {} has unreasonably negative treasury: {}",
                nation.name,
                nation.treasury
            );
        }
    }

    // All provinces should have valid owners.
    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();
    for province in &game.provinces {
        assert!(
            nation_ids.contains(&province.owner),
            "Province {} has invalid owner {}",
            province.name,
            province.owner
        );
    }
}

#[test]
fn test_100_turn_endurance() {
    let game = run_simulation("endurance", 100, Difficulty::Normal, 0);

    assert_valid_game_state(&game);

    // Turn number: started at 1, processed 100 turns => now at 101
    assert_eq!(game.turn, TurnNumber::new(101));

    // At least one AI nation should have researched at least one technology
    // after 100 turns. The AI researches the cheapest available tech each turn.
    let any_ai_researched = game
        .nations
        .iter()
        .filter(|n| n.is_great_power() && n.id != game.human_player_nation)
        .any(|n| !n.researched_techs.is_empty());
    assert!(
        any_ai_researched,
        "After 100 turns, at least one AI nation should have researched a technology"
    );

    // AI nations should have built some army units (total across all AI > 0).
    let total_ai_army: usize = game
        .nations
        .iter()
        .filter(|n| n.is_great_power() && n.id != game.human_player_nation)
        .map(|n| n.army.len())
        .sum();
    assert!(
        total_ai_army > 0,
        "After 100 turns, AI nations should have built some army units, but total is 0"
    );

    // The human player should have some resources in the warehouse from
    // resource collection and/or trade.
    let human = game.get_nation(game.human_player_nation).unwrap();
    let total_resources: u32 = human.warehouse.values().sum();
    let total_materials: u32 = human.materials.values().sum();
    let total_goods: u32 = human.goods.values().sum();
    let has_some_economic_activity = total_resources > 0
        || total_materials > 0
        || total_goods > 0
        || human.treasury > Money::ZERO;
    assert!(
        has_some_economic_activity,
        "After 100 turns, the human player should have some economic output"
    );
}

#[test]
fn test_full_game_to_1915() {
    // 400 turns: from 1815 Q1 (turn 1) to 1915 Q1 (turn 401 after advancing)
    let game = run_simulation("full_game", 400, Difficulty::Normal, 0);

    assert_valid_game_state(&game);

    // Verify game reaches 1915
    assert_eq!(game.turn.year(), 1915);
    assert_eq!(game.turn.quarter(), 1);
    assert_eq!(game.turn, TurnNumber::new(401));

    // Verify game is over
    assert!(
        game.is_game_over(),
        "Game should be over after 400 turns (1915 Q1)"
    );

    // All provinces should still have valid owners
    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();
    for province in &game.provinces {
        assert!(
            nation_ids.contains(&province.owner),
            "Province {} has invalid owner {} at end of game",
            province.name,
            province.owner
        );
    }
}

#[test]
fn test_determinism() {
    let turns = 50;
    let map_key = "determinism_test";

    let game_a = run_simulation(map_key, turns, Difficulty::Normal, 0);
    let game_b = run_simulation(map_key, turns, Difficulty::Normal, 0);

    // Both games should have identical turn numbers
    assert_eq!(game_a.turn, game_b.turn, "Turn numbers should be identical");

    // Same number of provinces per nation
    for (nation_a, nation_b) in game_a.nations.iter().zip(game_b.nations.iter()) {
        assert_eq!(
            nation_a.id, nation_b.id,
            "Nations should be in the same order"
        );
        assert_eq!(
            nation_a.province_ids.len(),
            nation_b.province_ids.len(),
            "Nation {} should have the same number of provinces in both runs",
            nation_a.name
        );
    }
}

#[test]
fn test_different_map_keys_diverge() {
    let turns = 20;

    let game_a = run_simulation("key_a", turns, Difficulty::Normal, 0);
    let game_b = run_simulation("key_b", turns, Difficulty::Normal, 0);

    // Both should be valid
    assert_valid_game_state(&game_a);
    assert_valid_game_state(&game_b);

    // The games should produce different resource totals because the maps
    // are different (different terrain placement, different tile yields).
    let total_resources_a: u32 = game_a
        .nations
        .iter()
        .flat_map(|n| n.warehouse.values())
        .sum();
    let total_resources_b: u32 = game_b
        .nations
        .iter()
        .flat_map(|n| n.warehouse.values())
        .sum();

    let total_treasury_a: i64 = game_a.nations.iter().map(|n| n.treasury.as_dollars()).sum();
    let total_treasury_b: i64 = game_b.nations.iter().map(|n| n.treasury.as_dollars()).sum();

    // At least one of these aggregate metrics should differ between the two map keys.
    let diverged = total_resources_a != total_resources_b || total_treasury_a != total_treasury_b;
    assert!(
        diverged,
        "Games with different map keys should produce different outcomes. \
         Resources: {} vs {}, Treasury: {} vs {}",
        total_resources_a, total_resources_b, total_treasury_a, total_treasury_b
    );
}

#[test]
fn test_multiple_difficulties() {
    let difficulties = [
        Difficulty::Introductory,
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
        Difficulty::NighOnImpossible,
    ];

    let mut starting_treasuries: Vec<Money> = Vec::new();

    for difficulty in &difficulties {
        let game = new_game("difficulty_test", *difficulty, 0);
        let human = game.get_nation(game.human_player_nation).unwrap();
        starting_treasuries.push(human.treasury);

        // Run 5 turns and verify valid state
        let game = run_simulation("difficulty_test", 5, *difficulty, 0);
        assert_valid_game_state(&game);
        assert_eq!(game.turn, TurnNumber::new(6));
    }

    // Starting treasury should decrease as difficulty increases.
    // Introductory ($15,000) > Easy ($12,000) > Normal ($10,000) > Hard ($8,000) > NighOnImpossible ($5,000)
    for i in 0..starting_treasuries.len() - 1 {
        assert!(
            starting_treasuries[i] > starting_treasuries[i + 1],
            "Difficulty {:?} should have higher starting treasury than {:?}: {} vs {}",
            difficulties[i],
            difficulties[i + 1],
            starting_treasuries[i],
            starting_treasuries[i + 1]
        );
    }
}

#[test]
fn test_all_nations_as_player() {
    for index in 0..7 {
        let game = run_simulation("player_nation_test", 5, Difficulty::Normal, index);
        assert_valid_game_state(&game);
        assert_eq!(game.turn, TurnNumber::new(6));

        // The human player nation should correspond to the selected index.
        // Each index selects a different Great Power.
        let great_power_ids: Vec<NationId> = game
            .nations
            .iter()
            .filter(|n| n.is_great_power())
            .map(|n| n.id)
            .collect();
        assert!(
            great_power_ids.contains(&game.human_player_nation),
            "Human player nation should be one of the Great Powers for index {}",
            index
        );
    }

    // Additionally verify that different indices give different human nations.
    let mut human_ids: Vec<NationId> = Vec::new();
    for index in 0..7 {
        let game = new_game("player_nation_test", Difficulty::Normal, index);
        human_ids.push(game.human_player_nation);
    }
    // All 7 should be unique
    let unique_count = {
        let mut s = std::collections::HashSet::new();
        for id in &human_ids {
            s.insert(*id);
        }
        s.len()
    };
    assert_eq!(
        unique_count, 7,
        "Each human_nation_index should select a different Great Power"
    );
}
