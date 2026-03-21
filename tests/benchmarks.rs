use std::time::Instant;

use domain::game_state::new_game;
use domain::turn::process_turn;
use domain::types::Difficulty;

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
