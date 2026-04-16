#![deny(warnings, clippy::all)]

mod batch;
mod cli;
mod commands;
mod display;
mod saves;

use clap::Parser;

use domain::game_state::new_game;
use domain::turn::{calculate_score, process_turn};
use domain::types::*;

fn main() {
    let args = cli::CliArgs::parse();

    // Batch mode: runs N games headless, outputs JSON
    if let Some(n) = args.batch {
        if n == 0 {
            eprintln!("Error: --batch requires a positive number.");
            std::process::exit(1);
        }
        batch::run_batch(n);
        return;
    }

    let ai_debug = args.ai_debug;

    println!("╔══════════════════════════════════════════════╗");
    println!("║         IMPERIALISM REMAKE v0.1.0            ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();

    let mut game = if let Some(scenario_id) = &args.scenario {
        let nation_index = args.nation_index.unwrap_or(0);
        match domain::scenarios::new_scenario_game(scenario_id, Difficulty::Normal, nation_index) {
            Ok(g) => {
                println!(
                    "  Starting scenario: {} ({})",
                    g.turn.year(),
                    domain::scenarios::list_scenarios()
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
                for s in domain::scenarios::list_scenarios() {
                    eprintln!("    {} — {} ({})", s.id, s.name, s.year);
                }
                std::process::exit(1);
            }
        }
    } else {
        let map_key = args.map_key.as_deref().unwrap_or("imperialism");
        let nation_index = args.nation_index.unwrap_or(0);
        new_game(map_key, Difficulty::Normal, nation_index)
    };

    game.ai_debug = ai_debug;

    // Show initial map
    println!("  Map key: \"{}\"", game.map_key);
    display::print_status(&game);
    println!();
    println!(
        "  MAP ({} x {}):",
        game.hex_map.width(),
        game.hex_map.height()
    );
    println!();
    display::render_map(&game.hex_map, &game.nations);
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
                        .map(|gp| (gp.name.clone(), calculate_score(gp).total))
                        .collect();
                    for (name, total) in gp_scores {
                        game.high_scores.push((name, total, date_str.clone()));
                    }
                    game.high_scores.sort_by(|a, b| b.1.cmp(&a.1));

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
                display::render_map(&game.hex_map, &game.nations);
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
                for s in domain::scenarios::list_scenarios() {
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
