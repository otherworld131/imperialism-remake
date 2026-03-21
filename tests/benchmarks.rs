use std::time::Instant;

use domain::game_state::new_game;
use domain::map::{UnitId, generate_map};
use domain::military::units::{ArmyUnit, ArmyUnitType};
use domain::turn::process_turn;
use domain::types::{Difficulty, NationId, ResourceType};

#[test]
fn benchmark_turn_resolution() {
    let mut game = new_game("bench", Difficulty::Normal, 0);
    // Warm up
    for _ in 0..10 {
        process_turn(&mut game);
    }

    let start = Instant::now();
    let turns = 100;
    for _ in 0..turns {
        process_turn(&mut game);
    }
    let elapsed = start.elapsed();

    let per_turn = elapsed / turns;
    println!(
        "Turn resolution: {:?} per turn ({} turns in {:?})",
        per_turn, turns, elapsed
    );

    // Performance target: < 50ms per turn (plan says < 5 seconds for full resolution)
    assert!(
        per_turn.as_millis() < 50,
        "Turn resolution too slow: {:?}",
        per_turn
    );
}

#[test]
fn benchmark_map_generation() {
    let start = Instant::now();
    let maps = 10;
    for i in 0..maps {
        new_game(&format!("bench_{}", i), Difficulty::Normal, 0);
    }
    let elapsed = start.elapsed();
    let per_map = elapsed / maps;
    println!(
        "Map generation: {:?} per map ({} maps in {:?})",
        per_map, maps, elapsed
    );

    // Target: < 3 seconds per map
    assert!(
        per_map.as_secs() < 3,
        "Map generation too slow: {:?}",
        per_map
    );
}

#[test]
fn benchmark_ai_processing() {
    let mut game = new_game("ai_bench", Difficulty::Normal, 0);
    for _ in 0..20 {
        process_turn(&mut game);
    } // establish some state

    let start = Instant::now();
    let turns = 50;
    for _ in 0..turns {
        process_turn(&mut game);
    }
    let elapsed = start.elapsed();

    println!(
        "AI + turn processing (50 turns): {:?} total, {:?} per turn",
        elapsed,
        elapsed / turns
    );
    // AI for all nations should be < 2 seconds per turn, total < 10s for 50 turns
    assert!(elapsed.as_secs() < 10, "AI processing too slow");
}

#[test]
fn benchmark_save_load() {
    use infrastructure::persistence::{load_game, save_game};
    use std::path::Path;

    let mut game = new_game("save_bench", Difficulty::Normal, 0);
    for _ in 0..50 {
        process_turn(&mut game);
    }

    let path = Path::new("/tmp/bench_save.json");

    let start = Instant::now();
    save_game(&game, path).unwrap();
    let save_time = start.elapsed();

    let start = Instant::now();
    let _loaded = load_game(path).unwrap();
    let load_time = start.elapsed();

    println!("Save: {:?}, Load: {:?}", save_time, load_time);
    assert!(save_time.as_secs() < 1, "Save too slow");
    assert!(load_time.as_secs() < 2, "Load too slow");

    std::fs::remove_file(path).ok();
}

#[test]
fn benchmark_full_game_to_1915() {
    let start = Instant::now();
    let mut game = new_game("full_bench", Difficulty::Normal, 0);
    let mut turns = 0u32;
    while !game.is_game_over() {
        process_turn(&mut game);
        turns += 1;
    }
    let elapsed = start.elapsed();
    println!(
        "Full game ({} turns): {:?} total, {:?} per turn",
        turns,
        elapsed,
        elapsed / turns
    );
    // Full game should complete in < 30 seconds
    assert!(elapsed.as_secs() < 30, "Full game too slow");
}

#[test]
fn benchmark_memory_usage() {
    let game = new_game("mem_bench", Difficulty::Normal, 0);
    let size = std::mem::size_of_val(&game);
    println!("GameState struct size: {} bytes", size);
    // This only measures the stack size, not heap. But useful as a sanity check.
    // Actual heap usage would need a memory profiler.
}

#[test]
fn memory_test_400_turn_game() {
    let mut game = new_game("mem_400", Difficulty::Normal, 0);
    for _ in 0..400 {
        process_turn(&mut game);
    }
    // Verify game state doesn't grow unboundedly
    // Check warehouse doesn't accumulate infinite resources
    let player = game.get_nation(game.human_player_nation).unwrap();
    // Resources should be bounded (consumed by food, trade, etc.)
    let total_resources: u32 = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Grain,
        ResourceType::Fruit,
        ResourceType::Livestock,
    ]
    .iter()
    .map(|r| player.resource_amount(*r))
    .sum();

    // After 400 turns, resources should be bounded (not growing infinitely)
    // History should be bounded too
    assert!(
        game.history.len() < 5000,
        "History grew too large: {}",
        game.history.len()
    );
    println!(
        "After 400 turns: {} resources in warehouse, {} history entries",
        total_resources,
        game.history.len()
    );
}

#[test]
fn stress_test_all_nations_at_war() {
    let mut game = new_game("stress", Difficulty::Normal, 0);

    // Declare war between all Great Powers
    let gp_ids: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
    for i in 0..gp_ids.len() {
        for j in (i + 1)..gp_ids.len() {
            game.diplomacy.declare_war(gp_ids[i], gp_ids[j]);
        }
    }

    // Build maximum units for each nation
    for nation in &mut game.nations {
        if nation.is_great_power() {
            for k in 0..10 {
                let unit = ArmyUnit::new(
                    UnitId(5_000_000 + nation.id.0 * 100 + k),
                    ArmyUnitType::Regulars,
                    nation.id,
                    nation.capital_province_id,
                );
                nation.army.push(unit);
            }
        }
    }

    // Run 50 turns of total war
    let start = Instant::now();
    for _ in 0..50 {
        process_turn(&mut game);
    }
    let elapsed = start.elapsed();

    println!(
        "Stress test (50 turns, all at war, 70 units): {:?}",
        elapsed
    );
    assert!(
        elapsed.as_secs() < 30,
        "Stress test too slow: {:?}",
        elapsed
    );
}

#[test]
fn profile_turn_resolution_steps() {
    let mut game = new_game("profile", Difficulty::Normal, 0);
    // Warm up
    for _ in 0..20 {
        process_turn(&mut game);
    }

    // Time a single turn in detail
    let start = Instant::now();
    let report = process_turn(&mut game);
    let elapsed = start.elapsed();

    println!("=== Turn Resolution Profile ===");
    println!("Total: {:?}", elapsed);
    println!("Resources collected: {}", report.resource_production.len());
    println!("Trade transactions: {}", report.trade_transactions.len());
    println!("Town production items: {}", report.town_production.len());
    println!("Battles: {}", report.battles.len());
    println!("Naval battles: {}", report.naval_battles.len());
    println!("Headlines: {}", report.newspaper_headlines.len());
    println!("AI actions: {}", report.ai_actions.len());
}

#[test]
fn performance_regression_baseline() {
    // Establish baseline timings for regression detection
    let mut game = new_game("regression", Difficulty::Normal, 0);

    let start = Instant::now();
    for _ in 0..100 {
        process_turn(&mut game);
    }
    let turns_100 = start.elapsed();

    let start = Instant::now();
    let _map = generate_map("regression_map");
    let map_gen = start.elapsed();

    println!("=== Performance Baseline ===");
    println!("100 turns: {:?} ({:?}/turn)", turns_100, turns_100 / 100);
    println!("Map gen: {:?}", map_gen);

    // These are the regression thresholds
    assert!(
        turns_100.as_millis() < 5000,
        "100 turns should complete in <5s"
    );
    assert!(map_gen.as_millis() < 1000, "Map gen should complete in <1s");
}
