#![deny(warnings, clippy::all)]

mod batch;
mod cli;
mod commands;
mod display;
mod flavor_bridge;
mod saves;

use clap::Parser;

use application::scenarios as app_scenarios;
use application::{Difficulty, calculate_score, process_turn};
use infrastructure::data_loader::load_embedded_game_data;

use crate::flavor_bridge::apply_flavor;

fn main() {
    let args = cli::CliArgs::parse();

    // Batch mode: runs N games headless, outputs JSON
    if let Some(n) = args.batch {
        if n == 0 {
            eprintln!("Error: --batch requires a positive number.");
            std::process::exit(1);
        }
        batch::run_batch(n, args.batch_verbose_cashflow, args.batch_max_turns);
        return;
    }

    let ai_debug = args.ai_debug;

    // Diagnostic mode: load (or new), run AI for one turn, dump transport state, exit.
    if args.dump_transport {
        let mut game = match args.load.as_deref() {
            Some(filename) => match saves::load_saved_game(filename) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("Failed to load save '{}': {}", filename, e);
                    std::process::exit(1);
                }
            },
            None => {
                let map_key = args.map_key.as_deref().unwrap_or("imperialism");
                let nation_index = args.nation_index.unwrap_or(0);
                application::new_game_with_data(
                    map_key,
                    Difficulty::Normal,
                    nation_index,
                    load_embedded_game_data(),
                )
            }
        };
        apply_flavor(&mut game, "");
        game.ai_debug = ai_debug;
        if args.force_observer {
            game.observer_mode = true;
        }
        dump_transport_diagnostic(&mut game, args.auto_turns);
        return;
    }

    println!("╔══════════════════════════════════════════════╗");
    println!("║         IMPERIALISM REMAKE v0.1.0            ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();

    let mut game = if let Some(scenario_id) = &args.scenario {
        let nation_index = args.nation_index.unwrap_or(0);
        match app_scenarios::new_scenario_game_with_data(
            scenario_id,
            Difficulty::Normal,
            nation_index,
            load_embedded_game_data(),
        ) {
            Ok(g) => {
                println!(
                    "  Starting scenario: {} ({})",
                    g.turn.year(),
                    app_scenarios::list_scenarios()
                        .iter()
                        .find(|s| s.id == scenario_id.as_str())
                        .map(|s| s.name)
                        .unwrap_or("Unknown")
                );
                g
            }
            Err(e) => {
                eprintln!("  Error: {}", e);
                eprintln!();
                eprintln!("  Available scenarios:");
                for s in app_scenarios::list_scenarios() {
                    eprintln!("    {} — {} ({})", s.id, s.name, s.year);
                }
                std::process::exit(1);
            }
        }
    } else {
        let map_key = args.map_key.as_deref().unwrap_or("imperialism");
        let nation_index = args.nation_index.unwrap_or(0);
        application::new_game_with_data(
            map_key,
            Difficulty::Normal,
            nation_index,
            load_embedded_game_data(),
        )
    };

    if let Some(filename) = args.load.as_deref() {
        match saves::load_saved_game(filename) {
            Ok(loaded) => {
                game = loaded;
                println!("  Loaded save: {}", filename);
            }
            Err(e) => {
                eprintln!("Failed to load save '{}': {}", filename, e);
                std::process::exit(1);
            }
        }
    }

    apply_flavor(&mut game, "");
    game.ai_debug = ai_debug;

    // Show initial map
    println!("  Map key: \"{}\"", game.world.map_key);
    display::print_status(&game);
    println!();
    println!(
        "  MAP ({} x {}):",
        game.world.hex_map.width(),
        game.world.hex_map.height()
    );
    println!();
    display::render_map(&game.world.hex_map, &game.world.nations);
    println!();
    display::print_provinces(&game);
    println!();
    display::print_legend();
    println!();

    // Nation selection hints
    println!("  Tip: Circular nations allow faster railroad expansion.");
    println!("  Tip: Nations with 2+ Minor Nation neighbors enable easier trade.");
    println!();

    // ── Interactive game loop ────────────────────────────────────
    loop {
        display::print_prompt(&game);
        let input = match commands::read_line() {
            Some(s) => s,
            None => {
                println!();
                println!("  Farewell, Your Excellency.");
                break;
            }
        };
        let cmd = input.trim().to_lowercase();

        match cmd.as_str() {
            "q" | "quit" | "exit" => {
                println!("  Farewell, Your Excellency.");
                break;
            }
            "t" | "turn" | "end turn" | "" => {
                let report = process_turn(&mut game);
                display::print_turn_report(&game, &report);

                // Autosave (silent)
                saves::atomic_save_game(&game, "autosave.json").ok();

                if game.is_game_over() {
                    // Record high score for each Great Power
                    let date_str = format!("{} Q{}", game.turn.year(), game.turn.quarter());
                    let gp_scores: Vec<(String, u32)> = game
                        .great_powers()
                        .iter()
                        .map(|gp| (gp.name.clone(), calculate_score(gp, &game.game_data).total))
                        .collect();
                    for (name, total) in gp_scores {
                        game.archive
                            .high_scores
                            .push((name, total, date_str.clone()));
                    }
                    game.archive.high_scores.sort_by(|a, b| b.1.cmp(&a.1));

                    println!("  ══════════════════════════════════════");
                    println!("  The year is 1915. The game has ended!");
                    println!("  ══════════════════════════════════════");
                    println!();
                    display::print_game_end_summary(&game, Some(&report));
                    break;
                }
            }
            "s" | "status" => {
                println!();
                display::print_status(&game);
            }
            "w" | "warehouse" => {
                println!();
                display::print_warehouse(&game);
            }
            "m" | "map" => {
                println!();
                display::render_map(&game.world.hex_map, &game.world.nations);
                println!();
                display::print_legend();
            }
            "p" | "provinces" => {
                println!();
                display::print_provinces(&game);
            }
            "n" | "nations" => {
                println!();
                display::print_nations(&game);
            }
            "scenarios" => {
                println!();
                println!("  Available scenarios (use --scenario flag at startup):");
                for s in app_scenarios::list_scenarios() {
                    println!("    {} — {} ({})", s.id, s.name, s.year);
                    println!("      {}", s.description);
                    println!("      Powers: {}", s.great_powers.join(", "));
                    if !s.difficulty_ratings.is_empty() {
                        let ratings: Vec<String> = s
                            .difficulty_ratings
                            .iter()
                            .map(|(nation, rating)| format!("{}: {}", nation, rating))
                            .collect();
                        println!("      Ratings: {}", ratings.join(", "));
                    }
                }
            }
            "b" | "buildings" => {
                println!();
                display::print_buildings(&game);
            }
            "pop" | "population" => {
                println!();
                display::print_population(&game);
            }
            "tech" => {
                println!();
                display::print_tech(&game);
            }
            "score" => {
                println!();
                display::print_scores(&game);
            }
            "trade" => {
                println!();
                display::print_trade(&game);
            }
            "h" | "help" | "?" => {
                println!();
                display::print_help();
            }
            "overview" => {
                println!();
                display::print_overview(&game);
            }
            "history" => {
                println!();
                display::print_history(&game);
            }
            "orders" | "pending" => {
                println!();
                display::print_pending_orders(&game);
            }
            "turn10" => {
                batch::cmd_auto(&mut game, 10);
            }
            "turn100" => {
                batch::cmd_auto(&mut game, 100);
            }
            _ if cmd.starts_with("auto ") => {
                let count_str = input.trim()[5..].trim();
                match count_str.parse::<u32>() {
                    Ok(n) if n > 0 => {
                        batch::cmd_auto(&mut game, n);
                    }
                    _ => {
                        println!("  Usage: auto <turns> (e.g. auto 50)");
                    }
                }
            }
            "auto" => {
                println!("  Usage: auto <turns> (e.g. auto 50)");
            }
            "save" => {
                saves::save_current_game(&game);
            }
            "savebin" => {
                saves::save_current_game_binary(&game);
            }
            "savegz" => {
                saves::save_current_game_gzip(&game);
            }
            "savezst" => {
                saves::save_current_game_zstd(&game);
            }
            "load" => {
                saves::list_saved_games();
            }
            _ if cmd.starts_with("load ") => {
                let filename = input.trim()[5..].trim();
                match saves::load_saved_game(filename) {
                    Ok(loaded) => {
                        game = loaded;
                        game.ai_debug = ai_debug;
                        println!("  Game loaded successfully.");
                        display::print_status(&game);
                    }
                    Err(e) => {
                        println!("  Failed to load: {}", e);
                    }
                }
            }
            _ if cmd.starts_with("delete ") => {
                let filename = input.trim()[7..].trim();
                saves::delete_saved_game(filename);
            }
            _ if cmd.starts_with("saveinfo ") => {
                let filename = input.trim()[9..].trim();
                saves::cmd_saveinfo(filename);
            }
            "quicksave" | "qs" => {
                saves::quicksave_game(&game);
            }
            "quicksavebin" | "qsb" => {
                saves::quicksave_game_binary(&game);
            }
            "quicksavegz" | "qsg" => {
                saves::quicksave_game_gzip(&game);
            }
            "quicksavezst" | "qsz" => {
                saves::quicksave_game_zstd(&game);
            }
            "quickload" | "ql" => match saves::load_saved_game("quicksave.json") {
                Ok(loaded) => {
                    game = loaded;
                    game.ai_debug = ai_debug;
                    println!("  Quickload successful.");
                    display::print_status(&game);
                }
                Err(e) => {
                    println!("  Quickload failed: {}", e);
                }
            },
            "quickloadbin" | "qlb" => match saves::load_saved_game("quicksave.bin") {
                Ok(loaded) => {
                    game = loaded;
                    game.ai_debug = ai_debug;
                    println!("  Binary quickload successful.");
                    display::print_status(&game);
                }
                Err(e) => {
                    println!("  Binary quickload failed: {}", e);
                }
            },
            "quickloadgz" | "qlg" => match saves::load_saved_game("quicksave.json.gz") {
                Ok(loaded) => {
                    game = loaded;
                    game.ai_debug = ai_debug;
                    println!("  Gzip quickload successful.");
                    display::print_status(&game);
                }
                Err(e) => {
                    println!("  Gzip quickload failed: {}", e);
                }
            },
            "quickloadzst" | "qlz" => match saves::load_saved_game("quicksave.bin.zst") {
                Ok(loaded) => {
                    game = loaded;
                    game.ai_debug = ai_debug;
                    println!("  Zstd quickload successful.");
                    display::print_status(&game);
                }
                Err(e) => {
                    println!("  Zstd quickload failed: {}", e);
                }
            },
            _ if cmd.starts_with("research ") => {
                let tech_query = input.trim()[9..].trim();
                commands::research_tech(&mut game, tech_query);
            }
            "build railroad" => {
                commands::cmd_build_railroad(&mut game);
            }
            "build depot" => {
                commands::cmd_build_depot(&mut game);
            }
            "build port" => {
                commands::cmd_build_port(&mut game);
            }
            "build fort" => {
                commands::cmd_build_fort(&mut game, None);
            }
            _ if cmd.starts_with("build fort ") => {
                let province_query = input.trim()[11..].trim();
                commands::cmd_build_fort(&mut game, Some(province_query));
            }
            "infrastructure" | "infra" => {
                println!();
                display::print_infrastructure(&game);
            }
            "build militia" => {
                commands::build_unit(&mut game, "militia");
            }
            "build freight" => {
                commands::build_freight_car(&mut game);
            }
            _ if cmd.starts_with("build unit ") => {
                let unit_query = input.trim()[11..].trim();
                commands::build_unit(&mut game, unit_query);
            }
            _ if cmd.starts_with("build ship ") => {
                let ship_query = input.trim()[11..].trim();
                commands::cmd_build_ship(&mut game, ship_query);
            }
            _ if cmd.starts_with("build warship ") => {
                let ship_query = input.trim()[14..].trim();
                commands::cmd_build_warship(&mut game, ship_query);
            }
            _ if cmd.starts_with("build ") => {
                let building_query = input.trim()[6..].trim();
                commands::build_building(&mut game, building_query);
            }
            _ if cmd.starts_with("expand ") => {
                let building_query = input.trim()[7..].trim();
                commands::expand_building(&mut game, building_query);
            }
            "recruit" => {
                commands::recruit_worker(&mut game);
            }
            "train" => {
                commands::train_worker(&mut game);
            }
            "diplomacy" | "diplo" | "d" => {
                println!();
                display::print_diplomacy(&game);
            }
            _ if cmd.starts_with("consulate ") => {
                let nation_query = input.trim()[10..].trim();
                commands::cmd_consulate(&mut game, nation_query);
            }
            _ if cmd.starts_with("embassy ") => {
                let nation_query = input.trim()[8..].trim();
                commands::cmd_embassy(&mut game, nation_query);
            }
            _ if cmd.starts_with("attack ") => {
                let nation_query = input.trim()[7..].trim();
                commands::cmd_attack(&mut game, nation_query);
            }
            _ if cmd.starts_with("beachhead ") || cmd.starts_with("landing ") => {
                let nation_query = input
                    .trim()
                    .split_once(' ')
                    .map(|(_, q)| q.trim())
                    .unwrap_or("");
                commands::cmd_beachhead(&mut game, nation_query);
            }
            "transport" | "freight" => {
                println!();
                display::print_transport(&game);
            }
            "build car" => {
                commands::build_freight_car(&mut game);
            }
            "military" | "army" => {
                println!();
                display::print_military(&game);
            }
            _ if cmd.starts_with("info ") => {
                let nation_query = input.trim()[5..].trim();
                println!();
                display::print_nation_info(&game, nation_query);
            }
            _ if cmd.starts_with("war ") => {
                let nation_query = input.trim()[4..].trim();
                commands::cmd_war(&mut game, nation_query);
            }
            _ if cmd.starts_with("peace ") => {
                let nation_query = input.trim()[6..].trim();
                commands::cmd_peace(&mut game, nation_query);
            }
            _ if cmd.starts_with("pact ") => {
                let nation_query = input.trim()[5..].trim();
                commands::cmd_pact(&mut game, nation_query);
            }
            _ if cmd.starts_with("alliance ") => {
                let nation_query = input.trim()[9..].trim();
                commands::cmd_alliance(&mut game, nation_query);
            }
            _ if cmd.starts_with("grant ") => {
                let grant_args = input.trim()[6..].trim();
                commands::cmd_grant(&mut game, grant_args);
            }
            "civilians" | "civilian" => {
                println!();
                display::print_civilians(&game);
            }
            _ if cmd.starts_with("hire ") => {
                let type_query = input.trim()[5..].trim();
                commands::cmd_hire_civilian(&mut game, type_query);
            }
            _ if cmd.starts_with("deploy ") => {
                let deploy_args = input.trim()[7..].trim();
                commands::cmd_deploy_civilian(&mut game, deploy_args);
            }
            _ if cmd.starts_with("move ") => {
                let move_args = input.trim()[5..].trim();
                commands::cmd_move_unit(&mut game, move_args);
            }
            "fleet" => {
                println!();
                display::print_fleet(&game);
            }
            "navy" => {
                println!();
                display::print_navy(&game);
            }
            "produce arms" => {
                commands::cmd_produce_arms(&mut game);
            }
            _ if cmd.starts_with("blockade ") => {
                let nation_query = input.trim()[9..].trim();
                commands::cmd_blockade(&game, nation_query);
            }
            _ if cmd.starts_with("sell ") => {
                let sell_args = input.trim()[5..].trim();
                commands::cmd_sell(&mut game, sell_args);
            }
            _ if cmd.starts_with("upgrade ") => {
                let index_str = input.trim()[8..].trim();
                commands::cmd_upgrade_unit(&mut game, index_str);
            }
            _ if cmd.starts_with("subsidy ") => {
                let subsidy_args = input.trim()[8..].trim();
                commands::cmd_subsidy(&mut game, subsidy_args);
            }
            _ => {
                println!("  Unknown command. Type 'help' for available commands.");
            }
        }
    }
}

/// Dump per-Great-Power transport state as JSON, then exit.
///
/// Runs `process_turn` once so the AI's freight-allocation pass observes the
/// loaded save (otherwise we'd just be printing back whatever was already on
/// disk). Output is one JSON object on stdout for easy `jq` consumption.
fn dump_transport_diagnostic(game: &mut domain::game_state::GameState, auto_turns: u32) {
    let human_id = game.human_player_nation;
    let observer_mode = game.observer_mode;

    let before = collect_transport_rows(game);

    for _ in 0..auto_turns {
        let _ = process_turn(game);
    }

    let after = collect_transport_rows(game);

    let json = serde_json::json!({
        "human_player_nation": human_id.0,
        "observer_mode": observer_mode,
        "turn_after": format!("{} Q{}", game.turn.year(), game.turn.quarter()),
        "turns_advanced": auto_turns,
        "before": before,
        "after": after,
    });
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}

fn collect_transport_rows(game: &domain::game_state::GameState) -> Vec<serde_json::Value> {
    use domain::economy::buildings::BuildingType;
    let human_id = game.human_player_nation;
    game.world
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| {
            let (mut lumber_cap, mut steel_cap, mut textile_cap, mut cannery_cap) = (0u32, 0, 0, 0);
            for b in &n.economy.buildings {
                let c = b.effective_capacity();
                match b.building_type {
                    BuildingType::LumberMill => lumber_cap += c,
                    BuildingType::SteelMill => steel_cap += c,
                    BuildingType::TextileMill | BuildingType::AdvancedTextileMill => {
                        textile_cap += c
                    }
                    BuildingType::FoodProcessing => cannery_cap += c,
                    _ => {}
                }
            }
            let allocations: Vec<(String, u32)> = n
                .military
                .transport
                .allocations
                .iter()
                .map(|(r, q)| (format!("{:?}", r), *q))
                .collect();
            let workers =
                n.economy.labor.untrained + n.economy.labor.trained + n.economy.labor.expert;
            serde_json::json!({
                "id": n.id.0,
                "name": n.name,
                "is_human": n.id == human_id,
                "workers": workers,
                "freight_cars": n.military.transport.freight_cars,
                "chain_targets": {
                    "timber_mill": n.economy.chain_targets.timber_mill,
                    "metal_mill": n.economy.chain_targets.metal_mill,
                    "textile_mill": n.economy.chain_targets.textile_mill,
                    "canned_food_factory": n.economy.chain_targets.canned_food_factory,
                },
                "building_cap": {
                    "lumber_mill": lumber_cap,
                    "steel_mill": steel_cap,
                    "textile_mill": textile_cap,
                    "cannery": cannery_cap,
                },
                "freight_allocations": allocations,
            })
        })
        .collect()
}
