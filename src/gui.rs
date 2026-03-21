#![deny(warnings)]

use domain::types::Difficulty;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse arguments: imperialism-gui [map_key] [nation_index]
    // Or: imperialism-gui --scenario <id> [nation_index]
    let (map_key, difficulty, nation_index) = if args.iter().any(|a| a == "--scenario") {
        let scenario_idx = args.iter().position(|a| a == "--scenario").unwrap();
        let scenario_id = args
            .get(scenario_idx + 1)
            .map(|s| s.as_str())
            .unwrap_or("1815");
        let nation_idx: usize = args
            .get(scenario_idx + 2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        (
            format!("scenario_{}", scenario_id),
            Difficulty::Normal,
            nation_idx,
        )
    } else {
        let map_key = args
            .get(1)
            .cloned()
            .unwrap_or_else(|| "imperialism".to_string());
        let nation_idx: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        (map_key, Difficulty::Normal, nation_idx)
    };

    presentation::run_game(&map_key, difficulty, nation_index);
}
